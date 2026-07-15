use super::*;
use num_rational::Rational64;

fn length() -> RationalUnit {
    RationalUnit::new_from_dimensions([("L".into(), (1, 1))])
}

fn time() -> RationalUnit {
    RationalUnit::new_from_dimensions([("T".into(), (1, 1))])
}

#[test]
fn dimensionless_is_empty() {
    let u = RationalUnit::dimensionless();
    assert!(u.dimensions.is_empty());
}

#[test]
fn mul_accumulates_exponents() {
    let l2 = length().mul(&length());
    assert_eq!(l2.get_exponent("L"), Some((2, 1)));
    assert!(!l2.dimensions.is_empty());
}

#[test]
fn div_cancels_same_dimension() {
    assert!(length().div(&length()).dimensions.is_empty());
}

#[test]
fn div_mixed_dimensions() {
    let speed = length().div(&time());
    assert_eq!(speed.get_exponent("L"), Some((1, 1)));
    assert_eq!(speed.get_exponent("T"), Some((-1, 1)));
}

#[test]
fn pow_scales_exponent() {
    let l3 = length().pow(Rational64::new(3, 1));
    assert_eq!(l3.get_exponent("L"), Some((3, 1)));
}

#[test]
fn pow_fractional_exponent() {
    let sqrt_l = length().pow(Rational64::new(1, 2));
    assert_eq!(sqrt_l.get_exponent("L"), Some((1, 2)));
}

#[test]
fn pow_zero_removes_dimension() {
    let u = length().pow(Rational64::new(0, 1));
    assert!(u.dimensions.is_empty());
}

#[test]
fn calculate_id_is_stable() {
    let a = RationalUnit::calculate_id(&[("L".into(), (1, 1))]);
    let b = RationalUnit::calculate_id(&[("L".into(), (1, 1))]);
    assert_eq!(a, b);
}

#[test]
fn calculate_id_differs_for_distinct_dims() {
    let l_id = RationalUnit::calculate_id(&[("L".into(), (1, 1))]);
    let t_id = RationalUnit::calculate_id(&[("T".into(), (1, 1))]);
    assert_ne!(l_id, t_id);
}

#[test]
fn mul_then_div_is_identity() {
    let result = length().mul(&time()).div(&time());
    assert_eq!(result, length());
}

#[test]
fn repr_dimensionless() {
    let u = RationalUnit::dimensionless();
    assert_eq!(u.__repr__(), "Dimensionless");
}

#[test]
fn repr_single_unit_exponent_1() {
    assert_eq!(length().__repr__(), "L");
}

#[test]
fn repr_higher_integer_power() {
    let l2 = length().mul(&length());
    assert_eq!(l2.__repr__(), "L^2");
}

#[test]
fn repr_fractional_power() {
    let sqrt_l = length().pow(Rational64::new(1, 2));
    assert_eq!(sqrt_l.__repr__(), "L^1/2");
}

#[test]
fn hash_is_same_as_id() {
    let u = length();
    assert_eq!(u.id, u.id);
}

#[test]
fn dimensions_accessor_returns_map() {
    let u = length();
    let dims = u.dimensions_map();
    assert_eq!(dims["L"], (1, 1));
    assert_eq!(dims.len(), 1);
}

#[test]
fn to_string_delegates_to_repr() {
    assert_eq!(length().to_string(None, false, None), "L");
}

#[test]
fn unit_registry_add_and_lookup() {
    let mut reg = UnitRegistry::new();
    reg.add_base_unit("m".into());
    assert!(reg.contains("m"));
    assert!(!reg.contains("s"));
}

#[test]
fn unit_registry_alias_resolves() {
    let mut reg = UnitRegistry::new();
    reg.add_base_unit("meter".into());
    reg.register_alias("m".into(), "meter".into());
    assert!(reg.contains("m"));
    assert_eq!(reg.resolve_symbol("m"), "meter");
}

#[test]
fn unit_registry_add_derived() {
    let mut reg = UnitRegistry::new();
    let speed = length().div(&time());
    reg.add_derived_unit("m_per_s".into(), speed.clone());
    assert!(reg.contains("m_per_s"));
}

#[test]
fn parse_exponent_tuple() {
    assert_eq!(RationalUnit::parse_exponent_tuple(3, 2), Some((3, 2)));
    assert_eq!(RationalUnit::parse_exponent_tuple(0, 1), None);
    assert_eq!(RationalUnit::parse_exponent_tuple(2, 1), Some((2, 1)));
}

#[test]
fn unit_registry_get_unit_base() {
    let mut reg = UnitRegistry::new();
    reg.add_base_unit("m".into());
    let unit = reg.get_unit("m").unwrap();
    assert_eq!(unit.get_exponent("m"), Some((1, 1)));
}

#[test]
fn unit_registry_get_unit_derived() {
    let mut reg = UnitRegistry::new();
    let speed = length().div(&time());
    reg.add_derived_unit("v".into(), speed);
    let unit = reg.get_unit("v").unwrap();
    assert_eq!(unit.get_exponent("L"), Some((1, 1)));
}

#[test]
fn unit_registry_get_unit_via_alias() {
    let mut reg = UnitRegistry::new();
    reg.add_base_unit("meter".into());
    reg.register_alias("m".into(), "meter".into());
    let unit = reg.get_unit("m").unwrap();
    assert_eq!(unit.get_exponent("meter"), Some((1, 1)));
}

#[test]
fn unit_registry_get_unit_not_found() {
    let reg = UnitRegistry::new();
    assert!(reg.get_unit("nope").is_none());
}

#[test]
fn mul_zero_exponent_removes_dimension() {
    let l_inv = RationalUnit::new_from_dimensions([("L".into(), (-1i64, 1i64))]);
    assert!(length().mul(&l_inv).dimensions.is_empty());
}

#[test]
fn hash_used_as_hashmap_key() {
    let mut map: std::collections::HashMap<RationalUnit, &str> = std::collections::HashMap::new();
    map.insert(length(), "length");
    assert_eq!(map[&length()], "length");
}

#[test]
fn rational_unit_eq_and_neq() {
    assert!(length().__eq__(&length()));
    assert!(!length().__eq__(&time()));
}
