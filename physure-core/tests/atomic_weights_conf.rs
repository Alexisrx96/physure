//! The line grammar of the atomic-weight sections of physure.conf, in g/mol.
//!
//! Every atomic-weight section obeys the same grammar: `[AtomicWeights]` (your definitions) and
//! `[AtomicWeights:<edition>]` (a table, shipped or your own). A line is `symbol = weight, half-width [, standard]`
//! or `symbol = [a, b]`. The second column of the first form is the half-width of the interval CIAAW publishes
//! ("±"); the registry keeps the standard uncertainty of a uniform distribution over that interval,
//! half-width / sqrt(3). A third field `standard` says the number already is a standard uncertainty.
//! `[a, b]` is an interval with no single value: its midpoint, and (b - a) / sqrt(12).
//!
//! Which table is read, how definitions combine with it and how editions are named is in
//! `atomic_weights_editions.rs`; the embedded data are held to the published tables in `atomic_weights_ciaaw.rs`.

use physure_core::units::conf::{build_registry_with, extend_registry};
use physure_core::units::UnitRegistry;
use std::collections::HashMap;

/// The shipped catalog and nothing else: a `physure.conf` in the working directory must not change what these
/// tests assert about the shipped editions.
fn embedded_catalog() -> (UnitRegistry, HashMap<String, String>) {
    build_registry_with("").unwrap()
}

/// The sections a text can fill. The shipped editions are not among them: user text cannot edit those (see
/// `atomic_weights_editions.rs`), and the embedded data are held to the published tables in `atomic_weights_ciaaw.rs`.
const SECTIONS: [&str; 3] = ["AtomicWeights", "AtomicWeights:acme-1", "AtomicWeights:acme-2"];

/// The entry `symbol` of the table that the section `[section]` fills.
fn entry(registry: &UnitRegistry, section: &str, symbol: &str) -> Option<(f64, f64)> {
    let table = match section.strip_prefix("AtomicWeights:") {
        Some(edition) => registry.atomic_weight_tables.get(edition)?,
        None => &registry.atomic_weights,
    };
    table.get(symbol).copied()
}

#[test]
fn the_standard_edition_has_all_118_elements_as_ciaaw_prints_them() {
    let (registry, _) = embedded_catalog();
    let standard = &registry.atomic_weight_tables["standard-2024"];
    // an interval: H = [1.00784, 1.00811] and Pb = [206.14, 207.94] (CIAAW 2024, as printed)
    assert_eq!(standard["H"], ((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt()));
    assert_eq!(standard["Pb"], ((206.14 + 207.94) / 2.0, (207.94 - 206.14) / 12f64.sqrt()));
    // a value with its uncertainty in the last digits: Fe 55.845(2), Na 22.989 769 28(2)
    assert_eq!(standard["Fe"], (55.845, 0.002 / 3f64.sqrt()));
    assert_eq!(standard["Na"], (22.98976928, 0.00000002 / 3f64.sqrt()));
    // no standard weight: the mass number and an uncertainty of 0
    assert_eq!(standard["Tc"], (98.0, 0.0));
    assert_eq!(standard["Og"], (294.0, 0.0));
}

#[test]
fn the_abridged_edition_has_the_values_and_the_plus_minus_as_ciaaw_prints_them() {
    let (registry, _) = embedded_catalog();
    let abridged = &registry.atomic_weight_tables["abridged-2024"];
    // H 1.0080 +- 0.0002, Na 22.990 +- 0.001, Pb 207.2 +- 1.1, Tc a mass number
    assert_eq!(abridged["H"], (1.0080, 0.0002 / 3f64.sqrt()));
    assert_eq!(abridged["Na"], (22.990, 0.001 / 3f64.sqrt()));
    assert_eq!(abridged["Pb"], (207.2, 1.1 / 3f64.sqrt()));
    assert_eq!(abridged["Tc"], (98.0, 0.0));
}

#[test]
fn the_third_field_says_whether_the_number_is_a_half_width_or_a_standard_uncertainty() {
    for section in SECTIONS {
        for (line, expected) in [
            ("X = 2.0, 0.3", 0.3 / 3f64.sqrt()),
            ("X = 2.0, 0.3, interval", 0.3 / 3f64.sqrt()),
            ("X = 2.0, 0.3, standard", 0.3),
            ("X = 2.0, 0.3 , standard ", 0.3),
        ] {
            let (mut registry, mut constants) = embedded_catalog();
            extend_registry(&mut registry, &mut constants, &format!("[{section}]\n{line}\n")).unwrap();
            assert_eq!(entry(&registry, section, "X"), Some((2.0, expected)), "{section} {line:?}");
        }
    }
}

#[test]
fn each_section_fills_its_own_table_and_no_other() {
    for section in SECTIONS {
        let (mut registry, mut constants) = embedded_catalog();
        let before = registry.clone();
        extend_registry(&mut registry, &mut constants, &format!("[{section}]\nX = 2.0, 0.3\n")).unwrap();
        for other in SECTIONS {
            assert_eq!(entry(&registry, other, "X").is_some(), other == section, "{section} / {other}");
        }
        // the shipped tables are exactly as they were
        for edition in ["standard-2024", "abridged-2024"] {
            assert_eq!(registry.atomic_weight_tables[edition], before.atomic_weight_tables[edition]);
        }
        assert_eq!(registry.atomic_weights.len(), usize::from(section == "AtomicWeights"));
    }
}

#[test]
fn a_line_that_does_not_read_is_reported_and_nothing_is_applied() {
    for section in SECTIONS {
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
            let (mut registry, mut constants) = embedded_catalog();
            let before = registry.clone();
            let text = format!("[{section}]\nNa = 23.0, 0.1\n{bad}\n");
            let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
            assert_eq!(problems.len(), 1, "{section} {bad:?}: {problems:?}");
            assert!(problems[0].contains(bad), "{bad:?} not named in {problems:?}");
            assert_eq!(registry, before, "{section} {bad:?} left a trace");
        }
    }
}

#[test]
fn an_interval_is_its_midpoint_with_the_standard_uncertainty_of_a_uniform_distribution() {
    // IUPAC: u = (b - a) / sqrt(12), the same as the half-width (b - a) / 2 divided by sqrt(3).
    for section in SECTIONS {
        for line in ["X = [1.0, 3.0]", "X =[1.0,3.0]", "X = [ 1.0 , 3.0 ]  "] {
            let (mut registry, mut constants) = embedded_catalog();
            extend_registry(&mut registry, &mut constants, &format!("[{section}]\n{line}\n")).unwrap();
            assert_eq!(entry(&registry, section, "X"), Some((2.0, 2.0 / 12f64.sqrt())), "{section} {line:?}");
        }
    }
}

#[test]
fn an_interval_takes_no_third_field_and_no_other_form() {
    for section in SECTIONS {
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
            let (mut registry, mut constants) = embedded_catalog();
            let before = registry.clone();
            let text = format!("[{section}]\nNa = 23.0, 0.1\n{bad}\n");
            let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
            assert_eq!(problems.len(), 1, "{section} {bad:?}: {problems:?}");
            assert!(problems[0].contains(bad), "{bad:?} not named in {problems:?}");
            assert_eq!(registry, before, "{section} {bad:?} left a trace");
        }
    }
}

#[test]
fn an_interval_is_not_lost_to_overflow_and_an_uncertainty_is_not_lost_to_underflow() {
    for section in SECTIONS {
        // (a + b) / 2 overflows to infinity for [1e308, 1.7e308]; a / 2 + b / 2 = 1.35e308 does not
        let (mut registry, mut constants) = embedded_catalog();
        extend_registry(&mut registry, &mut constants, &format!("[{section}]\nX = [1e308, 1.7e308]\n")).unwrap();
        let (weight, std) = entry(&registry, section, "X").unwrap();
        assert!(weight.is_finite() && std.is_finite(), "{section}: ({weight}, {std})");
        assert!((weight - 1.35e308).abs() <= 1e-12 * 1.35e308, "{section}: {weight}");

        // a non-zero width whose standard uncertainty underflows to 0 would be an exact weight: a lie
        let bad = "X = [5e-324, 1e-323]";
        let (mut registry, mut constants) = embedded_catalog();
        let before = registry.clone();
        let problems = extend_registry(&mut registry, &mut constants, &format!("[{section}]\n{bad}\n")).unwrap_err();
        assert_eq!(problems.len(), 1, "{section} {bad:?}: {problems:?}");
        assert!(problems[0].contains(bad), "{bad:?} not named in {problems:?}");
        assert_eq!(registry, before);
    }
}
