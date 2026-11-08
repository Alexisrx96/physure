//! Chemistry: formulas, molar masses, reaction balancing, kinetics and thermodynamics.
//!
//! The Python package `physure.ext.chemistry` began as the specification of this module; the golden
//! files in `tests/golden/chemistry_*.tsv` hold the two to the same answers.

pub mod elements;

pub mod formula;

pub use formula::{parse_formula, Composition};
