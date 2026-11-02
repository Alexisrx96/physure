//! The sources of a quantity as plain data, for a binding that keeps its quantities outside the core (Java):
//! read from a core quantity, carried as `(source id, contribution)` pairs, and rebuilt into a core quantity
//! that merges its lineage with others as if it had never left.

use physure_core::uncertainty::new_source_id;
use physure_core::Quantity;

fn measured(value: f64, u: f64, unit: &str) -> Quantity {
    Quantity::with_uncertainty(value, u, unit).unwrap()
}

#[test]
fn a_measurement_reads_back_as_one_source_with_its_own_uncertainty() {
    let x = measured(10.0, 0.5, "m");
    let sources = x.sources().expect("a Gaussian quantity has sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].1, 0.5);
    assert_eq!(Quantity::new(3.0, "m").unwrap().sources(), Some(vec![]), "an exact quantity has no sources");
}

#[test]
fn a_rebuilt_quantity_is_the_same_measurement_and_cancels_against_the_original() {
    let x = measured(10.0, 0.5, "m");
    let rebuilt = Quantity::from_sources(10.0, x.sources().unwrap(), x.unit.clone());
    let difference = x.sub(&rebuilt).unwrap();
    assert_eq!((difference.value.mean(), difference.value.std_dev()), (0.0, 0.0));
    let square = rebuilt.mul(&x).unwrap();
    assert!((square.value.std_dev() - 2.0 * 10.0 * 0.5).abs() < 1e-12, "x*x is 2 x u, not sqrt(2) x u");
}

#[test]
fn two_measurements_with_equal_numbers_do_not_cancel() {
    let (a, b) = (measured(10.0, 0.5, "m"), measured(10.0, 0.5, "m"));
    let difference = a.sub(&b).unwrap();
    assert!((difference.value.std_dev() - 0.5 * 2f64.sqrt()).abs() < 1e-12);
}

#[test]
fn the_sources_of_a_combination_add_up_to_its_uncertainty() {
    let (a, b) = (measured(10.0, 0.3, "m"), measured(4.0, 0.4, "m"));
    let sum = a.add(&b).unwrap();
    let sources = sum.sources().unwrap();
    assert_eq!(sources.len(), 2);
    let quadrature = sources.iter().map(|&(_, c)| c * c).sum::<f64>().sqrt();
    assert!((quadrature - 0.5).abs() < 1e-12);
}

#[test]
fn an_anonymous_compound_unit_folds_its_scale_into_the_sources_as_into_the_numbers() {
    // 2 m * 3 km is shown as 6000 m^2: the stored 6 carries the scale 1000 next to the symbol m^2
    let product = measured(2.0, 0.1, "m").mul(&measured(3.0, 0.2, "km")).unwrap();
    let (_, shown_uncertainty, _) = product.display_parts();
    let stored = product.sources().unwrap();
    let shown = product.display_sources().unwrap();
    assert_eq!(stored.len(), shown.len());
    for (&(stored_id, stored_c), &(shown_id, shown_c)) in stored.iter().zip(&shown) {
        assert_eq!(stored_id, shown_id);
        assert!((shown_c - stored_c * 1000.0).abs() <= 1e-9 * shown_c.abs(), "{stored_c} -> {shown_c}");
    }
    let quadrature = shown.iter().map(|&(_, c)| c * c).sum::<f64>().sqrt();
    assert!((quadrature - shown_uncertainty).abs() <= 1e-12 * shown_uncertainty, "{quadrature} vs {shown_uncertainty}");
}

#[test]
fn a_named_unit_is_not_folded() {
    let x = measured(2.0, 0.1, "km");
    assert_eq!(x.sources(), x.display_sources());
}

#[test]
fn a_model_without_sources_says_so() {
    let unit = measured(1.0, 0.1, "m").unit;
    let sampled = Quantity::new_scalar(1.0, 0.1, unit, Some("monte_carlo"), Some(100));
    assert_eq!(sampled.sources(), None);
    assert_eq!(sampled.display_sources(), None);
}

#[test]
fn new_source_ids_never_repeat_and_stay_above_the_ids_a_binding_brings_in() {
    let (first, second) = (new_source_id(), new_source_id());
    assert!(second > first);
    let imported = second + 1_000_000;
    let unit = measured(1.0, 0.1, "m").unit;
    let _ = Quantity::from_sources(1.0, vec![(imported, 0.1)], unit);
    assert!(new_source_id() > imported, "an id that came in from outside must not be handed out again");
}
