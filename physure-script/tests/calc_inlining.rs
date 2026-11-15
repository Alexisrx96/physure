//! A user function called inside the expression string of a calc builtin (`deriv("f(t)", "t")`,
//! `integral("f(x)", "x", 0, 1)`, `limit`, `solve`, `simplify`, `series`, ...) is inlined: its body, with each
//! argument put in place of its parameter.
//!
//! Before this file the inliner pasted the body without the arguments: with `f(x) = 2*x`, `deriv("f(t)", "t")` was
//! `0` (the body `2*x` is constant in `t`), `f(y)` became `2*x`, a second call of the same function was refused, the
//! body lost its parentheses (`(x + 1)*2` became `x + 1 * 2`), and a call of `sqf` matched inside `sqrt(`.
//!
//! Every expected value is a closed form written here, never a copy of the engine's output.

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

/// The number a value is (a plain number or a dimensionless quantity).
fn as_number(value: &PhsValue, what: &str) -> f64 {
    match value {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) if q.unit.is_plain_number() => q.value.mean(),
        other => panic!("{what} should be a number, it is {other:?}"),
    }
}

/// The symbolic answer of `program` (a string), evaluated as PHS with `bindings` for its free names.
fn at(program: &str, bindings: &[(&str, f64)]) -> f64 {
    let text = match calc(program) {
        PhsValue::String(s) => s,
        other => panic!("{program:?} should give a symbolic expression, it gave {other:?}"),
    };
    let mut source: String = bindings.iter().map(|(name, value)| format!("{name} = {value}\n")).collect();
    source.push_str(&text);
    let values = eval_phs(&source).unwrap_or_else(|e| panic!("{program:?} gave {text:?}, which does not evaluate: {e}"));
    as_number(values.last().expect("a value"), &format!("{program:?} = {text:?}"))
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() <= 1e-12 * expected.abs().max(1.0), "{what}: {actual} vs {expected}");
}

// ---- the argument takes the place of the parameter ----

#[test]
fn the_argument_replaces_the_parameter() {
    // before: 0, 2, 0 (the body `2*x` was pasted whatever the argument)
    assert_close(at(r#"f(x) = 2*x
deriv("f(t)", "t")"#, &[]), 2.0, "d/dt f(t), f(x) = 2x");
    assert_close(at(r#"f(x) = 2*x
deriv("f(x)", "x")"#, &[]), 2.0, "d/dx f(x), f(x) = 2x");
    assert_close(at(r#"f(x) = 2*x
deriv("f(y)", "y")"#, &[]), 2.0, "d/dy f(y), f(x) = 2x");
}

#[test]
fn an_argument_expression_is_one_operand() {
    // f(x) = x^2: d/dt f(2t) = 8t; before: 0
    assert_close(at("f(x) = x^2\nderiv(\"f(2*t)\", \"t\")", &[("t", 0.7)]), 8.0 * 0.7, "d/dt (2t)^2");
    // an argument that is a sum stays one operand: d/dt (t + s)^2 = 2(t + s); before: 0
    assert_close(at("f(x) = x^2\nderiv(\"f(t + s)\", \"t\")", &[("t", 0.7), ("s", 1.3)]), 2.0 * (0.7 + 1.3), "d/dt (t+s)^2");
}

#[test]
fn a_function_calling_another_is_inlined_through() {
    // g(x) = f(x) + 1 with f(x) = x^2: d/dt g(t) = 2t; before: 0
    assert_close(at("f(x) = x^2\ng(x) = f(x) + 1\nderiv(\"g(t)\", \"t\")", &[("t", 0.7)]), 2.0 * 0.7, "d/dt (t^2 + 1)");
    // g(y) = f(3y) + y: d/dt g(t) = 18t + 1; before: 0
    assert_close(at("f(x) = x^2\ng(y) = f(3*y) + y\nderiv(\"g(t)\", \"t\")", &[("t", 0.7)]), 18.0 * 0.7 + 1.0, "d/dt (9t^2 + t)");
    // a call as the argument of a call: f(f(t)) = t^4, d/dt = 4t^3
    assert_close(at("f(x) = x^2\nderiv(\"f(f(t))\", \"t\")", &[("t", 0.7)]), 4.0 * 0.7_f64.powi(3), "d/dt t^4");
}

#[test]
fn a_function_may_be_called_more_than_once() {
    // f(t) + f(2t) = 5t^2, d/dt = 10t; before: refused ("Unknown symbolic function 'f'")
    assert_close(at("f(x) = x^2\nderiv(\"f(t) + f(2*t)\", \"t\")", &[("t", 0.7)]), 10.0 * 0.7, "d/dt (t^2 + 4t^2)");
}

#[test]
fn several_parameters_are_replaced_at_once() {
    // h(t, 3) = 3t; before: 0
    assert_close(at("h(a, b) = a*b\nderiv(\"h(t, 3)\", \"t\")", &[]), 3.0, "d/dt 3t");
    // h(b, a) = b - a, so d/da = -1: the two arguments swap, they are not replaced one after the other; before: 1
    assert_close(at("h(a, b) = a - b\nderiv(\"h(b, a)\", \"a\")", &[]), -1.0, "d/da (b - a)");
}

#[test]
fn a_parameter_named_like_a_variable_of_the_expression_does_not_capture_it() {
    // f(t) = t^3 called on x: d/dx x^3 = 3x^2; before: 0
    assert_close(at("f(t) = t^3\nderiv(\"f(x)\", \"x\")", &[("x", 0.7)]), 3.0 * 0.49, "d/dx x^3");
    // f(x) = x^2 called on t next to a free x: d/dx (t^2 + x) = 1; before: 2 * x + 1
    assert_close(at("f(x) = x^2\nderiv(\"f(t) + x\", \"x\")", &[("t", 0.7), ("x", 0.3)]), 1.0, "d/dx (t^2 + x)");
}

#[test]
fn the_body_keeps_its_parentheses() {
    // (t + 1)*2, d/dt = 2; before: 0 (the body was printed `x + 1 * 2`)
    assert_close(at("f(x) = (x + 1)*2\nderiv(\"f(t)\", \"t\")", &[]), 2.0, "d/dt 2(t + 1)");
    // a sign: -(t^2), d/dt = -2t
    assert_close(at("f(x) = -x^2\nderiv(\"f(t)\", \"t\")", &[("t", 0.7)]), -1.4, "d/dt -t^2");
}

#[test]
fn a_function_name_is_matched_as_a_name_not_as_text() {
    // `sqf(` is not found inside `sqrt(`: d/dt (sqrt(t) + t^2) = 1/(2 sqrt t) + 2t; before: 1/(2 * t^0.5)
    let expected = 1.0 / (2.0 * 0.7_f64.sqrt()) + 2.0 * 0.7;
    assert_close(at("sqf(x) = x^2\nderiv(\"sqrt(t) + sqf(t)\", \"t\")", &[("t", 0.7)]), expected, "d/dt (sqrt t + t^2)");
}

// ---- the other builtins take the same path ----

#[test]
fn integral_inlines_with_the_argument() {
    // the integral of x^2 over [0, 1] is 1/3
    assert_close(as_number(&calc("f(x) = x^2\nintegral(\"f(x)\", \"x\", 0, 1)"), "integral"), 1.0 / 3.0, "∫ x^2");
    // the integral of 3t^2 over [0, 1] is 1; before: refused ("the integrand uses 'x'")
    assert_close(as_number(&calc("f(x) = 3*x^2\nintegral(\"f(t)\", \"t\", 0, 1)"), "integral"), 1.0, "∫ 3t^2");
    // the antiderivative of f(t) = t^2 + 1 at t = 3 is 9 + 3 = 12 (the constant of integration is 0)
    assert_close(at("f(x) = x^2 + 1\nintegral(\"f(t)\", \"t\")", &[("t", 3.0)]), 12.0, "∫ (t^2 + 1) dt");
}

#[test]
fn limit_inlines_with_the_argument() {
    // x^2/x -> 0 at 0
    assert_close(as_number(&calc("f(x) = x^2\nlimit(\"f(x)/x\", \"x\", 0)"), "limit"), 0.0, "x^2/x");
    // sin(t)/t -> 1 at 0; before: refused ("the expression uses 'x'")
    assert_close(as_number(&calc("f(x) = sin(x)\nlimit(\"f(t)/t\", \"t\", 0)"), "limit"), 1.0, "sin t / t");
}

#[test]
fn solve_simplify_and_series_inline_with_the_argument() {
    // 2t + 1 = 7 at t = 3
    assert_close(as_number(&calc("f(x) = 2*x + 1\nsolve(\"f(t) = 7\", \"t\")"), "solve"), 3.0, "2t + 1 = 7");
    // t^2 - t^2 = 0; before: x^2 - t^2
    assert_close(at("f(x) = x^2\nsimplify(\"f(t) - t^2\")", &[("t", 0.7)]), 0.0, "t^2 - t^2");
    // (t + 1)^2 = 1 + 2t + t^2, exact at order 3; before: x^2
    assert_close(at("f(x) = x^2\nseries(\"f(t + 1)\", \"t\", 0, 3)", &[("t", 0.7)]), 1.7 * 1.7, "(t + 1)^2");
    // the Laplace transform of f(t) = t is 1/s^2
    assert_close(at("f(x) = x\nlaplace(\"f(t)\", \"t\", \"s\")", &[("s", 2.0)]), 0.25, "L{t}");
}

#[test]
fn a_function_defined_where_the_builtin_is_called_is_seen() {
    // `phs eval` keeps its variables apart from the interpreter's own; before: "Unknown symbolic function 'f'"
    let report = evaluate(
        "use * from calc\nf(x) = 2*x\nderiv(\"f(t)\", \"t\")",
        Limits::sandboxed(Duration::from_secs(30)),
        DEFAULT_SEED,
    );
    let item = report.results.last().unwrap_or_else(|| panic!("no value: {:?}", report.diagnostics));
    assert_eq!(item.value.text, "2");
}

// ---- units ----

#[test]
fn a_unit_in_the_body_is_carried() {
    // f(x) = 2 m * x: the integral of f(t) over [0, 1] is 1 m; before: refused ("the integrand uses 'x'")
    match calc("f(x) = 2 m * x\nintegral(\"f(t)\", \"t\", 0, 1)") {
        PhsValue::Quantity(q) => {
            let metres = q.to("m").unwrap_or_else(|e| panic!("{q} is not a length: {e}"));
            assert_close(metres.value.mean(), 1.0, "∫ 2 m t dt over [0, 1]");
        }
        other => panic!("the integral should be a length, it is {other:?}"),
    }
    // symbolically the unit rides along as it does when written in the string: d/dt (2 m t) = 2 m; before: 0
    match calc("f(x) = 2 m * x\nderiv(\"f(t)\", \"t\")") {
        PhsValue::String(s) => {
            let expected = match calc("deriv(\"2 m * t\", \"t\")") {
                PhsValue::String(e) => e,
                other => panic!("{other:?}"),
            };
            assert_eq!(s, expected);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_parameter_with_a_declared_unit_is_refused() {
    // the declared unit converts and checks the argument, which a symbolic body cannot do; before: 0
    let message = refused("f(x: m) = x^2\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'f'") && message.contains("'x'") && message.contains("m"), "{message}");
}

#[test]
fn an_uncertain_literal_in_the_body_is_refused() {
    let message = refused("f(x) = (2 +/- 0.1) * x\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'f'") && message.contains("uncertain"), "{message}");
}

// ---- what cannot be inlined is refused ----

#[test]
fn recursion_is_refused_and_named() {
    // before: "Unknown symbolic function 'f'"
    let message = refused("f(x) = f(x - 1)\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("f -> f"), "{message}");
    let message = refused("f(x) = g(x)\ng(x) = f(x)\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("f -> g -> f"), "{message}");
}

#[test]
fn the_wrong_number_of_arguments_is_refused() {
    // before: 0 (the call was replaced by `a*b`)
    let message = refused("h(a, b) = a*b\nderiv(\"h(t)\", \"t\")");
    assert!(message.contains("'h'") && message.contains('2') && message.contains('1'), "{message}");
    let message = refused("f(x) = x^2\nderiv(\"f(t, 1)\", \"t\")");
    assert!(message.contains("'f'"), "{message}");
}

#[test]
fn a_body_that_is_not_one_expression_is_refused() {
    let message = refused("f(x) =\n    y = 2*x\n    y + 1\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'f'") && message.contains("statement"), "{message}");
    let message = refused("f(x) = if x > 0 then x else -x\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'f'"), "{message}");
}

#[test]
fn a_decorated_function_is_refused() {
    // the decorator's check would be dropped by inlining
    let message = refused("@requires(x > 0, \"x must be positive\")\nf(x) = x^2\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'f'") && message.contains("requires"), "{message}");
}
