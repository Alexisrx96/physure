//! A chemical species and its molar mass.
//!
//! The uncertainty of the molar mass combines the tabulated uncertainty of each element in
//! quadrature, weighted by the atom count (JCGM 100:2008 §5.1; the atoms are independent).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::chemistry::elements::atomic_weight;
use crate::chemistry::formula::{parse_formula, Composition};
use crate::error::PhysureResult;
use crate::quantity::Quantity;
use crate::uncertainty::new_source_id;

/// The one uncertainty source of each element for the life of the process: the tabulated
/// uncertainty of carbon is the same measurement in CO2 and in C8H18, and in every call.
fn element_source(symbol: &str) -> u32 {
    static IDS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    let mut ids = IDS.get_or_init(Mutex::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *ids.entry(symbol.to_string()).or_insert_with(new_source_id)
}

/// A compound or an element, read from a formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Species {
    /// The formula as it was written (`"H₂O"` stays `"H₂O"`).
    pub formula: String,
    composition: Composition,
}

impl Species {
    pub fn new(formula: &str) -> PhysureResult<Self> {
        Ok(Species { formula: formula.to_string(), composition: parse_formula(formula)? })
    }

    /// Atoms by element symbol.
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    /// `(molar mass in g/mol, its standard uncertainty)`.
    pub fn molar_mass_parts(&self) -> (f64, f64) {
        let (mut mass, mut variance) = (0.0, 0.0);
        for (element, &count) in &self.composition {
            // the composition is only built by `parse_formula`, which checks every symbol
            let (weight, std) = atomic_weight(element).expect("a parsed element has a weight");
            mass += f64::from(count) * weight;
            variance += (f64::from(count) * std).powi(2);
        }
        (mass, variance.sqrt())
    }

    /// The molar mass as a quantity in g/mol, with its uncertainty.
    ///
    /// Each element is one stable source, so the molar mass cancels against itself (`n = m / M`
    /// and back is `m` again) and two species with a common element are correlated through it.
    /// The quantity is Gaussian whatever the global propagation mode is: it carries sources.
    pub fn molar_mass(&self) -> PhysureResult<Quantity> {
        let unit = Quantity::new(1.0, "g/mol")?.unit;
        let mut mass = 0.0;
        let mut sources = Vec::with_capacity(self.composition.len());
        for (element, &count) in &self.composition {
            // the composition is only built by `parse_formula`, which checks every symbol
            let (weight, std) = atomic_weight(element).expect("a parsed element has a weight");
            mass += f64::from(count) * weight;
            sources.push((element_source(element), f64::from(count) * std));
        }
        Ok(Quantity::from_sources(mass, sources, unit))
    }
}

/// A mass as an amount of substance. The molar mass is measured too, and its uncertainty is in
/// the result. A quantity that is not a mass is a unit mismatch.
pub fn mass_to_moles(mass: &Quantity, species: &Species) -> PhysureResult<Quantity> {
    mass.to("g")?.div(&species.molar_mass()?)?.to("mol")
}

/// An amount of substance as a mass in grams, with the uncertainty of the molar mass in it.
pub fn moles_to_mass(moles: &Quantity, species: &Species) -> PhysureResult<Quantity> {
    moles.to("mol")?.mul(&species.molar_mass()?)?.to("g")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn water_is_18_015_with_its_atoms_uncertainties_in_quadrature() {
        let (mass, std) = Species::new("H2O").unwrap().molar_mass_parts();
        assert!(close(mass, 2.0 * 1.008 + 15.999));
        // sqrt((2 * 0.0002)^2 + 0.001^2)
        assert!(close(std, (0.0004_f64.powi(2) + 0.001_f64.powi(2)).sqrt()));
    }

    #[test]
    fn calcium_nitrate_and_copper_sulfate_pentahydrate() {
        let (mass, _) = Species::new("Ca(NO3)2").unwrap().molar_mass_parts();
        assert!(close(mass, 40.078 + 2.0 * 14.007 + 6.0 * 15.999));
        let (mass, _) = Species::new("CuSO4·5H2O").unwrap().molar_mass_parts();
        assert!(close(mass, 63.546 + 32.06 + 9.0 * 15.999 + 10.0 * 1.008));
    }

    #[test]
    fn the_molar_mass_is_a_quantity_in_g_per_mol() {
        let q = Species::new("CO2").unwrap().molar_mass().unwrap();
        assert!(close(q.value.mean(), 12.011 + 2.0 * 15.999));
        assert!(close(q.value.std_dev(), (0.001_f64.powi(2) + 0.002_f64.powi(2)).sqrt()));
        assert!(close(q.to("kg/mol").unwrap().value.mean(), 0.044009));
    }

    #[test]
    fn a_species_keeps_the_formula_it_was_given() {
        let s = Species::new("H₂O").unwrap();
        assert_eq!(s.formula, "H₂O");
        assert_eq!(s.composition()["H"], 2);
    }

    #[test]
    fn a_bad_formula_is_an_error() {
        assert!(Species::new("H2O)").is_err());
    }

    #[test]
    fn fifty_grams_of_co2_with_both_uncertainties() {
        let co2 = Species::new("CO2").unwrap();
        let mass = Quantity::with_uncertainty(50.0, 0.1, "g").unwrap();
        let n = mass_to_moles(&mass, &co2).unwrap();
        let m_co2 = 12.011 + 2.0 * 15.999;
        let u_m = (0.001_f64.powi(2) + 0.002_f64.powi(2)).sqrt();
        let expected = 50.0 / m_co2;
        let rel = ((0.1_f64 / 50.0).powi(2) + (u_m / m_co2).powi(2)).sqrt();
        assert!(close(n.value.mean(), expected));
        assert!(close(n.value.std_dev(), expected * rel));
        assert!(close(n.to("mol").unwrap().value.mean(), expected));
    }

    #[test]
    fn other_mass_units_are_converted() {
        let co2 = Species::new("CO2").unwrap();
        let kilo = Quantity::new(0.05, "kg").unwrap();
        let n = mass_to_moles(&kilo, &co2).unwrap();
        assert!(close(n.value.mean(), 50.0 / (12.011 + 2.0 * 15.999)));
    }

    #[test]
    fn a_quantity_that_is_not_a_mass_is_refused() {
        let co2 = Species::new("CO2").unwrap();
        assert!(mass_to_moles(&Quantity::new(1.0, "s").unwrap(), &co2).is_err());
        assert!(moles_to_mass(&Quantity::new(1.0, "g").unwrap(), &co2).is_err());
    }

    #[test]
    fn moles_to_mass_of_water() {
        let water = Species::new("H2O").unwrap();
        let n = Quantity::with_uncertainty(2.0, 0.01, "mol").unwrap();
        let mass = moles_to_mass(&n, &water).unwrap();
        let m = 2.0 * 1.008 + 15.999;
        assert!(close(mass.value.mean(), 2.0 * m));
        assert!(close(mass.to("kg").unwrap().value.mean(), 2.0 * m / 1000.0));
        let (_, u_m) = water.molar_mass_parts();
        let rel = ((0.01_f64 / 2.0).powi(2) + (u_m / m).powi(2)).sqrt();
        assert!(close(mass.value.std_dev(), 2.0 * m * rel));
    }

    fn relative(q: &Quantity) -> f64 {
        q.value.std_dev() / q.value.mean()
    }

    #[test]
    fn the_same_molar_mass_cancels_on_the_way_back() {
        let co2 = Species::new("CO2").unwrap();
        let mass = Quantity::with_uncertainty(50.0, 0.1, "g").unwrap();
        let back = moles_to_mass(&mass_to_moles(&mass, &co2).unwrap(), &co2).unwrap();
        assert!(close(back.value.mean(), 50.0));
        let (got, want) = (relative(&back), relative(&mass));
        assert!((got - want).abs() <= 1e-9 * want, "relative uncertainty {got} vs {want}");
    }

    #[test]
    fn species_sharing_an_element_are_correlated_through_it() {
        let a = Species::new("CO2").unwrap().molar_mass().unwrap();
        let again = Species::new("CO2").unwrap().molar_mass().unwrap();
        let zero = a.sub(&a).unwrap();
        assert_eq!((zero.value.mean(), zero.value.std_dev()), (0.0, 0.0));
        // two separate calls are the same measurement
        assert_eq!(a.sub(&again).unwrap().value.std_dev(), 0.0);
        // C8H18 and CO2 share carbon: M(C8H18) - 8 M(CO2) leaves only H and O, never the carbon
        let octane = Species::new("C8H18").unwrap().molar_mass().unwrap();
        let eight = Quantity::new(8.0, "").unwrap();
        let diff = octane.sub(&a.mul(&eight).unwrap()).unwrap();
        let (_, u_h) = Species::new("H18").unwrap().molar_mass_parts();
        let (_, u_o) = Species::new("O16").unwrap().molar_mass_parts();
        assert!(close(diff.value.std_dev(), (u_h * u_h + u_o * u_o).sqrt()));
    }

    #[test]
    fn the_molar_mass_next_to_a_monte_carlo_mass() {
        // The molar mass is always Gaussian (it carries source ids). Divided into a Monte Carlo
        // mass it still works: the result is a finite amount with a spread (about 0.2 % here, the
        // sampled mass dominating), not an error and not a dropped uncertainty.
        let co2 = Species::new("CO2").unwrap();
        let mass = Quantity::new_scalar(50.0, 0.1, Quantity::new(1.0, "g").unwrap().unit, Some("monte_carlo"), Some(500));
        let n = mass_to_moles(&mass, &co2).unwrap();
        assert!((n.value.mean() - 50.0 / (12.011 + 2.0 * 15.999)).abs() < 0.01);
        let rel = relative(&n);
        assert!(rel > 0.001 && rel < 0.004, "relative uncertainty {rel}");
    }
}
