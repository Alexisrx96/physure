//! Results that used to come out silently wrong, each now an answer that is right or an error
//! that says what to write instead.

use physure_script::interpreter::eval_phs;

/// The value of the last statement of `program`, as text.
fn run(program: &str) -> String {
    let values = eval_phs(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_uncertainty_after_a_unit_takes_no_unit_of_its_own() {
    // `1 m +/- 0.1 m` was `1.00 ± 0.10 m^2`: the second `m` multiplied the measurement.
    for (program, write) in [
        ("1 m +/- 0.1 m", "write `1 +/- 0.1 m`"),
        ("1 m ± 0.1 m", "write `1 ± 0.1 m`"),
        ("1 m +/- 10 % m", "write `1 +/- 10 % m`"),
        ("1 m +/- (0.2, 0.1) m", "write `1 +/- (0.2, 0.1) m`"),
        ("1 m +/- 2 sigma m", "write `1 +/- 2 sigma m`"),
        ("1 V +/- 0.1 dof 9 V", "write `1 +/- 0.1 dof 9 V`"),
        ("(1 + 1) m +/- 0.1 m", "write `(1 + 1) +/- 0.1 m`"),
        ("x = 2 m +/- 0.1 m/s", "write `2 +/- 0.1 m`"),
    ] {
        let message = refused(program);
        assert!(
            message.contains("the uncertainty takes no unit of its own") && message.contains(write),
            "{program}: {message}"
        );
    }
}

#[test]
fn the_message_shows_the_written_value_and_unit() {
    let message = refused("12.5 kPa +/- 0.3 kPa");
    assert!(message.contains("write `12.5 +/- 0.3 kPa`"), "{message}");
}

#[test]
fn the_forms_that_say_the_unit_once_still_read_as_before() {
    for (program, expected) in [
        ("1 +/- 0.1 m", "1.00 ± 0.10 m"),
        ("1 m +/- 0.1", "1.00 ± 0.10 m"),
        ("10.0 +/- 0.5 m", "10.0 ± 0.5 m"),
        ("1 m +/- 10 %", "1.00 ± 0.10 m"),
        ("1 +/- 10 % m", "1.00 ± 0.10 m"),
        ("(1 +/- 0.1) m", "1.00 ± 0.10 m"),
        ("1 +/- 0.1 m^2", "1.00 ± 0.10 m^2"),
        ("x = 3\n2 m +/- 0.1 * x", "6.0 ± 0.3 m"),
        // with no unit before the uncertainty, the unit after it is the measurement's
        ("(2) +/- 0.1 m", "2.00 ± 0.10 m"),
    ] {
        assert_eq!(run(program), expected, "{program}");
    }
}

#[test]
fn an_uncertainty_after_parentheses_is_kept_or_refused_never_dropped() {
    // `(x) +/- 0.1` was `x` and `(2 m) +/- 0.1` was `2.0 m`: the uncertainty vanished.
    for program in ["x = 2 m\n(x) +/- 0.1", "(2 + 3) +/- 0.1 m", "(2 + 3) m +/- 0.1", "x = 2\n(x +/- 0.1) m", "(1 +/- 0.1) +/- 0.2"] {
        let message = refused(program);
        assert!(message.contains("cannot be attached to the expression"), "{program}: {message}");
    }
    assert_eq!(run("(2 m) +/- 0.1"), "2.00 ± 0.10 m");
    assert_eq!(run("(2) m +/- 0.1"), "2.00 ± 0.10 m");
    let message = refused("(1 +/- 0.1) m +/- 0.2");
    assert!(message.contains("cannot be attached"), "{message}");
}

#[test]
fn a_sigma_bound_in_parentheses_stays_a_sigma_bound() {
    // `(10 +/- 2 sigma) m` was `10 ± 2 m`: the bound became a standard uncertainty.
    assert_eq!(run("(10 +/- 2 sigma) m"), run("10 m +/- 2 sigma"));
}

#[test]
fn a_unit_after_a_parenthesised_quantity_multiplies_it() {
    // `(2 m) s` was `2.0 s`: the metre was replaced.
    assert_eq!(run("(2 m) s => m*s"), run("2 m * s => m*s"));
    assert_eq!(run("(2) m"), "2.0 m");
}

#[test]
fn only_a_quantity_or_a_collection_of_them_converts_to_a_unit() {
    // `(a < b) => km` was `True` and `"x" => km` was `x`: the `=> km` was ignored.
    for (program, says) in [
        ("a = 1 m\nb = 2 m\n(a < b) => km", "a Bool (True)"),
        ("True => km", "a Bool (True) to `km`"),
        ("\"x\" => km", "a String (x)"),
        ("[True, 1 m] => cm", "a Bool (True)"),
        ("f(x) = 2*x\nf => km", "a Function"),
        ("x = 2 +/- 0.1 m\n(x == 2 m +/- 2 sigma) => km", "a Bool"),
    ] {
        let message = refused(program);
        assert!(message.contains("cannot convert") && message.contains(says), "{program}: {message}");
    }
    let message = refused("unbound_name => km");
    assert!(message.contains("`unbound_name` has no value"), "{message}");
}

#[test]
fn quantities_numbers_and_their_collections_still_convert() {
    for (program, expected) in [
        ("[1, 2] m => cm", "[100.0 cm, 200.0 cm]"),
        ("[[1 m, 2 m], [3 m, 4 m]] => cm", "[[100.0 cm, 200.0 cm], [300.0 cm, 400.0 cm]]"),
        ("(0 m .. 100 m) => km", "0.0 km .. 0.1 km"),
        ("0.5 => %", "50.0 %"),
        ("sigma(1 +/- 0.1 m) => cm", "10.0 cm"),
        ("10.04 mm +/- 2 sigma => cm", "1.004 cm ± 2.0σ"),
    ] {
        assert_eq!(run(program), expected, "{program}");
    }
    // a plain number is dimensionless: it converts to a ratio, not to a length
    let message = refused("5 => km");
    assert!(message.contains("km"), "{message}");
}

#[test]
fn a_level_keeps_its_unit_name_under_a_sign_or_a_plain_factor() {
    // `-5 dB` printed `-5.0`, `2 * 5 dB` printed `10.0`: still a level, but the name was lost.
    for (program, expected) in [
        ("-5 dB", "-5.0 dB"),
        ("2 * 5 dB", "10.0 dB"),
        ("5 dB * 2", "10.0 dB"),
        ("x = 5 dB\n-x", "-5.0 dB"),
        ("-5 pH", "-5.0 pH"),
        ("-(5 dB)", "-5.0 dB"),
        ("0.5 * 4 dB", "2.0 dB"),
        ("(5 dB)^1", "5.0 dB"),
        ("abs(-5 dB)", "5.0 dB"),
        ("(-5 +/- 1) dB", "-5.0 ± 1.0 dB"),
        ("[1, 2] dB * -1", "[-1.0 dB, -2.0 dB]"),
    ] {
        assert_eq!(run(program), expected, "{program}");
    }
    // Levels add as the power ratios they stand for: 10 log10(10^(-0.5) + 10^(0.1)) dB.
    let sum = run("-5 dB + 1 dB");
    assert!(sum.starts_with("1.97322793708") && sum.ends_with(" dB"), "{sum}");
    assert!(run("0 dB + 0 dB").starts_with("3.0102999566") && run("0 dB + 0 dB").ends_with(" dB"));
}
