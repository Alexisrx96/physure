//! The angle mode at the Rust API: the `periodic` flag, what a conversion may read as 1, and what nothing may.

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
