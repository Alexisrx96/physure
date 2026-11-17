//! `limit` at small scales (the review of audit T11): a bounded function seen from far off grows like an infinity,
//! and a step wider than the point itself crosses the edge of a domain at 0. A wrong answer with confident units is
//! worse than a refusal, so every case here answers right or is refused, never `inf` for a bounded function and never
//! "does not exist" for a limit that exists. Genuine infinities are still answered.

use physure_core::RationalUnit;
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// The answer of `limit(expr, "x", point[, side])` as a plain number, or `Err(message)` when it is refused.
fn limit(expr: &str, point: &str, side: Option<&str>) -> Result<f64, String> {
    let side = side.map(|s| format!(", \"{s}\"")).unwrap_or_default();
    let program = format!("use * from calc\nlimit(\"{expr}\", \"x\", {point}{side})");
    match eval_phs(&program) {
        Ok(values) => match values.last() {
            Some(PhsValue::Number(n)) => Ok(*n),
            Some(PhsValue::Quantity(q)) if q.unit.dimensions.is_empty() && q.value.std_dev() == 0.0 => {
                Ok(q.convert_to(&RationalUnit::dimensionless()).expect("a ratio is a plain number").value.mean())
            }
            other => panic!("{program:?} should be a plain number, it is {other:?}"),
        },
        Err(e) => Err(e.to_string()),
    }
}

/// Answered 1 or refused, never another value (an `inf` included).
fn one_or_refused(expr: &str, point: &str) {
    if let Ok(value) = limit(expr, point, None) {
        assert!((value - 1.0).abs() <= 1e-9, "limit({expr:?}) at {point} is 1, not {value}");
    }
}

#[test]
fn a_bounded_function_seen_from_far_off_is_not_answered_infinite() {
    for (expr, point) in [
        ("x/(x*(1+(x/(1 nm))^2))", "0 nm"),
        ("x/(x*(1+(x/(1 um))^2))", "0 um"),
        ("x/(x*(1+1e12*(x/(1 m))^2))", "0 nm"),
        ("x/(x*(1+(x/(1 mg))^2))", "0 mg"),
        ("x/(x*(1+1e9*x^2))", "0"),
    ] {
        one_or_refused(expr, point);
    }
    for unit in ["m", "mm", "km", "um", "Mm"] {
        one_or_refused("(x/(1 m))/((x/(1 m))*(1+1e6*(x/(1 m))^2))", &format!("0 {unit}"));
    }
}

#[test]
fn a_genuine_infinity_is_still_answered() {
    for (expr, point, side, expected) in [
        ("1/x", "0", Some("+"), f64::INFINITY),
        ("1/x", "0", Some("-"), f64::NEG_INFINITY),
        ("1/x^2", "0", None, f64::INFINITY),
        ("ln(x)", "0", Some("+"), f64::NEG_INFINITY),
        ("1/sqrt(x)", "0", Some("+"), f64::INFINITY),
        ("exp(1/x)", "0", Some("+"), f64::INFINITY),
        ("1/(x-2)^2", "2", None, f64::INFINITY),
        ("1/(x-2)", "2", Some("+"), f64::INFINITY),
        ("1/(x/(1 m))^2", "0 km", None, f64::INFINITY),
        ("1/(x/(1 m))^2", "0 um", None, f64::INFINITY),
        ("1/(x/(1 m)-0.001)^2", "1 mm", None, f64::INFINITY),
        ("ln(x/(1 m))", "0 nm", Some("+"), f64::NEG_INFINITY),
    ] {
        assert_eq!(limit(expr, point, side), Ok(expected), "limit({expr:?}) at {point} {side:?}");
    }
}

#[test]
fn a_limit_next_to_a_small_point_is_not_said_not_to_exist() {
    for (expr, point) in [
        ("ln(x/(2 mL))/(x/(2 mL)-1)", "2 mL"),
        ("ln(x/(5 mg))/(x/(5 mg)-1)", "5 mg"),
        ("ln(x/(3 um^2))/(x/(3 um^2)-1)", "3 um^2"),
        ("ln(x/(1 g))/(x/(1 g)-1)", "1 g"),
    ] {
        let value = limit(expr, point, None).unwrap_or_else(|e| panic!("limit({expr:?}) at {point} is 1: {e}"));
        assert!((value - 1.0).abs() <= 1e-9, "limit({expr:?}) at {point} is 1, not {value}");
    }
}
