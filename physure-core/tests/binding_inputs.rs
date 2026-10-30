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

// ---- what a custom definition changes

use physure_core::units::conf::extend_registry;
use std::collections::HashMap;

fn extended(conf: &str) -> physure_core::UnitRegistry {
    let (base, mut constants) = build_registry_from_conf();
    let mut registry = base;
    extend_registry(&mut registry, &mut constants, conf).unwrap_or_else(|problems| panic!("{problems:?}"));
    registry
}

#[test]
fn a_registry_that_changes_nothing_redefines_nothing() {
    let (base, _) = build_registry_from_conf();
    assert!(base.redefinitions_of(&base).is_empty());
    assert!(extended("[Units]\n").redefinitions_of(&base).is_empty());
}

#[test]
fn a_new_unit_is_not_a_redefinition() {
    let (base, _) = build_registry_from_conf();
    let custom = extended("[Units]\nfurlong = 201.168, L, [fur, furlong]\n");
    assert!(custom.knows("fur") && !base.knows("fur"));
    assert!(custom.redefinitions_of(&base).is_empty());
}

#[test]
fn a_unit_defined_again_with_another_value_is_named() {
    let (base, _) = build_registry_from_conf();
    // `km` is read from a prefix and a unit in the catalog; here it is a unit of its own with another scale
    assert_eq!(extended("[Units]\nkm = 7.0, L, [km]\n").redefinitions_of(&base), vec!["km".to_string()]);
    let again = extended("[Units]\nfoot = 0.3, L, [ft, foot, feet]\n").redefinitions_of(&base);
    assert!(again.contains(&"foot".to_string()), "{again:?}");
}

#[test]
fn a_unit_defined_again_with_the_same_value_is_not_a_redefinition() {
    let (base, _) = build_registry_from_conf();
    assert!(extended("[Units]\nfoot = 0.3048, L, [ft, foot, feet]\n").redefinitions_of(&base).is_empty());
}

#[test]
fn changed_prefixes_are_named() {
    let (base, _) = build_registry_from_conf();
    let custom = extended("[Prefixes]\nkilo = k, 2000\n");
    assert!(custom.redefinitions_of(&base).contains(&"prefixes".to_string()));
    let _: HashMap<String, f64> = custom.prefixes.clone();
}
