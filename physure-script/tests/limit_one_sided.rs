//! One-sided limits, `limit(expr, var, point, side)` with `side` `"+"` (from the right) or `"-"` (from the left),
//! positional or as the keyword `side=`; the strict two-sided limit (both sides must be defined); and the keyword
//! arguments of the calc builtins, which are refused when unknown instead of being dropped.
//!
//! Every program runs through `report::evaluate`, the entry `phs eval` uses. Every expected value is a closed form
//! written here, never a copy of the engine's output.

use physure_script::report::{evaluate, Report, DEFAULT_SEED};
use physure_script::Limits;
use std::time::Duration;

fn run(program: &str) -> Report {
    evaluate(&format!("use * from calc\n{program}"), Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED)
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    let report = run(program);
    match report.diagnostics.iter().find(|d| d.severity == "error") {
        Some(d) => d.message.clone(),
        None => panic!("{program:?} should be refused, it gave {:?}", report.results.last().map(|r| &r.value.text)),
    }
}

/// The last value of `program`: (text, value, uncertainty, unit).
fn answered(program: &str) -> (String, Option<f64>, Option<f64>, Option<String>) {
    let report = run(program);
    assert!(report.ok, "{program:?} failed: {:?}", report.diagnostics);
    let item = report.results.last().unwrap_or_else(|| panic!("{program:?} gave no value"));
    (item.value.text.clone(), item.value.value, item.value.uncertainty, item.value.unit.clone())
}

// ---- keyword arguments of the calc builtins ----

#[test]
fn an_unknown_keyword_argument_of_a_calc_builtin_is_refused() {
    // before: integral(..., foo=2) was 0.5 and series(..., foo=2) the series: the keyword was dropped
    for program in [
        "integral(\"x\", \"x\", 0, 1, foo=2)",
        "integral(\"x\", \"x\", foo: 2)",
        "series(\"sin(x)\", \"x\", foo=2)",
        "limit(\"sin(x)/x\", \"x\", 0, foo=2)",
        "deriv(\"x^2\", \"x\", order=2)",
        "simplify(\"x + x\", foo=1)",
    ] {
        let message = refused(program);
        assert!(message.contains("keyword") && message.contains('`'), "{program}: {message}");
    }
    assert!(refused("integral(\"x\", \"x\", 0, 1, foo=2)").contains("`foo`"));
    assert!(refused("integral(\"x\", \"x\", 0, 1, foo=2, foo=3)").contains("`foo`"));
    // without keywords the builtins answer as before
    assert_eq!(answered("integral(\"x\", \"x\", 0, 1)").1, Some(0.5));
}

/// The plain number `program` gives.
fn number(program: &str) -> f64 {
    let (text, value, uncertainty, unit) = answered(program);
    assert!(unit.is_none() && uncertainty.is_none_or(|u| u == 0.0), "{program:?} should be a plain number, it is {text}");
    match text.as_str() {
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        _ => value.unwrap_or_else(|| panic!("{program:?} gave {text}")),
    }
}

fn limit(f: &str, p: &str, side: &str) -> f64 {
    number(&format!("limit(\"{f}\", \"x\", {p}, \"{side}\")"))
}

fn limit_refused(f: &str, p: &str, side: Option<&str>) -> String {
    match side {
        Some(side) => refused(&format!("limit(\"{f}\", \"x\", {p}, \"{side}\")")),
        None => refused(&format!("limit(\"{f}\", \"x\", {p})")),
    }
}

fn assert_close(actual: f64, expected: f64, rel: f64, what: &str) {
    let error = (actual - expected).abs();
    assert!(error <= rel * expected.abs(), "{what}: {actual} vs {expected} (relative error {:e})", error / expected.abs());
}

// ---- one-sided limits ----

#[test]
fn a_pole_has_a_signed_infinity_on_each_side() {
    // before: "limit expects (expression_string, variable_string, target_point)"
    assert_eq!(limit("1/x", "0", "+"), f64::INFINITY);
    assert_eq!(limit("1/x", "0", "-"), f64::NEG_INFINITY);
    // the two-sided limit is still refused: the sides disagree
    let message = limit_refused("1/x", "0", None);
    assert!(message.contains("from the left") && message.contains("from the right"), "{message}");
}

#[test]
fn a_jump_has_a_value_on_each_side() {
    assert_eq!(limit("abs(x)/x", "0", "+"), 1.0);
    assert_eq!(limit("abs(x)/x", "0", "-"), -1.0);
    assert_eq!(limit("floor(x)", "1", "+"), 1.0);
    assert_eq!(limit("floor(x)", "1", "-"), 0.0);
    let message = limit_refused("floor(x)", "1", None);
    assert!(message.contains("from the left") && message.contains("from the right"), "{message}");
}

#[test]
fn the_keyword_side_equals_the_positional_one() {
    // before: limit("1/x","x",0,side="+") dropped the keyword and refused the two-sided limit
    assert_eq!(number("limit(\"1/x\", \"x\", 0, side=\"+\")"), f64::INFINITY);
    assert_eq!(number("limit(\"abs(x)/x\", \"x\", 0, side=\"-\")"), -1.0);
    assert_eq!(number("limit(\"abs(x)/x\", \"x\", 0, side: \"+\")"), 1.0);
    let message = refused("limit(\"abs(x)/x\", \"x\", 0, \"+\", side=\"+\")");
    assert!(message.contains("side") && message.contains("twice"), "{message}");
}

#[test]
fn a_side_is_plus_or_minus_and_an_infinite_point_takes_none() {
    for side in ["\"left\"", "\"\"", "\"+-\"", "1", "True"] {
        let message = refused(&format!("limit(\"x\", \"x\", 0, {side})"));
        assert!(message.contains("\"+\"") && message.contains("\"-\""), "{side}: {message}");
    }
    for (point, side) in [("\"inf\"", "\"-\""), ("\"-inf\"", "\"+\""), ("\"inf\"", "\"+\"")] {
        let message = refused(&format!("limit(\"1/x\", \"x\", {point}, {side})"));
        assert!(message.contains("side") && message.contains("one side"), "{point} {side}: {message}");
    }
    assert!(refused("limit(\"1/x\", \"x\", \"inf\", side=\"-\")").contains("one side"));
}

// ---- the domain: a two-sided limit needs both sides ----

#[test]
fn a_limit_at_the_edge_of_the_domain_is_taken_from_inside() {
    // before (two-sided): 0.0 for x ln x, sqrt(x), x^0.5 and sqrt(1 - x^2), and 1 for sin(sqrt x)/sqrt x, each read
    // from the one side where the expression is defined
    assert_eq!(limit("x*ln(x)", "0", "+"), 0.0);
    assert_eq!(limit("sqrt(x)", "0", "+"), 0.0);
    assert_eq!(limit("x^0.5", "0", "+"), 0.0);
    assert_eq!(limit("sqrt(1-x^2)", "1", "-"), 0.0);
    assert_close(limit("sin(sqrt(x))/sqrt(x)", "0", "+"), 1.0, 1e-12, "sin(sqrt x)/sqrt x at 0+");
    // the point itself is outside the domain: the limit is read from the samples
    assert_eq!(limit("ln(x)", "0", "+"), f64::NEG_INFINITY);
    assert_close(limit("(x^2-1)/(x-1)", "1", "+"), 2.0, 1e-12, "(x^2 - 1)/(x - 1) at 1+");
    assert_close(limit("(x^2-1)/(x-1)", "1", "-"), 2.0, 1e-12, "(x^2 - 1)/(x - 1) at 1-");
}

#[test]
fn a_two_sided_limit_needs_both_sides_defined() {
    for (f, p, missing, use_side) in [
        ("x*ln(x)", "0", "x < 0.0", "\"+\""),
        ("sqrt(x)", "0", "x < 0.0", "\"+\""),
        ("x^0.5", "0", "x < 0.0", "\"+\""),
        ("sin(sqrt(x))/sqrt(x)", "0", "x < 0.0", "\"+\""),
        ("sqrt(1-x^2)", "1", "x > 1.0", "\"-\""),
    ] {
        let message = limit_refused(f, p, None);
        assert!(
            message.contains("undefined") && message.contains(missing) && message.contains(&format!("side {use_side}")),
            "{f} at {p}: {message}"
        );
    }
    let message = limit_refused("sqrt(x)", "-1", None);
    assert!(message.contains("undefined on both sides"), "{message}");
}

#[test]
fn a_one_sided_limit_where_the_side_is_undefined_is_refused() {
    // sqrt is defined at 0 but not to its left: the value at the point is not the limit from the left
    let message = limit_refused("sqrt(x)", "0", Some("-"));
    assert!(message.contains("undefined") && message.contains("from the left"), "{message}");
    let message = limit_refused("sqrt(1-x^2)", "1", Some("+"));
    assert!(message.contains("undefined") && message.contains("from the right"), "{message}");
    let message = limit_refused("sqrt(x)", "-1", Some("+"));
    assert!(message.contains("undefined") && message.contains("from the right"), "{message}");
    let message = limit_refused("x*ln(x)", "0", Some("-"));
    assert!(message.contains("from the left"), "{message}");
}

#[test]
fn a_one_sided_limit_of_a_continuous_expression_uses_the_value_at_the_point() {
    // the samples of the right side start at 1/8, far past the pole at 1e-9; the value at 0 is the limit
    assert_close(limit("1/(x-1e-9)", "0", "+"), -1e9, 1e-12, "1/(x - 1e-9) at 0+");
    assert_close(limit("1/(x-1e-9)", "0", "-"), -1e9, 1e-12, "1/(x - 1e-9) at 0-");
    assert_eq!(limit("x^2", "2", "+"), 4.0);
    assert_eq!(answered("limit(\"x\", \"x\", 0, \"-\")").0, "0.0");
}

#[test]
fn a_domain_edge_near_the_point_moves_the_first_step_closer() {
    // before: "undefined farther from the point than x = -0.0625" (the first step was always 1/8)
    let ln10 = 10f64.ln();
    assert_close(number("limit(\"abs(ln(x+0.1))\", \"x\", 0)"), ln10, 1e-9, "|ln(x + 0.1)| at 0");
    assert_close(limit("abs(ln(x+0.1))", "0", "-"), ln10, 1e-9, "|ln(x + 0.1)| at 0-");
    // an isolated undefined sample at the first step (x = 1/8 is a pole) is stepped over
    assert_close(limit("sin(x)/x + 0*x/(x-0.125)", "0", "+"), 1.0, 1e-12, "sin(x)/x at 0+ next to a pole at 1/8");
    // undefined closer to the point than where it was defined is still refused
    let message = limit_refused("sqrt(x-0.01)", "0", Some("+"));
    assert!(message.contains("undefined"), "{message}");
}

// ---- units and uncertainty ----

#[test]
fn a_one_sided_limit_keeps_units_and_uncertainty() {
    let (text, value, _, unit) = answered("limit(\"x\", \"x\", 2 m, \"+\")");
    assert_eq!((value, unit.as_deref()), (Some(2.0), Some("m")), "{text}");
    let (text, value, _, unit) = answered("v = 3 m\nlimit(\"v*sin(x/(1 m))/(x/(1 m))\", \"x\", 0 m, \"-\")");
    assert_eq!(unit.as_deref(), Some("m"), "{text}");
    assert_close(value.unwrap(), 3.0, 1e-12, "3 m sin(x)/x from the left");
    // an uncertain point where the expression is continuous propagates through it, from either side
    for side in ["+", "-"] {
        let (text, value, u, _) = answered(&format!("limit(\"sin(x)/x\", \"x\", 1 +/- 0.1, \"{side}\")"));
        assert_close(value.unwrap(), 1f64.sin(), 1e-15, &text);
        assert_close(u.unwrap(), (1f64.cos() - 1f64.sin()).abs() * 0.1, 1e-9, &text);
    }
    // extrapolated: the weighted samples keep the lineage of an uncertain variable
    let (text, value, u, _) = answered("v = 3 +/- 0.1\nlimit(\"v*sin(x)/x\", \"x\", 0, \"+\")");
    assert_close(value.unwrap(), 3.0, 1e-12, &text);
    assert_close(u.unwrap(), 0.1, 1e-9, &text);
    // an uncertain point at a singularity is still refused
    assert!(refused("limit(\"(x^2-1)/(x-1)\", \"x\", 1 +/- 0.1, \"+\")").contains("uncertain"));
}

// ---- the continuous class (Task 5d) ----

#[test]
fn abs_is_continuous_so_its_value_at_the_point_is_the_limit() {
    // before: inf (the samples from 1/8 down never come within 1e-9 of the poles at +-1e-9; the true limit is -1e9)
    assert_close(number("limit(\"1/(abs(x)-1e-9)\", \"x\", 0)"), -1e9, 1e-12, "1/(|x| - 1e-9) at 0");
    assert_close(limit("1/(abs(x)-1e-9)", "0", "+"), -1e9, 1e-12, "1/(|x| - 1e-9) at 0+");
    assert_eq!(number("limit(\"abs(x)\", \"x\", 0)"), 0.0);
    assert_eq!(number("limit(\"abs(x-1)\", \"x\", 1)"), 0.0);
    // abs(x)/x is not defined at 0: the samples decide, and the sides still disagree
    let message = limit_refused("abs(x)/x", "0", None);
    assert!(message.contains("from the left") && message.contains("from the right"), "{message}");
}

#[test]
fn the_step_functions_stay_outside_the_continuous_class() {
    // their value at a jump is not the limit: floor(1) is 1, its limit from the left 0
    for (f, p) in [("floor(x)", "1"), ("ceil(x)", "1"), ("round(x)", "0.5"), ("x < 1 ? 0 : 1", "1"), ("if x < 1 then 0 else 1", "1")] {
        let message = limit_refused(f, p, None);
        assert!(message.contains("does not exist"), "{f} at {p}: {message}");
    }
    assert_eq!(limit("floor(x)", "1", "-"), 0.0);
    assert_eq!(limit("ceil(x)", "1", "+"), 2.0);
}

#[test]
fn a_point_isolated_from_the_rest_of_the_domain_is_not_a_limit() {
    // sqrt(x^2 (x^2 - 1e-4)) is 0 at 0 and defined for |x| >= 0.01 only: nothing approaches 0 within the domain.
    // before: 0.0, the value at the point, because the first steps (1/8) were defined
    for side in [None, Some("+"), Some("-")] {
        let message = limit_refused("sqrt(x^2*(x^2-1e-4))", "0", side);
        assert!(message.contains("undefined"), "{side:?}: {message}");
    }
    assert_close(number("limit(\"sqrt(x^2*(x^2-1e-4))\", \"x\", 1)"), (1.0f64 - 1e-4).sqrt(), 1e-12, "at 1");
}
