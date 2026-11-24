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

// ---- the checker and the interpreter agree

/// The checker's findings that are errors.
fn checker_errors(source: &str) -> Vec<&'static str> {
    use physure_script::check::{check_source, Severity};
    check_source(source)
        .unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"))
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn every_program_the_checker_flags_as_an_unknown_name_the_interpreter_refuses_naming_it() {
    for (source, name) in [
        ("5 fooz", "fooz"),
        ("x = 2 barrelz\nx", "barrelz"),
        ("5 m * foo", "foo"),
        ("5 m + foo", "foo"),
        ("x = 3 barz", "barz"),
    ] {
        assert!(checker_errors(source).contains(&"unknown-name"), "{source:?}: the checker is silent");
        let message = refused(source);
        assert!(message.contains(&format!("`{name}` has no value")), "{source:?}: {message}");
    }
}

#[test]
fn the_checker_never_flags_a_name_the_interpreter_gives_a_value() {
    for source in [
        "foo = 2\n5 m * foo",
        "fn f(foo) = 5 m * foo\nf(2)",
        "for i in 1..3 { 5 m * i }",
        "fn g(x) = y + 1 m where y = x * 2\ng(3 m)",
        "5 m * pi",
        "5 m * s",
        "5 m * (2 s)",
    ] {
        eval_phs(source).unwrap_or_else(|e| panic!("{source:?} should run: {e}"));
        assert!(!checker_errors(source).contains(&"unknown-name"), "{source:?}: a false positive");
    }
}

/// Every message names the name; a hint is added only when it is likely to be right (a bound name two edits away,
/// a unit one edit away for a name of four characters or more or a slip of case, a function of PHS written as a
/// value). The hint of the catalog's closest unit used to be added to every name, and was noise for most variables.
#[test]
fn the_hint_of_a_name_with_no_value_is_the_likely_one() {
    let function = |name: &str| {
        format!("`{name}` has no value: `{name}` is a function of PHS, not a value; pass a user function (`fn f(x) = {name}(x)`) or write the call")
    };
    let table: Vec<(&str, String)> = vec![
        ("x", "`x` has no value".into()),
        ("y", "`y` has no value".into()),
        ("i", "`i` has no value".into()),
        ("a", "`a` has no value".into()),
        ("e", "`e` has no value".into()),
        ("true", "`true` has no value".into()),
        ("masss", "`masss` has no value".into()),
        ("count", "`count` has no value".into()),
        ("lenght", "`lenght` has no value".into()),
        ("foobar", "`foobar` has no value".into()),
        ("zz_unknown_name", "`zz_unknown_name` has no value".into()),
        ("x = 5 x", "`x` has no value".into()),
        ("ab = 1\nac", "`ac` has no value".into()),
        ("5 metre", "`metre` has no value — did you mean the unit 'metro'?".into()),
        ("5 fooz", "`fooz` has no value — did you mean the unit 'foot'?".into()),
        ("5 barz", "`barz` has no value — did you mean the unit 'bar'?".into()),
        ("5 Km", "`Km` has no value — did you mean the unit 'km'?".into()),
        ("100 KPa", "`KPa` has no value — did you mean the unit 'kPa'?".into()),
        ("5 Kg", "`Kg` has no value — did you mean the unit 'kg'?".into()),
        ("speed = 3\nspeeed", "`speeed` has no value: did you mean `speed`?".into()),
        ("speeed = 3\nspeed", "`speed` has no value: did you mean `speeed`?".into()),
        ("velocity = 2\nvelocty", "`velocty` has no value: did you mean `velocity`?".into()),
        ("temp = 3\ntmep", "`tmep` has no value: did you mean `temp`?".into()),
        ("dist = 3 m\ndits + 1 m", "`dits` has no value: did you mean `dist`?".into()),
        ("mass = 2 kg\nmas * 2", "`mas` has no value: did you mean `mass`?".into()),
        ("fn force(m) = m\nforse", "`forse` has no value: did you mean `force`?".into()),
        ("sqrt", function("sqrt")),
        ("sin", function("sin")),
        ("parallel_map(sqrt, vector(1, 4))", function("sqrt")),
        ("use * from plot\nplot(sin, 0, 1)", function("sin")),
    ];
    for (source, expected) in table {
        assert_eq!(refused(source), expected, "{source:?}");
    }
}

/// A function of `calc` or `plot` takes its variables as text. A bare variable worked only because a name with no
/// value was its own text: it is refused, saying how to write it, and the quoted form works.
#[test]
fn a_bare_variable_of_a_symbolic_function_says_to_write_it_as_text() {
    for (bare, variable, quoted) in [
        ("deriv(\"x^2\", x)", "x", "deriv(\"x^2\", \"x\")"),
        ("deriv(x, x)", "x", "deriv(\"x\", \"x\")"),
        ("integral(\"x^2\", x)", "x", "integral(\"x^2\", \"x\")"),
        ("integral(\"x^2\", x, 0, 1)", "x", "integral(\"x^2\", \"x\", 0, 1)"),
        ("integral(x, x)", "x", "integral(\"x\", \"x\")"),
        ("limit(\"sin(x)/x\", x, 0)", "x", "limit(\"sin(x)/x\", \"x\", 0)"),
        ("lim(\"sin(x)/x\", x, 0)", "x", "lim(\"sin(x)/x\", \"x\", 0)"),
        ("substitute(\"x + 1\", x, 2)", "x", "substitute(\"x + 1\", \"x\", 2)"),
        ("series(\"sin(x)\", x, 0, 5)", "x", "series(\"sin(x)\", \"x\", 0, 5)"),
        ("taylor(\"sin(x)\", x, 0, 5)", "x", "taylor(\"sin(x)\", \"x\", 0, 5)"),
        ("dsolve(\"y' = y\", y, x)", "y", "dsolve(\"y' = y\", \"y\", \"x\")"),
        ("grad(\"x^2+y^2\", [x, y])", "x", "grad(\"x^2+y^2\", [\"x\", \"y\"])"),
        ("e1 = \"y = 2*x\"\nsolve(e1, x)", "x", "e1 = \"y = 2*x\"\nsolve(e1, \"x\")"),
        ("plot(\"sin(x)\", x, 0, 1)", "x", "plot(\"sin(x)\", \"x\", 0, 1)"),
    ] {
        let program = |body: &str| format!("use * from calc\nuse * from plot\n{body}");
        let message = refused(&program(bare));
        let function = bare.rsplit('\n').next().unwrap().split('(').next().unwrap();
        assert_eq!(message, format!("`{variable}` has no value: the variable of `{function}` is text, write \"{variable}\""), "{bare:?}");
        assert!(eval_phs(&program(quoted)).is_ok(), "{quoted:?}");
    }
}

/// `m = 5; [1, 2] m` was `[5.0, 10.0]` (the variable) while a unit after an expression was a name; it is ambiguous
/// now, as `5 m` is, and the message speaks of the unit written, not of a literal `1 m` the program does not have.
#[test]
fn a_unit_after_an_expression_that_a_variable_shadows_is_ambiguous() {
    for source in ["m = 5\n[1, 2] m", "m = 5\n(2 + 3) m"] {
        let message = refused(source);
        assert!(message.contains("Ambiguous 'm' in the unit `m` written after an expression"), "{source:?}: {message}");
        assert!(!message.contains("1 m"), "{source:?}: {message}");
    }
    assert_eq!(shows("m = 5\n[1, 2] * \"m\" => cm"), shows("[100, 200] cm"));
}
