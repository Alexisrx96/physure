//! PHS evaluated for a registry reads its units against that registry's catalog.
//!
//! The parser decides whether a word after a number is a unit (`5 fur`), and quantities are built from unit
//! text while a program runs; both used the embedded catalog whatever registry the caller had. Inside
//! `physure_core::units::with_catalog` they read the registry's.

use physure_core::quantity::Quantity;
use physure_core::units::conf::{build_registry_from_conf, extend_registry};
use physure_core::units::{with_catalog, UnitRegistry};
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

fn registry(conf: &str) -> UnitRegistry {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, conf).unwrap_or_else(|problems| panic!("{problems:?}"));
    registry
}

fn furlong() -> UnitRegistry {
    registry("[Units]\nfurlong = 201.168, L, [fur, furlong]\n")
}

fn km_is_seven() -> UnitRegistry {
    registry("[Units]\nkm = 7.0, L, [km]\n")
}

fn last_quantity(source: &str) -> Quantity {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e:?}")).into_iter().last() {
        Some(PhsValue::Quantity(q)) => q,
        other => panic!("{source:?} produced {other:?}, expected a quantity"),
    }
}

/// The value in coherent base units, which is what a unit of another scale is judged by.
fn base_magnitude(source: &str) -> f64 {
    last_quantity(source).canonical_magnitude()
}

#[test]
fn a_unit_the_registry_defines_is_a_unit_in_a_literal() {
    let custom = furlong();
    let metres = with_catalog(&custom, || base_magnitude("5 fur"));
    assert!((metres - 5.0 * 201.168).abs() < 1e-9, "{metres}");
    // arithmetic, a product with another unit and a conversion all read the same catalog
    let sum = with_catalog(&custom, || base_magnitude("1 fur + 10 m"));
    assert!((sum - 211.168).abs() < 1e-9, "{sum}");
    let speed = with_catalog(&custom, || base_magnitude("2 fur / 1 s"));
    assert!((speed - 402.336).abs() < 1e-9, "{speed}");
}

#[test]
fn a_unit_the_registry_defines_again_has_the_registrys_value() {
    let custom = km_is_seven();
    assert_eq!(with_catalog(&custom, || base_magnitude("1 km")), 7.0);
    assert_eq!(with_catalog(&custom, || base_magnitude("2 km + 1 m")), 15.0);
}

#[test]
fn a_conversion_is_asked_for_in_the_registrys_units() {
    let custom = furlong();
    let in_furlongs = with_catalog(&custom, || {
        let converted = last_quantity("402.336 m => fur");
        converted.value.mean()
    });
    assert!((in_furlongs - 2.0).abs() < 1e-9, "{in_furlongs}");
}

#[test]
fn a_function_defined_under_a_registry_and_called_under_it_uses_its_units() {
    let custom = furlong();
    let metres = with_catalog(&custom, || base_magnitude("distance(n) = n * 1 fur\ndistance(3)"));
    assert!((metres - 3.0 * 201.168).abs() < 1e-9, "{metres}");
}

#[test]
fn outside_a_scope_nothing_changed() {
    assert_eq!(base_magnitude("1 km"), 1000.0);
    assert!(eval_phs("5 fur").is_err() || !matches!(eval_phs("5 fur").unwrap().into_iter().last(), Some(PhsValue::Quantity(_))));
    // and a scope that has ended leaves nothing behind
    let custom = km_is_seven();
    with_catalog(&custom, || base_magnitude("1 km"));
    assert_eq!(base_magnitude("1 km"), 1000.0);
}

#[test]
fn two_registries_in_turn_each_answer_with_their_own() {
    let (seven, nine) = (km_is_seven(), registry("[Units]\nkm = 9.0, L, [km]\n"));
    assert_eq!(with_catalog(&seven, || base_magnitude("1 km")), 7.0);
    assert_eq!(with_catalog(&nine, || base_magnitude("1 km")), 9.0);
    assert_eq!(with_catalog(&seven, || with_catalog(&nine, || base_magnitude("1 km"))), 9.0);
}

#[test]
fn two_threads_each_evaluate_for_their_own_registry() {
    let handles: Vec<_> = [7.0, 9.0]
        .into_iter()
        .map(|km| {
            std::thread::spawn(move || {
                let custom = registry(&format!("[Units]\nkm = {km}, L, [km]\n"));
                (0..50).all(|_| with_catalog(&custom, || base_magnitude("1 km")) == km)
            })
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap());
    }
}
