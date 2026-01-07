use dyn_clone::DynClone;
use ndarray::Array1;
use rand::prelude::*;
use rand_distr::{Normal, Distribution};

pub trait UncertaintyBackend: DynClone + Send + Sync {
    fn mean(&self) -> f64;
    fn std_dev(&self) -> f64;
    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend>;
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend>;
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend>;
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend>;
    fn propagate_pow(&self, exponent: f64) -> Box<dyn UncertaintyBackend>;
    fn propagate_function(&self, func: &str) -> Box<dyn UncertaintyBackend>;
}

dyn_clone::clone_trait_object!(UncertaintyBackend);

#[derive(Clone)]
pub struct GaussianBackend {
    pub mean: f64,
    pub std_dev: f64,
}

impl UncertaintyBackend for GaussianBackend {
    fn mean(&self) -> f64 { self.mean }
    fn std_dev(&self) -> f64 { self.std_dev }
    
    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let new_mean = self.mean + other.mean();
        let new_std = (self.std_dev.powi(2) + other.std_dev().powi(2)).sqrt();
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }
    
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let new_mean = self.mean - other.mean();
        let new_std = (self.std_dev.powi(2) + other.std_dev().powi(2)).sqrt();
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }

    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m1 = self.mean;
        let s1 = self.std_dev;
        let m2 = other.mean();
        let s2 = other.std_dev();
        let new_mean = m1 * m2;
        let new_std = ((m2 * s1).powi(2) + (m1 * s2).powi(2)).sqrt();
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }

    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m1 = self.mean;
        let s1 = self.std_dev;
        let m2 = other.mean();
        let s2 = other.std_dev();
        let new_mean = m1 / m2;
        let new_std = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }

    fn propagate_pow(&self, exponent: f64) -> Box<dyn UncertaintyBackend> {
        let m = self.mean;
        let s = self.std_dev;
        let new_mean = m.powf(exponent);
        if m == 0.0 && exponent > 0.0 {
             return Box::new(GaussianBackend { mean: 0.0, std_dev: 0.0 });
        }
        let new_std = (exponent * m.powf(exponent - 1.0) * s).abs();
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }

    fn propagate_function(&self, func: &str) -> Box<dyn UncertaintyBackend> {
        let m = self.mean;
        let s = self.std_dev;
        let (new_mean, new_std) = match func {
            "sin" => (m.sin(), (m.cos() * s).abs()),
            "cos" => (m.cos(), (m.sin() * s).abs()),
            "exp" => (m.exp(), (m.exp() * s).abs()),
            "log" => (m.ln(), (s / m).abs()),
            _ => (m, s), // Fallback
        };
        Box::new(GaussianBackend { mean: new_mean, std_dev: new_std })
    }
}

#[derive(Clone)]
pub struct MonteCarloBackend {
    samples: Array1<f64>,
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

    fn ensure_samples(&self, other: &dyn UncertaintyBackend) -> Array1<f64> {
        let n = self.samples.len();
        let mut rng = thread_rng();
        let m = other.mean();
        let s = other.std_dev();
        if s == 0.0 {
            return Array1::from_elem(n, m);
        }
        let dist = Normal::new(m, s).expect("Invalid normal distribution parameters");
        Array1::from_shape_fn(n, |_| dist.sample(&mut rng))
    }
}

impl UncertaintyBackend for MonteCarloBackend {
    fn mean(&self) -> f64 { self.samples.mean().unwrap_or(0.0) }
    fn std_dev(&self) -> f64 { self.samples.std(0.0) }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let other_samples = self.ensure_samples(other);
        Box::new(MonteCarloBackend { samples: &self.samples + &other_samples })
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let other_samples = self.ensure_samples(other);
        Box::new(MonteCarloBackend { samples: &self.samples - &other_samples })
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let other_samples = self.ensure_samples(other);
        Box::new(MonteCarloBackend { samples: &self.samples * &other_samples })
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let other_samples = self.ensure_samples(other);
        Box::new(MonteCarloBackend { samples: &self.samples / &other_samples })
    }
    fn propagate_pow(&self, exponent: f64) -> Box<dyn UncertaintyBackend> {
        Box::new(MonteCarloBackend { samples: self.samples.mapv(|x| x.powf(exponent)) })
    }
    fn propagate_function(&self, func: &str) -> Box<dyn UncertaintyBackend> {
        let new_samples = match func {
            "sin" => self.samples.mapv(|x| x.sin()),
            "cos" => self.samples.mapv(|x| x.cos()),
            "exp" => self.samples.mapv(|x| x.exp()),
            "log" => self.samples.mapv(|x| x.ln()),
            _ => self.samples.clone(),
        };
        Box::new(MonteCarloBackend { samples: new_samples })
    }
}

#[derive(Clone)]
pub struct UnscentedBackend {
    sigma_points: Array1<f64>,
    weights: Array1<f64>,
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
        let mu = self.mean();
        let var: f64 = self.sigma_points.iter()
            .zip(self.weights.iter())
            .map(|(x, w)| w * (x - mu).powi(2))
            .sum();
        var.sqrt()
    }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m = self.mean() + other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Box::new(UnscentedBackend::new_scalar(m, s))
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m = self.mean() - other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Box::new(UnscentedBackend::new_scalar(m, s))
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m1 = self.mean();
        let m2 = other.mean();
        let s1 = self.std_dev();
        let s2 = other.std_dev();
        let m = m1 * m2;
        let s = ((m1*s2).powi(2) + (m2*s1).powi(2)).sqrt();
        Box::new(UnscentedBackend::new_scalar(m, s))
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> Box<dyn UncertaintyBackend> {
        let m1 = self.mean();
        let m2 = other.mean();
        let s1 = self.std_dev();
        let s2 = other.std_dev();
        let m = m1 / m2;
        let s = ((s1/m2).powi(2) + (m1*s2/m2.powi(2)).powi(2)).sqrt();
        Box::new(UnscentedBackend::new_scalar(m, s))
    }
    fn propagate_pow(&self, exponent: f64) -> Box<dyn UncertaintyBackend> {
        let new_points = self.sigma_points.mapv(|x| x.powf(exponent));
        Box::new(UnscentedBackend { sigma_points: new_points, weights: self.weights.clone() })
    }
    fn propagate_function(&self, func: &str) -> Box<dyn UncertaintyBackend> {
        let new_points = match func {
            "sin" => self.sigma_points.mapv(|x| x.sin()),
            "cos" => self.sigma_points.mapv(|x| x.cos()),
            "exp" => self.sigma_points.mapv(|x| x.exp()),
            "log" => self.sigma_points.mapv(|x| x.ln()),
            _ => self.sigma_points.clone(),
        };
        Box::new(UnscentedBackend { sigma_points: new_points, weights: self.weights.clone() })
    }
}
