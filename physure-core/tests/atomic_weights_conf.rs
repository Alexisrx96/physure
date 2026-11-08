//! The `[AtomicWeights]` section of physure.conf: `symbol = weight, half-width [, standard]` or
//! `symbol = [a, b]`, in g/mol.
//!
//! The second column of the first form is the half-width of the interval CIAAW publishes ("±"); the
//! registry keeps the standard uncertainty of a uniform distribution over that interval,
//! half-width / sqrt(3). A third field `standard` says the number already is a standard uncertainty.
//! `[a, b]` is an interval with no single value: its midpoint, and (b - a) / sqrt(12). The embedded
//! data are held to the published table in `atomic_weights_ciaaw.rs`.

use physure_core::units::conf::{build_registry_from_conf, extend_registry};

#[test]
fn the_embedded_catalog_has_all_118_elements() {
    let (registry, _) = build_registry_from_conf();
    assert_eq!(registry.atomic_weights.len(), 118);
    // an interval: H = [1.00784, 1.00811] and Pb = [206.14, 207.94] (CIAAW 2024, as printed)
    assert_eq!(registry.atomic_weights["H"], ((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt()));
    assert_eq!(registry.atomic_weights["Pb"], ((206.14 + 207.94) / 2.0, (207.94 - 206.14) / 12f64.sqrt()));
    // a value with its uncertainty in the last digits: Fe 55.845(2), Na 22.989 769 28(2)
    assert_eq!(registry.atomic_weights["Fe"], (55.845, 0.002 / 3f64.sqrt()));
    assert_eq!(registry.atomic_weights["Na"], (22.98976928, 0.00000002 / 3f64.sqrt()));
    // no standard weight: the mass number and an uncertainty of 0
    assert_eq!(registry.atomic_weights["Tc"], (98.0, 0.0));
    assert_eq!(registry.atomic_weights["Og"], (294.0, 0.0));
}

#[test]
fn a_later_definition_replaces_and_a_new_symbol_is_added() {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(
        &mut registry,
        &mut constants,
        "[AtomicWeights]\nFe = 56.0, 0.5\nD = 2.01410177812, 0.00000000012, standard\n",
    )
    .unwrap();
    assert_eq!(registry.atomic_weights["Fe"], (56.0, 0.5 / 3f64.sqrt()));
    assert_eq!(registry.atomic_weights["D"], (2.01410177812, 0.00000000012));
    assert_eq!(registry.atomic_weights.len(), 119);
    assert_eq!(
        registry.atomic_weights["H"],
        ((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt()),
        "what is not redefined stays"
    );
}

#[test]
fn the_third_field_says_whether_the_number_is_a_half_width_or_a_standard_uncertainty() {
    for (line, expected) in [
        ("X = 2.0, 0.3", 0.3 / 3f64.sqrt()),
        ("X = 2.0, 0.3, interval", 0.3 / 3f64.sqrt()),
        ("X = 2.0, 0.3, standard", 0.3),
        ("X = 2.0, 0.3 , standard ", 0.3),
    ] {
        let (mut registry, mut constants) = build_registry_from_conf();
        extend_registry(&mut registry, &mut constants, &format!("[AtomicWeights]\n{line}\n")).unwrap();
        assert_eq!(registry.atomic_weights["X"], (2.0, expected), "{line:?}");
    }
}

#[test]
fn a_line_that_does_not_read_is_reported_and_nothing_is_applied() {
    for bad in [
        "Fe = abc, 0.1",             // weight is not a number
        "Fe = 55.8",                 // no uncertainty
        "Fe = 55.8, x",              // uncertainty is not a number
        "fe = 55.8, 0.1",            // not an element symbol
        "Fe2 = 55.8, 0.1",           // not an element symbol
        "Fe = -1, 0",                // a weight must be positive
        "Fe = 0, 0",
        "Fe = 55.8, -0.1",           // an uncertainty cannot be negative
        "Fe = 55.8, nan",
        "Fe = inf, 0",
        "X = 2.0, 0.3, expanded",    // the third field is `interval` or `standard`
        "X = 2.0, 0.3, standard, extra",
        "X = 2.0, 0.3,",             // an empty third field
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

#[test]
fn an_interval_is_its_midpoint_with_the_standard_uncertainty_of_a_uniform_distribution() {
    // IUPAC: u = (b - a) / sqrt(12), the same as the half-width (b - a) / 2 divided by sqrt(3).
    for line in ["X = [1.0, 3.0]", "X =[1.0,3.0]", "X = [ 1.0 , 3.0 ]  "] {
        let (mut registry, mut constants) = build_registry_from_conf();
        extend_registry(&mut registry, &mut constants, &format!("[AtomicWeights]\n{line}\n")).unwrap();
        assert_eq!(registry.atomic_weights["X"], (2.0, 2.0 / 12f64.sqrt()), "{line:?}");
    }
}

#[test]
fn an_interval_takes_no_third_field_and_no_other_form() {
    for bad in [
        "X = [1.0, 3.0], standard", // an interval is not a number to be told apart
        "X = [1.0, 3.0], 0.1",
        "X = [3.0, 1.0]",           // reversed
        "X = [1.0, 1.0]",           // empty
        "X = [1.0]",
        "X = [1.0, 3.0",            // unclosed
        "X = [0.0, 1.0]",           // the weight must be positive
        "X = [-1.0, 1.0]",
        "X = [1.0, inf]",
        "X = [a, b]",
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
