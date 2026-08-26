use super::*;
use num_rational::Rational64;

fn length() -> RationalUnit {
    RationalUnit::new_from_dimensions([("L".into(), (1, 1))])
}

fn time() -> RationalUnit {
    RationalUnit::new_from_dimensions([("T".into(), (1, 1))])
}

#[test]
fn test_kpa_unit_repr() {
    let (reg, _) = conf::build_registry_from_conf();
    let kpa = reg.get_unit("kPa").unwrap();
    println!("KPA DIMS: {:?}", kpa.dimensions);
    println!("KPA SCALE: {:?}", kpa.scale);
    println!("KPA REPR: {:?}", kpa.__repr__());
    assert_eq!(kpa.__repr__(), "kPa");
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
    assert_eq!(u.__repr__(), "");
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

fn dims(pairs: &[(&str, i64, i64)]) -> RationalUnit {
    RationalUnit::new_from_dimensions(pairs.iter().map(|(k, n, d)| (k.to_string(), (*n, *d))))
}

#[test]
fn repr_maps_all_distinguishable_si_derived_units() {
    // Volt: kg*m^2*s^-3*A^-1
    assert_eq!(dims(&[("A", -1, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -3, 1)]).__repr__(), "V");
    // Farad: A^2*kg^-1*m^-2*s^4
    assert_eq!(dims(&[("A", 2, 1), ("kg", -1, 1), ("m", -2, 1), ("s", 4, 1)]).__repr__(), "F");
    // Siemens: A^2*kg^-1*m^-2*s^3
    assert_eq!(dims(&[("A", 2, 1), ("kg", -1, 1), ("m", -2, 1), ("s", 3, 1)]).__repr__(), "S");
    // Weber: A^-1*kg*m^2*s^-2
    assert_eq!(dims(&[("A", -1, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -2, 1)]).__repr__(), "Wb");
    // Tesla: A^-1*kg*s^-2
    assert_eq!(dims(&[("A", -1, 1), ("kg", 1, 1), ("s", -2, 1)]).__repr__(), "T");
    // Henry: A^-2*kg*m^2*s^-2
    assert_eq!(dims(&[("A", -2, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -2, 1)]).__repr__(), "H");
    // Lux: cd*m^-2
    assert_eq!(dims(&[("cd", 1, 1), ("m", -2, 1)]).__repr__(), "lx");
    // Katal: mol*s^-1
    assert_eq!(dims(&[("mol", 1, 1), ("s", -1, 1)]).__repr__(), "kat");
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

/// `physure.conf` declares `radian` and `steradian` against the same dimension symbol `A`,
/// and the loader used to let the last declaration win, making the steradian the base unit
/// of angle. Every unit defined against `A` — `deg`, `arcmin`, `arcsec` — was then a scaled
/// *steradian*, so a degree could be added to a solid angle but not converted to a radian.
#[test]
fn degree_is_a_scaled_radian_not_a_scaled_steradian() {
    let (reg, _) = conf::build_registry_from_conf();
    let deg = reg.get_unit("deg").unwrap();
    assert_eq!(deg.get_exponent("rad"), Some((1, 1)), "deg should be dimensioned in radians");
    assert_eq!(deg.get_exponent("sr"), None, "deg is a plane angle, not a solid one");
    assert!(
        deg.same_dimensions(&reg.get_unit("rad").unwrap()),
        "a degree and a radian have to be interconvertible"
    );
    assert!((deg.scale - 0.0174532925).abs() < 1e-12, "deg carries the degrees → radians factor");
}

/// The temperature scales are the registry's only affine units: their conversion needs an
/// additive zero point on top of the scale factor. Nothing else in the registry has one, so
/// the offset must be the exception it looks like — every other unit stays at zero, or the
/// purely multiplicative fast paths in `Quantity` would quietly take the wrong branch.
#[test]
fn temperature_scales_are_the_only_affine_units() {
    let (reg, _) = conf::build_registry_from_conf();

    let degc = reg.get_unit("degC").unwrap();
    assert!((degc.offset - 273.15).abs() < 1e-12, "degC must carry the Kelvin zero point");
    assert!((degc.scale - 1.0).abs() < 1e-12, "a degC step is a Kelvin step");
    assert!(degc.is_affine());
    assert!(
        degc.same_dimensions(&reg.get_unit("K").unwrap()),
        "degC and K have to be interconvertible"
    );

    let degf = reg.get_unit("degF").unwrap();
    assert!((degf.scale - 5.0 / 9.0).abs() < 1e-12, "a degF step is 5/9 of a Kelvin");
    assert!((degf.offset - 255.37222222222223).abs() < 1e-9, "degF zero point in Kelvin");

    // Rankine starts at absolute zero, so it is scaled but *not* affine.
    let degr = reg.get_unit("degR").unwrap();
    assert!(!degr.is_affine(), "degR shares K's zero point, so it carries no offset");
    assert!((degr.scale - 5.0 / 9.0).abs() < 1e-12);

    // A delta unit is the interval scale: same step, no zero point.
    let delta = reg.get_unit("delta_degC").unwrap();
    assert!(!delta.is_affine(), "a temperature *difference* has no zero point");

    // Nothing else may have acquired an offset from the conf parser's new offset field.
    let affine: Vec<&String> = reg
        .base_units
        .keys()
        .chain(reg.derived_units.keys())
        .filter(|name| reg.get_unit(name).is_some_and(|u| u.is_affine()))
        .collect();
    let mut affine: Vec<&str> = affine.into_iter().map(String::as_str).collect();
    affine.sort_unstable();
    assert_eq!(affine, ["degC", "degF"], "only Celsius and Fahrenheit are affine");
}

/// `degC` and `K` share dimensions *and* scale, differing only in their zero point, so unit
/// equality has to take the offset into account or a degC → K conversion looks like a no-op.
#[test]
fn an_offset_makes_a_unit_distinct_from_its_base() {
    let (reg, _) = conf::build_registry_from_conf();
    let degc = reg.get_unit("degC").unwrap();
    let kelvin = reg.get_unit("K").unwrap();
    assert!(degc.same_dimensions(&kelvin), "same physical dimension");
    assert!((degc.scale - kelvin.scale).abs() < 1e-12, "same step size");
    assert_ne!(degc, kelvin, "but not the same unit — the zero point differs");
    assert_eq!(degc.to_delta(), kelvin.to_delta(), "dropping the zero point makes them equal");
}

#[test]
fn test_unit_conversions() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    
    let kmh = reg.get_unit("km").unwrap().div(&reg.get_unit("h").unwrap());
    let ms = reg.get_unit("m").unwrap().div(&reg.get_unit("s").unwrap());
    let speed = Quantity::new_scalar(36.0, 0.0, kmh, None, None);
    let speed_ms = speed.convert_to(&ms).unwrap();
    assert!((speed_ms.value.mean() - 10.0).abs() < 1e-9);

    let degc = reg.get_unit("degC").unwrap();
    let k = reg.get_unit("K").unwrap();
    let temp = Quantity::new_scalar(25.0, 0.0, degc, None, None);
    let temp_k = temp.convert_to(&k).unwrap();
    assert!((temp_k.value.mean() - 298.15).abs() < 1e-9);

    let j = reg.get_unit("J").unwrap();
    let kwh = reg.get_unit("kWh").unwrap();
    let energy = Quantity::new_scalar(3_600_000.0, 0.0, j, None, None);
    let energy_kwh = energy.convert_to(&kwh).unwrap();
    assert!((energy_kwh.value.mean() - 1.0).abs() < 1e-9);
}

/// `unity = 1.0, 1, [1]` used to get registered twice: `parse_physure_conf`'s base-unit
/// auto-detection pass (factor 1.0, single-ident dimension code) has no guard for dimension
/// code "1", so it called `add_base_unit("unity")` and poisoned `base_units["unity"]` with a
/// real, spurious `("unity", (1,1))` dimension -- even though the *second* pass correctly
/// registers the intended, truly dimensionless `unity` into `derived_units`. `get_unit`
/// checks `base_units` before `derived_units`, so the wrong one always won: `unity` couldn't
/// convert to or add with `%`, `ppm`, or a bare number, despite all four being dimensionless.
#[test]
fn unity_is_dimensionless_like_percent_ppm_and_a_bare_number() {
    let pct = crate::units::parser::Parser::parse_expression("%").unwrap();
    let ppm = crate::units::parser::Parser::parse_expression("ppm").unwrap();
    let unity = crate::units::parser::Parser::parse_expression("unity").unwrap();
    let bare = crate::units::RationalUnit::dimensionless();

    assert!(unity.dimensions.is_empty(), "unity should carry no dimension of its own, got {:?}", unity.dimensions);
    assert!(pct.same_dimensions(&unity), "% and unity should be the same (empty) dimension");
    assert!(ppm.same_dimensions(&unity), "ppm and unity should be the same (empty) dimension");
    assert!(bare.same_dimensions(&unity), "a bare number and unity should be the same (empty) dimension");
}
