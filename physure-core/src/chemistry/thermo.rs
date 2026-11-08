//! Kinetics and thermodynamics: Arrhenius, Clausius-Clapeyron, Gibbs.
//!
//! They work on `Quantity`, so a dimension that does not fit is an error and the uncertainty of
//! every input is in the result. The gas constant is the catalog's `molar_gas_constant` (exact
//! since the 2019 SI), read from the catalog in scope, not written again here.
//!
//! References: Arrhenius, S. (1889), Z. Phys. Chem. 4, 226-248,
//! <https://doi.org/10.1515/zpch-1889-0416>; Gibbs, J. W. (1876), Trans. Conn. Acad. Arts Sci. 3,
//! 108-248, <https://doi.org/10.1080/14786447808639542>.

use crate::error::{PhysureError, PhysureResult};
use crate::quantity::Quantity;
use crate::chemistry::with_catalog_in_scope;
use crate::units::{RationalUnit, UnitRegistry};

fn read_gas_constant(registry: &UnitRegistry) -> PhysureResult<Quantity> {
    let missing = || PhysureError::Generic("the catalog defines no molar_gas_constant".into());
    let value = &registry.constants_meta.get("molar_gas_constant").ok_or_else(missing)?.value;
    let (number, unit) = value.split_once(char::is_whitespace).ok_or_else(missing)?;
    let number: f64 = number
        .parse()
        .map_err(|_| PhysureError::Generic(format!("molar_gas_constant is not a number: {value:?}")))?;
    Quantity::new(number, unit.trim())
}

/// The molar gas constant `R` of the catalog in scope, in J/(mol·K).
pub fn gas_constant() -> PhysureResult<Quantity> {
    with_catalog_in_scope(read_gas_constant)
}

fn gas_or(r: Option<&Quantity>) -> PhysureResult<Quantity> {
    match r {
        Some(r) => Ok(r.clone()),
        None => gas_constant(),
    }
}

/// Arrhenius equation: `k = A * exp(-Ea / (R T))`. `r` is `R`, the catalog's when `None`.
pub fn arrhenius(a: &Quantity, ea: &Quantity, t: &Quantity, r: Option<&Quantity>) -> PhysureResult<Quantity> {
    let exponent = ea.div(&gas_or(r)?.mul(t)?)?.convert_to(&RationalUnit::dimensionless())?;
    // `a * exp(-x)`, not `a / exp(x)`: for x above about 709 the exponential is infinite and the
    // rate (and its uncertainty) would come out 0 or NaN where `exp(-x)` is a real small number.
    a.mul(&exponent.mul(&Quantity::new(-1.0, "")?)?.exp()?)
}

/// Clausius-Clapeyron relation: the pressure `P2` at `t2`, given `p1` at `t1` and the enthalpy of
/// vaporisation. `ln(P2/P1) = -(dH/R) (1/T2 - 1/T1)`.
pub fn clausius_clapeyron(
    delta_h_vap: &Quantity,
    t1: &Quantity,
    p1: &Quantity,
    t2: &Quantity,
    r: Option<&Quantity>,
) -> PhysureResult<Quantity> {
    let one = Quantity::new(1.0, "")?;
    let inverse = |t: &Quantity| one.div(&t.to("K")?);
    let exponent = delta_h_vap
        .div(&gas_or(r)?)?
        .mul(&inverse(t1)?.sub(&inverse(t2)?)?)?
        .convert_to(&RationalUnit::dimensionless())?;
    p1.mul(&exponent.exp()?)
}

/// Gibbs free energy: `dG = dH - T dS`.
pub fn gibbs_free_energy(delta_h: &Quantity, t: &Quantity, delta_s: &Quantity) -> PhysureResult<Quantity> {
    let entropy_term = t.mul(delta_s)?.convert_to(&delta_h.unit)?;
    delta_h.sub(&entropy_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1e-300)
    }

    const R: f64 = 8.31446261815324;

    #[test]
    fn the_gas_constant_is_the_catalogs() {
        let r = gas_constant().unwrap();
        assert!(close(r.value.mean(), R));
        assert_eq!(r.value.std_dev(), 0.0);
        assert!(close(r.to("J/(mol*K)").unwrap().value.mean(), R));
    }

    #[test]
    fn arrhenius_against_the_closed_form() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        let expected = 1e13 * (-75_000.0 / (R * 298.15)).exp();
        assert!(close(k.to("s^-1").unwrap().value.mean(), expected));
        assert!((expected - 0.73).abs() < 0.005, "k is 0.73 s^-1, got {expected}");
    }

    #[test]
    fn arrhenius_keeps_the_uncertainty_of_the_activation_energy() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::with_uncertainty(75.0, 1.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        // dk/k = (dEa / (R T)); the uncertainty is not dropped
        let rel = 1000.0 / (R * 298.15);
        assert!(close(k.value.std_dev() / k.value.mean(), rel));
    }

    #[test]
    fn arrhenius_with_a_given_gas_constant() {
        let a = Quantity::new(1.0, "s^-1").unwrap();
        let ea = Quantity::new(1000.0, "J/mol").unwrap();
        let t = Quantity::new(300.0, "K").unwrap();
        let r = Quantity::new(8.0, "J/(mol*K)").unwrap();
        let k = arrhenius(&a, &ea, &t, Some(&r)).unwrap();
        assert!(close(k.value.mean(), (-1000.0_f64 / (8.0 * 300.0)).exp()));
    }

    #[test]
    fn an_activation_energy_in_kelvin_is_a_dimension_error() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "K").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        assert!(arrhenius(&a, &ea, &t, None).is_err());
    }

    #[test]
    fn gibbs_free_energy_of_water_formation_is_minus_237_1() {
        let dh = Quantity::new(-285.8, "kJ/mol").unwrap();
        let ds = Quantity::new(-163.2, "J/(mol*K)").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let g = gibbs_free_energy(&dh, &t, &ds).unwrap();
        let expected = -285.8 + 298.15 * 163.2 / 1000.0;
        assert!(close(g.to("kJ/mol").unwrap().value.mean(), expected));
        assert!((expected + 237.1).abs() < 0.05);
    }

    #[test]
    fn gibbs_refuses_an_entropy_that_is_an_energy() {
        let dh = Quantity::new(-285.8, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        assert!(gibbs_free_energy(&dh, &t, &dh).is_err());
    }

    #[test]
    fn clausius_clapeyron_for_water_cooling_to_90_c() {
        let dh = Quantity::new(40.7, "kJ/mol").unwrap();
        let t1 = Quantity::new(373.15, "K").unwrap();
        let t2 = Quantity::new(363.15, "K").unwrap();
        let p1 = Quantity::new(101.325, "kPa").unwrap();
        let p2 = clausius_clapeyron(&dh, &t1, &p1, &t2, None).unwrap();
        let expected = 101.325 * (-(40_700.0 / R) * (1.0 / 363.15 - 1.0 / 373.15)).exp();
        assert!(close(p2.to("kPa").unwrap().value.mean(), expected));
        assert!(expected > 69.5 && expected < 71.5, "about 70 kPa, got {expected}");
    }

    #[test]
    fn clausius_clapeyron_round_trips() {
        let dh = Quantity::new(40.7, "kJ/mol").unwrap();
        let t1 = Quantity::new(373.15, "K").unwrap();
        let t2 = Quantity::new(363.15, "K").unwrap();
        let p1 = Quantity::new(101.325, "kPa").unwrap();
        let p2 = clausius_clapeyron(&dh, &t1, &p1, &t2, None).unwrap();
        let back = clausius_clapeyron(&dh, &t2, &p2, &t1, None).unwrap();
        assert!(close(back.to("kPa").unwrap().value.mean(), 101.325));
    }

    #[test]
    fn arrhenius_with_a_huge_barrier_underflows_instead_of_turning_nan() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::with_uncertainty(2000.0, 10.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        assert!(k.value.mean().is_finite() && k.value.mean() >= 0.0 && k.value.mean() < 1e-300);
        assert!(!k.value.std_dev().is_nan(), "std_dev {}", k.value.std_dev());
    }

    #[test]
    fn arrhenius_where_the_exponential_overflows_but_the_rate_does_not() {
        // Ea / RT is about 718: exp(718) is infinite, 1e13 * exp(-718) is a real (tiny) rate.
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::with_uncertainty(1780.0, 1.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        let x = 1_780_000.0 / (R * 298.15);
        assert!(x > 710.0);
        let expected = 1e13 * (-x).exp();
        assert!(expected > 0.0);
        assert!((k.value.mean() - expected).abs() <= 1e-6 * expected, "{} vs {expected}", k.value.mean());
        // (the spread itself is about 0.4 * k, whose square is below the smallest f64, so the
        // quadrature sum of the core reports 0; what matters is that it is not NaN)
        assert!(k.value.std_dev().is_finite());
    }

    #[test]
    fn arrhenius_keeps_the_uncertainty_of_the_temperature() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "kJ/mol").unwrap();
        let t = Quantity::with_uncertainty(298.15, 0.5, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        // dk/k = Ea / (R T^2) * dT
        let rel = 75_000.0 / (R * 298.15 * 298.15) * 0.5;
        assert!(close(k.value.std_dev() / k.value.mean(), rel));
    }

    #[test]
    fn arrhenius_keeps_the_uncertainty_of_a_given_gas_constant() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let r = Quantity::with_uncertainty(8.3, 0.01, "J/(mol*K)").unwrap();
        let k = arrhenius(&a, &ea, &t, Some(&r)).unwrap();
        // dk/k = (Ea / (R T)) * dR / R
        let rel = 75_000.0 / (8.3 * 298.15) * 0.01 / 8.3;
        assert!(close(k.value.std_dev() / k.value.mean(), rel));
    }

    #[test]
    fn arrhenius_takes_an_absolute_temperature_in_celsius() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "kJ/mol").unwrap();
        let kelvin = arrhenius(&a, &ea, &Quantity::new(298.15, "K").unwrap(), None).unwrap();
        let celsius = arrhenius(&a, &ea, &Quantity::new(25.0, "degC").unwrap(), None).unwrap();
        assert!(close(celsius.to("s^-1").unwrap().value.mean(), kelvin.to("s^-1").unwrap().value.mean()));
    }

    #[test]
    fn clausius_clapeyron_keeps_the_uncertainty_of_the_enthalpy() {
        let dh = Quantity::with_uncertainty(40.7, 0.4, "kJ/mol").unwrap();
        let t1 = Quantity::new(373.15, "K").unwrap();
        let t2 = Quantity::new(363.15, "K").unwrap();
        let p1 = Quantity::new(101.325, "kPa").unwrap();
        let p2 = clausius_clapeyron(&dh, &t1, &p1, &t2, None).unwrap();
        // dP/P = (dH / R) (1/T1 - 1/T2) * (u_dH / dH)
        let rel = (40_700.0 / R) * (1.0 / 373.15 - 1.0 / 363.15) * (0.4 / 40.7);
        assert!(p2.value.std_dev() > 0.0);
        let got = p2.value.std_dev() / p2.value.mean();
        assert!((got - rel.abs()).abs() <= 1e-6 * rel.abs(), "{got} vs {rel}");
    }

    #[test]
    fn the_gas_constant_is_read_from_the_catalog_in_scope() {
        let (mut registry, _) = crate::units::conf::build_registry_from_conf();
        registry.constants_meta.get_mut("molar_gas_constant").unwrap().value = "8.0 J/(mol*K)".into();
        let scoped = crate::units::with_catalog(&registry, || gas_constant().unwrap());
        assert!(close(scoped.value.mean(), 8.0));
        assert!(close(gas_constant().unwrap().value.mean(), R));
    }

    fn constant(value: &str) -> UnitRegistry {
        let mut registry = UnitRegistry::new();
        registry.constants_meta.insert(
            "molar_gas_constant".into(),
            crate::units::registry::ConstantMetadata { value: value.into(), description: None, latex_symbol: None },
        );
        registry
    }

    #[test]
    fn a_catalog_without_a_usable_gas_constant_is_an_error() {
        let message = |registry: &UnitRegistry| read_gas_constant(registry).unwrap_err().to_string();
        assert!(message(&UnitRegistry::new()).contains("no molar_gas_constant"));
        assert!(read_gas_constant(&constant("8.314")).is_err(), "a value with no unit");
        assert!(message(&constant("eight J/(mol*K)")).contains("not a number"));
    }
}
