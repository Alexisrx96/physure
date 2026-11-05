//! The angle mode at the Rust API: the `periodic` flag, what a conversion may read as 1, and what nothing may.

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
