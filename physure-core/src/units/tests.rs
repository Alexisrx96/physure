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
    assert_eq!(deg.scale, std::f64::consts::PI / 180.0, "deg carries the degrees → radians factor");
}

/// A relation that holds by definition has to come out exact, not as 60.000043 or 760.0021:
/// the factors in physure.conf are the exact values of their definitions.
#[test]
fn scale_ratios_that_are_exact_by_definition_come_out_exact() {
    let (reg, _) = conf::build_registry_from_conf();
    let scale = |symbol: &str| reg.get_unit(symbol).unwrap().scale;
    let close = |got: f64, expected: f64| (got - expected).abs() <= expected.abs() * 1e-14;
    assert!(close(scale("deg") / scale("arcmin"), 60.0));
    assert!(close(scale("arcmin") / scale("arcsec"), 60.0));
    assert!(close(scale("atm") / scale("Torr"), 760.0));
    assert!(close(scale("gal") / scale("inch").powi(3), 231.0));
    assert!(close(scale("acre") / scale("ft").powi(2), 43560.0));
    assert!(close(scale("hp") / (scale("ft") * scale("lbf")), 550.0));
    assert!(close(scale("psi") / (scale("lbf") / scale("inch").powi(2)), 1.0));
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
    // the registry keys the two scales by their symbols, the first name the catalog lists
    assert_eq!(affine, ["°C", "°F"], "only Celsius and Fahrenheit are affine");
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

/// The doubles 1e-6 and 0.01 divide to 9.999999999999999e-05; the rationals the catalog states
/// divide to exactly 1e-4. A conversion is rounded once, from the exact scales.
#[test]
fn a_conversion_is_rounded_once_from_the_exact_scales() {
    let (reg, _) = conf::build_registry_from_conf();
    let ratio = |from: &str, to: &str| {
        let from = Parser::parse_expression_with_registry(from, &reg).unwrap();
        let to = Parser::parse_expression_with_registry(to, &reg).unwrap();
        from.ratio_to(&to)
    };
    assert_eq!(ratio("ppm", "percent"), 1e-4);
    assert_eq!(ratio("g/cm^3", "kg/m^3"), 1000.0);
    assert_eq!(ratio("ft", "inch"), 12.0);
    assert_eq!(ratio("mile", "ft"), 5280.0);
    assert_eq!(ratio("lb", "oz"), 16.0);
    assert_eq!(ratio("gal", "inch^3"), 231.0);
    assert_eq!(ratio("acre", "ft^2"), 43560.0);
}

/// `scale` is the exact scale rounded once, for every unit of the catalog and for the units
/// that prefixes and products build from them.
#[test]
fn the_scale_is_the_exact_scale_rounded_once() {
    let (reg, _) = conf::build_registry_from_conf();
    for (name, unit) in reg.base_units.iter().chain(reg.derived_units.iter()) {
        let exact = unit.exact.as_ref().unwrap_or_else(|| panic!("{name} has no exact scale"));
        assert_eq!(unit.scale.to_bits(), exact.to_f64().to_bits(), "{name}");
    }
    let tiny = reg.get_unit("qarcsec").unwrap();
    assert_eq!(tiny.scale, 4.84813681109536e-36);
    assert!(tiny.exact.is_some());
    let product = reg.get_unit("km").unwrap().mul(&reg.get_unit("ms").unwrap());
    assert_eq!(product.scale, 1.0);
    assert!(product.exact.as_ref().is_some_and(ExactScale::is_one));
}

/// A fractional power cannot be held exactly (Python falls back to a float there too), except
/// that 1 to any power is 1. A scale set from a bare `f64` is not exact either.
#[test]
fn what_cannot_be_exact_falls_back_to_the_double() {
    let (reg, _) = conf::build_registry_from_conf();
    let root = reg.get_unit("km").unwrap().pow(Rational64::new(1, 2));
    assert!(root.exact.is_none());
    assert_eq!(root.scale, 1000f64.powf(0.5));
    let root_of_one = reg.get_unit("m").unwrap().pow(Rational64::new(1, 2));
    assert!(root_of_one.exact.as_ref().is_some_and(ExactScale::is_one));
    let loose = length().with_scale(0.5);
    assert!(loose.exact.is_none());
    assert_eq!(loose.ratio_to(&length()), 0.5);
    let sure = length().with_exact_scale(ExactScale::parse("1/3").unwrap());
    assert_eq!(sure.scale, 1.0 / 3.0);
    assert_eq!(sure.ratio_to(&length()), 1.0 / 3.0);
}

/// The catalog is parsed once and copied: a copy that is changed does not change the next one.
#[test]
fn every_call_gets_its_own_copy_of_the_catalog() {
    let (mut first, mut constants) = conf::build_registry_from_conf();
    first.add_base_unit("zzz_scratch".to_string());
    constants.clear();
    let (second, second_constants) = conf::build_registry_from_conf();
    assert!(!second.contains("zzz_scratch"));
    assert!(!second_constants.is_empty());
    let (third, _) = conf::build_registry_from_conf();
    assert_eq!(second, third);
}

/// The temperature scales are written in the catalog as the fractions they are, so the fixed
/// points of the scales convert exactly in both directions.
#[test]
fn the_fixed_points_of_the_temperature_scales_convert_exactly() {
    let (reg, _) = conf::build_registry_from_conf();
    let convert = |value: f64, from: &str, to: &str| {
        let (scale, shift) = reg.get_unit(from).unwrap().affine_to(&reg.get_unit(to).unwrap());
        value * scale + shift
    };
    assert_eq!(convert(100.0, "degC", "degF"), 212.0);
    assert_eq!(convert(212.0, "degF", "degC"), 100.0);
    assert_eq!(convert(0.0, "degC", "degF"), 32.0);
    assert_eq!(convert(32.0, "degF", "degC"), 0.0);
    assert_eq!(convert(-40.0, "degC", "degF"), -40.0);
    assert_eq!(convert(-40.0, "degF", "degC"), -40.0);
    assert_eq!(convert(0.0, "degC", "K"), 273.15);
    assert_eq!(convert(273.15, "K", "degC"), 0.0);
    assert_eq!(convert(-459.67, "degF", "K"), 0.0);
    assert_eq!(convert(491.67, "degR", "degF"), 32.0);
}

/// A scale that arrived as a double has no exact zero point, and the shift falls back to the
/// doubles; a conversion between two units without zero points has no shift at all.
#[test]
fn the_shift_falls_back_to_the_doubles_and_is_zero_without_zero_points() {
    let celsius = length().with_scale(1.0).with_offset(273.15);
    let fahrenheit = length().with_scale(5.0 / 9.0).with_offset(255.37222222222223);
    let (scale, shift) = celsius.affine_to(&fahrenheit);
    assert_eq!(scale, 1.0 / (5.0 / 9.0));
    assert_eq!(shift, (273.15 - 255.37222222222223) / (5.0 / 9.0));
    let (reg, _) = conf::build_registry_from_conf();
    let (_, no_shift) = reg.get_unit("km").unwrap().affine_to(&reg.get_unit("mile").unwrap());
    assert_eq!(no_shift, 0.0);
}

/// A temperature difference is a kelvin wide, whichever scale it is counted in: `delta_celsius`
/// was declared at scale 1 on the dimension of kelvin and became a base unit of its own, so it
/// converted to nothing.
#[test]
fn a_temperature_difference_converts_to_kelvin_and_to_the_other_differences() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    let kelvin = reg.get_unit("K").unwrap();
    for name in ["delta_celsius", "delta_degC", "degC_delta"] {
        let converted = Quantity::new(5.0, name).unwrap().convert_to(&kelvin).unwrap();
        assert_eq!(converted.value.mean(), 5.0, "{name}");
    }
    let delta_celsius = reg.get_unit("delta_celsius").unwrap();
    let converted = Quantity::new(9.0, "delta_degF").unwrap().convert_to(&delta_celsius).unwrap();
    assert_eq!(converted.value.mean(), 5.0);
}

/// Radian and steradian share the dimension symbol `A`; the catalog flags the steradian as a
/// kind of its own, so an angle never converts into a solid angle.
#[test]
fn an_angle_does_not_convert_into_a_solid_angle() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    let steradian = reg.get_unit("sr").unwrap();
    let radian = reg.get_unit("rad").unwrap();
    assert!(Quantity::new(1.0, "deg").unwrap().convert_to(&steradian).is_err());
    assert!(Quantity::new(1.0, "rad").unwrap().convert_to(&steradian).is_err());
    assert!(Quantity::new(1.0, "sr").unwrap().convert_to(&radian).is_err());
    assert!(Quantity::new(180.0, "deg").unwrap().convert_to(&radian).is_ok());
}

/// `log10` and `pow10` are real functions of every backend: the Monte Carlo and unscented ones
/// used to hand an unknown name back unchanged, which would have read a level as its own ratio.
#[test]
fn log10_and_pow10_are_functions_of_every_uncertainty_backend() {
    use crate::quantity::Quantity;
    for mode in ["gaussian", "monte_carlo", "unscented"] {
        let level = Quantity::new_scalar(2.0, 0.01, RationalUnit::dimensionless(), Some(mode), Some(2000));
        let ratio = level.value.propagate_function("pow10").unwrap();
        assert!((ratio.mean() - 100.0).abs() < 1.0, "{mode}: {}", ratio.mean());
        let back = ratio.propagate_function("log10").unwrap();
        assert!((back.mean() - 2.0).abs() < 0.01, "{mode}: {}", back.mean());
    }
    let level = Quantity::new_scalar(2.0, 0.01, RationalUnit::dimensionless(), Some("gaussian"), None);
    let ratio = level.value.propagate_function("pow10").unwrap();
    assert_eq!(ratio.mean(), 100.0);
    assert!((ratio.std_dev() - 100.0 * std::f64::consts::LN_10 * 0.01).abs() < 1e-12);
}

/// A level is a unit of its own kind: the catalog's `Log` lines are in the registry, `pH` is the
/// level and no longer a picohenry, and a prefix has nothing to scale on a level.
#[test]
fn the_logarithmic_units_of_the_catalog_are_levels() {
    use crate::units::parser::Parser;
    let (reg, _) = conf::build_registry_from_conf();
    for name in ["dB", "decibel", "pH"] {
        let unit = reg.get_unit(name).unwrap_or_else(|| panic!("{name} should be known"));
        assert!(unit.is_logarithmic(), "{name}");
        assert!(unit.dimensions.is_empty(), "{name}");
        assert_eq!(unit.scale, 1.0, "{name}");
    }
    assert_eq!(reg.get_unit("dB").unwrap(), reg.get_unit("decibel").unwrap());
    assert_ne!(reg.get_unit("dB").unwrap(), reg.get_unit("pH").unwrap());
    assert_ne!(reg.get_unit("dB").unwrap(), reg.get_unit("unity").unwrap());
    // pH is the level; the picohenry is still there under its spelled-out name
    assert!(!reg.get_unit("picohenry").unwrap().is_logarithmic());
    assert_eq!(reg.get_unit("picohenry").unwrap().scale, 1e-12);
    // no prefixes on a level, and no level inside a compound unit
    assert!(reg.get_unit("mdB").is_none());
    assert!(reg.get_unit("kilodecibel").is_none());
    assert!(Parser::parse_expression("dB").unwrap().is_logarithmic());
    for compound in ["dB/km", "dB*m", "dB^2", "1/dB", "dB/dB"] {
        assert!(Parser::parse_expression(compound).is_err(), "{compound}");
    }
    assert!(Parser::parse_expression("dB/1").unwrap().is_logarithmic());
}

/// Levels convert through the ratio they stand for, with the operations of Python's
/// `LogarithmicConverter`, so both sides agree to the last bit.
#[test]
fn levels_convert_to_ratios_and_to_each_other() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    let convert = |value: f64, from: &str, to: &str| -> f64 {
        Quantity::new(value, from).unwrap().convert_to(&reg.get_unit(to).unwrap()).unwrap().value.mean()
    };
    assert_eq!(convert(20.0, "dB", "unity"), 100.0);
    assert_eq!(convert(20.0, "dB", "percent"), 100.0 / 0.01);
    assert_eq!(convert(7.0, "pH", "unity"), 10f64.powf(7.0 / -1.0));
    assert_eq!(convert(50.0, "percent", "dB"), 10.0 * (50.0f64 * 0.01).log10());
    assert!((convert(50.0, "percent", "dB") + 3.010299956639812).abs() < 1e-12);
    assert_eq!(convert(1.0, "unity", "dB"), 0.0);
    assert_eq!(convert(3.0, "dB", "decibel"), 3.0);
    assert!((convert(3.0, "dB", "pH") + 0.3).abs() < 1e-12);
    assert!((convert(10.0, "ppm", "dB") + 50.0).abs() < 1e-9);
}

#[test]
fn a_level_does_not_convert_into_a_dimension_or_an_angle_and_a_ratio_must_be_positive() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    let db = reg.get_unit("dB").unwrap();
    for from in ["m", "rad", "sr", "deg"] {
        assert!(Quantity::new(1.0, from).unwrap().convert_to(&db).is_err(), "{from} => dB");
        assert!(Quantity::new(1.0, "dB").unwrap().convert_to(&reg.get_unit(from).unwrap()).is_err(), "dB => {from}");
    }
    assert!(Quantity::new(0.0, "unity").unwrap().convert_to(&db).is_err());
    assert!(Quantity::new(-1.0, "unity").unwrap().convert_to(&db).is_err());
}

#[test]
fn the_uncertainty_of_a_level_follows_the_logarithm() {
    use crate::quantity::Quantity;
    let (reg, _) = conf::build_registry_from_conf();
    let unity = reg.get_unit("unity").unwrap();
    let ratio = Quantity::with_uncertainty(10.0, 0.5, "dB").unwrap().convert_to(&unity).unwrap();
    assert_eq!(ratio.value.mean(), 10.0);
    // d(10^(x/10))/dx = 10^(x/10) ln(10) / 10; Python gives 1.151292546497023
    assert!((ratio.value.std_dev() - 1.151292546497023).abs() < 1e-12);
    let level = Quantity::with_uncertainty(2.0, 0.1, "unity").unwrap().convert_to(&reg.get_unit("dB").unwrap()).unwrap();
    assert!((level.value.std_dev() - 0.21714724095162588).abs() < 1e-12);
}

/// Levels add as the quantities they stand for; a level and a plain quantity do not add; a
/// level times a plain number is that level; nothing else is defined for a level.
#[test]
fn levels_combine_as_ratios_and_refuse_what_has_no_meaning() {
    use crate::quantity::Quantity;
    let q = |value: f64, unit: &str| Quantity::new(value, unit).unwrap();
    let sum = q(10.0, "dB").add(&q(20.0, "dB")).unwrap();
    assert!(sum.unit.is_logarithmic());
    assert_eq!(sum.value.mean(), 10.0 * (10f64.powf(1.0) + 10f64.powf(2.0)).log10());
    assert!((sum.value.mean() - 20.41392685158225).abs() < 1e-12);
    let difference = q(20.0, "dB").sub(&q(10.0, "dB")).unwrap();
    assert!((difference.value.mean() - 19.54242509439325).abs() < 1e-12);
    assert!(q(10.0, "dB").sub(&q(20.0, "dB")).is_err());
    assert!(q(10.0, "dB").add(&q(1.0, "unity")).is_err());
    assert!(q(1.0, "unity").add(&q(10.0, "dB")).is_err());
    assert!(q(3.0, "dB").add(&q(3.0, "pH")).is_ok());

    let doubled = q(10.0, "dB").mul(&q(2.0, "unity")).unwrap();
    assert!(doubled.unit.is_logarithmic());
    assert_eq!(doubled.value.mean(), 20.0);
    assert_eq!(q(2.0, "unity").mul(&q(10.0, "dB")).unwrap().value.mean(), 20.0);
    assert!(q(10.0, "dB").div(&q(2.0, "unity")).unwrap().unit.is_logarithmic());
    for refused in [
        q(10.0, "dB").mul(&q(2.0, "m")),
        q(10.0, "dB").div(&q(2.0, "m")),
        q(2.0, "m").div(&q(10.0, "dB")),
        q(10.0, "dB").mul(&q(2.0, "dB")),
        q(10.0, "dB").pow(2.0),
        q(10.0, "dB").sqrt(),
        q(10.0, "dB").ln(),
        q(10.0, "dB").exp(),
        q(10.0, "dB").sin(),
    ] {
        assert!(refused.is_err());
    }
    assert!(q(10.0, "dB").abs().unwrap().unit.is_logarithmic());
    // the coherent value of a level is the ratio, so levels compare by what they stand for
    assert!(q(20.0, "dB").approx_eq(&q(100.0, "unity"), 1e-12, 0.0));
    assert!(!q(20.0, "dB").approx_eq(&q(20.0, "unity"), 1e-12, 0.0));
}

/// An absolute temperature is a point on an interval scale: an interval moves it, two points give
/// an interval, and the sum of two points (or a point taken from an interval) has no meaning.
#[test]
fn temperature_points_and_intervals_follow_the_affine_algebra() {
    use crate::quantity::Quantity;
    let q = |value: f64, unit: &str| Quantity::new(value, unit).unwrap();
    let is = |result: crate::error::PhysureResult<Quantity>, mean: f64, unit: &str| {
        let result = result.unwrap();
        assert!((result.value.mean() - mean).abs() < 1e-9, "{} != {mean} {unit}", result.value.mean());
        assert_eq!(result.unit.__repr__(), unit);
    };
    is(q(20.0, "degC").add(&q(5.0, "K")), 25.0, "degC");
    is(q(5.0, "K").add(&q(20.0, "degC")), 25.0, "degC");
    is(q(20.0, "degC").add(&q(9.0, "delta_fahrenheit")), 25.0, "degC");
    is(q(20.0, "degF").add(&q(5.0, "K")), 29.0, "degF");
    is(q(20.0, "degC").sub(&q(5.0, "K")), 15.0, "degC");
    is(q(20.0, "degC").sub(&q(5.0, "degC")), 15.0, "K");
    is(q(20.0, "degC").sub(&q(5.0, "degF")), 35.0, "K");
    assert!(q(20.0, "degC").add(&q(5.0, "degC")).is_err());
    assert!(q(20.0, "degC").add(&q(5.0, "degF")).is_err());
    assert!(q(5.0, "K").sub(&q(20.0, "degC")).is_err());
    assert!(q(20.0, "degC").add(&q(1.0, "m")).is_err());
    assert!(q(20.0, "degC").sub(&q(1.0, "m")).is_err());
}

/// A sign keeps the scale; any other plain number would pick a scale for the caller; a quantity with
/// dimensions takes the temperature as the absolute one it is.
#[test]
fn a_sign_keeps_the_scale_and_any_other_number_is_refused() {
    use crate::quantity::Quantity;
    let q = |value: f64, unit: &str| Quantity::new(value, unit).unwrap();
    for negated in [q(-1.0, "unity").mul(&q(25.0, "degC")), q(25.0, "degC").mul(&q(-1.0, "unity")), q(25.0, "degC").div(&q(-1.0, "unity"))] {
        let negated = negated.unwrap();
        assert_eq!(negated.value.mean(), -25.0);
        assert_eq!(negated.unit.__repr__(), "degC");
    }
    assert_eq!(q(1.0, "unity").mul(&q(25.0, "degC")).unwrap().value.mean(), 25.0);
    for refused in [
        q(2.0, "unity").mul(&q(20.0, "degC")),
        q(20.0, "degC").mul(&q(2.0, "unity")),
        q(20.0, "degC").div(&q(2.0, "unity")),
        q(1.0, "unity").div(&q(20.0, "degC")),
        q(-1.0, "unity").div(&q(20.0, "degC")),
        q(20.0, "degC").mul(&q(50.0, "percent")),
    ] {
        assert!(refused.is_err());
    }
    let energy = q(8.314, "J/(mol*K)").mul(&q(25.0, "degC")).unwrap();
    assert!((energy.value.mean() - 8.314 * 298.15).abs() < 1e-9);
    assert!((q(25.0, "degC").pow(2.0).unwrap().value.mean() - 298.15 * 298.15).abs() < 1e-9);
    let ratio = q(100.0, "degC").div(&q(0.0, "degC").add(&q(50.0, "K")).unwrap()).unwrap();
    assert!(ratio.unit.dimensions.is_empty());
}

/// Quantities compare by what they stand for, so the zero point of a temperature scale counts.
#[test]
fn a_temperature_compares_by_its_absolute_value() {
    use crate::quantity::Quantity;
    let q = |value: f64, unit: &str| Quantity::new(value, unit).unwrap();
    assert_eq!(q(20.0, "degC").canonical_magnitude(), 293.15);
    assert!((q(32.0, "degF").canonical_magnitude() - 273.15).abs() < 1e-12);
    assert!(q(20.0, "degC").approx_eq(&q(293.15, "K"), 1e-12, 0.0));
    assert!(q(20.0, "degC").approx_eq(&q(68.0, "degF"), 1e-12, 0.0));
    assert!(!q(20.0, "degC").approx_eq(&q(20.0, "K"), 1e-12, 0.0));
    assert_eq!(q(0.0, "degC").base_display(), "273.15 K");
}

/// A unit written as symbols prints the way a `CompoundUnit` does.
#[test]
fn a_unit_written_as_symbols_prints_like_a_compound_unit() {
    use crate::units::parser::Parser;
    let show = |text: &str| Parser::parse_expression_atomic(text).unwrap().symbolic_repr();
    assert_eq!(show("kg*m/s^2"), "kg·m/s²");
    assert_eq!(show("m/s^2"), "m/s²");
    assert_eq!(show("s^-1"), "1/s");
    assert_eq!(show("kg/(m*s^2)"), "kg/(m·s²)");
    assert_eq!(show("m^(1/2)"), "m⁰⋅⁵");
    assert_eq!(show("m"), "m");
    assert_eq!(show("1"), "1");
}

/// What the core says about a unit expression: dimension, kind, linearity, alias, simplified form.
#[test]
fn the_facts_of_a_unit_expression() {
    use crate::units::UnitKind;
    let (reg, _) = conf::build_registry_from_conf();
    let facts = |text: &str| reg.facts(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    let dimension = |pairs: &[(&str, i64)]| pairs.iter().map(|(s, e)| (s.to_string(), (*e, 1))).collect::<Vec<_>>();

    // a compound of recipe units is written out in base symbols and named by its recipe
    let watt = facts("J/s");
    assert_eq!(watt.dimension, dimension(&[("L", 2), ("M", 1), ("T", -3)]));
    assert_eq!((watt.alias.as_str(), watt.simplified.as_str()), ("W", "kg·m²/s³"));
    assert_eq!((watt.kind, watt.is_linear), (UnitKind::Delta, true));
    assert_eq!(facts("N*m").alias, "J");
    assert_eq!(facts("kg*m/s^2").alias, "N");
    assert_eq!(facts("Hz").simplified, "1/s");
    assert_eq!(facts("Hz").alias, "Hz");
    assert_eq!(facts("Pa").simplified, "kg/(m·s²)");
    // a unit at another scale stays what it is
    for (kept, shown) in [("km/h", "km/h"), ("L/min", "L/min"), ("BTU/h", "BTU/h"), ("g/cm^3", "g/cm³"), ("ft*lbf", "ft·lbf")] {
        assert_eq!(facts(kept).simplified, shown, "{kept}");
    }
    assert_eq!(facts("km/h").dimension, dimension(&[("L", 1), ("T", -1)]));
    assert_eq!(facts("g/cm^3").alias, "g/cm³");
    assert_eq!(facts("ft*lbf").alias, "ft·lbf");
    // temperature scales and levels
    let celsius = facts("degC");
    assert_eq!((celsius.kind, celsius.is_linear), (UnitKind::Point, false));
    assert_eq!(celsius.dimension, dimension(&[("O", 1)]));
    assert_eq!((facts("delta_celsius").kind, facts("delta_celsius").is_linear), (UnitKind::Delta, true));
    let decibel = facts("dB");
    assert!(decibel.dimension.is_empty());
    assert_eq!((decibel.kind, decibel.is_linear), (UnitKind::Delta, false));
    // the unit one is nothing to write; a prefixed unit that happens to be 1 is a unit
    assert_eq!(facts("unity").simplified, "1");
    assert_eq!(facts("hectopercent").simplified, "hectopercent");
    // a unit written by its symbol is shown by its shortest name, by another name as written
    assert_eq!(facts("deg").alias, "°");
    assert_eq!(facts("gallon").alias, "gallon");
    assert_eq!(facts("kilometer").alias, "km");
    assert_eq!(facts("attoampere").alias, "aA");
    assert_eq!(facts("kilometer").simplified, "kilometer");
    assert!(reg.facts("furlongs_per_fortnight").is_err());
}

/// A unit written as symbols stands for what the registry says each symbol is, with exact scales:
/// `km/h` is 5/18 m/s, and the factor between two of them is rounded once.
#[test]
fn a_unit_written_as_symbols_expands_and_converts_by_the_registry() {
    use crate::units::parser::Parser;
    let (reg, _) = conf::build_registry_from_conf();
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();

    let kmh = reg.expand(&atomic("km/h")).unwrap();
    assert!(kmh.same_dimensions(&reg.expand(&atomic("m/s")).unwrap()));
    assert_eq!(kmh.scale, 1000.0 / 3600.0);

    assert_eq!(reg.factor_between(&atomic("km/h"), &atomic("m/s")).unwrap(), 1000.0 / 3600.0);
    assert_eq!(reg.factor_between(&atomic("kWh"), &atomic("J")).unwrap(), 3.6e6);
    assert_eq!(reg.factor_between(&atomic("kg*m/s^2"), &atomic("N")).unwrap(), 1.0);
    // the offset of a temperature scale is not part of a factor
    assert_eq!(reg.factor_between(&atomic("degC"), &atomic("K")).unwrap(), 1.0);

    assert!(reg.factor_between(&atomic("m"), &atomic("s")).is_err());
    assert!(reg.expand(&atomic("furlongs_per_fortnight")).is_err());
    assert!(reg.expand(&atomic("dB/km")).is_err());
}
