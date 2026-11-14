//! The `limit` builtin of PHS, `limit("f(x)", "x", p)` (audit B19 and the review's Task 2c), run as programs: the
//! value at a point where the expression is continuous, the extrapolated limit at a removable singularity (from both
//! sides), the limit at an infinite point, the divergences it reports, and what it refuses.
//!
//! Every expected value is a closed form written here, never a copy of the engine's output. Where f64 cannot reach
//! 1e-12 the bound is the measured accuracy of the method, said at the assertion. Sums are parenthesised wherever
//! they meet a comparison (`op_compare` binds tighter than `+`, audit B26).

use physure_core::Quantity;
use physure_script::interpreter::eval_phs;
use physure_script::report::{evaluate, DEFAULT_SEED};
use physure_script::value::PhsValue;
use physure_script::Limits;
use std::time::Duration;

/// The value of the last statement of `program`, run after `use * from calc`.
fn calc(program: &str) -> PhsValue {
    let source = format!("use * from calc\n{program}");
    let values = eval_phs(&source).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    values.last().cloned().expect("a value")
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(&format!("use * from calc\n{program}")) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

/// A plain number: the answer when neither the point nor the expression carries a unit or an uncertainty.
fn number(program: &str) -> f64 {
    match calc(program) {
        PhsValue::Number(n) => n,
        other => panic!("{program:?} should be a plain number, it is {other:?}"),
    }
}

/// A quantity: the answer when a unit or an uncertainty is involved.
fn quantity(program: &str) -> Quantity {
    match calc(program) {
        PhsValue::Quantity(q) => q,
        other => panic!("{program:?} should be a quantity, it is {other:?}"),
    }
}

/// `q` expressed in `unit`: (mean, standard uncertainty).
fn in_unit(q: &Quantity, unit: &str) -> (f64, f64) {
    let converted = q.to(unit).unwrap_or_else(|e| panic!("{q} is not convertible to {unit}: {e}"));
    (converted.value.mean(), converted.value.std_dev())
}

fn assert_close(actual: f64, expected: f64, rel: f64, what: &str) {
    let error = (actual - expected).abs();
    assert!(error <= rel * expected.abs(), "{what}: {actual} vs {expected} (relative error {:e})", error / expected.abs());
}

fn limit(f: &str, p: &str) -> f64 {
    number(&format!("limit(\"{f}\", \"x\", {p})"))
}

fn limit_refused(f: &str, p: &str) -> String {
    refused(&format!("limit(\"{f}\", \"x\", {p})"))
}

// ---- (1) a point where the expression is continuous: its value there ----

#[test]
fn at_a_point_of_continuity_the_limit_is_the_value() {
    // before: 0.0 (a probe below 1e-5 was read as 0)
    assert_close(limit("0.000001*x", "1"), 1e-6, 1e-15, "0.000001*x at 1");
    // before: inf (a probe above 1e6 was read as infinity)
    assert_close(limit("2000000+x", "1"), 2_000_001.0, 1e-15, "2000000+x at 1");
    // before: inf (true value -1e9; probes at 1e-10..1e-12 never settle there, the value at 0 is exact)
    assert_close(limit("1/(x-1e-9)", "0"), -1e9, 1e-15, "1/(x - 1e-9) at 0");
    // before: 4.00000004 (the probe at 2 + 1e-8)
    assert_eq!(limit("x^2", "2"), 4.0);
    // a limit of exactly 0 is answered exactly
    assert_eq!(limit("x", "0"), 0.0);
    assert_eq!(calc("limit(\"x\", \"x\", 0)").to_string(), "0.0");
}

// ---- (2) a removable singularity: extrapolated from both sides ----

#[test]
fn a_removable_singularity_is_extrapolated_from_both_sides() {
    assert_close(limit("sin(x)/x", "0"), 1.0, 1e-12, "sin(x)/x at 0");
    assert_close(limit("(x^2-1)/(x-1)", "1"), 2.0, 1e-15, "(x^2 - 1)/(x - 1) at 1");
    assert_close(limit("(sqrt(1+x)-1)/x", "0"), 0.5, 1e-12, "(sqrt(1 + x) - 1)/x at 0");
    // (1 - cos x)/x^2 and (exp x - 1 - x)/x^2 cancel catastrophically next to 0 (each sample carries a rounding
    // error of about eps/h^2): measured 4.6e-11 and 2e-11 from 0.5 with extrapolation over h = 1/8 .. 1/16384, so
    // the bound is the method's acceptance tolerance, 1e-9, not 1e-12. Before: 0.0 and -0.6077.
    assert_close(limit("(1-cos(x))/x^2", "0"), 0.5, 1e-9, "(1 - cos x)/x^2 at 0");
    assert_close(limit("(exp(x)-1-x)/x^2", "0"), 0.5, 1e-9, "(exp x - 1 - x)/x^2 at 0");
}

#[test]
fn a_limit_of_zero_at_a_singularity_is_accepted() {
    // relative to the size of the sampled values (about 0.12 here), not to the limit itself, which is 0
    let l = limit("sin(x)^2/x", "0");
    assert!(l.abs() <= 1e-12, "{l}");
}

#[test]
fn a_discontinuous_builtin_is_read_from_both_sides() {
    // floor, abs, comparisons and ternaries are never trusted at the point itself
    assert_eq!(limit("floor(x)", "1.5"), 1.0);
    assert_eq!(limit("abs(x)", "0"), 0.0);
    assert_eq!(limit("x > 0 ? 1 : 1", "0"), 1.0);
}

#[test]
fn an_expression_defined_on_one_side_only_has_the_limit_within_its_domain() {
    // sqrt and ln refuse negative arguments, so only the right side exists: the limit within the domain
    assert_close(limit("sin(sqrt(x))/sqrt(x)", "0"), 1.0, 1e-12, "sin(sqrt x)/sqrt x at 0+");
    // x ln x is not a power series in x; its samples fall geometrically to 0, which is what the limit is
    assert_eq!(limit("x*ln(x)", "0"), 0.0);
}

// ---- (3) infinite points ----

#[test]
fn an_infinite_point_is_approached_along_powers_of_two() {
    // x = 8, 16, 32, ... : extrapolated in 1/x
    assert_eq!(limit("1/x", "\"inf\""), 0.0);
    assert_eq!(calc("limit(\"1/x\", \"x\", \"inf\")").to_string(), "0.0");
    assert_close(limit("(1+1/x)^x", "\"inf\""), std::f64::consts::E, 1e-12, "(1 + 1/x)^x at inf");
    assert_close(limit("x/(x+100)", "\"inf\""), 1.0, 1e-12, "x/(x + 100) at inf");
    assert_close(limit("atan(x)", "\"inf\""), std::f64::consts::FRAC_PI_2, 1e-12, "atan(x) at inf");
    assert_close(limit("sqrt(x^2+x)-x", "\"inf\""), 0.5, 1e-12, "sqrt(x^2 + x) - x at inf");
    let decay = limit("exp(-x)", "\"inf\"");
    assert!(decay.abs() <= 1e-20, "{decay}");
    assert_eq!(limit("1/sqrt(x)", "\"inf\""), 0.0);
    assert_close(limit("1/x", "\"-inf\"").abs() + 1.0, 1.0, 0.0, "1/x at -inf");
}

#[test]
fn a_monotone_divergence_is_a_signed_infinity() {
    assert_eq!(limit("exp(x)", "\"inf\""), f64::INFINITY);
    assert_eq!(calc("limit(\"exp(x)\", \"x\", \"inf\")").to_string(), "inf");
    // before: inf, the probe went to +1e7 whatever the sign of the point
    assert_eq!(limit("exp(-x)", "\"-inf\""), f64::INFINITY);
    assert_eq!(limit("ln(x)", "\"inf\""), f64::INFINITY);
    // both sides agree
    assert_eq!(limit("1/x^2", "0"), f64::INFINITY);
    assert_eq!(limit("ln(abs(x))", "0"), f64::NEG_INFINITY);
}

// ---- (4) what it refuses ----

#[test]
fn sides_that_disagree_are_refused() {
    // before: 1.0 (the right side only)
    let message = limit_refused("abs(x)/x", "0");
    assert!(message.contains("left") && message.contains("right"), "{message}");
    // before: 1.0
    assert!(limit_refused("floor(x)", "1").contains("left"));
    // before: inf; -inf from the left, +inf from the right
    assert!(limit_refused("1/x", "0").contains("left"));
}

#[test]
fn an_undefined_neighbourhood_is_refused() {
    // before: 0.0 (an evaluation error read as 0)
    let message = limit_refused("sqrt(x)", "-1");
    assert!(message.contains("undefined") || message.contains("cannot be evaluated"), "{message}");
}

#[test]
fn a_sequence_that_does_not_settle_is_refused() {
    let message = limit_refused("sin(1/x)", "0");
    assert!(message.contains("did not settle"), "{message}");
    // before: 0.4205 (sin(1e7))
    assert!(limit_refused("sin(x)", "\"inf\"").contains("did not settle"));
}

#[test]
fn a_value_that_is_not_a_number_is_refused() {
    let message = refused("limit(\"vector(x, x)\", \"x\", 1)");
    assert!(message.contains("number or a quantity"), "{message}");
    assert!(refused("limit(\"x\", \"x\", True)").contains("point"));
}

// ---- names, scope, units, uncertainty ----

#[test]
fn the_expression_sees_the_scope_and_unknown_names_are_refused() {
    // before: 0.0 (an unbound name evaluated to a string and the probe failed)
    let message = limit_refused("q*x", "1");
    assert!(message.contains("'q'") && message.starts_with("limit"), "{message}");
    assert_close(number("v = 3\nlimit(\"v*sin(x)/x\", \"x\", 0)"), 3.0, 1e-12, "v sin(x)/x with v = 3");
    assert_close(number("f(k) = limit(\"k*sin(x)/x\", \"x\", 0)\nf(5)"), 5.0, 1e-12, "k sin(x)/x with k = 5");
    let report = evaluate(
        "use * from calc\nv = 3\nlimit(\"v*x\", \"x\", 2)",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let item = report.results.last().unwrap_or_else(|| panic!("no value: {:?}", report.diagnostics));
    assert_eq!(item.value.text, "6.0");
}

#[test]
fn a_point_may_be_a_named_constant() {
    // before: 1.0 (`pi` arrived as the string "pi" and was read as 0)
    let at_pi = limit("sin(x)/x", "pi");
    assert!(at_pi.abs() <= 1e-15, "{at_pi}");
    assert_close(limit("sin(x)", "\"pi/2\""), 1.0, 1e-15, "sin at pi/2");
    assert!(limit_refused("x", "\"a\"").contains("'a'"));
}

#[test]
fn a_quantity_point_makes_the_variable_a_quantity_and_the_unit_is_kept() {
    // before: 2.00000001, with the unit dropped
    let q = quantity("limit(\"x\", \"x\", 2 m)");
    assert_eq!(in_unit(&q, "m"), (2.0, 0.0));
    // extrapolated with the variable in metres: the value is a length
    let l = quantity("v = 3 m\nlimit(\"v*sin(x/(1 m))/(x/(1 m))\", \"x\", 0 m)");
    assert_close(in_unit(&l, "m").0, 3.0, 1e-12, "3 m sin(x)/x");
    // a temperature point is refused, as a bound of `integral` is
    assert!(refused("limit(\"x\", \"x\", 20 degC)").contains("temperature"));
}

#[test]
fn uncertainty_is_propagated_not_dropped() {
    // at a point of continuity: the value there, with the core's propagation
    let direct = quantity("v = 3 +/- 0.1\nlimit(\"v*x\", \"x\", 2)");
    let (mean, u) = in_unit(&direct, "");
    assert_close(mean, 6.0, 1e-15, "v x at 2");
    assert_close(u, 0.2, 1e-12, "u(v x) at 2");
    // extrapolated: the weighted sum of the samples keeps the lineage of v
    let extrapolated = quantity("v = 3 +/- 0.1\nlimit(\"v*sin(x)/x\", \"x\", 0)");
    let (mean, u) = in_unit(&extrapolated, "");
    assert_close(mean, 3.0, 1e-12, "v sin(x)/x");
    assert_close(u, 0.1, 1e-9, "u(v sin(x)/x)");
    let correlated = quantity("v = 3 +/- 0.1\nlimit(\"v*sin(x)/x\", \"x\", 0) - v");
    assert!(in_unit(&correlated, "").1 <= 1e-9, "{correlated}");
    // an uncertain point where the expression is continuous propagates through it
    let at_uncertain = quantity("limit(\"sin(x)/x\", \"x\", 1 +/- 0.1)");
    let (mean, u) = in_unit(&at_uncertain, "");
    assert_close(mean, 1f64.sin(), 1e-15, "sin(1)/1");
    assert_close(u, (1f64.cos() - 1f64.sin()).abs() * 0.1, 1e-9, "u(sin(x)/x) at 1 +/- 0.1");
    // an uncertain point at a singularity has no derivative of the limit to propagate with
    assert!(refused("limit(\"(x^2-1)/(x-1)\", \"x\", 1 +/- 0.1)").contains("uncertain"));
}

#[test]
fn an_expression_that_calls_a_calc_builtin_is_answered_not_deadlocked() {
    let report = evaluate(
        "use * from calc\nlimit(\"simplify(x)\", \"x\", 1)",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let message = &report.diagnostics.last().unwrap_or_else(|| panic!("{:?}", report.results)).message;
    assert!(message.contains("simplify") && !message.contains("time limit"), "{message}");
}
