//! Chemistry: formulas, molar masses, reaction balancing, kinetics and thermodynamics.
//!
//! The Python package `physure.ext.chemistry` began as the specification of this module; the golden
//! files in `tests/golden/chemistry_*.tsv` hold the two to the same answers.

pub mod elements;

pub use elements::atomic_weight;

pub mod formula;

pub use formula::{parse_formula, subscripts_to_ascii, Composition};

pub mod species;

pub use species::{mass_to_moles, moles_to_mass, Species};

pub mod reaction;

pub use reaction::{balance, Reaction};

pub mod thermo;

pub use thermo::{arrhenius, clausius_clapeyron, gas_constant, gibbs_free_energy};
