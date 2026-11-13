//! The definite integral of PHS, `integral("f(x)", "x", a, b)` (audit B20 and the review's Task 2a/2b), run as
//! programs: what its bounds may be, which names the integrand sees, the unit and the uncertainty of the answer,
//! and what it refuses.
//!
//! Every expected value is a closed form written here, never a copy of the engine's output. Sums are parenthesised
//! wherever they meet a comparison (`op_compare` binds tighter than `+`, audit B26).

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

/// A plain number: the answer when neither the bounds nor the integrand carry a unit or an uncertainty.
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

/// The integral of `exp(-x^2)` over [0, 1], `sqrt(pi)/2 * erf(1)`.
const GAUSSIAN_0_1: f64 = 0.746_824_132_812_427;

// ---- Task 2a-i: bounds, names, units, uncertainty ----

#[test]
fn a_bound_may_be_a_named_constant_and_pi_is_one() {
    // before: `pi` arrived as the string "pi" and was read as 0, so this was 0.0
    assert_close(number(r#"integral("sin(x)", "x", 0, pi)"#), 2.0, 1e-12, "sin over [0, pi]");
    // a bound written as text is an expression over the same names
    assert_close(number(r#"integral("cos(x)", "x", 0, "pi/2")"#), 1.0, 1e-12, "cos over [0, pi/2]");
    // a constant of the catalog is a bound with its unit: the variable is then a length
    let bohr = quantity(r#"integral("1", "x", 0, "bohr_radius")"#);
    assert_close(in_unit(&bohr, "m").0, 5.291_772_105_44e-11, 1e-12, "integral of 1 over [0, a0]");
}

#[test]
fn a_bound_that_names_nothing_is_refused() {
    // before: an unknown string was read as 0, so this was 0.5
    let message = refused(r#"integral("x", "x", "a", 1)"#);
    assert!(message.contains("'a'"), "{message}");
    assert!(refused(r#"integral("x", "x", 0, true)"#).contains("bound"));
}

#[test]
fn the_integrand_sees_the_variables_of_the_program() {
    // before: a variable of the program was invisible to the integrand and the answer was 0.0 (true value 6)
    assert_close(number("v = 3\nintegral(\"v*x\", \"x\", 0, 2)"), 6.0, 1e-12, "v*x over [0, 2]");
    let metres = quantity("v = 3 m\nintegral(\"v*x\", \"x\", 0, 2)");
    assert_close(in_unit(&metres, "m").0, 6.0, 1e-12, "v*x with v in m");
    // the numerical path sees them too
    let gaussian = number("k = 2\nintegral(\"k*exp(-x^2)\", \"x\", 0, 1)");
    assert_close(gaussian, 2.0 * GAUSSIAN_0_1, 1e-9, "k*exp(-x^2)");
}

#[test]
fn a_name_the_program_does_not_define_is_refused() {
    let message = refused(r#"integral("y*x", "x", 0, 1)"#);
    assert!(message.contains("'y'"), "{message}");
    let numeric = refused(r#"integral("y*exp(-x^2)", "x", 0, 1)"#);
    assert!(numeric.contains("'y'"), "{numeric}");
}

#[test]
fn quantity_bounds_make_the_variable_a_quantity() {
    // before: the units of the bounds were dropped and this was the number 2.0
    let area = quantity(r#"integral("x", "x", 0 m, 2 m)"#);
    assert_close(in_unit(&area, "m^2").0, 2.0, 1e-12, "x over [0 m, 2 m]");
    // a plain zero is the same point in every unit
    let area = quantity(r#"integral("x", "x", 0, 2 m)"#);
    assert_close(in_unit(&area, "m^2").0, 2.0, 1e-12, "x over [0, 2 m]");
    // two bounds in different units of one dimension: the result is exact in either
    let mixed = quantity(r#"integral("x", "x", 0 cm, 2 m)"#);
    assert_close(in_unit(&mixed, "m^2").0, 2.0, 1e-12, "x over [0 cm, 2 m]");
}

#[test]
fn the_unit_of_the_answer_is_the_integrands_times_the_variables() {
    // a velocity integrated over time is a length
    let distance = quantity("v = 3 m/s\nintegral(\"v\", \"t\", 0 s, 2 s)");
    assert_close(in_unit(&distance, "m").0, 6.0, 1e-12, "3 m/s over 2 s");
    // over hours the unit algebra converts exactly: 3 m/s * 1 h = 10800 m
    let hour = quantity("v = 3 m/s\nintegral(\"v\", \"t\", 0 h, 1 h)");
    assert_close(in_unit(&hour, "m").0, 10_800.0, 1e-12, "3 m/s over 1 h");
    // and the numerical path keeps it too
    let numeric = quantity("v = 3 m/s\nintegral(\"v*exp(-(t/(1 s))^2)\", \"t\", 0 s, 1 s)");
    assert_close(in_unit(&numeric, "m").0, 3.0 * GAUSSIAN_0_1, 1e-9, "a decaying velocity");
}

#[test]
fn bounds_of_different_dimensions_or_on_a_point_scale_are_refused() {
    assert!(refused(r#"integral("x", "x", 1, 2 m)"#).contains("dimension"));
    assert!(refused(r#"integral("x", "x", 1 s, 2 m)"#).contains("dimension"));
    assert!(refused(r#"integral("1", "x", 0 degC, 10 degC)"#).contains("temperature"));
}

#[test]
fn an_uncertain_integrand_propagates_its_uncertainty() {
    // closed form: v * 2 s, so u = 0.1 m/s * 2 s = 0.2 m
    let symbolic = quantity("v = (3 +/- 0.1) m/s\nintegral(\"v\", \"t\", 0 s, 2 s)");
    let (mean, u) = in_unit(&symbolic, "m");
    assert_close(mean, 6.0, 1e-12, "mean");
    assert_close(u, 0.2, 1e-9, "uncertainty");
    // numerical: v * GAUSSIAN_0_1 s
    let numeric = quantity("v = (3 +/- 0.1) m/s\nintegral(\"v*exp(-(t/(1 s))^2)\", \"t\", 0 s, 1 s)");
    let (mean, u) = in_unit(&numeric, "m");
    assert_close(mean, 3.0 * GAUSSIAN_0_1, 1e-9, "mean");
    assert_close(u, 0.1 * GAUSSIAN_0_1, 1e-9, "uncertainty");
    // and the answer stays correlated with v: subtracting v times the same factor leaves no uncertainty
    let residual = quantity(
        "v = (3 +/- 0.1) m/s\nd = integral(\"v*exp(-(t/(1 s))^2)\", \"t\", 0 s, 1 s)\nd - v * 0.746824132812427 s",
    );
    assert!(in_unit(&residual, "m").1 < 1e-9, "{residual}");
}

#[test]
fn an_uncertain_bound_propagates_its_uncertainty() {
    // d/db of the integral is f(b): u = f(1) * 0.01 = exp(-1) * 0.01
    let numeric = quantity(r#"integral("exp(-x^2)", "x", 0, 1 +/- 0.01)"#);
    assert_close(numeric.value.mean(), GAUSSIAN_0_1, 1e-9, "mean");
    assert_close(numeric.value.std_dev(), (-1.0f64).exp() * 0.01, 1e-9, "uncertainty");
    // closed form: v * T, u = 3 m/s * 0.1 s
    let symbolic = quantity("v = 3 m/s\nT = (2 +/- 0.1) s\nintegral(\"v\", \"t\", 0 s, T)");
    let (mean, u) = in_unit(&symbolic, "m");
    assert_close(mean, 6.0, 1e-12, "mean");
    assert_close(u, 0.3, 1e-9, "uncertainty");
}

#[test]
fn an_integrand_that_cannot_be_evaluated_inside_the_interval_is_refused() {
    // before: an evaluation error counted as 0.0
    assert!(refused(r#"integral("sqrt(x - 0.5)", "x", 0, 1)"#).contains("x = "));
    assert!(refused(r#"integral("ln(x - 0.5)", "x", 0, 1)"#).contains("x = "));
}

#[test]
fn an_antiderivative_left_as_an_integral_does_not_hang() {
    // `integral("sqrt(x)", "x")` is `integral(sqrt(x), x)`; evaluating that text called a calc builtin from inside
    // one, which waited forever on the lock of the unlocked builtins (the probe timed out; tan(x) did too)
    let report = evaluate(
        "use * from calc\nintegral(\"sqrt(x)\", \"x\", 0, 1)",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let item = report.results.last().unwrap_or_else(|| panic!("no value: {:?}", report.diagnostics));
    let value: f64 = item.value.text.parse().unwrap_or_else(|_| panic!("{:?}", item.value.text));
    assert!((value - 2.0 / 3.0).abs() < 1e-4, "{value}");
}

#[test]
fn an_integrand_that_calls_a_calc_builtin_is_answered_not_deadlocked() {
    // the same lock: a calc builtin evaluated from inside `integral` waited for the one `integral` held
    let report = evaluate(
        "use * from calc\nintegral(\"simplify(x)\", \"x\", 0, 1)",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let message = &report.diagnostics.last().unwrap_or_else(|| panic!("{:?}", report.results)).message;
    assert!(message.contains("simplify") && !message.contains("time limit"), "{message}");
}

#[test]
fn the_integrand_sees_the_scope_of_the_call_in_phs_eval_and_in_a_function() {
    // `phs eval` keeps the program's variables in the statement's scope, not in `interpreter.env`
    let report = evaluate(
        "use * from calc\nv = 3\nintegral(\"v*x\", \"x\", 0, 2)",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let item = report.results.last().unwrap_or_else(|| panic!("no value: {:?}", report.diagnostics));
    assert_eq!(item.value.text, "6.0");
    // a parameter of a function is a variable of the integrand
    assert_close(number("f(k) = integral(\"k*x\", \"x\", 0, 2)\nf(5)"), 10.0, 1e-12, "k*x with k = 5");
}
