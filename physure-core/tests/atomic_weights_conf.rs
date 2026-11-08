//! The `[AtomicWeights]` section of physure.conf: `symbol = weight, standard uncertainty` in g/mol.

use physure_core::units::conf::{build_registry_from_conf, extend_registry};

#[test]
fn the_embedded_catalog_has_all_118_elements() {
    let (registry, _) = build_registry_from_conf();
    assert_eq!(registry.atomic_weights.len(), 118);
    assert_eq!(registry.atomic_weights["H"], (1.008, 0.0002));
    assert_eq!(registry.atomic_weights["Fe"], (55.845, 0.002));
    assert_eq!(registry.atomic_weights["Pb"], (207.2, 1.1));
    // no standard weight: the mass number and an uncertainty of 0
    assert_eq!(registry.atomic_weights["Tc"], (98.0, 0.0));
    assert_eq!(registry.atomic_weights["Og"], (294.0, 0.0));
}

#[test]
fn a_later_definition_replaces_and_a_new_symbol_is_added() {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, "[AtomicWeights]\nFe = 56.0, 0.5\nD = 2.01410177812, 0.00000000012\n")
        .unwrap();
    assert_eq!(registry.atomic_weights["Fe"], (56.0, 0.5));
    assert_eq!(registry.atomic_weights["D"], (2.01410177812, 0.00000000012));
    assert_eq!(registry.atomic_weights.len(), 119);
    assert_eq!(registry.atomic_weights["H"], (1.008, 0.0002), "what is not redefined stays");
}

#[test]
fn a_line_that_does_not_read_is_reported_and_nothing_is_applied() {
    for bad in [
        "Fe = abc, 0.1",   // weight is not a number
        "Fe = 55.8",       // no uncertainty
        "Fe = 55.8, x",    // uncertainty is not a number
        "fe = 55.8, 0.1",  // not an element symbol
        "Fe2 = 55.8, 0.1", // not an element symbol
        "Fe = -1, 0",      // a weight must be positive
        "Fe = 0, 0",
        "Fe = 55.8, -0.1", // an uncertainty cannot be negative
        "Fe = 55.8, nan",
        "Fe = inf, 0",
    ] {
        let (mut registry, mut constants) = build_registry_from_conf();
        let before = registry.atomic_weights.clone();
        let text = format!("[AtomicWeights]\nNa = 23.0, 0.1\n{bad}\n");
        let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
        assert_eq!(problems.len(), 1, "{bad:?}: {problems:?}");
        assert!(problems[0].contains(bad), "{bad:?} not named in {problems:?}");
        assert_eq!(registry.atomic_weights, before, "{bad:?} left a trace");
    }
}
