//! A catalog for the duration of a call: unit text is read against the registry a scope names, on that
//! thread, and against the embedded catalog everywhere else.

use physure_core::quantity::Quantity;
use physure_core::units::conf::{build_registry_from_conf, extend_registry};
use physure_core::units::{with_catalog, with_current_catalog, UnitRegistry};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn extended(conf: &str) -> UnitRegistry {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, conf).unwrap_or_else(|problems| panic!("{problems:?}"));
    registry
}

/// `km` is a prefix and a unit in the catalog; here it is a unit of its own, seven metres.
fn km_is_seven() -> UnitRegistry {
    extended("[Units]\nkm = 7.0, L, [km]\n")
}

fn furlong() -> UnitRegistry {
    extended("[Units]\nfurlong = 201.168, L, [fur, furlong]\n")
}

fn scale_of(unit: &str) -> f64 {
    Quantity::new(1.0, unit).unwrap().unit.scale
}

#[test]
fn without_a_scope_unit_text_is_read_against_the_embedded_catalog() {
    assert_eq!(scale_of("km"), 1000.0);
    assert!(Quantity::new(1.0, "fur").is_err());
    with_current_catalog(|scoped| assert!(scoped.is_none()));
}

#[test]
fn inside_a_scope_unit_text_is_read_against_its_registry_and_afterwards_against_the_embedded_one() {
    let custom = km_is_seven();
    assert_eq!(with_catalog(&custom, || scale_of("km")), 7.0);
    assert_eq!(scale_of("km"), 1000.0);
}

#[test]
fn a_unit_only_the_scope_knows_is_known_inside_it_and_unknown_outside() {
    let custom = furlong();
    with_catalog(&custom, || {
        let distance = Quantity::new(5.0, "fur").unwrap();
        assert!((distance.canonical_magnitude() - 5.0 * 201.168).abs() < 1e-9);
        // the unit the answer is asked in is read against the scope too
        assert!((Quantity::new(201.168, "m").unwrap().to("fur").unwrap().value.mean() - 1.0).abs() < 1e-12);
    });
    assert!(Quantity::new(5.0, "fur").is_err());
}

#[test]
fn the_scope_gives_its_value_back_and_is_visible_to_the_current_catalog_reader() {
    let custom = km_is_seven();
    let answer = with_catalog(&custom, || {
        with_current_catalog(|scoped| {
            let scoped = scoped.expect("a scope is set");
            assert!(std::ptr::eq(scoped, &custom));
            42
        })
    });
    assert_eq!(answer, 42);
}

#[test]
fn scopes_nest_and_the_inner_one_wins() {
    let (seven, furlongs) = (km_is_seven(), furlong());
    with_catalog(&seven, || {
        assert_eq!(scale_of("km"), 7.0);
        with_catalog(&furlongs, || {
            // the inner catalog has the catalog's kilometre and a furlong
            assert_eq!(scale_of("km"), 1000.0);
            assert!(Quantity::new(1.0, "fur").is_ok());
        });
        // the outer one is back
        assert_eq!(scale_of("km"), 7.0);
        assert!(Quantity::new(1.0, "fur").is_err());
    });
    assert_eq!(scale_of("km"), 1000.0);
}

#[test]
fn a_scope_that_ends_in_a_panic_gives_way_to_the_outer_one() {
    let (outer, inner) = (km_is_seven(), furlong());
    with_catalog(&outer, || {
        let result = catch_unwind(AssertUnwindSafe(|| with_catalog(&inner, || panic!("in the inner scope"))));
        assert!(result.is_err());
        assert_eq!(scale_of("km"), 7.0, "the outer scope is back");
        with_current_catalog(|scoped| assert!(std::ptr::eq(scoped.unwrap(), &outer)));
    });
    with_current_catalog(|scoped| assert!(scoped.is_none()));
}

#[test]
fn a_scope_belongs_to_its_thread() {
    let custom = km_is_seven();
    with_catalog(&custom, || {
        assert_eq!(scale_of("km"), 7.0);
        let other_thread = std::thread::spawn(|| scale_of("km")).join().unwrap();
        assert_eq!(other_thread, 1000.0, "another thread reads the embedded catalog");
    });
}

#[test]
fn two_threads_with_two_registries_each_read_their_own() {
    let handles: Vec<_> = [7.0, 9.0]
        .into_iter()
        .map(|km| {
            std::thread::spawn(move || {
                let custom = extended(&format!("[Units]\nkm = {km}, L, [km]\n"));
                (0..200).map(|_| with_catalog(&custom, || scale_of("km"))).all(|scale| scale == km)
            })
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap());
    }
}
