//! Chemistry: molar masses, an amount of substance, balancing, and the Arrhenius, Clausius-Clapeyron and Gibbs
//! relations.
//!
//! Atomic weights come from the configuration in force (the selected edition and the user's `[AtomicWeights]`
//! definitions), the standard enthalpies and entropies likewise (`[Chemistry] thermochemistry` and the user's
//! `[Thermochemistry]` definitions), and the gas constant is its `molar_gas_constant`, so a changed table or constant
//! shows in the witnesses that use it. An element the selected table does not have is a recorded failure (`failed=...`), not an
//! error. The cases that combine uncertain quantities run under a thread-local `correlated` propagation mode, so
//! the process's `[Settings] propagation_mode` does not reach them; balancing reads no setting and no catalog.

use super::units::witness_of;
use super::{case, Case, CaseResult};
use crate::chemistry::{
    arrhenius, clausius_clapeyron, gibbs_free_energy, mass_to_moles, standard_enthalpy, standard_entropy, Reaction, Species,
};
use crate::frozen::witness::WitnessValue;
use crate::quantity::Quantity;
use crate::uncertainty::mode::scoped;
use crate::uncertainty::PropagationMode;

/// Runs `f` under the correlated propagation mode on this thread only.
fn correlated(f: impl FnOnce() -> CaseResult) -> CaseResult {
    let _mode = scoped(PropagationMode::Correlated);
    f()
}

/// The molar mass of `formula` in g/mol, with its uncertainty.
fn molar_mass(formula: &str) -> CaseResult {
    correlated(|| Ok(witness_of(&Species::new(formula)?.molar_mass()?)))
}

/// The canonical text of a balanced reaction (decision S10): `<reactants> -> <products>`, every coefficient
/// written as an integer (1 included), the species in the order of the input, single spaces.
fn balanced(equation: &str) -> CaseResult {
    let reaction = Reaction::from_string(equation)?;
    let side = |species: &[Species], coefficients: &[i64]| {
        species.iter().zip(coefficients).map(|(s, c)| format!("{c} {}", s.formula())).collect::<Vec<_>>().join(" + ")
    };
    Ok(WitnessValue::text(&format!(
        "{} -> {}",
        side(reaction.reactants(), reaction.reactant_coeffs()),
        side(reaction.products(), reaction.product_coeffs())
    )))
}

pub(super) fn cases() -> Vec<Case> {
    vec![
        case("chem.molar_mass_h2o", || molar_mass("H2O")),
        case("chem.molar_mass_glucose", || molar_mass("C6H12O6")),
        case("chem.molar_mass_calcium_hydroxide", || molar_mass("Ca(OH)2")),
        // iron(II) sulfate
        case("chem.molar_mass_iron_sulfate", || molar_mass("FeSO4")),
        // 100 g of sodium chloride, in mol
        case("chem.mass_to_moles_nacl", || {
            correlated(|| Ok(witness_of(&mass_to_moles(&Quantity::new(100.0, "g")?, &Species::new("NaCl")?)?)))
        }),
        case("chem.balance_propane_combustion", || balanced("C3H8 + O2 -> CO2 + H2O")),
        case("chem.balance_iron_oxide_reduction", || balanced("Fe2O3 + CO -> Fe + CO2")),
        // A = 1e13 s^-1, Ea = 75 kJ/mol, T = 298.15 K: k in s^-1
        case("chem.arrhenius", || {
            correlated(|| {
                let a = Quantity::new(1e13, "s^-1")?;
                let ea = Quantity::new(75.0, "kJ/mol")?;
                let t = Quantity::new(298.15, "K")?;
                Ok(witness_of(&arrhenius(&a, &ea, &t, None)?.to("s^-1")?))
            })
        }),
        // water: dH_vap = 40.7 kJ/mol, 101.325 kPa at 373.15 K; the pressure at 363.15 K in kPa
        case("chem.clausius_clapeyron_water", || {
            correlated(|| {
                let dh = Quantity::new(40.7, "kJ/mol")?;
                let t1 = Quantity::new(373.15, "K")?;
                let p1 = Quantity::new(101.325, "kPa")?;
                let t2 = Quantity::new(363.15, "K")?;
                Ok(witness_of(&clausius_clapeyron(&dh, &t1, &p1, &t2, None)?.to("kPa")?))
            })
        }),
        // the formation of liquid water at 298.15 K: dH = -285.8 kJ/mol, dS = -163.2 J/(mol K); dG in kJ/mol
        case("chem.gibbs_298", || {
            correlated(|| {
                let dh = Quantity::new(-285.8, "kJ/mol")?;
                let t = Quantity::new(298.15, "K")?;
                let ds = Quantity::new(-163.2, "J/(mol*K)")?;
                Ok(witness_of(&gibbs_free_energy(&dh, &t, &ds)?.to("kJ/mol")?))
            })
        }),
        // the standard data of liquid water from the thermochemistry in force (`legacy-python-1` in the shipped
        // catalog): dHf in kJ/mol, S in J/(mol K)
        case("chem.dhf_h2o_l", || correlated(|| Ok(witness_of(&standard_enthalpy("H2O(l)")?)))),
        case("chem.entropy_h2o_l", || correlated(|| Ok(witness_of(&standard_entropy("H2O(l)")?)))),
        // H2(g) + 1/2 O2(g) -> H2O(l) at 298.15 K from the table's entries: dG = dH - T dS in kJ/mol (unlike
        // chem.gibbs_298, whose dS is a literal of its own)
        case("chem.gibbs_formation_h2o_l", || {
            correlated(|| {
                let half = Quantity::new(0.5, "")?;
                let of_formation = |read: fn(&str) -> crate::PhysureResult<Quantity>| -> crate::PhysureResult<Quantity> {
                    read("H2O(l)")?.sub(&read("H2(g)")?)?.sub(&half.mul(&read("O2(g)")?)?)
                };
                let t = Quantity::new(298.15, "K")?;
                let dg = gibbs_free_energy(&of_formation(standard_enthalpy)?, &t, &of_formation(standard_entropy)?)?;
                Ok(witness_of(&dg.to("kJ/mol")?))
            })
        }),
    ]
}
