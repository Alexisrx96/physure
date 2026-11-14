pub mod angle_mode;
pub mod conf;
pub mod dimension;
pub mod exact;
pub mod parser;
pub mod rational;
pub mod registry;
pub mod scope;

pub use dimension::{dim_index, to_superscript, DimVector, SI_ORDER};
pub use exact::ExactScale;
pub use parser::Parser;
pub use rational::{formula_text, power_text, unit_text, DimVec, RationalUnit, UnitKind};
pub use registry::{is_atomic_weight_edition_name, resolve_atomic_weight_edition, UnitFacts, UnitRegistry};
pub use scope::{with_catalog, with_current_catalog, with_scoped_catalog};

#[cfg(test)]
mod tests;
