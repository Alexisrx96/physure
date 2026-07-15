use ndarray::Array1;
use crate::error::{PhysureError, PhysureResult};
use super::trait_def::UncertaintyBackend;

#[derive(Clone)]
pub struct UnscentedBackend {
    pub sigma_points: Array1<f64>,
    pub weights: Array1<f64>,
}

impl UnscentedBackend {
    pub fn new_scalar(mean: f64, std_dev: f64) -> Self {
        if std_dev == 0.0 {
            return UnscentedBackend { 
                sigma_points: Array1::from_elem(1, mean),
                weights: Array1::from_elem(1, 1.0)
            };
        }
        let n: f64 = 1.0;
        let lambda = 3.0 - n; 
        let sigma = ((n + lambda).sqrt() * std_dev).abs();
        
        UnscentedBackend {
            sigma_points: Array1::from_vec(vec![mean, mean + sigma, mean - sigma]),
            weights: Array1::from_vec(vec![lambda/(n+lambda), 1.0/(2.0*(n+lambda)), 1.0/(2.0*(n+lambda))]),
        }
    }
}

impl UncertaintyBackend for UnscentedBackend {
    fn mean(&self) -> f64 { (&self.sigma_points * &self.weights).sum() }
    fn std_dev(&self) -> f64 {
        let mu = (&self.sigma_points * &self.weights).sum();
        let var: f64 = self.sigma_points.iter()
            .zip(self.weights.iter())
            .map(|(x, w)| w * (x - mu).powi(2))
            .sum();
        var.sqrt()
    }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean() + other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean() - other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean(); let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        let m = m1 * m2;
        let s = ((m1 * s2).powi(2) + (m2 * s1).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean(); let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        if m2 == 0.0 {
            return Err(PhysureError::DivisionByZero("Division by zero in uncertainty propagation".into()));
        }
        let m = m1 / m2;
        let s = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_points = self.sigma_points.mapv(|x| x.powf(exponent));
        Ok(Box::new(UnscentedBackend { sigma_points: new_points, weights: self.weights.clone() }))
    }
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_points = match func {
            "sin" => self.sigma_points.mapv(|x| x.sin()),
            "cos" => self.sigma_points.mapv(|x| x.cos()),
            "exp" => self.sigma_points.mapv(|x| x.exp()),
            "log" => self.sigma_points.mapv(|x| x.ln()),
            "abs" => self.sigma_points.mapv(|x| x.abs()),
            _ => self.sigma_points.clone(),
        };
        Ok(Box::new(UnscentedBackend { sigma_points: new_points, weights: self.weights.clone() }))
    }

    fn get_model_name(&self) -> &str { "unscented" }
}
