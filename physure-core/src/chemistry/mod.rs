//! Chemistry: formulas, molar masses, reaction balancing, kinetics and thermodynamics.
//!
//! The Python package `physure.ext.chemistry` began as the specification of this module; the golden
//! files in `tests/golden/chemistry_*.tsv` hold the two to the same answers.

pub mod elements;

use crate::units::{conf::with_conf_registry, with_current_catalog, UnitRegistry};

/// Runs `f` against the catalog in scope, or against the embedded one plus the working directory's conf.
///
/// Every reader of the catalog goes through this, so the catalog is resolved once per operation:
/// `with_conf_registry` builds a merged registry on every call when a local `physure.conf` exists.
pub(crate) fn with_catalog_in_scope<R>(f: impl FnOnce(&UnitRegistry) -> R) -> R {
    with_current_catalog(|scoped| match scoped {
        Some(registry) => f(registry),
        None => with_conf_registry(f),
    })
}

pub use elements::{atomic_weight, atomic_weight_edition};

pub mod formula;

pub use formula::{parse_formula, subscripts_to_ascii, Composition};

pub mod species;

pub use species::{mass_to_moles, moles_to_mass, Species};

pub mod reaction;

pub use reaction::{balance, Reaction};

pub mod thermo;

pub use thermo::{arrhenius, clausius_clapeyron, gas_constant, gibbs_free_energy};
