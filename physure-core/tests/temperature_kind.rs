//! The third state of a temperature (spec `2026-11-20-temperature-kind-design.md`): a unit without a zero point is a
//! known point, a known interval, or of unknown kind. Every row of the table of section 2.3, both operand orders, the
//! conversions that mark a point, the consumers that read the kind, and the programs of section 1.

use physure_core::quantity::Quantity;
use physure_core::units::{Parser, RationalUnit};

fn q(mean: f64, unit: &str) -> Quantity {
    let unit = if unit.is_empty() {
        RationalUnit::dimensionless()
    } else {
        Parser::parse_expression(unit).unwrap_or_else(|e| panic!("{unit}: {e}"))
    };
    Quantity::new_scalar(mean, 0.0, unit, None, None)
}

fn unit(text: &str) -> RationalUnit {
    Parser::parse_expression(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn ok(result: physure_core::error::PhysureResult<Quantity>, what: &str) -> Quantity {
    result.unwrap_or_else(|e| panic!("{what}: {e}"))
}

/// `20 degC => K`: a known absolute temperature, 293.15 K.
fn point_k() -> Quantity {
    ok(q(20.0, "degC").convert_to(&unit("K")), "20 degC => K")
}

/// `30 degC => K`.
fn later_k() -> Quantity {
    ok(q(30.0, "degC").convert_to(&unit("K")), "30 degC => K")
}

fn interval() -> Quantity {
    q(5.0, "delta_degC")
}

const COMPARE: &str = "cannot compare a temperature point";

fn refused(a: &Quantity, b: &Quantity) -> String {
    a.phs_compare(b, "<").expect_err(&format!("{a} < {b} should be refused")).to_string()
}

/// `x` is a known point: refused against an interval in both orders, compared with a point.
fn assert_point(x: &Quantity, what: &str) {
    assert!(x.unit.is_point() && !x.unit.is_interval(), "{what} = {x} should be a point");
    for (a, b) in [(x, &interval()), (&interval(), x)] {
        let error = refused(a, b);
        assert!(error.starts_with(COMPARE), "{what}: {error}");
    }
    x.phs_compare(&q(20.0, "degC"), "<").unwrap_or_else(|e| panic!("{what} against a point: {e}"));
}

/// `x` is a known interval: refused against a point (`20 degC` and a marked `K`), compared with an interval.
fn assert_interval(x: &Quantity, what: &str) {
    assert!(x.unit.is_interval() && !x.unit.is_point(), "{what} = {x} should be an interval");
    for point in [q(20.0, "degC"), point_k()] {
        for (a, b) in [(x, &point), (&point, x)] {
            let error = refused(a, b);
            assert!(error.starts_with(COMPARE), "{what}: {error}");
        }
    }
    x.phs_compare(&interval(), "<").unwrap_or_else(|e| panic!("{what} against an interval: {e}"));
}

/// `x` is of unknown kind: it compares with a point, a marked point and an interval.
fn assert_unknown(x: &Quantity, what: &str) {
    assert!(!x.unit.is_point() && !x.unit.is_interval(), "{what} = {x} should be of unknown kind");
    for other in [q(20.0, "degC"), point_k(), interval()] {
        x.phs_compare(&other, "<").unwrap_or_else(|e| panic!("{what} < {other}: {e}"));
        other.phs_compare(x, "<").unwrap_or_else(|e| panic!("{other} < {what}: {e}"));
    }
}

fn assert_shown(x: &Quantity, shown: &str) {
    assert_eq!(x.to_string(), shown);
}

#[test]
fn a_kelvin_written_by_hand_is_of_unknown_kind_and_a_converted_point_is_a_point() {
    for name in ["K", "degR", "mK"] {
        assert_unknown(&q(300.0, name), name);
    }
    let marked = point_k();
    assert_shown(&marked, "293.15 K");
    assert_point(&marked, "20 degC => K");
    let rankine = ok(q(68.0, "degF").convert_to(&unit("degR")), "68 degF => degR");
    assert_shown(&rankine, "527.67 degR");
    assert_point(&rankine, "68 degF => degR");
    // a point stays one in another unit without a zero point
    assert_point(&ok(marked.convert_to(&unit("degR")), "T => degR"), "(20 degC => K) => degR");
    assert_point(&ok(marked.convert_to(&unit("K")), "T => K"), "(20 degC => K) => K");
    assert_shown(&ok(marked.convert_to(&unit("degC")), "T => degC"), "20.0 degC");
    // a degC is a point by its zero point
    assert!(q(20.0, "degC").unit.is_point());
}

#[test]
fn the_kind_is_not_part_of_the_identity_of_a_unit() {
    let (marked, plain) = (point_k().unit, unit("K"));
    assert_eq!(marked, plain);
    let hash = |unit: &RationalUnit| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        unit.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&marked), hash(&plain));
    assert!(point_k().phs_eq(&q(293.15, "K")).unwrap());
}

/// Section 2.3, rows 1-2: a point moved by an interval is a point, two points make an interval.
#[test]
fn a_point_and_an_interval_make_a_point_and_two_points_an_interval() {
    let (t, d) = (point_k(), interval());
    let sum = ok(t.add(&d), "T + d");
    assert_shown(&sum, "298.15 K");
    assert_point(&sum, "T + d");
    let reversed = ok(d.add(&t), "d + T");
    assert_point(&reversed, "d + T");
    // a point is written in the point's unit, as `5 delta_degC + 20 degC` is in degC
    assert_shown(&reversed, "298.15 K");
    let lowered = ok(t.sub(&d), "T - d");
    assert_shown(&lowered, "288.15 K");
    assert_point(&lowered, "T - d");
    // an interval of another scale moves a rankine point in its own unit
    let rankine = ok(q(68.0, "degF").convert_to(&unit("degR")), "68 degF => degR");
    let warmer = ok(q(9.0, "delta_degF").add(&rankine), "9 delta_degF + T");
    assert_shown(&warmer, "536.67 degR");
    assert_point(&warmer, "9 delta_degF + (68 degF => degR)");
    let difference = ok(later_k().sub(&t), "T2 - T1");
    assert_shown(&difference, "10.0 K");
    assert_interval(&difference, "T2 - T1");
}

/// Rows 3-5: intervals with intervals, and anything with a kelvin of unknown kind.
#[test]
fn intervals_make_intervals_and_an_unknown_operand_makes_an_unknown_kind() {
    let (t, d, k) = (point_k(), interval(), q(1.0, "K"));
    assert_interval(&ok(d.add(&d), "d + d"), "d + d");
    assert_interval(&ok(d.sub(&q(9.0, "delta_degF")), "d - f"), "d - f");
    let difference = ok(later_k().sub(&t), "T2 - T1");
    assert_interval(&ok(difference.add(&d), "(T2 - T1) + d"), "(T2 - T1) + d");
    assert_interval(&ok(d.sub(&difference), "d - (T2 - T1)"), "d - (T2 - T1)");
    for (result, what) in [
        (t.add(&k), "T + 1 K"),
        (k.add(&t), "1 K + T"),
        (t.sub(&k), "T - 1 K"),
        (k.sub(&t), "1 K - T"),
        (d.add(&k), "d + 1 K"),
        (k.add(&d), "1 K + d"),
        (d.sub(&k), "d - 1 K"),
        (k.sub(&d), "1 K - d"),
        (k.add(&k), "1 K + 1 K"),
        (k.sub(&k), "1 K - 1 K"),
    ] {
        assert_unknown(&ok(result, what), what);
    }
    // printed as before: in the unit of the left operand
    assert_shown(&ok(d.add(&k), "d + K"), "6.0 delta_degC");
    assert_shown(&ok(k.add(&d), "K + d"), "6.0 K");
    // a hand-written kelvin plus an interval is still a place on the scale when it is one (`300 K + 5 delta_degC`)
    assert_shown(&ok(ok(q(300.0, "K").add(&d), "300 K + d").convert_to(&unit("degC")), "=> degC"), "31.85 degC");
}

/// Row 6: two points without a zero point add to a number of unknown kind, so an average is not refused.
#[test]
fn two_points_without_a_zero_point_add_to_an_unknown_kind_and_an_average_works() {
    let (t1, t2) = (point_k(), later_k());
    let sum = ok(t1.add(&t2), "T1 + T2");
    assert_unknown(&sum, "T1 + T2");
    let average = ok(sum.div(&q(2.0, "")), "(T1 + T2) / 2");
    assert_shown(&average, "298.15 K");
    assert_unknown(&average, "(T1 + T2) / 2");
    assert_shown(&ok(average.convert_to(&unit("degC")), "=> degC"), "25.0 degC");
    assert!(average.phs_compare(&q(10.0, "degC"), ">").unwrap());
    // two points with a zero point are still refused
    assert!(q(20.0, "degC").add(&q(30.0, "degC")).is_err());
}

/// Rows 7-8: a ratio scales an interval into an interval, and a point into a number of unknown kind.
#[test]
fn a_ratio_keeps_an_interval_and_drops_a_point() {
    let (t, d) = (point_k(), interval());
    assert_interval(&ok(d.mul(&q(2.0, "")), "d * 2"), "d * 2");
    assert_interval(&ok(q(2.0, "").mul(&d), "2 * d"), "2 * d");
    assert_interval(&ok(d.div(&q(50.0, "%")), "d / 50 %"), "d / 50 %");
    for (result, what) in [
        (t.mul(&q(2.0, "")), "T * 2"),
        (q(2.0, "").mul(&t), "2 * T"),
        (t.div(&q(2.0, "")), "T / 2"),
        (t.mul(&q(50.0, "%")), "T * 50 %"),
        (q(-1.0, "").mul(&t), "-1 * T"),
    ] {
        assert_unknown(&ok(result, what), what);
    }
    assert_shown(&ok(t.mul(&q(2.0, "")), "T * 2"), "586.3 K");
    // the first power is the unit itself
    assert_point(&ok(t.pow(1.0), "T^1"), "T^1");
}

/// Row 9: a step through another dimension drops the kind, as it drops the interval flag.
#[test]
fn another_dimension_drops_the_kind() {
    let t = point_k();
    assert!(!ok(t.div(&q(1.0, "m")), "T / m").unit.is_point());
    assert_unknown(&ok(ok(t.mul(&q(1.0, "s")), "T * s").div(&q(1.0, "s")), "T * s / s"), "T * s / s");
    assert_unknown(&ok(ok(t.pow(2.0), "T^2").sqrt(), "sqrt(T^2)"), "sqrt(T^2)");
    // R*T: a product with a temperature of its own dimension, in both forms
    let r = q(8.314, "J/(mol*K)");
    let from_point = ok(r.mul(&q(20.0, "degC")), "R * 20 degC");
    let from_marked = ok(r.mul(&t), "R * T");
    assert!((from_point.canonical_magnitude() - from_marked.canonical_magnitude()).abs() < 1e-9);
    assert!(!from_marked.unit.is_point());
}

/// Point 4: a point (written with a zero point, or marked) does not convert to an interval unit; a kelvin of unknown
/// kind converts both ways, and an interval reads as a kelvin of unknown kind.
#[test]
fn a_marked_point_does_not_convert_to_an_interval_unit() {
    let error = point_k().convert_to(&unit("delta_degC")).unwrap_err().to_string();
    assert!(error.starts_with("cannot convert a temperature point (`293.15 K`) to a temperature interval (`delta_degC`)"), "{error}");
    assert!(ok(q(68.0, "degF").convert_to(&unit("degR")), "68 degF => degR").convert_to(&unit("delta_degF")).is_err());
    assert_shown(&ok(q(300.0, "K").convert_to(&unit("delta_degC")), "300 K => delta_degC"), "300.0 delta_degC");
    assert_shown(&ok(q(5.0, "delta_degC").convert_to(&unit("K")), "5 delta_degC => K"), "5.0 K");
    assert_unknown(&ok(q(5.0, "delta_degC").convert_to(&unit("K")), "=> K"), "5 delta_degC => K");
    // the plan a binding applies with its own numbers says the same of a unit marked a point
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    physure_core::units::conf::with_conf_registry(|catalog| {
        let mut marked = atomic("K");
        marked.temperature = physure_core::units::TemperatureKind::Point;
        assert!(catalog.conversion(&marked, &atomic("delta_degC")).is_err());
        assert!(catalog.conversion(&marked, &atomic("degC")).is_ok());
        assert!(catalog.conversion(&atomic("K"), &atomic("delta_degC")).is_ok());
        let expanded = catalog.expand(&marked).unwrap();
        assert!(expanded.is_point() && expanded.point_against_interval(&catalog.expand(&atomic("delta_degC")).unwrap()));
    });
}

/// Section 1: the holes a forgotten absolute temperature left.
#[test]
fn the_holes_of_a_forgotten_point() {
    let t = point_k();
    assert!(refused(&t, &interval()).contains("cannot compare a temperature point (`293.15 K`) with a temperature interval (`5.0 delta_degC`)"));
    let moved = ok(t.add(&interval()), "T + d");
    assert!(refused(&moved, &interval()).starts_with(COMPARE));
    assert!(t.convert_to(&unit("delta_degC")).is_err());
    // the programs that must keep working
    let t1 = q(293.15, "K");
    assert_shown(&ok(ok(t1.add(&interval()), "T1 + d").convert_to(&unit("degC")), "=> degC"), "25.0 degC");
    assert!(ok(t.sub(&interval()), "T - d").phs_compare(&q(10.0, "degC"), ">").unwrap());
    let heat = ok(ok(q(2.0, "kg").mul(&q(4186.0, "J/(kg*K)")), "m * c").mul(&ok(later_k().sub(&t), "T2 - T1")), "Q");
    assert_shown(&heat, "83720.0 J");
    let rate = ok(ok(later_k().sub(&t), "T2 - T1").div(&q(1.0, "s")), "(T2 - T1) / 1 s");
    assert!((rate.canonical_magnitude() - 10.0).abs() < 1e-9);
    assert!(ok(later_k().sub(&t), "T2 - T1").phs_compare(&q(5.0, "K"), ">").unwrap());
}
