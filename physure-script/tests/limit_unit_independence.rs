//! `limit` decides the same whatever the unit the point is written in (audit T11): the first step is taken in the
//! coherent unit of the variable's dimension, so `0 m`, `0 mm`, `0 km` and `0 um` are the same problem. Each
//! expression is written in `S = x/(1 B)`, `B` the coherent unit of the group, so that the physical limit is the same
//! in every unit; every cell must give the outcome of the coherent unit (a value to 9 digits, or a refusal).
//!
//! Before the fix the first step was 1/8 of the unit the point was written in: in `km` it was wider than the feature
//! of the expression (the extrapolation did not settle, or the hole `S/(S (1 + 1e6 S^2))` grew and was answered
//! `inf`), in `um` so narrow that rounding swallowed the change of the variable.

use physure_core::RationalUnit;
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// "ok <value>" or "refused" for `limit(expr, "x", point[, side])`; values to 9 digits.
fn outcome(expr: &str, point: &str, side: Option<&str>) -> String {
    let side = side.map(|s| format!(", \"{s}\"")).unwrap_or_default();
    match eval_phs(&format!("use * from calc\nlimit(\"{expr}\", \"x\", {point}{side})")) {
        Ok(values) => match values.last() {
            Some(PhsValue::Number(n)) => format!("ok {n:.9e}"),
            // a ratio such as `m/mm` (the answer when the variable was a quantity of the point's own unit)
            Some(PhsValue::Quantity(q)) if q.unit.dimensions.is_empty() && q.value.std_dev() == 0.0 => {
                let plain = q.convert_to(&RationalUnit::dimensionless()).expect("a ratio is a plain number");
                format!("ok {:.9e}", plain.value.mean())
            }
            other => panic!("limit({expr:?}, {point}) should be a plain number, it is {other:?}"),
        },
        Err(_) => "refused".to_string(),
    }
}

/// The expression of `S` with `S` replaced by `x/(1 base)`.
fn of(template: &str, base: &str) -> String {
    template.replace('S', &format!("(x/(1 {base}))"))
}

/// The point `p` (in the coherent unit) written in `unit`, `factor` being how many `unit` make one coherent unit.
fn written(p: f64, unit: &str, factor: f64) -> String {
    format!("{} {unit}", format!("{:.12e}", p * factor).parse::<f64>().expect("a number"))
}

/// Asserts that every unit of `units` gives the outcome of the first (the coherent one) for each expression.
fn same_in_every_unit(base: &str, units: &[(&str, f64)], family: &[(&str, f64, Option<&str>)]) {
    let mut differing = Vec::new();
    for &(template, p, side) in family {
        let expr = of(template, base);
        let outcomes: Vec<(&str, String)> =
            units.iter().map(|&(unit, factor)| (unit, outcome(&expr, &written(p, unit, factor), side))).collect();
        for (unit, o) in &outcomes[1..] {
            if *o != outcomes[0].1 {
                differing.push(format!("{expr} at {p} {base}: {} in {base}, {o} in {unit}", outcomes[0].1));
            }
        }
    }
    assert!(differing.is_empty(), "answers that depend on the unit of the point:\n{}", differing.join("\n"));
}

/// `sin(S)/S` and the hole at 0 expose the bug in every group; the rest are the measured family.
const AT_ZERO: [(&str, f64, Option<&str>); 2] = [("sin(S)/S", 0.0, None), ("S/(S*(1+1e6*S^2))", 0.0, None)];

#[test]
fn a_limit_is_decided_the_same_in_every_unit_of_a_length() {
    let family = [
        AT_ZERO[0],
        AT_ZERO[1],
        ("(1-cos(S))/S^2", 0.0, None),
        ("S*ln(S)", 0.0, Some("+")),
        ("sin(1e-6*S)/S", 0.0, None),
        ("(exp(S)-1)/S", 0.0, None),
        ("abs(S)/S", 0.0, None),
        ("1/S^2", 0.0, None),
        ("sin(S-1)/(S-1)", 1.0, None),
        ("sin(S+3)/(S+3)", -3.0, None),
    ];
    same_in_every_unit("m", &[("m", 1.0), ("mm", 1e3), ("km", 1e-3), ("um", 1e6), ("Mm", 1e-6)], &family);
}

#[test]
fn a_limit_is_decided_the_same_in_every_unit_of_a_time() {
    same_in_every_unit("s", &[("s", 1.0), ("ms", 1e3), ("us", 1e6)], &AT_ZERO);
}

#[test]
fn a_limit_is_decided_the_same_in_every_unit_of_an_angle() {
    same_in_every_unit("rad", &[("rad", 1.0), ("mrad", 1e3), ("deg", 180.0 / std::f64::consts::PI)], &AT_ZERO);
}

#[test]
fn a_removable_hole_is_never_answered_infinite_in_a_large_unit() {
    let hole = of("S/(S*(1+1e6*S^2))", "m");
    let in_metres = outcome(&hole, "0 m", None);
    for point in ["0 km", "0 Mm"] {
        let o = outcome(&hole, point, None);
        assert_ne!(o, "ok inf", "{hole} at {point}: the limit is 1, not inf");
        assert_eq!(o, in_metres, "{hole} at {point}");
    }
    // the hole is 1e-3 m wide: in metres it is refused (the extrapolation does not settle); an answer must be 1
    if let Some(value) = in_metres.strip_prefix("ok ") {
        let value: f64 = value.parse().expect("a number");
        assert!((value - 1.0).abs() <= 1e-9, "{hole} at 0 m: {value}");
    }
}
