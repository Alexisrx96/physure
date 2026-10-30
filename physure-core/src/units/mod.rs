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
pub use rational::{DimVec, RationalUnit, UnitKind};
pub use registry::{UnitFacts, UnitRegistry};
pub use scope::{with_catalog, with_current_catalog};

#[cfg(test)]
mod tests;
