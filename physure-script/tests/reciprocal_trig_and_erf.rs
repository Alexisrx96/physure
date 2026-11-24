//! `sec`, `csc`, `cot` and `erf` are core functions of PHS (audit T4): the symbolic engine produced them, and the
//! interpreter did not know them, so `sec(1)` was "Undefined function" and an antiderivative that used them could not
//! be confirmed by the quadrature. Every expected value is a closed form or a std f64 value, never the engine's output.

use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// The value of the last statement of `program`, as a plain number or the mean of a quantity.
fn mean(program: &str) -> f64 {
    let values = eval_phs(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    match values.last().expect("a value") {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) => q.value.mean(),
        other => panic!("{program:?} should be a number, it is {other:?}"),
    }
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

fn assert_close(actual: f64, expected: f64, rel: f64, what: &str) {
    assert!((actual - expected).abs() <= rel * expected.abs(), "{what}: {actual} vs {expected}");
}

#[test]
fn sec_csc_cot_and_erf_of_a_plain_number() {
    assert_close(mean("sec(1)"), 1.8508157176809255, 1e-14, "sec(1)");
    assert_close(mean("csc(1)"), 1.1883951057781212, 1e-14, "csc(1)");
    assert_close(mean("cot(1)"), 0.6420926159343306, 1e-14, "cot(1)");
    assert_close(mean("erf(1)"), 0.8427007929497149, 1e-15, "erf(1)");
}

/// A literal is a dimensionless quantity; the index of a range is a bare number, which takes the f64 path.
#[test]
fn a_bare_number_takes_the_number_path() {
    for (name, expected) in [("sec", 1.0 / 1.0_f64.cos()), ("csc", 1.0 / 1.0_f64.sin()), ("cot", 1.0 / 1.0_f64.tan()), ("erf", 0.8427007929497149)] {
        let program = format!("for i in 1..2 {{ {name}(i) }}");
        match eval_phs(&program).unwrap().last() {
            Some(PhsValue::Vector(v)) => match v.first() {
                Some(PhsValue::Number(n)) => assert_close(*n, expected, 1e-15, &program),
                other => panic!("{program}: {other:?}"),
            },
            other => panic!("{program}: {other:?}"),
        }
    }
    assert!(refused("for i in 0..1 { cot(i) }").contains("cot of 0 is undefined"));
    assert!(refused("for i in 0..1 { csc(i) }").contains("csc of 0 is undefined"));
}

#[test]
fn sec_csc_and_cot_take_an_angle_and_a_length_is_refused() {
    assert_close(mean("sec(60 deg)"), 2.0, 1e-12, "sec(60 deg)");
    assert_close(mean("csc(30 deg)"), 2.0, 1e-12, "csc(30 deg)");
    assert_close(mean("cot(45 deg)"), 1.0, 1e-12, "cot(45 deg)");
    assert!(refused("sec(1 m)").contains("sec"));
    assert!(refused("csc(1 m)").contains("csc"));
    assert!(refused("cot(1 m)").contains("cot"));
}

#[test]
fn cot_and_csc_of_zero_are_refused_not_infinite() {
    assert!(refused("cot(0)").contains("cot of 0 is undefined"));
    assert!(refused("csc(0)").contains("csc of 0 is undefined"));
    assert!(refused("cot(0 deg)").contains("cot of 0 is undefined"));
}

#[test]
fn erf_needs_a_dimensionless_value() {
    assert!(refused("erf(1 rad)").contains("erf"));
    assert!(refused("erf(1 m)").contains("erf"));
    assert_close(mean("erf(50 %)"), 0.5204998778130465, 1e-15, "erf(50 %)");
}

#[test]
fn the_uncertainty_follows_the_derivative() {
    let std_dev = |program: &str| match eval_phs(program).unwrap().last() {
        Some(PhsValue::Quantity(q)) => q.value.std_dev(),
        other => panic!("{program:?}: {other:?}"),
    };
    let x = 0.5_f64;
    assert_close(std_dev("sec(0.5 +/- 0.01)"), 0.01 * x.tan() / x.cos(), 1e-12, "sec");
    assert_close(std_dev("cot(0.5 +/- 0.01)"), 0.01 / x.sin().powi(2), 1e-12, "cot");
    assert_close(std_dev("erf(0.5 +/- 0.01)"), 0.01 * std::f64::consts::FRAC_2_SQRT_PI * (-0.25_f64).exp(), 1e-12, "erf");
}
