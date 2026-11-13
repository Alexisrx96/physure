//! What PHS comparisons answer (audit B16, B17, B18, B22, B11 and the review's D1-D10), through the entry the CLI's
//! `phs eval` uses (`report::evaluate`), with error messages read from the interpreter.
//!
//! The rules: `==` is `Quantity::phs_eq` (canonical magnitudes within a relative 1e-12, IEEE for non-finite values,
//! different dimensions an error); `!=` is exactly `!(==)`; `≈` is relative with `[Settings] approx_rel_tol`, and
//! `approx(a, b, rel)` names the tolerance; `x == t ± k sigma` asks whether the measured `x` lies within `k` of its
//! own standard uncertainties of the exact target `t`; any other pair of operand types is a type error; a
//! condition is false only for an exact zero, and a temperature point, a level or NaN is not a condition.
//!
//! Sums are parenthesised: `op_compare` binds tighter than `+` in the grammar (`0.1 + 0.2 == 0.3` is a type error,
//! audit B26), and this track does not change the grammar.

use physure_script::interpreter::eval_phs;
use physure_script::report::{evaluate, Report, DEFAULT_SEED};
use physure_script::Limits;
use std::time::Duration;

fn run(source: &str) -> Report {
    evaluate(source, Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED)
}

/// The text of the value of the last line of `source`, as `phs eval` reports it.
fn value(source: &str) -> String {
    let last = source.lines().count();
    let report = run(source);
    match report.results.last() {
        Some(item) if item.line == last => item.value.text.clone(),
        _ => panic!("{source:?} gave no value: {:?}", report.diagnostics),
    }
}

/// The boolean `phs eval` reports for the last line of `source`.
fn truth(source: &str) -> bool {
    let last = source.lines().count();
    let report = run(source);
    match report.results.last() {
        Some(item) if item.line == last && item.value.kind == "bool" => item.value.text == "True",
        _ => panic!("{source:?} gave no boolean: {:?} {:?}", report.results.last(), report.diagnostics),
    }
}

/// The error the interpreter raises for `source`; `phs eval` reports no value for that line.
fn refused(source: &str) -> String {
    let last = source.lines().count();
    let report = run(source);
    assert!(
        !report.results.iter().any(|item| item.line == last),
        "{source:?} was answered: {:?}",
        report.results.last()
    );
    match eval_phs(source) {
        Err(e) => e.to_string(),
        Ok(values) => panic!("{source:?} was answered by the interpreter: {:?}", values.last()),
    }
}

#[test]
fn equality_is_relative_on_canonical_magnitudes() {
    for (source, expected) in [
        ("1 pm == 2 pm", false),
        ("1 nm == 2 nm", false),
        ("1 fm == 1000 fm", false),
        ("1 km == 1000 m", true),
        ("1000 m == 1 km", true),
        ("(0.1 m + 0.2 m) == 0.3 m", true),
        ("(0.1 + 0.2) == 0.3", true),
        ("(1000 * 0.1) == 100", true),
        ("0.01 degC == 273.16 K", true),
        ("273.16 K == 0.01 degC", true),
        ("20 degC == 68 degF", true),
        ("20 degC == 20 K", false),
        ("20 dB == 100", true),
        ("-0.0 == 0.0", true),
        ("0 m == 0 km", true),
        ("1 m > 0", true),
        ("1 pm != 2 pm", true),
        ("1 km != 1000 m", false),
        ("0.01 degC != 273.16 K", false),
    ] {
        assert_eq!(truth(source), expected, "{source}");
    }
}

#[test]
fn a_relative_rule_never_accepts_a_difference_against_an_exact_zero() {
    // documented consequence: these were true under the absolute 1e-9
    assert!(!truth("cos(90 deg) == 0"));
    assert!(!truth("(0.1 m + 0.2 m - 0.3 m) == 0 m"));
    // the unit-aware way to write it
    assert!(truth("abs(0.1 m + 0.2 m - 0.3 m) < 1e-9 m"));
    assert!(truth("abs(cos(90 deg)) < 1e-12"));
}

#[test]
fn non_finite_operands_compare_by_ieee() {
    assert!(truth("(1e308 m * 10) == (1e308 m * 10)"));
    assert!(!truth("(1e308 m * 10) == 1 m"), "the relative formula would make inf equal to 1 m");
    assert!(!truth("(1e308 * 10) == (-1e308 * 10)"));
    assert!(!truth("n = (1e308 * 10) - (1e308 * 10)\nn == n"));
    assert!(truth("n = (1e308 * 10) - (1e308 * 10)\nn != n"));
}

#[test]
fn different_dimensions_are_an_error_and_zero_is_not_a_temperature() {
    for source in ["5 m == 5 s", "5 m != 5 s", "5 m > 2 s", "1 m ≈ 1 s"] {
        assert!(refused(source).contains("mismatch"), "{source}");
    }
    // 0 read as 0 K made `-10 degC > 0` true; 0 dB is the ratio 1, not a zero
    refused("-10 degC > 0");
    refused("5 m > 0 dB");
}

#[test]
fn approx_is_relative_with_the_catalog_tolerance() {
    for (source, expected) in [
        ("1 mm ≈ 1.9 mm", false),
        ("1 km ≈ 1.0009 km", true),
        ("1 km ≈ 1000.9 m", true),
        ("1000.9 m ≈ 1 km", true),
        ("1 km ≈ 1.002 km", false),
        ("0 m ≈ 0 km", true),
        ("approx(1 km, 1.0009 km)", true),
        ("approx(1 mm, 1.9 mm, 0.5)", true),
        ("approx(1 mm, 1.9 mm, 0.4)", false),
        ("approx(1 mm, 1.9 mm, 50 %)", true),
        ("approx(1 mm, 1.9 mm, 1e-3)", false),
    ] {
        assert_eq!(truth(source), expected, "{source}");
    }
    assert!(refused("1 m ≈ 0 m").contains("zero"));
    assert!(refused("0 m ≈ 1 m").contains("zero"));
    for bad in ["0", "1", "-0.1", "2", "1 m"] {
        refused(&format!("approx(1 m, 1 m, {bad})"));
    }
    refused("approx(1 m)");
    refused("approx(1 m, \"a\")");
}

#[test]
fn a_sigma_bound_asks_whether_the_measured_value_lies_within_k_of_its_own_uncertainties() {
    for (source, expected) in [
        ("(10 +/- 0.05) mm == 10.04 mm +/- 1 sigma", true),
        ("(10 +/- 0.05) mm == 1.004 cm +/- 1 sigma", true),
        ("(10 +/- 0.01) m == 10.04 m +/- 1 sigma", false),
        ("(9.81 +/- 0.01) m/s^2 == 9.80 m/s^2 +/- 2 sigma", true),
        ("(9.81 +/- 0.004) m/s^2 == 9.80 m/s^2 +/- 2 sigma", false),
        ("10.04 mm +/- 1 sigma == (10 +/- 0.05) mm", true),
        ("(10 +/- 0.1) m != 20 m +/- 1 sigma", true),
        ("(10 +/- 0.1) m != 10 m +/- 1 sigma", false),
        ("g_exp = 9.81 +/- 0.05 m / s^2\ng_exp == 9.80 m / s^2 +/- 2 sigma", true),
    ] {
        assert_eq!(truth(source), expected, "{source}");
    }
    // the target is exact: `+/- 2 sigma` is k, not an uncertainty of the target
    assert_eq!(value("y = 10.04 mm +/- 2 sigma"), "10.04 mm ± 2.0σ");
    // an exact measured value has no sigma to count in (the 0.05 default mixed units)
    for source in ["10.0 mm == 10.04 mm +/- 1 sigma", "9.81 m/s^2 == 9.80 m/s^2 +/- 2 sigma", "10 m != 20 m +/- 1 sigma"] {
        assert!(refused(source).contains("uncertainty"), "{source}");
    }
    refused("(10 +/- 0.1) m == 10 s +/- 1 sigma");
    // a sigma bound is not something to order or approximate
    refused("1 m ≈ 20 m +/- 1 sigma");
    refused("1 m < 20 m +/- 1 sigma");
}

#[test]
fn operand_types_that_cannot_be_compared_are_a_type_error() {
    for source in [
        "vector(1, 2) == vector(1, 2)",
        "vector(1, 2) != vector(1, 3)",
        "1 m != \"a\"",
        "1 m == \"a\"",
        "\"a\" < \"b\"",
        "True == 1",
        "1 m < True",
    ] {
        let why = refused(source);
        assert!(why.contains("cannot compare"), "{source}: {why}");
    }
    assert!(truth("\"a\" == \"a\""));
    assert!(truth("\"a\" != \"b\""));
    assert!(truth("True == True"));
    assert!(truth("True != False"));
}

#[test]
fn a_condition_is_false_only_for_an_exact_zero() {
    for (source, expected) in [
        ("if 1e-16 m then 1 else 0", "1.0"),
        ("if 1e-16 km then 1 else 0", "1.0"),
        ("if 1e-16 then 1 else 0", "1.0"),
        ("if 0 m then 1 else 0", "0.0"),
        ("if 0 km then 1 else 0", "0.0"),
        ("if -0.0 m then 1 else 0", "0.0"),
        ("if 0 then 1 else 0", "0.0"),
        ("if -5 then 1 else 0", "1.0"),
        ("if 273.15 K then 1 else 0", "1.0"),
        ("if True then 1 else 0", "1.0"),
    ] {
        assert_eq!(value(source), expected, "{source}");
    }
    for source in ["if 0 degC then 1 else 0", "if 20 dB then 1 else 0", "n = (1e308 * 10) - (1e308 * 10)\nif n then 1 else 0"] {
        assert!(refused(source).contains("condition"), "{source}");
    }
    // stopped after 10 steps only because 1.39e-16 read as false; now it runs to the loop guard
    let report = run("x = 1\nwhile x {\n  x = x - 0.1\n}");
    let stopped = report.diagnostics.iter().any(|d| d.line == 2 && d.message.contains("did not converge"));
    assert!(stopped, "{:?}", report.diagnostics);
}

#[test]
fn assert_of_two_quantities_is_equality() {
    for source in ["assert(1 fm, 1000 fm)", "assert(1 pm, 2 pm)", "assert(1 m, 1.0000000001 m)", "assert(1 pm == 2 pm)"] {
        assert!(refused(source).contains("assert"), "{source}");
    }
    for source in ["assert(1 m, 1000 mm)", "assert(0.01 degC, 273.16 K)", "assert((0.1 m + 0.2 m), 0.3 m)", "assert(1 km == 1000 m)"] {
        assert!(eval_phs(source).is_ok(), "{source}");
    }
}

/// `a` in `unit` written so the parser reads back the same f64.
fn lit(a: f64, unit: &str) -> String {
    format!("{a:?} {unit}")
}

#[test]
fn the_answer_does_not_depend_on_the_units_the_operands_are_written_in() {
    let lengths = [("m", 1.0), ("mm", 1e-3), ("km", 1e3), ("µm", 1e-6)];
    for metres in [1e-15, 3.7e-9, 0.1, 1.0, 42.5, 1e6, 7.3e12] {
        for (u1, f1) in lengths {
            for (u2, f2) in lengths {
                let (a, b) = (metres / f1, metres / f2);
                let near = b * (1.0 + 1e-4);
                let far = b * (1.0 + 1e-9);
                for (x, y) in [(lit(a, u1), lit(b, u2)), (lit(b, u2), lit(a, u1))] {
                    assert!(truth(&format!("{x} == {y}")), "{x} == {y}");
                    assert!(!truth(&format!("{x} != {y}")), "{x} != {y}");
                    assert!(truth(&format!("{x} ≈ {y}")), "{x} ≈ {y}");
                }
                for (x, y) in [(lit(a, u1), lit(far, u2)), (lit(far, u2), lit(a, u1))] {
                    assert!(!truth(&format!("{x} == {y}")), "{x} == {y}");
                    assert!(truth(&format!("{x} != {y}")), "{x} != {y}");
                }
                for (x, y) in [(lit(a, u1), lit(near, u2)), (lit(near, u2), lit(a, u1))] {
                    assert!(truth(&format!("{x} ≈ {y}")), "{x} ≈ {y}");
                    assert!(!truth(&format!("{x} == {y}")), "{x} == {y}");
                }
            }
        }
    }
}

#[test]
fn temperatures_and_levels_compare_by_what_they_stand_for_in_either_order() {
    // not much nearer absolute zero than 1 K: there a written degC or degF value (hundreds of degrees) cannot carry
    // the kelvin value to 1e-12 of itself, and the operands really differ by more than the bound
    for kelvin in [1.0, 250.0, 273.15, 300.0, 1e4] {
        let celsius = kelvin - 273.15;
        let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
        let off = (kelvin * (1.0 + 1e-9)) - 273.15;
        let spellings = [lit(kelvin, "K"), lit(celsius, "degC"), lit(fahrenheit, "degF")];
        for x in &spellings {
            for y in &spellings {
                assert!(truth(&format!("{x} == {y}")), "{x} == {y}");
                assert!(truth(&format!("{x} ≈ {y}")), "{x} ≈ {y}");
            }
            let o = lit(off, "degC");
            assert!(!truth(&format!("{x} == {o}")), "{x} == {o}");
            assert!(!truth(&format!("{o} == {x}")), "{o} == {x}");
        }
    }
    for level in [-20.0, 0.0, 3.0, 20.0, 60.0] {
        let ratio: f64 = 10f64.powf(level / 10.0);
        let (l, r) = (lit(level, "dB"), format!("{ratio:?}"));
        assert!(truth(&format!("{l} == {r}")), "{l} == {r}");
        assert!(truth(&format!("{r} == {l}")), "{r} == {l}");
        let off = format!("{:?}", ratio * (1.0 + 1e-9));
        assert!(!truth(&format!("{l} == {off}")), "{l} == {off}");
    }
}
