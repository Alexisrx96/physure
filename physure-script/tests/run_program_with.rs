//! `PhsInterpreter::run_program_with`: a program evaluated with quantities bound by name, as a binding calls a
//! function with values it holds. The quantities arrive **with their sources** (not as text, which would make
//! a new measurement of each), and the interpreter is left exactly as it was.

use std::collections::BTreeSet;

use physure_core::Quantity;
use physure_script::{parse_phs, PhsInterpreter, PhsValue};

fn measured(value: f64, u: f64, unit: &str) -> PhsValue {
    PhsValue::Quantity(Quantity::with_uncertainty(value, u, unit).unwrap())
}

fn bind(name: &str, value: &PhsValue) -> (String, PhsValue) {
    (name.to_string(), value.clone())
}

fn interpreter() -> PhsInterpreter {
    let mut interpreter = PhsInterpreter::default();
    interpreter.eval_str("f(a, b) = a - b\nscale = 2 m").unwrap();
    interpreter
}

fn names(interpreter: &PhsInterpreter) -> BTreeSet<String> {
    interpreter.env.keys().cloned().collect()
}

fn run(interpreter: &PhsInterpreter, source: &str, bindings: Vec<(String, PhsValue)>) -> Result<PhsValue, String> {
    interpreter.run_program_with(&parse_phs(source).unwrap(), bindings).map_err(|e| e.to_string())
}

fn spread(value: &PhsValue) -> (f64, f64) {
    match value {
        PhsValue::Quantity(q) => (q.value.mean(), q.value.std_dev()),
        other => panic!("{other:?} is not a quantity"),
    }
}

#[test]
fn one_measurement_bound_under_two_names_is_still_one_measurement() {
    let interpreter = interpreter();
    let x = measured(10.0, 0.5, "m");
    let difference = run(&interpreter, "f(_arg0, _arg1)", vec![bind("_arg0", &x), bind("_arg1", &x)]).unwrap();
    assert_eq!(spread(&difference), (0.0, 0.0), "the function saw the same source twice and it cancelled");
}

#[test]
fn two_measurements_with_equal_numbers_are_two_measurements() {
    let interpreter = interpreter();
    let (x, y) = (measured(10.0, 0.5, "m"), measured(10.0, 0.5, "m"));
    let difference = run(&interpreter, "f(_arg0, _arg1)", vec![bind("_arg0", &x), bind("_arg1", &y)]).unwrap();
    let (value, u) = spread(&difference);
    assert_eq!(value, 0.0);
    assert!((u - 0.5 * 2f64.sqrt()).abs() < 1e-12, "{u}");
}

#[test]
fn nothing_is_left_behind() {
    let interpreter = interpreter();
    let before = names(&interpreter);
    let x = measured(3.0, 0.1, "m");
    run(&interpreter, "tmp = _arg0 * 2\ntmp + _arg0", vec![bind("_arg0", &x)]).unwrap();
    assert_eq!(names(&interpreter), before, "neither the binding nor the variable the program assigned stays");
}

#[test]
fn a_binding_shadows_a_variable_only_for_the_call() {
    let interpreter = interpreter();
    let seven = measured(7.0, 0.0, "m");
    let seen = run(&interpreter, "scale + 0 m", vec![bind("scale", &seven)]).unwrap();
    assert_eq!(spread(&seen).0, 7.0);
    assert_eq!(spread(interpreter.get_var("scale").unwrap()).0, 2.0, "the interpreter's own variable is intact");
}

#[test]
fn an_error_leaves_the_interpreter_as_it_was() {
    let interpreter = interpreter();
    let before = names(&interpreter);
    let x = measured(1.0, 0.1, "m");
    let message = run(&interpreter, "undefined_function(_arg0)", vec![bind("_arg0", &x)]).unwrap_err();
    assert!(message.contains("undefined_function"), "{message}");
    assert_eq!(names(&interpreter), before);
}

#[test]
fn an_empty_program_has_no_value() {
    assert!(matches!(run(&interpreter(), "", vec![]), Ok(PhsValue::None)));
}
