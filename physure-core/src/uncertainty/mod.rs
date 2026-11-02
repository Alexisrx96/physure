pub mod trait_def;
pub mod lineage;
pub mod dof;
pub mod expanded;
pub mod sources;
pub mod mode;
pub mod gaussian;
pub mod moments;
pub mod shapes;
pub mod monte_carlo;
pub mod unscented;

pub use trait_def::{UncertaintyBackend, UncertaintyValue};
pub use gaussian::GaussianBackend;
pub use lineage::Lineage;
pub use dof::{declare_dof, dof_of};
pub use expanded::{coverage_factor_for, type_a, Expanded, TypeA};
pub use sources::new_source_id;
pub use mode::{ModeGuard, PropagationMode, propagation_mode, set_propagation_mode};
pub use moments::{AsymmetricMoments, MomentLineage, MomentsBackend, SourceMoments};
pub use shapes::{AsymmetryShape, DimidiatedGaussian, FechnerGaussian, ShapeKind};
pub use monte_carlo::{with_seed, MonteCarloBackend};
pub use unscented::UnscentedBackend;

#[cfg(test)]
mod tests;
