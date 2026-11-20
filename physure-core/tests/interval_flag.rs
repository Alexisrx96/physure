//! The `interval` flag of the catalog (spec `2026-11-20-interval-flag-design.md`): a temperature interval is a unit
//! the catalog flags, not a unit whose name starts with `delta_`. Every expression of section 2 points 3 and 4, both
//! operand orders: which results are intervals (refused against a temperature point), which are points, which are
//! neutral (`K`, compared with both), and which conversions are refused.

use std::collections::HashMap;

use physure_core::quantity::Quantity;
use physure_core::units::conf::{build_registry_with, extend_registry};
use physure_core::units::{with_catalog, Parser, RationalUnit, UnitRegistry};

fn q(mean: f64, unit: &str) -> Quantity {
    let unit = if unit.is_empty() {
        RationalUnit::dimensionless()
    } else {
        Parser::parse_expression(unit).unwrap_or_else(|e| panic!("{unit}: {e}"))
    };
    Quantity::new_scalar(mean, 0.0, unit, None, None)
}

fn ok(result: physure_core::error::PhysureResult<Quantity>, what: &str) -> Quantity {
    result.unwrap_or_else(|e| panic!("{what}: {e}"))
}

/// `x` is a temperature interval: the core refuses it against `20 degC`, in both orders, with the words of the rule.
fn assert_interval(x: &Quantity, what: &str) {
    assert!(x.unit.is_interval(), "{what} = {x} should be an interval");
    let point = q(20.0, "degC");
    for (a, b) in [(x, &point), (&point, x)] {
        let error = a.phs_compare(b, "<").expect_err(&format!("{what}: {a} < {b} should be refused"));
        assert!(error.to_string().starts_with("cannot compare a temperature point"), "{what}: {error}");
    }
}

/// `x` is not an interval: it compares with `20 degC` (a point, or a neutral `K`).
fn assert_not_interval(x: &Quantity, what: &str) {
    assert!(!x.unit.is_interval(), "{what} = {x} should not be an interval");
    let point = q(20.0, "degC");
    for (a, b) in [(x, &point), (&point, x)] {
        a.phs_compare(b, "<").unwrap_or_else(|e| panic!("{what}: {a} < {b} should compare: {e}"));
    }
}

fn assert_shown(x: &Quantity, shown: &str) {
    assert_eq!(x.to_string(), shown);
}

#[test]
fn the_catalog_flags_the_delta_units_and_nothing_else() {
    for name in ["delta_celsius", "delta_degC", "degC_delta", "delta_fahrenheit", "delta_degF", "degF_delta", "kdelta_degC", "mdelta_degF"] {
        assert!(q(1.0, name).unit.is_interval(), "{name}");
    }
    for name in ["K", "kelvin", "mK", "degR", "rankine", "degC", "degF", "m", "percent"] {
        assert!(!q(1.0, name).unit.is_interval(), "{name}");
    }
}

#[test]
fn the_flag_is_not_part_of_the_identity_of_a_unit() {
    let (delta, kelvin) = (q(1.0, "delta_degC").unit, q(1.0, "K").unit);
    assert_eq!(delta, kelvin, "delta_degC is a kelvin: one scale, no zero point");
    let hash = |unit: &RationalUnit| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        unit.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&delta), hash(&kelvin));
    assert_eq!(delta.ratio_to(&kelvin), 1.0);
    assert!(q(5.0, "delta_degC").phs_eq(&q(5.0, "K")).unwrap());
    assert_shown(&ok(q(5.0, "delta_degC").convert_to(&kelvin), "=> K"), "5.0 K");
    assert_shown(&ok(q(10.0, "K").convert_to(&delta), "=> delta_degC"), "10.0 delta_degC");
}

#[test]
fn an_interval_times_or_over_a_plain_number_is_an_interval() {
    let d = q(5.0, "delta_degC");
    assert_interval(&ok(d.mul(&q(2.0, "")), "d * 2"), "d * 2");
    assert_interval(&ok(q(2.0, "").mul(&d), "2 * d"), "2 * d");
    assert_interval(&ok(d.div(&q(2.0, "")), "d / 2"), "d / 2");
    assert_interval(&ok(q(-1.0, "").mul(&d), "-d"), "-1 * d");
    // a dimensionless unit that is not a plain number: the scale changes, the kind does not (the name was lost here)
    assert_interval(&ok(d.mul(&q(50.0, "%")), "d * 50 %"), "d * 50 %");
    assert_interval(&ok(q(50.0, "%").mul(&d), "50 % * d"), "50 % * d");
    assert_interval(&ok(d.div(&q(50.0, "%")), "d / 50 %"), "d / 50 %");
    assert_interval(&ok(d.pow(1.0), "d^1"), "d^1");
    // the printed unit and value do not change
    assert_shown(&ok(d.mul(&q(2.0, "")), "d * 2"), "10.0 delta_degC");
}

#[test]
fn a_sum_of_intervals_is_an_interval_and_a_kelvin_with_an_interval_is_of_unknown_kind() {
    let (d, f, k) = (q(5.0, "delta_degC"), q(9.0, "delta_degF"), q(1.0, "K"));
    assert_interval(&ok(d.add(&d), "d + d"), "d + d");
    assert_interval(&ok(d.sub(&f), "d - f"), "d - f");
    assert_interval(&ok(f.add(&d), "f + d"), "f + d");
    // a plain K may be an absolute temperature (`20 degC => K`) or a difference: with an interval, the sum is of
    // unknown kind too, in either order, and compares with a point (300 K + 5 delta_degC is 31.85 degC)
    assert_not_interval(&ok(d.add(&k), "d + K"), "5 delta_degC + 1 K");
    assert_not_interval(&ok(k.add(&d), "K + d"), "1 K + 5 delta_degC");
    assert_not_interval(&ok(d.sub(&k), "d - K"), "5 delta_degC - 1 K");
    assert_not_interval(&ok(k.sub(&d), "K - d"), "1 K - 5 delta_degC");
    // a difference of two points is a known interval: with an interval, an interval
    let difference = ok(q(20.0, "degC").sub(&q(10.0, "degC")), "20 degC - 10 degC");
    assert_interval(&ok(difference.add(&d), "(p - p) + d"), "(20 degC - 10 degC) + 5 delta_degC");
    assert_interval(&ok(d.add(&difference), "d + (p - p)"), "5 delta_degC + (20 degC - 10 degC)");
    assert_not_interval(&ok(difference.add(&k), "(p - p) + K"), "(20 degC - 10 degC) + 1 K");
    let absolute = ok(q(300.0, "K").add(&d), "300 K + d");
    assert_shown(&ok(absolute.convert_to(&q(1.0, "degC").unit), "=> degC"), "31.85 degC");
    // printed as before: in the unit of the left operand
    assert_shown(&ok(k.add(&d), "K + d"), "6.0 K");
    assert_shown(&ok(d.add(&k), "d + K"), "6.0 delta_degC");
    // a kelvin with a kelvin stays neutral
    assert_not_interval(&ok(k.add(&k), "K + K"), "1 K + 1 K");
}

#[test]
fn a_point_and_an_interval_or_two_points_combine_as_before_and_two_points_make_an_interval() {
    let (p, d) = (q(20.0, "degC"), q(5.0, "delta_degC"));
    let later = q(30.0, "degC");
    for (sum, what) in [(ok(p.add(&d), "p + d"), "p + d"), (ok(d.add(&p), "d + p"), "d + p"), (ok(p.sub(&d), "p - d"), "p - d")] {
        assert!(sum.unit.is_affine() && !sum.unit.is_interval(), "{what}: {sum}");
        sum.phs_compare(&later, "<").unwrap_or_else(|e| panic!("{what}: {e}"));
    }
    assert_shown(&ok(p.add(&d), "p + d"), "25.0 degC");
    assert_shown(&ok(d.add(&p), "d + p"), "25.0 degC");
    assert_shown(&ok(p.sub(&d), "p - d"), "15.0 degC");
    // a point minus a point is an interval, printed in K as before
    let difference = ok(later.sub(&p), "30 degC - 20 degC");
    assert_shown(&difference, "10.0 K");
    assert_interval(&difference, "30 degC - 20 degC");
    assert!(difference.phs_compare(&d, ">").unwrap(), "an interval compares with an interval");
    let fahrenheit = ok(q(68.0, "degF").sub(&q(32.0, "degF")), "68 degF - 32 degF");
    assert_interval(&fahrenheit, "68 degF - 32 degF");
    // the refusals of the point arithmetic are unchanged
    assert!(p.add(&later).is_err() && d.sub(&p).is_err());
}

/// Spec section 2 point 3: "the flag is dropped when the resulting dimension is not exactly the temperature dimension
/// to the first power". Each step decides, so a step through another dimension loses the kind; documented, and an
/// open line of the roadmap.
#[test]
fn a_unit_combined_with_another_dimension_is_no_longer_an_interval() {
    let d = q(5.0, "delta_degC");
    let gradient = ok(d.div(&q(1.0, "m")), "d / m");
    assert!(!gradient.unit.is_interval(), "{gradient}");
    assert_not_interval(&ok(gradient.mul(&q(1.0, "m")), "d / m * m"), "(d / m) * m");
    assert_not_interval(&ok(ok(d.mul(&q(1.0, "s")), "d * s").div(&q(1.0, "s")), "d * s / s"), "d * s / s");
    assert_not_interval(&ok(ok(d.pow(2.0), "d^2").sqrt(), "sqrt(d^2)"), "sqrt(d^2)");
    assert!(!ok(d.mul(&d), "d * d").unit.is_interval());
    assert!(!ok(d.div(&d), "d / d").unit.is_interval());
}

#[test]
fn a_kelvin_and_a_rankine_stay_neutral() {
    for unit in ["K", "degR", "mK"] {
        let x = q(300.0, unit);
        assert_not_interval(&x, unit);
        x.phs_compare(&q(5.0, "delta_degC"), ">").unwrap_or_else(|e| panic!("{unit} against an interval: {e}"));
    }
    assert_not_interval(&ok(q(300.0, "K").mul(&q(2.0, "")), "K * 2"), "300 K * 2");
}

const POINT_TO_INTERVAL: &str = "cannot convert a temperature point (`20.0 degC`) to a temperature interval (`delta_degC`)";

#[test]
fn a_point_converted_to_an_interval_unit_is_refused() {
    let error = q(20.0, "degC").convert_to(&q(1.0, "delta_degC").unit).unwrap_err().to_string();
    assert!(error.starts_with(POINT_TO_INTERVAL), "{error}");
    assert!(error.contains("a point says where, an interval says how far"), "{error}");
    assert!(error.contains("T2 - T1"), "{error}");
    for (point, interval) in [("degF", "delta_degF"), ("degC", "delta_fahrenheit"), ("degF", "degC_delta"), ("degC", "kdelta_degC")] {
        let refused = q(68.0, point).convert_to(&q(1.0, interval).unit);
        assert!(refused.is_err(), "{point} => {interval}: {refused:?}");
    }
    // through the plan a binding applies with its own numbers
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    physure_core::units::conf::with_conf_registry(|catalog| {
        assert!(catalog.conversion(&atomic("degC"), &atomic("delta_degC")).is_err());
        assert!(catalog.conversion(&atomic("delta_degC"), &atomic("degC")).is_err());
        assert!(catalog.conversion(&atomic("K"), &atomic("delta_degC")).is_ok());
        assert!(catalog.conversion(&atomic("delta_degC"), &atomic("K")).is_ok());
        assert!(catalog.conversion(&atomic("degC"), &atomic("K")).is_ok());
    });
}

#[test]
fn an_interval_converted_to_a_point_unit_is_refused() {
    let error = q(5.0, "delta_degC").convert_to(&q(1.0, "degC").unit).unwrap_err().to_string();
    assert!(error.starts_with("cannot convert a temperature interval (`5.0 delta_degC`) to a temperature point (`degC`)"), "{error}");
    assert!(q(9.0, "delta_degF").convert_to(&q(1.0, "degF").unit).is_err());
    let difference = ok(q(30.0, "degC").sub(&q(20.0, "degC")), "30 degC - 20 degC");
    assert!(difference.convert_to(&q(1.0, "degC").unit).is_err(), "a difference of two points is no point");
    assert_shown(&ok(difference.convert_to(&q(1.0, "delta_degC").unit), "=> delta_degC"), "10.0 delta_degC");
}

#[test]
fn a_kelvin_or_an_interval_converts_to_an_interval_unit_and_a_point_to_a_kelvin_as_before() {
    let delta_c = q(1.0, "delta_degC").unit;
    let converted = ok(q(10.0, "K").convert_to(&delta_c), "10 K => delta_degC");
    assert_shown(&converted, "10.0 delta_degC");
    assert_interval(&converted, "10 K => delta_degC");
    let from_f = ok(q(9.0, "delta_degF").convert_to(&delta_c), "9 delta_degF => delta_degC");
    assert!((from_f.value.mean() - 5.0).abs() < 1e-12);
    assert_interval(&from_f, "9 delta_degF => delta_degC");
    // `=> K` stays allowed: the advice of the comparison says what it reads
    assert_not_interval(&ok(q(5.0, "delta_degC").convert_to(&q(1.0, "K").unit), "5 delta_degC => K"), "5 delta_degC => K");
    assert_shown(&ok(q(25.0, "degC").convert_to(&q(1.0, "K").unit), "25 degC => K"), "298.15 K");
    assert_shown(&ok(q(300.0, "K").convert_to(&q(1.0, "degC").unit), "300 K => degC"), "26.85 degC");
}

fn custom(extra: &str) -> UnitRegistry {
    build_registry_with(extra).unwrap_or_else(|problems| panic!("{extra:?}: {problems:?}")).0
}

#[test]
fn a_unit_of_a_users_conf_flagged_interval_behaves_like_delta_degc() {
    let registry = custom("[Units]\ndT = 1.0, O, interval, [dT, dTs]\ndU = 1.0, O, [dU]\n");
    with_catalog(&registry, || {
        for name in ["dT", "dTs", "kdT"] {
            assert_interval(&q(5.0, name), name);
        }
        assert_interval(&ok(q(2.0, "dT").add(&q(5.0, "dT")), "dT + dT"), "2 dT + 5 dT");
        assert_not_interval(&ok(q(2.0, "dT").add(&q(1.0, "K")), "dT + K"), "2 dT + 1 K");
        assert!(q(20.0, "degC").convert_to(&q(1.0, "dT").unit).is_err());
        assert!(q(5.0, "dT").convert_to(&q(1.0, "degC").unit).is_err());
        // without the flag a scale-1 temperature unit is a kelvin under another name: neutral
        assert_not_interval(&q(5.0, "dU"), "dU");
    });
}

fn problems(registry: &mut UnitRegistry, text: &str) -> Vec<String> {
    extend_registry(registry, &mut HashMap::new(), text).err().unwrap_or_default()
}

#[test]
fn the_flag_on_a_point_a_level_or_a_base_unit_is_a_problem() {
    let mut registry = custom("");
    for line in ["dC = 1.0, 273.15, O, interval, [dC]", "dBi = Log, 10, 1.0, 1, interval, noprefix, [dBi]"] {
        let found = problems(&mut registry, &format!("[Units]\n{line}\n"));
        assert!(found.iter().any(|p| p.contains("interval")), "{line}: {found:?}");
    }
    // the first unit of a dimension is its base unit, the unit a difference of two points is written in
    let mut empty = UnitRegistry::new();
    let found = problems(&mut empty, "[Units]\nkelvin = 1.0, O, interval, [K]\n");
    assert!(found.iter().any(|p| p.contains("interval")), "{found:?}");
}

#[test]
fn the_facts_of_a_unit_say_whether_it_is_an_interval() {
    physure_core::units::conf::with_conf_registry(|registry| {
        for (name, interval) in [("delta_degC", true), ("degF_delta", true), ("kdelta_degC", true), ("K", false), ("degC", false), ("m", false)] {
            assert_eq!(registry.facts(name).unwrap().interval, interval, "{name}");
        }
        assert!(!registry.facts("dB*m").map(|facts| facts.interval).unwrap_or(false));
    });
}

#[test]
fn a_users_line_that_restates_an_interval_entry_without_the_flag_is_refused_and_the_entry_stays() {
    for line in [
        "delta_celsius = 1.0, O, [delta_degC, degC_delta, delta_celsius]",
        "delta_fahrenheit = 5/9, O, [delta_degF, degF_delta, delta_fahrenheit]",
        "delta_celsius = 1.0, O",
    ] {
        let mut registry = custom("");
        let found = problems(&mut registry, &format!("[Units]\n{line}\n"));
        assert!(found.iter().any(|p| p.contains("is a temperature interval: the line needs the `interval` flag")), "{line}: {found:?}");
        with_catalog(&registry, || assert_interval(&q(5.0, "delta_degC"), line));
    }
    // with the flag it is a definition like any other
    let registry = custom("[Units]\ndelta_celsius = 1.0, O, interval, [delta_degC, degC_delta, delta_celsius]\n");
    with_catalog(&registry, || assert_interval(&q(5.0, "delta_degC"), "restated with the flag"));
    // a user's own unit of another name is what its line says
    let registry = custom("[Units]\ndV = 1.0, O, [dV]\n");
    with_catalog(&registry, || assert_not_interval(&q(5.0, "dV"), "dV"));
}

#[test]
fn the_flag_needs_a_point_of_its_dimension() {
    let mut registry = custom("");
    let found = problems(&mut registry, "[Units]\nzm = 2.0, L, interval, [zm]\n");
    assert!(found.iter().any(|p| p.contains("no unit with a zero point")), "{found:?}");
    // in a text of its own, the point comes first
    let mut fresh = UnitRegistry::new();
    let found = problems(&mut fresh, "[Units]\nkelvin = 1.0, O, [K]\ndT = 1.0, O, interval, [dT]\ncelsius = 1.0, 273.15, O, [degC]\n");
    assert!(found.iter().any(|p| p.contains("no unit with a zero point")), "{found:?}");
    let mut ordered = UnitRegistry::new();
    let found = problems(&mut ordered, "[Units]\nkelvin = 1.0, O, [K]\ncelsius = 1.0, 273.15, O, [degC]\ndT = 1.0, O, interval, [dT]\n");
    assert!(found.is_empty(), "{found:?}");
    assert!(ordered.get_unit("dT").unwrap().is_interval());
}
