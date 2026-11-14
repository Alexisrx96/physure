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
