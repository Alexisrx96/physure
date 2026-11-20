//! The `interval` flag of the catalog in PHS (spec `2026-11-20-interval-flag-design.md`): every expression of its
//! section 1, refused or with its value, and the propagation of section 2 point 3 and the conversions of point 4 as a
//! program writes them.

use physure_core::units::conf::{build_registry_from_conf, extend_registry};
use physure_core::units::{with_catalog, UnitRegistry};
use physure_script::interpreter::eval_phs;
use physure_script::PhsValue;

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

fn last(source: &str) -> String {
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).pop().unwrap().to_string()
}

fn truth(source: &str) -> bool {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).pop().unwrap() {
        PhsValue::Bool(b) => b,
        other => panic!("{source:?} gave {other:?}, expected a boolean"),
    }
}

const COMPARE: &str = "cannot compare a temperature point";

fn refused_against_a_point(source: &str) {
    let error = refused(source);
    assert!(error.contains(COMPARE), "{source:?}: {error}");
}

/// Section 1: the holes the name heuristic left, with what the flag answers.
#[test]
fn the_holes_of_the_name_heuristic() {
    // the kind no longer follows the name of the left operand
    refused_against_a_point("(1 K + 5 delta_degC) < 20 degC");
    refused_against_a_point("(5 delta_degC + 1 K) < 20 degC");
    // a point minus a point is an interval, still printed in K
    refused_against_a_point("(20 degC - 10 degC) < 20 degC");
    assert_eq!(last("20 degC - 10 degC"), "10.0 K");
    // a point converted to an interval unit
    let error = refused("20 degC => delta_degC");
    assert!(error.contains("cannot convert a temperature point (`20.0 degC`) to a temperature interval (`delta_degC`)"), "{error}");
    // a step through another dimension loses the kind: decided by the spec (section 2 point 3), an open line of the roadmap
    assert!(truth("(5 delta_degC * 1 s / 1 s) < 20 degC"));
    assert!(truth("sqrt((5 delta_degC)^2) < 20 degC"));
}

#[test]
fn an_interval_scaled_by_a_plain_number_or_a_ratio_is_an_interval() {
    for source in [
        "(5 delta_degC * 2) < 20 degC",
        "(2 * 5 delta_degC) < 20 degC",
        "(5 delta_degC / 2) < 20 degC",
        "(5 delta_degC * 50 %) < 20 degC",
        "(-5 delta_degC) < 20 degC",
        "20 degC > (5 delta_degC * 50 %)",
    ] {
        refused_against_a_point(source);
    }
    assert_eq!(last("5 delta_degC * 2"), "10.0 delta_degC");
}

#[test]
fn sums_of_intervals_and_kelvins_are_intervals_in_either_order() {
    for source in [
        "(5 delta_degC + 5 delta_degC) < 20 degC",
        "(5 delta_degC - 1 K) < 20 degC",
        "(1 K - 5 delta_degC) < 20 degC",
        "(9 delta_degF + 1 K) < 20 degC",
        "20 degC > (1 K + 5 delta_degC)",
    ] {
        refused_against_a_point(source);
    }
    assert_eq!(last("1 K + 5 delta_degC"), "6.0 K");
    assert_eq!(last("5 delta_degC + 1 K"), "6.0 delta_degC");
    assert!(truth("(1 K + 1 K) < 20 degC"), "a kelvin with a kelvin stays neutral");
    assert!(truth("(1 K + 5 delta_degC) < 10 delta_degC"), "an interval compares with an interval");
}

#[test]
fn a_point_moved_by_an_interval_is_a_point() {
    assert_eq!(last("20 degC + 5 delta_degC"), "25.0 degC");
    assert_eq!(last("5 delta_degC + 20 degC"), "25.0 degC");
    assert_eq!(last("20 degC - 5 delta_degC"), "15.0 degC");
    assert!(truth("(20 degC + 5 delta_degC) < 30 degC"));
    assert!(truth("(20 degC - 5 delta_degC) < 30 degC"));
    assert!(truth("(20 degC + (30 degC - 20 degC)) == 30 degC"), "a difference of two points moves a point");
}

#[test]
fn conversions_between_points_and_intervals() {
    for source in ["20 degC => delta_degC", "68 degF => delta_degF", "(20 degC) => delta_degC", "20 degC => delta_fahrenheit"] {
        let error = refused(source);
        assert!(error.contains("cannot convert a temperature point"), "{source:?}: {error}");
    }
    for source in ["5 delta_degC => degC", "5 delta_degC => degF", "(20 degC - 10 degC) => degC"] {
        let error = refused(source);
        assert!(error.contains("cannot convert a temperature interval"), "{source:?}: {error}");
    }
    assert_eq!(last("10 K => delta_degC"), "10.0 delta_degC");
    assert_eq!(last("5 delta_degC => K"), "5.0 K");
    assert_eq!(last("9 delta_degF => delta_degC"), "5.0 delta_degC");
    assert_eq!(last("(20 degC - 10 degC) => delta_degC"), "10.0 delta_degC");
    assert_eq!(last("25 degC => K"), "298.15 K");
    refused_against_a_point("(10 K => delta_degC) < 20 degC");
}

fn registry(conf: &str) -> UnitRegistry {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, conf).unwrap_or_else(|problems| panic!("{problems:?}"));
    registry
}

#[test]
fn a_unit_of_a_users_conf_flagged_interval_is_an_interval() {
    let registry = registry("[Units]\ndT = 1.0, O, interval, [dT]\ndU = 1.0, O, [dU]\n");
    with_catalog(&registry, || {
        refused_against_a_point("5 dT < 20 degC");
        refused_against_a_point("(2 dT + 1 K) < 20 degC");
        assert!(refused("20 degC => dT").contains("cannot convert a temperature point"));
        assert_eq!(last("20 degC + 5 dT"), "25.0 degC");
        assert!(truth("5 dU < 20 degC"), "without the flag it is a kelvin under another name");
    });
}

#[test]
fn max_min_vector_and_ranges_read_the_flag() {
    for source in [
        "max(1 K + 5 delta_degC, 20 degC)",
        "min(20 degC - 10 degC, 20 degC)",
        "vector(5 delta_degC * 50 %, 20 degC)",
        "use * from array
linspace(1 K + 5 delta_degC, 20 degC, 3)",
    ] {
        let error = refused(source);
        assert!(error.contains("temperature point") && error.contains("temperature interval"), "{source:?}: {error}");
    }
}
