//! A zero needs a unit, a level and an angle are not plain numbers (the comparison rule of
//! `Quantity::comparable_magnitudes`), and the static checker (`phs check`) and the interpreter agree on every case.
//!
//! The rule: two operands compare only when they have the same dimension, so `x > 0` with `x` in metres is an error
//! (zeros of different scales are different values, 0 degC is 273.15 K and 0 dB is a ratio of 1: the unit says which
//! one is meant, `x > 0 m`); an angle is a dimension in every angle mode (`1 rad > 0` is an error); a level (`dB`)
//! and a dimensionless value that is not a level do not compare (`20 dB == 100`); a dimensionless ratio in any unit
//! (`unity`, `%`, `ppm`) compares with plain numbers by its ratio (`50 % == 0.5`).

use physure_core::units::angle_mode::{scoped, AngleMode};
use physure_script::check::{check_source, Severity};
use physure_script::interpreter::eval_phs;

/// Whether the static checker reports an error for `source`.
fn checker_refuses(source: &str) -> bool {
    check_source(source)
        .unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"))
        .iter()
        .any(|d| d.severity == Severity::Error)
}

/// Whether the interpreter refuses `source`.
fn runtime_refuses(source: &str) -> bool {
    eval_phs(source).is_err()
}

/// The error the interpreter raises for `source`.
fn runtime_error(source: &str) -> String {
    match eval_phs(source) {
        Err(e) => e.to_string(),
        Ok(values) => panic!("{source:?} was answered: {:?}", values.last()),
    }
}

/// `(snippet, accepted)`: the agreement matrix. Each is run through the checker and the interpreter in both angle
/// modes, and both must accept or both refuse, as `accepted` says.
const MATRIX: &[(&str, bool)] = &[
    // quantities of several dimensions against a plain zero, in both orders and with every operator
    ("1 m > 0", false),
    ("0 < 1 m", false),
    ("1 m == 0", false),
    ("0 m == 0", false),
    ("1 m != 0", false),
    ("1 m >= 0", false),
    ("1 m <= 0", false),
    ("-1 m < 0", false),
    ("1 km > 0", false),
    ("5 kg > 0", false),
    ("3 s > 0", false),
    ("2 m/s > 0", false),
    ("9.81 m/s^2 > 0", false),
    ("1 J > 0", false),
    ("1 W > 0", false),
    ("1 Pa > 0", false),
    ("1 A > 0", false),
    ("1 mol > 0", false),
    ("1 cd > 0", false),
    ("300 K > 0", false),
    ("x = 2 m\nx > 0", false),
    ("x = 2 m\nx >= 0", false),
    ("x = 0\ny = 2 m\ny > x", false),
    // ... and against other plain numbers
    ("1 m > 5", false),
    ("5 < 1 m", false),
    ("1 m == 1", false),
    ("2 kg >= 0.5", false),
    // with the zero written in a unit of the same dimension
    ("1 m > 0 m", true),
    ("1 m >= 0 km", true),
    ("0 m == 0 km", true),
    ("-1 m < 0 m", true),
    ("x = 2 m\nx > 0 m", true),
    ("300 K > 0 K", true),
    // temperature points
    ("20 degC > 0", false),
    ("-10 degC > 0", false),
    ("0 degC == 0", false),
    ("20 degF > 0", false),
    ("20 degC > 0 degC", true),
    ("-10 degC < 0 degC", true),
    ("0 degC == 273.15 K", true),
    ("20 degC > 0 K", true),
    // levels against plain numbers, zero or not
    ("5 dB > 0", false),
    ("-5 dB > 0", false),
    ("-5 dB < 0", false),
    ("0 dB == 0", false),
    ("20 dB == 100", false),
    ("20 dB > 10", false),
    ("1 < 20 dB", false),
    ("2 * 5 dB > 0", false),
    ("5 dB * 2 > 10", false),
    ("x = 5 dB\nx > 0", false),
    ("20 dB == 50 %", false),
    ("20 dB == 100 unity", false),
    ("3 pH > 0", false),
    // levels with levels, and a level read as its ratio
    ("20 dB == 20 dB", true),
    ("20 dB > 10 dB", true),
    ("-5 dB < 0 dB", true),
    ("(20 dB => unity) == 100", true),
    // angles, in either angle mode
    ("1 rad > 0", false),
    ("30 deg > 0", false),
    ("1 rad > 0.5", false),
    ("0 rad == 0", false),
    ("1 sr > 0", false),
    ("1 rad > 0 rad", true),
    ("30 deg > 0 deg", true),
    ("30 deg > 0.5 rad", true),
    // dimensionless ratios compare with plain numbers
    ("1 % > 0", true),
    ("50 % == 0.5", true),
    ("x = 1 %\nx > 0", true),
    ("1 unity > 0", true),
    ("3 ppm == 3e-6", true),
    ("(2 m / 1 m) > 0", true),
    ("(0.1 + 0.2) == 0.3", true),
    ("0 == 0", true),
    ("1 > 0", true),
];

#[test]
fn the_checker_and_the_interpreter_agree_on_every_comparison_in_both_angle_modes() {
    let mut disagreements = Vec::new();
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (source, accepted) in MATRIX {
            let checker = !checker_refuses(source);
            let runtime = !runtime_refuses(source);
            if checker != *accepted || runtime != *accepted {
                disagreements.push(format!("{mode:?} {source:?}: expected accepted={accepted}, checker={checker}, runtime={runtime}"));
            }
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

#[test]
fn a_zero_written_without_a_unit_says_to_write_it_with_one() {
    assert_eq!(
        runtime_error("1 m > 0"),
        "cannot compare 1.0 m with the plain number 0: a zero needs a unit, write `x > 0 m` (for a temperature point or \
         a level, each scale's zero is a different value: 0 degC is 273.15 K, 0 dB is a ratio of 1)"
    );
    assert!(runtime_error("0 < 20 degC").contains("write `x > 0 degC`"));
    assert!(runtime_error("30 deg > 0").contains("write `x > 0 deg`"));
    assert!(runtime_error("1 m > 5").contains("the plain number 5: a number compared with a quantity needs a unit, write `x > 5 m`"));
    let diagnostics = check_source("1 m > 0").unwrap();
    assert!(diagnostics[0].message.contains("a zero needs a unit"), "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "dimension-mismatch");
}

#[test]
fn a_level_against_a_plain_number_says_to_write_the_level_or_compare_the_ratio() {
    let why = runtime_error("20 dB == 100");
    assert!(
        why.starts_with("cannot compare 20.0 dB with the plain number 100: write the level with its unit (`x > 20 dB`) or compare the ratio explicitly"),
        "{why}"
    );
    assert!(runtime_error("0 dB == 0").contains("0 dB is a ratio of 1"));
    // a negated level prints without its unit (a defect of its own) but is still a level
    assert!(runtime_error("-5 dB > 0").contains("the level -5.0"));
    let diagnostics = check_source("20 dB == 100").unwrap();
    assert!(diagnostics[0].message.contains("write the level with its unit"), "{diagnostics:?}");
}

#[test]
fn assert_and_approx_follow_the_same_rule() {
    assert!(runtime_error("assert(1 m, 0)").contains("a zero needs a unit"));
    assert!(runtime_error("assert(20 dB, 100)").contains("write the level with its unit"));
    assert!(runtime_error("approx(20 dB, 100)").contains("write the level with its unit"));
    assert!(runtime_error("1 m ≈ 0").contains("a zero needs a unit"));
    assert!(eval_phs("assert(50 %, 0.5)").is_ok());
    assert!(eval_phs("assert(0 m, 0 km)").is_ok());
}

// ---- a condition is a Bool

/// Every place PHS reads a condition (and the operands of `and`, `or`, `not`, `assert`), with `{c}` for the
/// condition. Each is a complete program.
const CONTEXTS: &[&str] = &[
    "if {c} then 1 else 0",
    "{c} ? 1 : 0",
    "ternary({c}, 1, 0)",
    "while {c} {\n  y = 1\n}",
    "if {c} then return 1",
    "fn g(x) =\n  if {c} then return 1\n  2\ng(0)",
    "@requires({c}, \"must hold\")\nfn h(m) = m\nh(1)",
    "@ensures({c}, \"must hold\")\nfn h(m) = m\nh(1)",
    "{c} and True",
    "True and {c}",
    "{c} or False",
    "False or {c}",
    "not {c}",
    "assert({c})",
    "assert({c}, \"must hold\")",
];

/// `(condition, what the error must say)`: every one is refused in every context.
const NOT_CONDITIONS: &[(&str, &str)] = &[
    ("1", "got a Number (1.0): a number is not a condition; compare it, e.g. `x != 0`"),
    ("0", "got a Number (0.0)"),
    ("-5", "got a Number (-5.0)"),
    ("1e-16", "got a Number"),
    ("((1e308 * 10) - (1e308 * 10))", "got a Number (NaN"),
    ("1 m", "got a Quantity (1.0 m): a quantity is not a condition; compare it, e.g. `x != 0 m`"),
    ("0 m", "got a Quantity (0.0 m)"),
    ("1e-16 km", "got a Quantity"),
    ("-0.0 m", "got a Quantity"),
    ("0 degC", "e.g. `x != 0 degC`"),
    ("20 dB", "e.g. `x != 0 dB`"),
    ("\"true\"", "got a String (\"true\"): text is not a condition; the Bool literals are True and False"),
    ("\"false\"", "got a String (\"false\")"),
    ("\"True\"", "got a String (\"True\")"),
    ("\"\"", "got a String (\"\"): text is not a condition; compare it, e.g. `s == \"\"`"),
    ("\"1\"", "got a String (\"1\")"),
    ("vector(1, 2)", "got a Vector: a Vector is not a condition"),
    ("(0 .. 3)", "got a Range: a Range is not a condition"),
    ("(20 m +/- 1 sigma)", "got a SigmaBound: a SigmaBound is not a condition"),
    ("true", "`true` has no value; the Bool literals are True and False"),
    ("false", "`false` has no value; the Bool literals are True and False"),
    ("n", "`n` has no value; the Bool literals are True and False"),
];

/// The value of the last statement, as text.
fn last(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => values.last().map(|v| v.to_string()).unwrap_or_default(),
        Err(e) => panic!("{source:?} failed: {e}"),
    }
}

#[test]
fn only_a_bool_is_a_condition_in_every_context() {
    let mut wrong = Vec::new();
    for context in CONTEXTS {
        for (condition, says) in NOT_CONDITIONS {
            let source = context.replace("{c}", condition);
            match eval_phs(&source) {
                Ok(values) => wrong.push(format!("{source:?} was answered: {:?}", values.last())),
                Err(e) if !e.to_string().contains(says) => wrong.push(format!("{source:?}: {e}")),
                Err(_) => {}
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_bool_is_a_condition_in_every_context() {
    for context in CONTEXTS.iter().filter(|c| !c.starts_with("while")) {
        for condition in ["True", "(1 < 2)", "(1 m > 0 m)", "(\"a\" == \"a\")", "(True and not False)"] {
            let source = context.replace("{c}", condition);
            eval_phs(&source).unwrap_or_else(|e| panic!("{source:?}: {e}"));
        }
    }
    assert_eq!(last("if True then 1 else 0"), "1.0");
    assert_eq!(last("if 1 m > 0 m then 1 else 0"), "1.0");
    assert_eq!(last("if 2 < 1 then 1 else 0"), "0.0");
    assert_eq!(last("(1 m != 0 m) ? \"nonzero\" : \"zero\""), "nonzero");
    assert_eq!(last("x = 5\nwhile x != 0 {\n  x = x - 1\n}\nx"), "0.0");
    assert_eq!(last("while False {\n  y = 1\n}\n2"), "2.0");
}

#[test]
fn every_kind_of_value_that_is_not_a_bool_is_named() {
    let refusal = |source: &str| eval_phs(source).expect_err(source).to_string();
    assert!(refusal("f(x) = x\nif f then 1 else 0").contains("got a Function: a Function is not a condition"));
    assert!(refusal("x = assert(True)\nif x then 1 else 0").contains("got None"));
    assert!(refusal("use solve from calc\neq = solve(\"V = R * I\", \"R\")\nif eq then 1 else 0").contains("got an Equation"));
    // `while true` read the text "true" and ran to the loop guard; `while "1"` did too
    assert!(refusal("i = 0\nwhile true {\n  i = i + 1\n}").contains("`true` has no value"));
    assert!(refusal("while \"1\" {\n  y = 1\n}").contains("got a String (\"1\")"));
    // a name that is bound to text is text, not a name with no value
    assert!(refusal("s = \"on\"\nif s then 1 else 0").contains("got a String (\"on\")"));
}

#[test]
fn the_logical_operators_say_which_side_is_not_a_bool() {
    let refusal = |source: &str| eval_phs(source).expect_err(source).to_string();
    assert!(refusal("1 and True").starts_with("the left side of `and` needs a Bool (True or False), got a Number"));
    assert!(refusal("True and 1 m").starts_with("the right side of `and` needs a Bool"));
    assert!(refusal("False or \"a\"").starts_with("the right side of `or` needs a Bool"));
    assert!(refusal("not 1").starts_with("`not` needs a Bool"));
    // short-circuit: the right side of `False and _` is never read
    assert_eq!(last("False and 1"), "False");
    assert_eq!(last("True or \"a\""), "True");
}
