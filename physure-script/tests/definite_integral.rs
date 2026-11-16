//! The definite integral of PHS, `integral("f(x)", "x", a, b)` (audit B20 and the review's Task 2a/2b), run as
//! programs: what its bounds may be, which names the integrand sees, the unit and the uncertainty of the answer,
//! and what it refuses.
//!
//! Every expected value is a closed form written here, never a copy of the engine's output. Sums were written in
//! parentheses wherever they meet a comparison while `op_compare` bound tighter than `+` (audit B26, since fixed:
//! see `tests/precedence.rs`); the parentheses are harmless and stay.

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
    assert_close(gaussian, 2.0 * GAUSSIAN_0_1, 1e-12, "k*exp(-x^2)");
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
    assert_close(in_unit(&numeric, "m").0, 3.0 * GAUSSIAN_0_1, 1e-12, "a decaying velocity");
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
    assert_close(mean, 3.0 * GAUSSIAN_0_1, 1e-12, "mean");
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
    assert_close(numeric.value.mean(), GAUSSIAN_0_1, 1e-12, "mean");
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
    assert!((value - 2.0 / 3.0).abs() < 1e-14, "{value}");
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

// ---- Task 2a-ii: the closed form is confirmed; one mapping for infinite ends ----

#[test]
fn a_closed_form_that_jumps_inside_the_interval_is_refused() {
    // before: -2.0, a negative answer for a positive integrand whose integral diverges
    // the midpoint 0 is a node of the quadrature, where x^-2 is not finite
    assert!(refused(r#"integral("x^(-2)", "x", -1, 1)"#).contains("not finite at x = 0"));
    // before: -0.1425 (tan(3) - tan(0), across the pole of sec^2 at pi/2)
    assert!(refused(r#"integral("sec(x)^2", "x", 0, 3)"#).starts_with("integral: "));
    assert!(refused(r#"integral("1/cos(x)^2", "x", 0, 3)"#).contains("did not converge"));
    // before: did not stop (the deadlock), and once unblocked -1.99 across the pole of tan at pi/2
    assert!(refused(r#"integral("tan(x)", "x", 0, 3)"#).starts_with("integral: "));
    // the symbolic engine reads `x < 0.5` as `x` and integrates it to x^2/2 (0.5 here); the integrand is a Bool
    assert!(refused(r#"integral("x < 0.5", "x", 0, 1)"#).contains("Bool"));
}

#[test]
fn a_confirmed_closed_form_keeps_its_exact_digits() {
    assert_eq!(number(r#"integral("x^2", "x", 0, 3)"#), 9.0);
    assert_eq!(number(r#"integral("x", "x", 0, 2)"#), 2.0);
    // reversed bounds change the sign
    assert_eq!(number(r#"integral("x^2", "x", 3, 0)"#), -9.0);
    // an empty interval is zero, with the unit of the integral
    assert_eq!(number(r#"integral("x^2", "x", 1, 1)"#), 0.0);
    assert_eq!(in_unit(&quantity(r#"integral("x", "x", 1 m, 1 m)"#), "m^2"), (0.0, 0.0));
}

#[test]
fn an_infinite_end_and_a_finite_non_zero_end() {
    let e_inv = (-1.0f64).exp();
    // before: 0.6065 = exp(-0.5), the mapping's inverse did not match the substitution
    assert_close(number(r#"integral("exp(-x)", "x", 1, "inf")"#), e_inv, 1e-12, "exp(-x) over [1, inf)");
    // before: 1.999998
    assert_close(number(r#"integral("1/x^2", "x", 1, "inf")"#), 1.0, 1e-12, "1/x^2 over [1, inf)");
    assert_close(number(r#"integral("1/x^2", "x", "-inf", -1)"#), 1.0, 1e-12, "1/x^2 over (-inf, -1]");
    assert_close(number(r#"integral("exp(x)", "x", "-inf", 2)"#), 2.0f64.exp(), 1e-12, "exp(x) over (-inf, 2]");
    assert_close(number(r#"integral("exp(-x)", "x", "inf", 1)"#), -e_inv, 1e-12, "reversed");
    let pi = std::f64::consts::PI;
    assert_close(number(r#"integral("exp(-x^2)", "x", "-inf", "inf")"#), pi.sqrt(), 1e-12, "the Gaussian integral");
    assert_close(number(r#"integral("1/(1+x^2)", "x", "-inf", "inf")"#), pi, 1e-12, "the Lorentzian");
    // with a unit: a decaying velocity over all later times
    let distance = quantity("v = 3 m/s\nintegral(\"v*exp(-t/(2 s))\", \"t\", 0 s, \"inf\")");
    assert_close(in_unit(&distance, "m").0, 6.0, 1e-12, "3 m/s decaying in 2 s");
}

#[test]
fn an_integral_that_does_not_converge_at_infinity_is_refused() {
    assert!(refused(r#"integral("1/x", "x", 1, "inf")"#).starts_with("integral: "));
    assert!(refused(r#"integral("sin(x)", "x", 0, "inf")"#).starts_with("integral: "));
}

// ---- Task 2b: tanh-sinh, closed forms written in the test ----

#[test]
fn integrable_singularities_and_smooth_integrands_reach_full_precision() {
    // 1/sqrt(x) has no antiderivative in the symbolic engine: numerical. before: 1.97508560641784
    assert_close(number(r#"integral("1/sqrt(x)", "x", 0, 1)"#), 2.0, 1e-12, "x^-1/2");
    // ln x: the antiderivative x ln x - x is not finite at 0, so numerical. before: -0.999476660223709
    assert_close(number(r#"integral("ln(x)", "x", 0, 1)"#), -1.0, 1e-12, "ln x");
    // x^-0.9: the closed form 10 x^0.1 (as 10.000000000000002 x^0.09999999999999998), confirmed numerically
    assert_close(number(r#"integral("x^(-0.9)", "x", 0, 1)"#), 10.0, 1e-12, "x^-0.9");
    // sqrt(pi)/2 erf(1): no antiderivative here
    assert_close(number(r#"integral("exp(-x^2)", "x", 0, 1)"#), GAUSSIAN_0_1, 1e-12, "exp(-x^2)");
    assert_close(number(r#"integral("sin(x)", "x", 0, pi)"#), 2.0, 1e-12, "sin");
    assert_close(number(r#"integral("sqrt(x)", "x", 0, 1)"#), 2.0 / 3.0, 1e-12, "sqrt");
}

#[test]
fn a_tiny_integral_is_judged_relative_to_its_own_size() {
    // 1e-18/3: an absolute floor of 1e-15 would have accepted any estimate, even level 1's
    assert_close(number(r#"integral("x^2", "x", 0, 1e-6)"#), 1e-18 / 3.0, 1e-12, "x^2 over [0, 1e-6]");
    // the same through the quadrature alone (no antiderivative for this spelling of x^2)
    assert_close(number(r#"integral("x^2*exp(0*sin(1/(x+1)))", "x", 0, 1e-6)"#), 1e-18 / 3.0, 1e-12, "numerically");
}

#[test]
fn an_oscillating_integral_smaller_than_its_l1_is_not_refused() {
    // (1 - cos 1000) / 1000; the L1 of the samples is about 0.64, the integral 4.4e-4
    let expected = (1.0 - 1000f64.cos()) / 1000.0;
    assert_close(number(r#"integral("sin(1000*x)", "x", 0, 1)"#), expected, 1e-9, "sin(1000 x)");
}

#[test]
fn what_the_quadrature_cannot_resolve_is_refused() {
    // a singularity at a non-zero end (stalls at 1.99999998, 7.7e-9 from 2)
    assert!(refused(r#"integral("1/sqrt(1-x)", "x", 0, 1)"#).contains("did not converge"));
    // the closed form 10 (x - 1)^0.1 is right, but the quadrature stalls at 9.74 next to x = 1 and cannot confirm it
    assert!(refused(r#"integral("(x-1)^(-0.9)", "x", 1, 2)"#).contains("cannot be confirmed"));
    // a jump at 0.3, a kink at 1/3, sin(1/x) next to 0
    assert!(refused(r#"integral("floor(x + 0.7)", "x", 0, 1)"#).contains("did not converge"));
    assert!(refused(r#"integral("abs(x - 1/3)", "x", 0, 1)"#).contains("did not converge"));
    assert!(refused(r#"integral("sin(1/x)", "x", 0, 1)"#).starts_with("integral: "));
}

#[test]
fn a_refusal_comes_well_within_the_sandbox_deadline() {
    // the cap is 20000 samples: a refusal must not be a time-out of the 5 s deadline `phs eval` uses
    let started = std::time::Instant::now();
    let report = evaluate(
        "use * from calc\nintegral(\"abs(x - 1/3)\", \"x\", 0, 1)",
        Limits::sandboxed(Duration::from_secs(5)),
        DEFAULT_SEED,
    );
    let message = &report.diagnostics.last().unwrap_or_else(|| panic!("{:?}", report.results)).message;
    assert!(message.contains("did not converge"), "{message}");
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}

#[test]
fn an_infinite_end_with_a_growing_factor_and_an_uncertain_integrand() {
    // x^2 exp(-x) over [0, inf) is 2! = 2; far out x^2 overflows while exp(-x) is 0, which the tail never reaches
    assert_close(number(r#"integral("x^2*exp(-x)", "x", 0, "inf")"#), 2.0, 1e-12, "x^2 exp(-x)");
    // k exp(-x) over [0, inf) is k: 2 +/- 0.1
    let q = quantity("k = 2 +/- 0.1\nintegral(\"k*exp(-x)\", \"x\", 0, \"inf\")");
    assert_close(q.value.mean(), 2.0, 1e-12, "mean");
    assert_close(q.value.std_dev(), 0.1, 1e-12, "uncertainty");
}
