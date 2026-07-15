use crate::error::{PhysureError, PhysureResult};
use super::trait_def::UncertaintyBackend;

#[derive(Clone)]
pub struct GaussianBackend {
    pub mean: f64,
    pub std_dev: f64,
}

impl UncertaintyBackend for GaussianBackend {
    fn mean(&self) -> f64 { self.mean }
    fn std_dev(&self) -> f64 { self.std_dev }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_mean = self.mean + other.mean();
        let new_std = (self.std_dev.powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_mean = self.mean - other.mean();
        let new_std = (self.std_dev.powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean; let s1 = self.std_dev;
        let m2 = other.mean(); let s2 = other.std_dev();
        let new_mean = m1 * m2;
        let new_std = ((m2 * s1).powi(2) + (m1 * s2).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean; let s1 = self.std_dev;
        let m2 = other.mean(); let s2 = other.std_dev();
        if m2 == 0.0 {
            return Err(PhysureError::DivisionByZero("Division by zero in uncertainty propagation".into()));
        }
        let new_mean = m1 / m2;
        let new_std = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean; let s = self.std_dev;
        let new_mean = m.powf(exponent);
        if m == 0.0 && exponent > 0.0 {
            return Ok(Box::new(GaussianBackend { mean: 0.0, std_dev: 0.0 }));
        }
        let new_std = (exponent * m.powf(exponent - 1.0) * s).abs();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean; let s = self.std_dev;
        let (new_mean, new_std) = match func {
            "sin" => (m.sin(), (m.cos() * s).abs()),
            "cos" => (m.cos(), (m.sin() * s).abs()),
            "exp" => (m.exp(), (m.exp() * s).abs()),
            "log" => (m.ln(), (s / m).abs()),
            "abs" => (m.abs(), s),
            _ => (m, s),
        };
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn get_model_name(&self) -> &str { "gaussian" }
}
