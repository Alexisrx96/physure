//! The angle mode at the Rust API: the `periodic` flag, what a conversion may read as 1, and what nothing may.

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
fn nothing_else_is_periodic() {
    for text in ["s^-1", "1/s", "Hz*rad", "Hz^2", "Hz/Hz", "rad/s", "m/s", "kg*m/s^2", "rad", "unity"] {
        assert!(!unit(text).periodic, "{text} should not carry the periodic flag");
    }
}

#[test]
fn the_flag_is_not_part_of_equality() {
    assert_eq!(unit("Hz"), unit("s^-1"));
}
