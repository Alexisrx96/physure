//! What the bindings feed the core, and what the core's catalog promises them.

use physure_core::quantity::Quantity;
use physure_core::units::conf::build_registry_from_conf;
use physure_core::units::parser::Parser;

/// `NaN` and infinity were read as "no uncertainty": the quantity came out exact.
#[test]
fn an_uncertainty_that_is_not_a_finite_number_is_refused() {
    for spread in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = Quantity::with_uncertainty(1.0, spread, "m").unwrap_err().to_string();
        assert!(error.contains("finite"), "{spread}: {error}");
    }
}

#[test]
fn a_finite_uncertainty_is_kept_and_a_negative_one_is_its_magnitude() {
    assert_eq!(Quantity::with_uncertainty(1.0, 0.5, "m").unwrap().value.std_dev(), 0.5);
    assert_eq!(Quantity::with_uncertainty(1.0, -0.5, "m").unwrap().value.std_dev(), 0.5);
    assert_eq!(Quantity::with_uncertainty(1.0, 0.0, "m").unwrap().value.std_dev(), 0.0);
}

/// The categories are the lists a user picks units from: each name has to be a unit (`u` was listed
/// under mass and is only the prefix micro).
#[test]
fn every_unit_listed_in_a_category_is_a_unit() {
    let (registry, _) = build_registry_from_conf();
    assert!(registry.categories.len() >= 10);
    let mut broken = Vec::new();
    for (category, units) in &registry.categories {
        for unit in units {
            if let Err(error) = Parser::parse_expression_with_registry(unit, &registry) {
                broken.push(format!("{category}: {unit} ({error})"));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}
