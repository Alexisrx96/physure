//! The third state of a temperature in PHS (spec `2026-11-20-temperature-kind-design.md`): a `K` that comes out of a
//! point (`20 degC => K`) is a known absolute temperature, a `K` written by hand is of unknown kind. The programs of
//! heat, gas laws and averages that must keep working, and the refusals of section 1.

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

/// The programs that must keep working: an absolute temperature in K moved by an interval, a heat, a rate, the gas
/// law in both forms, a difference compared with a kelvin, an average of two absolute temperatures.
#[test]
fn heat_gas_and_average_programs_keep_working() {
    assert_eq!(last("T1 = 293.15 K\nT1 + 5 delta_degC => degC"), "25.0 degC");
    assert!(truth("T = 20 degC => K\nT - 5 delta_degC > 10 degC"));
    assert!(truth("T = 20 degC => K\n5 delta_degC + T > 10 degC"));
    let heat = "m = 2 kg\nc = 4186 J/(kg*K)\nT1 = 20 degC => K\nT2 = 30 degC => K\n";
    assert_eq!(last(&format!("{heat}m*c*(T2 - T1)")), "83720.0 J");
    assert_eq!(last("m = 2 kg\nc = 4186 J/(kg*delta_degC)\nT1 = 20 degC\nT2 = 30 degC\nm*c*(T2 - T1)"), "83720.0 J");
    assert_eq!(last(&format!("{heat}(T2 - T1)/(1 s)")), "10.0 K * s^-1");
    assert!(truth(&format!("{heat}T2 - T1 > 5 K")));
    assert!(truth(&format!("{heat}T2 - T1 > 5 delta_degC")));
    assert_eq!(last(&format!("{heat}(T1 + T2) / 2")), "298.15 K");
    assert!(truth(&format!("{heat}(T1 + T2) / 2 > 10 degC")));
    assert_eq!(last(&format!("{heat}(T1 + T2) / 2 => degC")), "25.0 degC");
    assert!(truth(&format!("{heat}T_mid = (T1 + T2) / 2\nT_mid - T1 < 6 delta_degC")));
    let gas = "R = 8.314 J/(mol*K)\nn = 1 mol\n";
    assert_eq!(last(&format!("{gas}T = 20 degC\nn*R*T")), "2437.2491 J");
    assert_eq!(last(&format!("{gas}T = 20 degC => K\nn*R*T")), "2437.2491 J");
    assert_eq!(last(&format!("{gas}T = 20 degC => K\nn*R*T/(1 m^3)")), "2437.2491 Pa");
    assert!(truth("T = 20 degC => K\nT > 10 degC"));
    assert!(truth("T = 20 degC => K\nT == 293.15 K"));
    assert!(truth("T = 20 degC => K\nT == 20 degC"));
    assert!(truth("T = 20 degC => K\nmin(T, 300 K) < 25 degC"));
    assert_eq!(last("T = 20 degC => K\nT => degR"), "527.67 degR");
    assert_eq!(last("T = 20 degC => K\nT => degC"), "20.0 degC");
    // a scaled absolute temperature is of unknown kind: no refusal
    assert!(!truth("T = 20 degC => K\nT * 2 < 5 delta_degC"));
    assert!(!truth("T = 20 degC => K\nT / 2 < 5 delta_degC"));
    assert!(!truth("T = 20 degC => K\nsqrt(T^2) < 5 delta_degC"));
    // a kelvin written by hand stands for either
    assert!(!truth("300 K < 5 delta_degC"));
    assert_eq!(last("300 K => delta_degC"), "300.0 delta_degC");
    assert!(!truth("T = 20 degC => K\n(T - 300 K) > 5 delta_degC"));
}

/// Section 1: a K that comes out of a point is not compared with an interval nor converted to one.
#[test]
fn a_converted_point_is_not_an_interval() {
    let error = refused("(20 degC => K) < 5 delta_degC");
    assert!(error.contains("cannot compare a temperature point (`293.15 K`) with a temperature interval (`5.0 delta_degC`)"), "{error}");
    for source in [
        "T = 20 degC => K\n(T + 5 delta_degC) < 5 delta_degC",
        "T = 20 degC => K\n(5 delta_degC + T) < 5 delta_degC",
        "T = 20 degC => K\n(T - 5 delta_degC) < 5 delta_degC",
        "T = 20 degC => K\n(T => degR) < 5 delta_degC",
        "T = 68 degF => degR\nT < 5 delta_degF",
        "T1 = 20 degC => K\nT2 = 30 degC => K\n(T2 - T1) < 20 degC",
        "T1 = 20 degC => K\nT2 = 30 degC => K\n(T2 - T1) < T1",
    ] {
        let error = refused(source);
        assert!(error.contains(COMPARE), "{source:?}: {error}");
    }
    let error = refused("T = 20 degC => K\nT => delta_degC");
    assert!(error.contains("cannot convert a temperature point (`293.15 K`) to a temperature interval (`delta_degC`)"), "{error}");
    // a point moved by an interval prints in the point's unit, in either order
    assert_eq!(last("T = 20 degC => K\n5 delta_degC + T"), "298.15 K");
    assert_eq!(last("T = 20 degC => K\nT + 5 delta_degC"), "298.15 K");
}

#[test]
fn max_min_vector_and_ranges_read_the_kind() {
    for source in [
        "T = 20 degC => K\nmax(T, 5 delta_degC)",
        "T = 20 degC => K\nmin(5 delta_degC, T)",
        "T = 20 degC => K\nvector(T, 5 delta_degC)",
        "T = 20 degC => K\n[T, 300 K, 5 delta_degC]",
        "T = 20 degC => K\nfor x in 5 delta_degC .. T { x }",
    ] {
        let error = refused(source);
        assert!(error.contains("temperature point") && error.contains("temperature interval"), "{source:?}: {error}");
    }
    assert_eq!(last("T = 20 degC => K\nmax(T, 300 K, 20 degC)"), "300.0 K");
}
