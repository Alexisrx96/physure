//! A bare name with no binding, no `pi`/constant and no unit symbol is an error, not its own text (audit T5).

use physure_script::interpreter::eval_phs;

fn shows(source: &str) -> String {
    let values = eval_phs(source).unwrap_or_else(|e| panic!("{source:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_name_with_no_value_is_an_error_in_every_position() {
    for source in [
        "foo",
        "foo + 1",
        "1 + foo",
        "2 * foo",
        "sin(foo)",
        "x = foo",
        "if foo > 1 then 1 else 2",
        "for i in foo { i }",
        "foo .. 3",
        "[foo, 2]",
        "foo => m",
    ] {
        let message = refused(source);
        assert!(message.contains("`foo` has no value"), "{source:?}: {message}");
    }
}

#[test]
fn a_typo_in_a_variable_names_the_typo() {
    assert!(refused("speed = 3 m/s\nspeeed * 2 s").contains("`speeed` has no value"));
}

#[test]
fn what_has_a_value_is_not_unbound() {
    // a variable, a constant, a unit, a function parameter, a name bound by `where`
    assert!(eval_phs("v = 2\nv + 1").is_ok());
    assert!(eval_phs("pi * 2").is_ok());
    assert!(eval_phs("5 m + 2 m").is_ok());
    assert!(eval_phs("fn f(a) = a * 2\nf(3)").is_ok());
    assert!(eval_phs("x = 2 * y where y = 1").is_ok());
}

#[test]
fn a_condition_that_is_a_name_still_says_which_literals_are_bool() {
    let message = refused("while true {\n  1\n}");
    assert!(message.contains("`true` has no value"), "{message}");
    assert!(message.contains("True and False"), "{message}");
}

#[test]
fn text_stays_text_and_a_failing_span_stays_raw() {
    assert_eq!(shows("\"foo\""), "foo");
    assert_eq!(shows("\"{foo}\""), "{foo}");
}

#[test]
fn a_unit_written_after_an_expression_is_a_unit_not_a_name() {
    // the parser used to hand the unit's text over as a name, which worked only because a name was its own text
    assert_eq!(shows("(2 + 3) m/s => km/h"), shows("5 m/s => km/h"));
    assert_eq!(shows("[1, 2] m => cm"), shows("vector(100 cm, 200 cm)"));
}

#[test]
fn the_free_variable_of_a_symbolic_derivative_is_its_variable() {
    assert_eq!(shows("f(x) = x^3 - 3*x\nf'(x)"), "3 * x^2 - 3");
}

#[test]
fn a_typo_of_a_unit_names_the_unit() {
    let message = refused("5 metre");
    assert!(message.starts_with("`metre` has no value") && message.contains("did you mean the unit"), "{message}");
}
