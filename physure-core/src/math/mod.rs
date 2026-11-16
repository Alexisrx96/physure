pub mod dimension_vector;
pub mod dual;
pub mod interval;
pub mod hessian_propagation;
pub mod sparse_kernels;
pub mod stats;

pub use dimension_vector::{DimensionVector, BaseDimension};
pub use dual::{DualNumber, HyperDualNumber};
pub use interval::Interval;
pub use hessian_propagation::HessianPropagation;
pub use sparse_kernels::{sparse_sandwich, SandwichProduct};

#[cfg(test)]
mod tests;
