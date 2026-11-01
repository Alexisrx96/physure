//! What a binding hands to its caller as a quantity: one consistent magnitude, uncertainty and unit.
//!
//! The Java and WASM bindings used to read `value.mean()` and `unit.__repr__()` separately, so
//! `2 m * 3 km` reached them as `6` next to the scale-less symbol `m^2`: 1000 times too small, with
//! no error. `Display` already folded the scale in; `display_parts` is that same fold, shared.

use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser;

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

#[test]
fn a_product_of_two_scales_is_read_in_the_scale_it_displays() {
    let product = q(2.0, "m").mul(&q(3.0, "km")).unwrap();
    assert_eq!(product.display_parts(), (6000.0, 0.0, "m^2".to_string()));
    assert_eq!(product.to_string(), "6000.0 m^2");
}

#[test]
fn the_order_of_the_factors_does_not_change_what_is_read() {
    let a = q(2.0, "m").mul(&q(3.0, "km")).unwrap().display_parts();
    let b = q(3.0, "km").mul(&q(2.0, "m")).unwrap().display_parts();
    assert_eq!(a, b);
}

#[test]
fn a_quotient_of_two_scales_is_dimensionless_and_scaled() {
    assert_eq!(q(3.0, "km").div(&q(2.0, "m")).unwrap().display_parts(), (1500.0, 0.0, String::new()));
    let small = q(2.0, "m").div(&q(3.0, "km")).unwrap().display_parts();
    assert!((small.0 - 2.0 / 3000.0).abs() < 1e-15 && small.2.is_empty(), "{small:?}");
}

#[test]
fn the_uncertainty_is_scaled_with_the_value() {
    let a = Quantity::with_uncertainty(2.0, 0.1, "m").unwrap();
    let b = q(3.0, "km");
    let (value, uncertainty, unit) = a.mul(&b).unwrap().display_parts();
    // relative uncertainty 5%: 6000 +/- 300, not 6 +/- 0.3
    assert_eq!(value, 6000.0);
    assert!((uncertainty - 300.0).abs() < 1e-9, "{uncertainty}");
    assert_eq!(unit, "m^2");
}

#[test]
fn a_unit_with_a_name_is_read_as_it_was_written() {
    assert_eq!(q(5.0, "km").display_parts(), (5.0, 0.0, "km".to_string()));
    assert_eq!(q(20.0, "degC").display_parts(), (20.0, 0.0, "degC".to_string()));
    let with_uncertainty = Quantity::with_uncertainty(10.0, 0.5, "m").unwrap().display_parts();
    assert_eq!(with_uncertainty, (10.0, 0.5, "m".to_string()));
}

#[test]
fn display_is_the_parts_formatted() {
    for quantity in [q(2.0, "m").mul(&q(3.0, "km")).unwrap(), q(1.5, "kg"), q(4.0, "m").pow(0.5).unwrap()] {
        let (value, _, unit) = quantity.display_parts();
        assert_eq!(quantity.to_string(), format!("{value:?} {unit}"), "{quantity}");
    }
}

/// A unit the core writes has to be a unit the core reads, with the same dimensions and the scale 1
/// it was written in; `m^1/2` was read back as `m^1` divided by 2 and refused.
#[test]
fn every_unit_the_core_writes_is_read_back() {
    for expression in [
        "m", "m^2", "m^-1", "m^0.5", "m^1.5", "s^-0.5", "kg^0.25*m/s^0.5", "m^(1/3)", "s^(-1/2)", "m^(-3/2)", "Hz^0.5", "N*m",
    ] {
        let unit = Parser::parse_expression(expression).unwrap().with_unit_scale();
        let written = unit.__repr__();
        let read = Parser::parse_expression(&written).unwrap_or_else(|e| panic!("{expression} was written as {written:?}, which does not parse: {e}"));
        assert_eq!(read.dimensions, unit.dimensions, "{expression} written as {written:?}");
    }
}

#[test]
fn fractional_exponents_are_written_in_parentheses() {
    assert_eq!(q(4.0, "m").pow(0.5).unwrap().unit.__repr__(), "m^(1/2)");
    assert_eq!(q(8.0, "m").pow(-1.0 / 3.0).unwrap().unit.__repr__(), "m^(-1/3)");
}
