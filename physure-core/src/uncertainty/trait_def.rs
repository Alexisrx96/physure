use dyn_clone::DynClone;
use crate::error::PhysureResult;
use super::gaussian::GaussianBackend;
use super::monte_carlo::MonteCarloBackend;
use super::unscented::UnscentedBackend;

/// Core trait for uncertainty propagation. Uses native Rust types — no PyO3.
pub trait UncertaintyBackend: DynClone + Send + Sync {
    fn mean(&self) -> f64;
    fn std_dev(&self) -> f64;
    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn get_model_name(&self) -> &str;
}

dyn_clone::clone_trait_object!(UncertaintyBackend);

/// Zero-allocation inline enum for built-in uncertainty models.
#[derive(Clone)]
pub enum UncertaintyValue {
    Gaussian(GaussianBackend),
    MonteCarlo(MonteCarloBackend),
    Unscented(UnscentedBackend),
    Custom(Box<dyn UncertaintyBackend>),
}

impl UncertaintyValue {
    pub fn mean(&self) -> f64 {
        match self {
            Self::Gaussian(g) => g.mean(),
            Self::MonteCarlo(m) => m.mean(),
            Self::Unscented(u) => u.mean(),
            Self::Custom(c) => c.mean(),
        }
    }

    pub fn std_dev(&self) -> f64 {
        match self {
            Self::Gaussian(g) => g.std_dev(),
            Self::MonteCarlo(m) => m.std_dev(),
            Self::Unscented(u) => u.std_dev(),
            Self::Custom(c) => c.std_dev(),
        }
    }

    pub fn get_model_name(&self) -> &str {
        match self {
            Self::Gaussian(g) => g.get_model_name(),
            Self::MonteCarlo(m) => m.get_model_name(),
            Self::Unscented(u) => u.get_model_name(),
            Self::Custom(c) => c.get_model_name(),
        }
    }

    pub fn as_backend_ref(&self) -> &dyn UncertaintyBackend {
        match self {
            Self::Gaussian(g) => g,
            Self::MonteCarlo(m) => m,
            Self::Unscented(u) => u,
            Self::Custom(c) => c.as_ref(),
        }
    }

    pub fn propagate_add(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m = g1.mean + g2.mean;
                let s = (g1.std_dev.powi(2) + g2.std_dev.powi(2)).sqrt();
                Ok(Self::Gaussian(GaussianBackend { mean: m, std_dev: s }))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = m1.ensure_samples(other_val.as_backend_ref())?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples + &other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m = u1.mean() + other_val.mean();
                let s = (u1.std_dev().powi(2) + other_val.std_dev().powi(2)).sqrt();
                Ok(Self::Unscented(UnscentedBackend::new_scalar(m, s)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_add(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_sub(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m = g1.mean - g2.mean;
                let s = (g1.std_dev.powi(2) + g2.std_dev.powi(2)).sqrt();
                Ok(Self::Gaussian(GaussianBackend { mean: m, std_dev: s }))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = m1.ensure_samples(other_val.as_backend_ref())?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples - &other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m = u1.mean() - other_val.mean();
                let s = (u1.std_dev().powi(2) + other_val.std_dev().powi(2)).sqrt();
                Ok(Self::Unscented(UnscentedBackend::new_scalar(m, s)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_sub(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_mul(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m1 = g1.mean; let s1 = g1.std_dev;
                let m2 = g2.mean; let s2 = g2.std_dev;
                let new_mean = m1 * m2;
                let new_std = ((m2 * s1).powi(2) + (m1 * s2).powi(2)).sqrt();
                Ok(Self::Gaussian(GaussianBackend { mean: new_mean, std_dev: new_std }))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = m1.ensure_samples(other_val.as_backend_ref())?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples * &other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m1 = u1.mean(); let s1 = u1.std_dev();
                let m2 = other_val.mean(); let s2 = other_val.std_dev();
                let m = m1 * m2;
                let s = ((m1 * s2).powi(2) + (m2 * s1).powi(2)).sqrt();
                Ok(Self::Unscented(UnscentedBackend::new_scalar(m, s)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_mul(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_div(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m1 = g1.mean; let s1 = g1.std_dev;
                let m2 = g2.mean; let s2 = g2.std_dev;
                if m2 == 0.0 {
                    return Err(crate::error::PhysureError::DivisionByZero("Uncertainty propagation denominator is zero".into()));
                }
                let new_mean = m1 / m2;
                let new_std = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
                Ok(Self::Gaussian(GaussianBackend { mean: new_mean, std_dev: new_std }))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = m1.ensure_samples(other_val.as_backend_ref())?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples / &other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m1 = u1.mean(); let s1 = u1.std_dev();
                let m2 = other_val.mean(); let s2 = other_val.std_dev();
                if m2 == 0.0 {
                    return Err(crate::error::PhysureError::DivisionByZero("Uncertainty propagation denominator is zero".into()));
                }
                let m = m1 / m2;
                let s = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
                Ok(Self::Unscented(UnscentedBackend::new_scalar(m, s)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_div(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_pow(&self, exponent: f64) -> PhysureResult<UncertaintyValue> {
        match self {
            Self::Gaussian(g) => {
                let m = g.mean; let s = g.std_dev;
                let new_mean = m.powf(exponent);
                if m == 0.0 && exponent > 0.0 {
                    return Ok(Self::Gaussian(GaussianBackend { mean: 0.0, std_dev: 0.0 }));
                }
                let new_std = (exponent * m.powf(exponent - 1.0) * s).abs();
                Ok(Self::Gaussian(GaussianBackend { mean: new_mean, std_dev: new_std }))
            }
            Self::MonteCarlo(m) => {
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: m.samples.mapv(|x| x.powf(exponent)) }))
            }
            Self::Unscented(u) => {
                let new_points = u.sigma_points.mapv(|x| x.powf(exponent));
                Ok(Self::Unscented(UnscentedBackend { sigma_points: new_points, weights: u.weights.clone() }))
            }
            Self::Custom(c) => {
                let b = c.propagate_pow(exponent)?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_function(&self, func: &str) -> PhysureResult<UncertaintyValue> {
        match self {
            Self::Gaussian(g) => {
                let m = g.mean; let s = g.std_dev;
                let (new_mean, new_std) = match func {
                    "sin" => (m.sin(), (m.cos() * s).abs()),
                    "cos" => (m.cos(), (m.sin() * s).abs()),
                    "exp" => (m.exp(), (m.exp() * s).abs()),
                    "log" => (m.ln(), (s / m).abs()),
                    "abs" => (m.abs(), s),
                    _ => (m, s),
                };
                Ok(Self::Gaussian(GaussianBackend { mean: new_mean, std_dev: new_std }))
            }
            Self::MonteCarlo(m) => {
                let new_samples = match func {
                    "sin" => m.samples.mapv(|x| x.sin()),
                    "cos" => m.samples.mapv(|x| x.cos()),
                    "exp" => m.samples.mapv(|x| x.exp()),
                    "log" => m.samples.mapv(|x| x.ln()),
                    "abs" => m.samples.mapv(|x| x.abs()),
                    _ => m.samples.clone(),
                };
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: new_samples }))
            }
            Self::Unscented(u) => {
                let new_points = match func {
                    "sin" => u.sigma_points.mapv(|x| x.sin()),
                    "cos" => u.sigma_points.mapv(|x| x.cos()),
                    "exp" => u.sigma_points.mapv(|x| x.exp()),
                    "log" => u.sigma_points.mapv(|x| x.ln()),
                    "abs" => u.sigma_points.mapv(|x| x.abs()),
                    _ => u.sigma_points.clone(),
                };
                Ok(Self::Unscented(UnscentedBackend { sigma_points: new_points, weights: u.weights.clone() }))
            }
            Self::Custom(c) => {
                let b = c.propagate_function(func)?;
                Ok(Self::Custom(b))
            }
        }
    }
}
