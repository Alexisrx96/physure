//! `phs check` says statically what the interpreter refuses at run time (Task 3): a condition is a Bool. Where the
//! checker knows the type of a condition (a number, a quantity, text, a vector literal, a name with no value) it
//! reports it with the interpreter's words; where it does not, it says nothing.

use physure_script::check::{check_source, Severity};
use physure_script::interpreter::eval_phs;

/// The error messages of the checker for `source`.
fn checker_errors(source: &str) -> Vec<String> {
    check_source(source)
        .unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"))
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

/// `(program, what the checker and the interpreter both say)`: refused at run time and reported statically.
const REFUSED: &[(&str, &str)] = &[
    ("x = 5\nif x then 1 else 2", "the condition of `if` needs a Bool (True or False), got a Number"),
    ("if 5 m then 1 else 2", "the condition of `if` needs a Bool (True or False), got a Quantity"),
    ("x = 2 m\nx ? 1 : 2", "the condition of `? :` needs a Bool (True or False), got a Quantity"),
    ("i = 3\nwhile i {\n  i = i - 1\n}", "the condition of `while` needs a Bool (True or False), got a Number"),
    ("s = \"on\"\nif s then 1 else 2", "got a String (\"on\"): text is not a condition; compare it, e.g. `s == \"on\"`"),
    ("if \"false\" then 1 else 2", "got a String (\"false\"): text is not a condition; the Bool literals are True and False"),
    ("if [1, 2] then 1 else 2", "got a Vector: a Vector is not a condition"),
    ("while true {\n  1\n}", "`true` has no value; the Bool literals are True and False"),
    ("fn f(m: kg) =\n  if m then return 1\n  return 2\nf(1 kg)", "the condition of `if ... then return` needs a Bool"),
    ("@requires(m, \"m\")\nfn f(m: kg) = m\nf(1 kg)", "the condition of `@requires` needs a Bool"),
    ("@ensures(result, \"r\")\nfn f(m: kg) = m\nf(1 kg)", "the condition of `@ensures` needs a Bool"),
    ("x = 1 m\nx and True", "the left side of `and` needs a Bool"),
    ("not 5", "`not` needs a Bool"),
];

/// Accepted by the checker (no condition error) and by the interpreter.
const ACCEPTED: &[&str] = &[
    "x = 5\nif x > 0 then 1 else 2",
    "x = 5 m\nif x != 0 m then 1 else 2",
    "b = 1 m < 2 m\nb ? 1 : 2",
    "if True then 1 else 2",
    "b = True and not False\nif b then 1 else 2",
    "i = 3\nwhile i > 0 {\n  i = i - 1\n}\ni",
    "fn positive(x) = x > 0\nif positive(3) then 1 else 2",
    "@requires(m > 0 kg, \"m\")\nfn f(m: kg) = m\nf(1 kg)",
    "@ensures(result > 0 kg, \"r\")\nfn f(m: kg) = m\nf(1 kg)",
    "fn g(c) = if c then 1 else 2\ng(True)",
];

#[test]
fn a_condition_of_a_known_type_that_is_not_a_bool_is_reported_statically() {
    for (program, says) in REFUSED {
        let errors = checker_errors(program);
        assert!(errors.iter().any(|e| e.contains(says)), "checker, {program:?}: {errors:?}");
    }
}

#[test]
fn the_interpreter_refuses_what_the_checker_reports_with_the_same_words() {
    for (program, says) in REFUSED {
        let error = match eval_phs(program) {
            Ok(values) => panic!("{program:?} was answered: {:?}", values.last()),
            Err(e) => e.to_string(),
        };
        assert!(error.contains(says), "interpreter, {program:?}: {error}");
    }
}

#[test]
fn a_bool_condition_or_one_of_unknown_type_is_not_reported() {
    for program in ACCEPTED {
        let errors = checker_errors(program);
        assert!(errors.is_empty(), "checker, {program:?}: {errors:?}");
        eval_phs(program).unwrap_or_else(|e| panic!("interpreter, {program:?}: {e}"));
    }
    // the type of a parameter without a unit is not known where the body is first checked: silent
    assert!(checker_errors("fn g(c) = if c then 1 else 2").is_empty());
}

#[test]
fn a_level_or_a_temperature_point_is_described_and_compared_in_its_own_unit() {
    for (program, says) in [
        ("if 20 dB then 1 else 2", "got a Quantity (a level): a quantity is not a condition; compare it with a value in its own unit"),
        ("if 0 degC then 1 else 2", "got a Quantity (an absolute temperature): a quantity is not a condition; compare it with a value"),
        ("if 2 km then 1 else 2", "got a Quantity (of dimension L): a quantity is not a condition; compare it, e.g. `x != 0 m`"),
    ] {
        let errors = checker_errors(program);
        assert!(errors.iter().any(|e| e.contains(says)), "{program}: {errors:?}");
        assert!(eval_phs(program).is_err(), "{program}");
    }
}

#[test]
fn converting_a_bool_or_text_to_a_unit_is_reported_statically_as_the_interpreter_refuses_it() {
    for program in ["a = 1 m\nb = 2 m\n(a < b) => km", "True => km", "\"x\" => km"] {
        let errors = checker_errors(program);
        assert!(errors.iter().any(|e| e.contains("cannot convert a") && e.contains("to `km`")), "{program}: {errors:?}");
        assert!(eval_phs(program).unwrap_err().to_string().contains("cannot convert a"), "{program}");
    }
    for program in ["[1 m, 2 m] => cm", "x = 5 m\nx => km", "0.5 => %"] {
        assert!(checker_errors(program).is_empty(), "{program}");
        eval_phs(program).unwrap_or_else(|e| panic!("{program}: {e}"));
    }
}
