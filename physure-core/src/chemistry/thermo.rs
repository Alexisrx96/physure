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
use crate::units::conf::with_conf_registry;
use crate::units::{with_current_catalog, RationalUnit, UnitRegistry};

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
    with_current_catalog(|scoped| match scoped {
        Some(registry) => read_gas_constant(registry),
        None => with_conf_registry(read_gas_constant),
    })
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
    a.div(&exponent.exp()?)
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
}
