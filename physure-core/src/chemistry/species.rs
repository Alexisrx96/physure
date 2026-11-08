//! A chemical species and its molar mass.
//!
//! The uncertainty of the molar mass combines the tabulated uncertainty of each element in
//! quadrature, weighted by the atom count (JCGM 100:2008 §5.1; the atoms are independent).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::chemistry::formula::{parse_formula, Composition};
use crate::error::{PhysureError, PhysureResult};
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
    formula: String,
    composition: Composition,
}

impl Species {
    pub fn new(formula: &str) -> PhysureResult<Self> {
        Ok(Species { formula: formula.to_string(), composition: parse_formula(formula)? })
    }

    /// The formula as it was written (`"H₂O"` stays `"H₂O"`).
    pub fn formula(&self) -> &str {
        &self.formula
    }

    /// Atoms by element symbol.
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    /// For each element: `(symbol, count * weight, count * standard uncertainty)`, from the catalog in scope.
    fn terms(&self) -> PhysureResult<Vec<(String, f64, f64)>> {
        super::with_catalog_in_scope(|registry| {
            super::elements::check_table_is_defined(registry)?;
            self.composition
                .iter()
                .map(|(element, &count)| {
                    let (weight, std) = registry.atomic_weight(element).ok_or_else(|| {
                        PhysureError::DomainError(format!(
                            "no atomic weight for '{element}' in edition '{}'",
                            registry.selected_atomic_weight_edition()
                        ))
                    })?;
                    Ok((element.clone(), f64::from(count) * weight, f64::from(count) * std))
                })
                .collect()
        })
    }

    /// `(molar mass in g/mol, its standard uncertainty)`, with the atomic weights of the catalog in
    /// scope. An element the catalog has no weight for is an error.
    pub fn molar_mass_parts(&self) -> PhysureResult<(f64, f64)> {
        let terms = self.terms()?;
        let mass = terms.iter().map(|(_, weight, _)| weight).sum();
        let variance: f64 = terms.iter().map(|(_, _, std)| std * std).sum();
        Ok((mass, variance.sqrt()))
    }

    /// The molar mass as a quantity in g/mol, with its uncertainty.
    ///
    /// Each element is one stable source, so the molar mass cancels against itself (`n = m / M`
    /// and back is `m` again) and two species with a common element are correlated through it.
    /// The quantity is Gaussian whatever the global propagation mode is: it carries sources.
    pub fn molar_mass(&self) -> PhysureResult<Quantity> {
        let unit = Quantity::new(1.0, "g/mol")?.unit;
        let terms = self.terms()?;
        let mass = terms.iter().map(|(_, weight, _)| weight).sum();
        let sources = terms.iter().map(|(element, _, std)| (element_source(element), *std)).collect();
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

    // The CIAAW Standard Atomic Weights 2024 used below, written down from the table
    // (tests/golden/ciaaw_standard_atomic_weights_2024.tsv), never read back from the code under test.
    // An interval [a, b] is its midpoint, with standard uncertainty (b - a) / sqrt(12); a value x(n) is x,
    // with standard uncertainty (n units of the last digit) / sqrt(3).
    const H: f64 = (1.00784 + 1.00811) / 2.0; // [1.00784, 1.00811]
    const C: f64 = (12.0096 + 12.0116) / 2.0; // [12.0096, 12.0116]
    const N: f64 = (14.00643 + 14.00728) / 2.0; // [14.00643, 14.00728]
    const O: f64 = (15.99903 + 15.99977) / 2.0; // [15.99903, 15.99977]
    const S: f64 = (32.059 + 32.076) / 2.0; // [32.059, 32.076]
    const CA: f64 = 40.078; // 40.078(4)
    const CU: f64 = 63.546; // 63.546(3)

    fn u_h() -> f64 {
        (1.00811 - 1.00784) / 12f64.sqrt()
    }

    fn u_c() -> f64 {
        (12.0116 - 12.0096) / 12f64.sqrt()
    }

    fn u_o() -> f64 {
        (15.99977 - 15.99903) / 12f64.sqrt()
    }

    #[test]
    fn water_is_18_015_with_its_atoms_uncertainties_in_quadrature() {
        let (mass, std) = Species::new("H2O").unwrap().molar_mass_parts().unwrap();
        assert!(close(mass, 2.0 * H + O));
        // H and O are intervals: u(H) = 0.00027 / sqrt(12), u(O) = 0.00074 / sqrt(12); sqrt((2 u(H))^2 + u(O)^2)
        assert!(close(std, ((2.0 * u_h()).powi(2) + u_o().powi(2)).sqrt()));
    }

    #[test]
    fn the_molar_mass_uncertainty_is_the_iupac_quadrature_of_the_standard_uncertainties() {
        // IUPAC, "Interpretation and use of standard atomic weights": u^2(Mr) = sum over elements of
        // nu_E^2 u^2(Ar(E)), with u = (b - a) / sqrt(12) for the CIAAW intervals (H [1.00784, 1.00811], O [15.99903, 15.99977]).
        let (_, std) = Species::new("H2O").unwrap().molar_mass_parts().unwrap();
        let expected = ((2.0 * (1.00811 - 1.00784) / 12f64.sqrt()).powi(2) + ((15.99977 - 15.99903) / 12f64.sqrt()).powi(2)).sqrt();
        assert!((std - expected).abs() <= 1e-12 * expected, "{std} vs {expected}");
    }

    #[test]
    fn calcium_nitrate_and_copper_sulfate_pentahydrate() {
        let (mass, _) = Species::new("Ca(NO3)2").unwrap().molar_mass_parts().unwrap();
        assert!(close(mass, CA + 2.0 * N + 6.0 * O));
        let (mass, _) = Species::new("CuSO4·5H2O").unwrap().molar_mass_parts().unwrap();
        assert!(close(mass, CU + S + 9.0 * O + 10.0 * H));
    }

    #[test]
    fn the_molar_mass_is_a_quantity_in_g_per_mol() {
        let q = Species::new("CO2").unwrap().molar_mass().unwrap();
        assert!(close(q.value.mean(), C + 2.0 * O));
        assert!(close(q.value.std_dev(), (u_c().powi(2) + (2.0 * u_o()).powi(2)).sqrt()));
        // 12.0106 + 2 * 15.9994 = 44.0094 g/mol
        assert!(close(q.to("kg/mol").unwrap().value.mean(), 0.0440094));
    }

    #[test]
    fn a_species_keeps_the_formula_it_was_given() {
        let s = Species::new("H₂O").unwrap();
        assert_eq!(s.formula(), "H₂O");
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
        let m_co2 = C + 2.0 * O;
        let u_m = (u_c().powi(2) + (2.0 * u_o()).powi(2)).sqrt();
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
        assert!(close(n.value.mean(), 50.0 / (C + 2.0 * O)));
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
        let m = 2.0 * H + O;
        assert!(close(mass.value.mean(), 2.0 * m));
        assert!(close(mass.to("kg").unwrap().value.mean(), 2.0 * m / 1000.0));
        let (_, u_m) = water.molar_mass_parts().unwrap();
        let rel = ((0.01_f64 / 2.0).powi(2) + (u_m / m).powi(2)).sqrt();
        assert!(close(mass.value.std_dev(), 2.0 * m * rel));
    }

    #[test]
    fn the_molar_mass_uses_the_weights_of_the_catalog_in_scope() {
        use crate::units::conf::build_registry_from_conf;
        let (mut registry, _) = build_registry_from_conf();
        registry.atomic_weights.insert("H".into(), (2.0, 0.0));
        let h2 = Species::new("H2").unwrap();
        assert_eq!(crate::units::with_catalog(&registry, || h2.molar_mass_parts()).unwrap(), (4.0, 0.0));
        let (mass, std) = h2.molar_mass_parts().unwrap();
        assert!(close(mass, 2.0 * H) && close(std, 2.0 * u_h()), "outside the scope it is the default");
    }

    /// The embedded catalog with `texts` applied in order.
    fn catalog_with(texts: &[&str]) -> crate::units::UnitRegistry {
        use crate::units::conf::{build_registry_from_conf, extend_registry};
        let (mut registry, mut constants) = build_registry_from_conf();
        for text in texts {
            extend_registry(&mut registry, &mut constants, text).unwrap();
        }
        registry
    }

    #[test]
    fn the_abridged_edition_is_used_when_it_is_selected_and_only_inside_that_scope() {
        let registry = catalog_with(&["[Chemistry]\natomic_weights = abridged-2024\n"]);
        let water = Species::new("H2O").unwrap();
        let (mass, std) = crate::units::with_catalog(&registry, || water.molar_mass_parts()).unwrap();
        // abridged-2024: H 1.0080 +- 0.0002, O 15.999 +- 0.001, each +- read as a half-width, u = (+-) / sqrt(3);
        // IUPAC (van der Veen et al.): u^2(Mr) = sum over the elements of nu_E^2 u^2(Ar(E))
        let expected_mass = 2.0 * 1.0080 + 15.999;
        assert!((mass - expected_mass).abs() <= 1e-12 * expected_mass, "{mass}");
        let expected = ((2.0 * 0.0002 / 3f64.sqrt()).powi(2) + (0.001 / 3f64.sqrt()).powi(2)).sqrt();
        assert!((std - expected).abs() <= 1e-12 * expected, "{std} vs {expected}");
        // outside the scope it is standard-2024: 2 * 1.007975 + 15.9994
        let (mass, _) = water.molar_mass_parts().unwrap();
        assert!(close(mass, 2.0 * 1.007975 + 15.9994), "{mass}");
        // the quantity reads the same table
        let q = crate::units::with_catalog(&registry, || water.molar_mass()).unwrap();
        assert!(close(q.value.mean(), 2.0 * 1.0080 + 15.999));
        // and a report can say which table it was
        assert_eq!(crate::units::with_catalog(&registry, crate::chemistry::atomic_weight_edition), "abridged-2024");
        assert_eq!(crate::chemistry::atomic_weight_edition(), "standard-2024");
    }

    #[test]
    fn a_definition_of_yours_wins_under_every_edition() {
        let h2 = Species::new("H2").unwrap();
        for selection in ["standard", "standard-2024", "abridged", "abridged-2024"] {
            let text = format!("[AtomicWeights]\nH = 2.0, 0\n[Chemistry]\natomic_weights = {selection}\n");
            let registry = catalog_with(&[&text]);
            assert_eq!(crate::units::with_catalog(&registry, || h2.molar_mass_parts()).unwrap(), (4.0, 0.0), "{selection}");
        }
    }

    #[test]
    fn a_table_of_your_own_has_no_fallback_and_the_error_names_the_element_and_the_edition() {
        let registry = catalog_with(&[
            "[AtomicWeights:acme-2026-03]\nH = 1.00794, 0.00007\nO = 15.9994, 0.0003\n[Chemistry]\natomic_weights = acme-2026-03\n",
        ]);
        let water = crate::units::with_catalog(&registry, || Species::new("H2O")).unwrap();
        let (mass, _) = crate::units::with_catalog(&registry, || water.molar_mass_parts()).unwrap();
        assert!(close(mass, 2.0 * 1.00794 + 15.9994), "{mass}");
        assert_eq!(crate::units::with_catalog(&registry, crate::chemistry::atomic_weight_edition), "acme-2026-03");
        // Na is in neither of the tables of this edition: nothing is taken from the shipped ones
        let err = crate::units::with_catalog(&registry, || Species::new("NaCl")).unwrap_err();
        assert!(matches!(err, PhysureError::ParseError(_)), "{err:?}");
        let text = err.to_string();
        assert!(text.contains("'Na'") && text.contains("acme-2026-03"), "{text}");
        // the same species, outside the scope, is read from standard-2024
        assert!(Species::new("NaCl").is_ok());
    }

    #[test]
    fn an_undefined_selected_edition_fails_every_use_naming_it() {
        let registry = catalog_with(&["[Chemistry]\natomic_weights = nope-1\n"]);
        let err = crate::units::with_catalog(&registry, || Species::new("H2O")).unwrap_err();
        assert!(matches!(err, PhysureError::DomainError(_)), "{err:?}");
        assert!(err.to_string().contains("nope-1"), "{err}");
        // a species made outside the scope and used inside it fails the same way
        let water = Species::new("H2O").unwrap();
        let (parts, quantity) = crate::units::with_catalog(&registry, || (water.molar_mass_parts(), water.molar_mass()));
        assert!(parts.unwrap_err().to_string().contains("nope-1"));
        assert!(quantity.unwrap_err().to_string().contains("nope-1"));
        assert_eq!(crate::units::with_catalog(&registry, crate::chemistry::atomic_weight_edition), "nope-1");
    }

    #[test]
    fn an_element_the_catalog_lacks_is_an_error_naming_it() {
        use crate::units::conf::build_registry_from_conf;
        let zirconia = Species::new("ZrO2").unwrap();
        let (mut registry, _) = build_registry_from_conf();
        registry.atomic_weight_tables.get_mut("standard-2024").unwrap().remove("Zr");
        let (parts, quantity) =
            crate::units::with_catalog(&registry, || (zirconia.molar_mass_parts(), zirconia.molar_mass()));
        let parts = parts.unwrap_err().to_string();
        assert!(parts.contains("'Zr'") && parts.contains("standard-2024"), "{parts}");
        assert!(quantity.unwrap_err().to_string().contains("'Zr'"));
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
        let (_, u_h) = Species::new("H18").unwrap().molar_mass_parts().unwrap();
        let (_, u_o) = Species::new("O16").unwrap().molar_mass_parts().unwrap();
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
        assert!((n.value.mean() - 50.0 / (C + 2.0 * O)).abs() < 0.01);
        let rel = relative(&n);
        assert!(rel > 0.001 && rel < 0.004, "relative uncertainty {rel}");
    }
}
