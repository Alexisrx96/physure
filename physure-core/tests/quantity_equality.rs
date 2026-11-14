//! What two quantities being "equal" means, at the Rust API (audit B10, B11, B16).
//!
//! `Quantity: PartialEq` is exact: the same unit and the same bits of the mean. The PHS meaning of `==` (and of
//! `assert(a, b)`) is `Quantity::phs_eq`: the canonical magnitudes agree within a relative `EQ_REL` of the larger,
//! whatever units the two operands are written in. No absolute epsilon anywhere: it made `1 pm == 2 pm` true and
//! `1 nm == 2 nm` false.

use physure_core::quantity::{Quantity, EQ_REL};
use physure_core::units::{Parser, RationalUnit};

fn q(mean: f64, unit: &str) -> Quantity {
    let unit = if unit.is_empty() {
        RationalUnit::dimensionless()
    } else {
        Parser::parse_expression(unit).unwrap_or_else(|e| panic!("{unit}: {e}"))
    };
    Quantity::new_scalar(mean, 0.0, unit, None, None)
}

fn eq(a: &Quantity, b: &Quantity) -> bool {
    a.phs_eq(b).unwrap_or_else(|e| panic!("{a} == {b}: {e}"))
}

#[test]
fn partial_eq_is_exact() {
    assert_eq!(q(1.0, "m"), q(1.0, "m"));
    assert_eq!(q(1.0, "m"), q(1.0, "meter"), "an alias is the same unit");
    assert_ne!(q(1.0, "pm"), q(2.0, "pm"), "the absolute 1e-9 made these equal");
    assert_ne!(q(1.0, "m"), q(1.0 + 1e-12, "m"));
    assert_ne!(q(1.0, "km"), q(1000.0, "m"), "PartialEq does not convert: it is the same unit and the same bits");
}

#[test]
fn phs_eq_is_relative_and_unit_independent() {
    assert!(!eq(&q(1.0, "pm"), &q(2.0, "pm")));
    assert!(!eq(&q(1.0, "nm"), &q(2.0, "nm")));
    assert!(!eq(&q(1.0, "fm"), &q(1000.0, "fm")));
    assert!(eq(&q(1.0, "km"), &q(1000.0, "m")));
    assert!(eq(&q(1000.0, "m"), &q(1.0, "km")));
    assert!(eq(&q(0.1 + 0.2, "m"), &q(0.3, "m")));
    assert!(eq(&q(1000.0 * 0.1, ""), &q(100.0, "")));
    assert!(!eq(&q(1.0, "m"), &q(1.0 + 1e-9, "m")));
    assert!(eq(&q(1.0, "m"), &q(1.0 + EQ_REL / 2.0, "m")));
}

#[test]
fn phs_eq_reads_a_temperature_point_and_a_level_by_what_they_stand_for() {
    // converting K to degC cancels; the canonical magnitudes do not, so both orders agree
    assert!(eq(&q(0.01, "degC"), &q(273.16, "K")));
    assert!(eq(&q(273.16, "K"), &q(0.01, "degC")));
    assert!(eq(&q(20.0, "degC"), &q(68.0, "degF")));
    assert!(!eq(&q(20.0, "degC"), &q(20.0, "K")));
    assert!(eq(&q(20.0, "dB"), &q(20.0, "dB")));
    assert!(!eq(&q(20.0, "dB"), &q(10.0, "dB")));
}

#[test]
fn phs_eq_on_non_finite_and_zero_operands_follows_ieee() {
    let inf = q(f64::INFINITY, "m");
    assert!(eq(&inf, &inf));
    assert!(!eq(&inf, &q(f64::NEG_INFINITY, "m")));
    assert!(!eq(&inf, &q(1.0, "m")), "the relative formula made (1e308 m * 10) == 1 m true");
    let nan = q(f64::NAN, "m");
    assert!(!eq(&nan, &nan));
    assert!(eq(&q(-0.0, "m"), &q(0.0, "m")));
    assert!(eq(&q(0.0, "m"), &q(0.0, "km")));
    // a relative rule never accepts a difference against an exact zero
    assert!(!eq(&q(1e-17, "m"), &q(0.0, "m")));
}

#[test]
fn phs_eq_refuses_different_dimensions_and_a_zero_needs_a_unit() {
    assert!(q(5.0, "m").phs_eq(&q(5.0, "s")).is_err());
    // zeros of different scales are different values: only the unit says which one is meant
    for unit in ["m", "km", "m/s", "degC", "degF", "K", "rad", "deg", "sr"] {
        let error = q(1.0, unit).phs_eq(&q(0.0, "")).unwrap_err().to_string();
        assert!(error.contains("a zero needs a unit"), "{unit}: {error}");
        assert!(error.contains(&format!("`x > 0 {}`", q(1.0, unit).unit.__repr__())), "{unit}: {error}");
        assert!(q(0.0, "").phs_eq(&q(0.0, unit)).is_err(), "0 == 0 {unit}");
    }
    let error = q(1.0, "m").phs_eq(&q(0.0, "")).unwrap_err().to_string();
    assert_eq!(
        error,
        "cannot compare 1.0 m with the plain number 0: a zero needs a unit, write `x > 0 m` (for a temperature point \
         or a level, each scale's zero is a different value: 0 degC is 273.15 K, 0 dB is a ratio of 1)"
    );
    let error = q(1.0, "m").phs_eq(&q(5.0, "")).unwrap_err().to_string();
    assert!(error.contains("the plain number 5") && error.contains("`x > 5 m`"), "{error}");
    assert!(eq(&q(0.0, "m"), &q(0.0, "km")));
}

#[test]
fn a_level_and_a_dimensionless_value_that_is_not_one_have_no_comparison() {
    for (level, plain) in [(20.0, 100.0), (0.0, 0.0), (-5.0, 0.0), (5.0, 1.0)] {
        assert!(q(level, "dB").phs_eq(&q(plain, "")).is_err(), "{level} dB == {plain}");
        assert!(q(plain, "").phs_compare(&q(level, "dB"), "<").is_err(), "{plain} < {level} dB");
    }
    let error = q(20.0, "dB").phs_eq(&q(100.0, "")).unwrap_err().to_string();
    assert!(error.starts_with("cannot compare 20.0 dB with the plain number 100: write the level with its unit"), "{error}");
    assert!(error.contains("compare the ratio explicitly"), "{error}");
    assert!(q(20.0, "dB").phs_eq(&q(50.0, "%")).is_err(), "a percentage is a ratio, not a level");
    assert!(q(20.0, "dB").phs_eq(&q(100.0, "unity")).is_err());
    // two levels compare by the ratios they stand for
    assert!(q(20.0, "dB").phs_compare(&q(10.0, "dB"), ">").unwrap());
}

#[test]
fn a_dimensionless_ratio_compares_with_plain_numbers() {
    assert!(eq(&q(50.0, "%"), &q(0.5, "")));
    assert!(q(1.0, "%").phs_compare(&q(0.0, ""), ">").unwrap());
    assert!(q(1.0, "unity").phs_compare(&q(0.0, ""), ">").unwrap());
    assert!(eq(&q(3.0, "ppm"), &q(3e-6, "")));
}

#[test]
fn an_angle_is_not_a_plain_number_in_either_angle_mode() {
    use physure_core::units::angle_mode::{scoped, AngleMode};
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        assert!(q(1.0, "rad").phs_compare(&q(0.0, ""), ">").is_err(), "{mode:?}");
        assert!(q(30.0, "deg").phs_compare(&q(0.0, ""), ">").is_err(), "{mode:?}");
        assert!(q(1.0, "rad").phs_compare(&q(0.5, ""), ">").is_err(), "{mode:?}");
        assert!(q(30.0, "deg").phs_compare(&q(0.0, "deg"), ">").unwrap(), "{mode:?}");
    }
}

#[test]
fn phs_compare_is_the_interpreter_s_comparison() {
    let (a, b) = (q(1.0, "km"), q(999.0, "m"));
    for (op, expected) in [(">", true), ("<", false), (">=", true), ("<=", false), ("==", false), ("!=", true), ("≥", true), ("≤", false), ("≠", true)] {
        assert_eq!(a.phs_compare(&b, op).unwrap(), expected, "1 km {op} 999 m");
    }
    assert!(q(1.0, "km").phs_compare(&q(1000.0, "m"), "==").unwrap());
    assert!(q(1.0, "m").phs_compare(&q(1.0, "s"), "<").is_err());
    assert!(q(1.0, "m").phs_compare(&q(0.0, ""), ">").is_err());
    assert!(a.phs_compare(&b, "<>").is_err());
}

#[test]
fn assert_uses_phs_eq() {
    assert!(q(1.0, "fm").phs_assert(&q(1000.0, "fm")).is_err(), "ABS_TOL 1e-12 let 1 fm pass for 1000 fm");
    assert!(q(1.0, "pm").phs_assert(&q(2.0, "pm")).is_err());
    assert!(q(1.0, "m").phs_assert(&q(1000.0, "mm")).is_ok());
    assert!(q(0.01, "degC").phs_assert(&q(273.16, "K")).is_ok());
    let msg = q(1.0, "pm").phs_assert(&q(2.0, "pm")).unwrap_err().to_string();
    assert!(msg.contains("!="), "{msg}");
    let msg = q(1.0, "m").phs_assert(&q(1.0, "s")).unwrap_err().to_string();
    assert!(msg.contains("incompatible dimensions"), "{msg}");
    let msg = q(1.0, "m").phs_assert(&q(0.0, "")).unwrap_err().to_string();
    assert!(msg.contains("a zero needs a unit"), "{msg}");
}

#[test]
fn approx_is_relative_and_refuses_a_lone_zero() {
    let approx = |a: &Quantity, b: &Quantity, rel: f64| a.phs_approx(b, rel).unwrap_or_else(|e| panic!("{a} ≈ {b}: {e}"));
    assert!(!approx(&q(1.0, "mm"), &q(1.9, "mm"), 1e-3));
    assert!(approx(&q(1.0, "km"), &q(1.0009, "km"), 1e-3));
    assert!(approx(&q(1.0, "km"), &q(1000.9, "m"), 1e-3));
    assert!(approx(&q(1.0, "mm"), &q(1.9, "mm"), 0.5));
    assert!(approx(&q(0.0, "m"), &q(0.0, "km"), 1e-3));
    assert!(q(1.0, "m").phs_approx(&q(0.0, "m"), 1e-3).is_err(), "≈ against zero has no relative meaning");
    assert!(q(1.0, "m").phs_approx(&q(1.0, "s"), 1e-3).is_err());
    let inf = q(f64::INFINITY, "m");
    assert!(approx(&inf, &inf, 1e-3));
    assert!(!approx(&inf, &q(1.0, "m"), 1e-3));
    for bad in [0.0, -1e-3, 1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert!(q(1.0, "m").phs_approx(&q(1.0, "m"), bad).is_err(), "rel {bad} accepted");
    }
}
