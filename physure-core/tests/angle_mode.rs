//! The angle mode at the Rust API: the `periodic` flag, what a conversion may read as 1, and what nothing may.

use physure_core::quantity::Quantity;
use physure_core::units::angle_mode::{scoped, AngleMode};
use physure_core::units::conf::build_registry_from_conf;
use physure_core::units::{Parser, RationalUnit};

fn unit(text: &str) -> RationalUnit {
    Parser::parse_expression(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn the_hertz_is_periodic_under_every_spelling() {
    for spelling in ["Hz", "hertz", "kHz", "kilohertz", "MHz", "mHz"] {
        assert!(unit(spelling).periodic, "{spelling} should carry the periodic flag");
    }
}

#[test]
fn the_hertz_stays_periodic_through_what_leaves_it_unchanged() {
    for text in ["Hz*unity", "unity*Hz", "1*Hz", "Hz/unity", "(Hz)", "Hz^1", "Hz1"] {
        assert!(unit(text).periodic, "{text} is a bare hertz and should carry the periodic flag");
    }
}

#[test]
fn nothing_else_is_periodic() {
    for text in ["s^-1", "1/s", "Hz*rad", "Hz^2", "Hz/Hz", "rad/s", "m/s", "kg*m/s^2", "rad", "unity"] {
        assert!(!unit(text).periodic, "{text} should not carry the periodic flag");
    }
}

#[test]
fn a_product_or_quotient_that_changes_the_hertz_is_not_the_hertz() {
    for text in ["unity/Hz", "1/Hz", "Hz*percent", "Hz*rad"] {
        assert!(!unit(text).periodic, "{text} is not a bare hertz and should not carry the periodic flag");
    }
}

#[test]
fn the_flag_is_not_part_of_equality() {
    assert_eq!(unit("Hz"), unit("s^-1"));
}

#[test]
fn the_registry_keeps_the_flag_where_it_expands_a_unit_written_as_symbols() {
    let (registry, _) = build_registry_from_conf();
    let expand = |text: &str| {
        let atomic = Parser::parse_expression_atomic(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        registry.expand(&atomic).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    assert!(expand("Hz").periodic, "Hz");
    assert!(expand("Hz*unity").periodic, "Hz*unity");
    assert!(!expand("Hz*rad").periodic, "Hz*rad");
}

/// Conversions that the strict comparison already accepts, or that the SI licenses by reading a marker as 1.
const CONVERTIBLE: &[(&str, &str)] = &[
    // strict: identical base exponents
    ("Hz", "s^-1"),
    ("kHz", "Hz"),
    ("rad", "deg"),
    ("dB", "percent"),
    ("Hz*rad", "rad/s"),
    // a marker read as 1
    ("m*rad/s", "m/s"),
    ("m/s", "m*rad/s"),
    ("rad", "unity"),
    ("deg", "unity"),
    ("unity", "rad"),
    ("kg*m^2*rad^2/s^2", "J"),
    ("cd/m^2", "lx"),
    ("lm", "cd"),
    ("rad/s", "s^-1"),
];

/// Conversions that no mode allows.
const NEVER: &[(&str, &str)] = &[
    // a rate of angle is not a frequency
    ("rad/s", "Hz"),
    ("rad/s", "kHz"),
    ("rad/s", "hertz"),
    ("Hz", "rad/s"),
    // two markers do not convert into each other, whatever they carry
    ("rad", "sr"),
    ("sr", "rad"),
    ("deg", "sr"),
    ("rad/s", "sr/s"),
    ("rad^2", "rad"),
    ("m", "m*rad*sr"),
    // a level is not an angle
    ("rad", "dB"),
    ("sr", "dB"),
    ("dB", "rad"),
    ("dB", "sr"),
    // the physical part differs
    ("m*rad", "m^2"),
];

#[test]
fn the_default_mode_reads_a_marker_as_one_in_a_conversion() {
    let _mode = scoped(AngleMode::Dimensionless);
    for (from, to) in CONVERTIBLE {
        assert!(unit(from).convertible_to(&unit(to)), "{from} => {to} should convert");
    }
}

#[test]
fn no_mode_converts_what_the_si_does_not_license() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (from, to) in NEVER {
            assert!(!unit(from).convertible_to(&unit(to)), "{from} => {to} should be refused in {mode:?}");
        }
    }
}

#[test]
fn the_strict_mode_converts_only_what_has_identical_exponents() {
    let _mode = scoped(AngleMode::Dimension);
    for (from, to) in CONVERTIBLE {
        let strict = unit(from).same_dimensions(&unit(to));
        assert_eq!(unit(from).convertible_to(&unit(to)), strict, "{from} => {to}");
    }
}

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::new(value, unit).unwrap_or_else(|e| panic!("{unit}: {e}"))
}

fn rotating_disk_energy() -> Quantity {
    let (mass, radius, omega, half) = (q(4.0, "kg"), q(0.5, "m"), q(10.0, "rad/s"), q(0.5, ""));
    let inertia = mass.mul(&radius.pow(2.0).unwrap()).unwrap().mul(&half).unwrap();
    inertia.mul(&omega.pow(2.0).unwrap()).unwrap().mul(&half).unwrap()
}

#[test]
fn the_energy_of_a_rotating_disk_converts_to_joules() {
    let _mode = scoped(AngleMode::Dimensionless);
    let joules = rotating_disk_energy().to("J").unwrap();
    assert!((joules.value.mean() - 25.0).abs() < 1e-12, "{}", joules.value.mean());
    assert_eq!(joules.unit.__repr__(), "J");
}

#[test]
fn a_speed_from_an_angular_velocity_converts_to_metres_per_second() {
    let _mode = scoped(AngleMode::Dimensionless);
    let speed = q(10.0, "rad/s").mul(&q(0.5, "m")).unwrap().to("m/s").unwrap();
    assert!((speed.value.mean() - 5.0).abs() < 1e-12);
}

#[test]
fn the_strict_mode_refuses_both() {
    let _mode = scoped(AngleMode::Dimension);
    assert!(rotating_disk_energy().to("J").is_err());
    assert!(q(10.0, "rad/s").mul(&q(0.5, "m")).unwrap().to("m/s").is_err());
}

#[test]
fn a_rate_of_angle_does_not_convert_to_a_frequency() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        assert!(q(10.0, "rad/s").to("Hz").is_err(), "{mode:?}");
        assert!(q(10.0, "rad/s").to("kHz").is_err(), "{mode:?}");
        assert!(q(10.0, "Hz").to("rad/s").is_err(), "{mode:?}");
    }
}

#[test]
fn dividing_by_two_pi_radians_is_the_way_to_a_frequency() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        let hertz = q(10.0, "rad/s").div(&q(std::f64::consts::TAU, "rad")).unwrap().to("Hz").unwrap();
        assert!((hertz.value.mean() - 10.0 / std::f64::consts::TAU).abs() < 1e-12, "{mode:?}");
    }
}

#[test]
fn a_sum_or_a_comparison_never_reads_a_marker_as_one() {
    let refused = [
        (2.0, "rad", 1.0, ""),
        (5.0, "deg", 50.0, "percent"),
        (2.0, "m*rad", 1.0, "m"),
        (25.0, "kg*m^2*rad^2/s^2", 25.0, "J"),
        (1.0, "rad^2", 1.0, "rad"),
        (1.0, "rad", 1.0, "sr"),
    ];
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (a, unit_a, b, unit_b) in refused {
            let (left, right) = (q(a, unit_a), q(b, unit_b));
            assert!(left.add(&right).is_err(), "{unit_a} + {unit_b} in {mode:?}");
            assert!(left.sub(&right).is_err(), "{unit_a} - {unit_b} in {mode:?}");
            assert!(!left.approx_eq(&right, 1e-9, 1e-12), "{unit_a} ~ {unit_b} in {mode:?}");
        }
    }
}

#[test]
fn an_explicit_conversion_first_makes_the_sum_possible() {
    let _mode = scoped(AngleMode::Dimensionless);
    let converted = q(2.0, "rad").to("unity").unwrap();
    let sum = converted.add(&q(1.0, "")).unwrap();
    assert!((sum.value.mean() - 3.0).abs() < 1e-12);
}

const UNITS: &[&str] = &[
    "rad", "deg", "sr", "unity", "percent", "m", "m*rad", "m*sr", "kg*m^2*rad^2/s^2", "J", "Hz", "kHz", "rad/s",
    "s^-1", "cd", "lm", "lx", "cd/m^2", "dB", "rad^2", "N*m", "W", "W/sr",
];

/// What a sum, a difference and a comparison answer, as a comparable value.
type Answer = Result<(f64, String), String>;

fn operators(a: &Quantity, b: &Quantity) -> (Answer, Answer, bool) {
    let shown = |r: Result<Quantity, physure_core::PhysureError>| {
        r.map(|q| (q.value.mean(), q.unit.__repr__())).map_err(|e| e.to_string())
    };
    (shown(a.add(b)), shown(a.sub(b)), a.approx_eq(b, 1e-9, 1e-12))
}

#[test]
fn sums_and_comparisons_do_not_depend_on_the_angle_mode() {
    for first in UNITS {
        for second in UNITS {
            let (a, b) = (q(2.0, first), q(3.0, second));
            let default = {
                let _mode = scoped(AngleMode::Dimensionless);
                operators(&a, &b)
            };
            let strict = {
                let _mode = scoped(AngleMode::Dimension);
                operators(&a, &b)
            };
            assert_eq!(default, strict, "{first} with {second}");
        }
    }
}
