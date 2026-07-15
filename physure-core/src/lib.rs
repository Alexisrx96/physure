#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod units;
pub mod uncertainty;
pub mod quantity;
pub mod covariance;
pub mod math;
pub mod serialization;
pub mod symbolic;

pub use units::{RationalUnit, UnitRegistry};
pub use quantity::Quantity;
pub use covariance::{CovarianceStore, PruningConfig};
pub use uncertainty::{UncertaintyBackend, GaussianBackend, MonteCarloBackend, UnscentedBackend};

/// Convert batch values in-place using factor.
pub fn batch_to_si(data: &mut [f64], factor: f64) {
    for val in data.iter_mut() {
        *val *= factor;
    }
}

/// Simple Euler integration step over positions and velocities.
pub fn step_euler(positions: &mut [f64], velocities: &[f64], dt: f64) {
    let len = positions.len().min(velocities.len());
    for i in 0..len {
        positions[i] += velocities[i] * dt;
    }
}
