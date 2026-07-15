use ndarray::Array1;
use rand::prelude::*;
use rand_distr::{Normal, Distribution};
use crate::error::{PhysureError, PhysureResult};
use super::trait_def::UncertaintyBackend;

#[derive(Clone)]
pub struct MonteCarloBackend {
    pub samples: Array1<f64>,
}

impl MonteCarloBackend {
    pub fn from_stats(mean: f64, std_dev: f64, n_samples: usize) -> Self {
        let mut rng = thread_rng();
        if std_dev == 0.0 {
            return MonteCarloBackend { samples: Array1::from_elem(n_samples, mean) };
        }
        let dist = Normal::new(mean, std_dev).expect("Invalid normal distribution parameters");
        MonteCarloBackend { samples: Array1::from_shape_fn(n_samples, |_| dist.sample(&mut rng)) }
    }

    pub fn ensure_samples(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Array1<f64>> {
        let n = self.samples.len();
        let mut rng = thread_rng();
        let m = other.mean();
        let s = other.std_dev();
        if s == 0.0 {
            return Ok(Array1::from_elem(n, m));
        }
        let dist = Normal::new(m, s).map_err(|e| PhysureError::Generic(e.to_string()))?;
        Ok(Array1::from_shape_fn(n, |_| dist.sample(&mut rng)))
    }
}

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
            "abs" => self.samples.mapv(|x| x.abs()),
            _ => self.samples.clone(),
        };
        Ok(Box::new(MonteCarloBackend { samples: new_samples }))
    }

    fn get_model_name(&self) -> &str { "monte_carlo" }
}
