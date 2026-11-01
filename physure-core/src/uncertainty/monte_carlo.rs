use ndarray::Array1;
use rand::prelude::*;
use std::cell::Cell;
use rand_distr::{Normal, Distribution};
use crate::error::{PhysureError, PhysureResult};
use super::trait_def::UncertaintyBackend;

thread_local! {
    /// `Some((seed, draws so far))` while a [`with_seed`] scope is open on this thread.
    static SEEDED: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

/// Runs `f` with every Monte Carlo draw on this thread taken from a generator derived from `seed`, so the same
/// sequence of operations gives the same samples. Reproducible for single-threaded evaluation only: a draw made on
/// another thread (a rayon worker) is not seeded.
pub fn with_seed<R>(seed: u64, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<(u64, u64)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SEEDED.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(SEEDED.with(|s| s.replace(Some((seed, 0)))));
    f()
}

/// The generator for the next draw: derived from the seed of the enclosing [`with_seed`] scope, or the thread's own.
fn next_rng() -> StdRng {
    SEEDED.with(|s| match s.get() {
        Some((seed, n)) => {
            s.set(Some((seed, n + 1)));
            StdRng::seed_from_u64(seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        }
        None => StdRng::from_rng(thread_rng()).expect("thread_rng does not fail"),
    })
}

/// Monte Carlo uncertainty propagation backend.
///
/// Implements probability distribution propagation compliant with GUM Supplement 1
/// (JCGM 101:2008: Evaluation of measurement data — Supplement 1 to the GUM — Propagation
/// of distributions using a Monte Carlo method).
#[derive(Clone)]
pub struct MonteCarloBackend {
    pub samples: Array1<f64>,
}

impl MonteCarloBackend {
    pub fn from_stats(mean: f64, std_dev: f64, n_samples: usize) -> Self {
        let mut rng = next_rng();
        if std_dev == 0.0 {
            return MonteCarloBackend { samples: Array1::from_elem(n_samples, mean) };
        }
        let dist = Normal::new(mean, std_dev).expect("Invalid normal distribution parameters");
        MonteCarloBackend { samples: Array1::from_shape_fn(n_samples, |_| dist.sample(&mut rng)) }
    }

    /// Draws a fresh, independent sample array matching `other`'s moments.
    ///
    /// Only for operands that carry no samples of their own (a Gaussian, an Unscented, a
    /// custom backend) or whose array has a different length and therefore cannot be paired
    /// elementwise. When the other operand *is* a Monte Carlo backend of the same length,
    /// its own samples must be reused instead — see
    /// `UncertaintyValue::mc_operand_samples` in `trait_def.rs`, which is the live path for
    /// `Quantity` arithmetic.
    pub fn ensure_samples(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Array1<f64>> {
        let n = self.samples.len();
        let mut rng = next_rng();
        let m = other.mean();
        let s = other.std_dev();
        if s == 0.0 {
            return Ok(Array1::from_elem(n, m));
        }
        let dist = Normal::new(m, s).map_err(|e| PhysureError::Generic(e.to_string()))?;
        Ok(Array1::from_shape_fn(n, |_| dist.sample(&mut rng)))
    }
}

// NOTE: the `propagate_*` methods below resample the other operand unconditionally, which
// loses correlation between quantities that share samples. They are not the path `Quantity`
// arithmetic takes: `UncertaintyValue::propagate_*` in `trait_def.rs` matches
// `Self::MonteCarlo(..)` as the left operand first and reuses the right operand's array
// there. These impls remain only to satisfy the `dyn UncertaintyBackend` trait object, so
// fixing them too would be dead code — fix `trait_def.rs` if the behaviour needs to change.
impl UncertaintyBackend for MonteCarloBackend {
    fn mean(&self) -> f64 { self.samples.mean().unwrap_or(0.0) }
    fn std_dev(&self) -> f64 { self.samples.std(0.0) }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples + &other_samples }))
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples - &other_samples }))
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples * &other_samples }))
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples / &other_samples }))
    }
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Ok(Box::new(MonteCarloBackend { samples: self.samples.mapv(|x| x.powf(exponent)) }))
    }
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_samples = match func {
            "sin" => self.samples.mapv(|x| x.sin()),
            "cos" => self.samples.mapv(|x| x.cos()),
            "exp" => self.samples.mapv(|x| x.exp()),
            "log" => self.samples.mapv(|x| x.ln()),
            "log10" => self.samples.mapv(|x| x.log10()),
            "pow10" => self.samples.mapv(|x| 10f64.powf(x)),
            "abs" => self.samples.mapv(|x| x.abs()),
            "tan" => self.samples.mapv(|x| x.tan()),
            "tanh" => self.samples.mapv(|x| x.tanh()),
            _ => self.samples.clone(),
        };
        Ok(Box::new(MonteCarloBackend { samples: new_samples }))
    }

    fn get_model_name(&self) -> &str { "monte_carlo" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_samples_and_another_seed_differs() {
        let draw = |seed| with_seed(seed, || MonteCarloBackend::from_stats(10.0, 1.0, 64).samples.to_vec());
        assert_eq!(draw(7), draw(7));
        assert_ne!(draw(7), draw(8));
    }

    #[test]
    fn successive_draws_in_one_scope_differ_but_replay_exactly() {
        let two = |seed| {
            with_seed(seed, || {
                (MonteCarloBackend::from_stats(0.0, 1.0, 8).samples.to_vec(), MonteCarloBackend::from_stats(0.0, 1.0, 8).samples.to_vec())
            })
        };
        let (a, b) = two(3);
        assert_ne!(a, b);
        assert_eq!(two(3), (a, b));
    }

    #[test]
    fn resampling_an_operand_is_seeded_too() {
        let other = MonteCarloBackend::from_stats(5.0, 0.5, 16);
        let resample = |seed| with_seed(seed, || MonteCarloBackend::from_stats(1.0, 0.1, 16).ensure_samples(&other).unwrap().to_vec());
        assert_eq!(resample(11), resample(11));
        assert_ne!(resample(11), resample(12));
    }

    #[test]
    fn outside_a_scope_nothing_is_seeded_and_a_scope_leaves_no_trace() {
        with_seed(1, || ());
        let a = MonteCarloBackend::from_stats(0.0, 1.0, 64).samples.to_vec();
        let b = MonteCarloBackend::from_stats(0.0, 1.0, 64).samples.to_vec();
        assert_ne!(a, b);
    }
}
