use dyn_clone::DynClone;
use ndarray::Array1;
use rand::prelude::*;
use rand_distr::{Normal, Distribution};

use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;
// use pyo3::types::{PyFloat, PyTuple};

pub trait UncertaintyBackend: DynClone + Send + Sync {
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn propagate_add(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn propagate_sub(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn propagate_mul(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn propagate_div(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn propagate_pow(&self, py: Python<'_>, exponent: f64) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn propagate_function(&self, py: Python<'_>, func: &str) -> PyResult<Box<dyn UncertaintyBackend>>;
    fn get_model_name(&self) -> &str;
}

dyn_clone::clone_trait_object!(UncertaintyBackend);

#[derive(Clone)]
pub struct GaussianBackend {
    pub mean: f64,
    pub std_dev: f64,
}

impl UncertaintyBackend for GaussianBackend {
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { self.mean.into_py_any(py) }
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> { self.std_dev.into_py_any(py) }
    
    fn propagate_add(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_mean: f64 = other.mean(py)?.bind(py).extract()?;
        let other_std: f64 = other.std_dev(py)?.bind(py).extract()?;

        let new_mean = self.mean + other_mean;
        let new_std = (self.std_dev.powi(2) + other_std.powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }
    
    fn propagate_sub(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_mean: f64 = other.mean(py)?.bind(py).extract()?;
        let other_std: f64 = other.std_dev(py)?.bind(py).extract()?;

        let new_mean = self.mean - other_mean;
        let new_std = (self.std_dev.powi(2) + other_std.powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_mul(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_mean: f64 = other.mean(py)?.bind(py).extract()?;
        let other_std: f64 = other.std_dev(py)?.bind(py).extract()?; 

        let m1 = self.mean;
        let s1 = self.std_dev;
        let m2 = other_mean;
        let s2 = other_std;
        let new_mean = m1 * m2;
        let new_std = ((m2 * s1).powi(2) + (m1 * s2).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_div(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_mean: f64 = other.mean(py)?.bind(py).extract()?;
        let other_std: f64 = other.std_dev(py)?.bind(py).extract()?;

        let m1 = self.mean;
        let s1 = self.std_dev;
        let m2 = other_mean;
        let s2 = other_std;
        let new_mean = m1 / m2;
        let new_std = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_pow(&self, _py: Python<'_>, exponent: f64) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean;
        let s = self.std_dev;
        let new_mean = m.powf(exponent);
        if m == 0.0 && exponent > 0.0 {
             return Ok(Box::new(GaussianBackend { mean: 0.0, std_dev: 0.0 }));
        }
        let new_std = (exponent * m.powf(exponent - 1.0) * s).abs();
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn propagate_function(&self, _py: Python<'_>, func: &str) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean;
        let s = self.std_dev;
        let (new_mean, new_std) = match func {
            "sin" => (m.sin(), (m.cos() * s).abs()),
            "cos" => (m.cos(), (m.sin() * s).abs()),
            "exp" => (m.exp(), (m.exp() * s).abs()),
            "log" => (m.ln(), (s / m).abs()),
            "abs" => (m.abs(), s),
            _ => (m, s), // Fallback
        };
        Ok(Box::new(GaussianBackend { mean: new_mean, std_dev: new_std }))
    }

    fn get_model_name(&self) -> &str { "gaussian" }
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

    fn ensure_samples(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Array1<f64>> {
        let n = self.samples.len();
        let mut rng = thread_rng();
        
        // This is risky if other is TensorBackend?
        // For now, assume MonteCarlo only mixes with Scalars or compatible MC backends.
        let m: f64 = other.mean(py)?.bind(py).extract()?;
        let s: f64 = other.std_dev(py)?.bind(py).extract()?;
        
        if s == 0.0 {
            return Ok(Array1::from_elem(n, m));
        }
        let dist = Normal::new(m, s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Array1::from_shape_fn(n, |_| dist.sample(&mut rng)))
    }
}

impl UncertaintyBackend for MonteCarloBackend {
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { self.samples.mean().unwrap_or(0.0).into_py_any(py) }
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> { self.samples.std(0.0).into_py_any(py) }

    fn propagate_add(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(py, other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples + &other_samples }))
    }
    fn propagate_sub(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(py, other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples - &other_samples }))
    }
    fn propagate_mul(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(py, other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples * &other_samples }))
    }
    fn propagate_div(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(py, other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples / &other_samples }))
    }
    fn propagate_pow(&self, _py: Python<'_>, exponent: f64) -> PyResult<Box<dyn UncertaintyBackend>> {
        Ok(Box::new(MonteCarloBackend { samples: self.samples.mapv(|x| x.powf(exponent)) }))
    }
    fn propagate_function(&self, _py: Python<'_>, func: &str) -> PyResult<Box<dyn UncertaintyBackend>> {
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
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { (&self.sigma_points * &self.weights).sum().into_py_any(py) }
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> {
        let mu = (&self.sigma_points * &self.weights).sum(); // Recompute locally for f64 math
        let var: f64 = self.sigma_points.iter()
            .zip(self.weights.iter())
            .map(|(x, w)| w * (x - mu).powi(2))
            .sum();
        var.sqrt().into_py_any(py)
    }

    fn propagate_add(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m1: f64 = self.mean(py)?.bind(py).extract()?;
        let s1: f64 = self.std_dev(py)?.bind(py).extract()?;
        let m2: f64 = other.mean(py)?.bind(py).extract()?;
        let s2: f64 = other.std_dev(py)?.bind(py).extract()?;

        // Simple Gaussian Approx for addition (UT for addition is just addition of means/vars if independent)
        let m = m1 + m2;
        let s = (s1.powi(2) + s2.powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_sub(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m1: f64 = self.mean(py)?.bind(py).extract()?;
        let s1: f64 = self.std_dev(py)?.bind(py).extract()?;
        let m2: f64 = other.mean(py)?.bind(py).extract()?;
        let s2: f64 = other.std_dev(py)?.bind(py).extract()?;

        let m = m1 - m2;
        let s = (s1.powi(2) + s2.powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_mul(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m1: f64 = self.mean(py)?.bind(py).extract()?;
        let s1: f64 = self.std_dev(py)?.bind(py).extract()?;
        let m2: f64 = other.mean(py)?.bind(py).extract()?;
        let s2: f64 = other.std_dev(py)?.bind(py).extract()?;
        
        let m = m1 * m2;
        let s = ((m1*s2).powi(2) + (m2*s1).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_div(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
        let m1: f64 = self.mean(py)?.bind(py).extract()?;
        let s1: f64 = self.std_dev(py)?.bind(py).extract()?;
        let m2: f64 = other.mean(py)?.bind(py).extract()?;
        let s2: f64 = other.std_dev(py)?.bind(py).extract()?;

        let m = m1 / m2;
        let s = ((s1/m2).powi(2) + (m1*s2/m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_pow(&self, _py: Python<'_>, exponent: f64) -> PyResult<Box<dyn UncertaintyBackend>> {
        let new_points = self.sigma_points.mapv(|x| x.powf(exponent));
        Ok(Box::new(UnscentedBackend { sigma_points: new_points, weights: self.weights.clone() }))
    }
    fn propagate_function(&self, _py: Python<'_>, func: &str) -> PyResult<Box<dyn UncertaintyBackend>> {
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

// --- Tensor Backend Implementation ---
pub struct TensorBackend {
    pub value: PyObject,
    pub uncertainty: PyObject,
}

impl Clone for TensorBackend {
    fn clone(&self) -> Self {
        Python::with_gil(|py| {
            TensorBackend {
                value: self.value.clone_ref(py),
                uncertainty: self.uncertainty.clone_ref(py),
            }
        })
    }
}

impl UncertaintyBackend for TensorBackend {
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { Ok(self.value.clone_ref(py)) }
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> { Ok(self.uncertainty.clone_ref(py)) }

    fn propagate_add(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
         // Optimization: If both are Tensors, use Tensor Add.
         let other_val = other.mean(py)?;
         let new_val = self.value.bind(py).call_method1("__add__", (other_val,))?.unbind();
         // Placeholder uncertainty
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }
    
    fn propagate_sub(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
         let other_val = other.mean(py)?;
         let new_val = self.value.bind(py).call_method1("__sub__", (other_val,))?.unbind();
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }

    fn propagate_mul(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
         let other_val = other.mean(py)?;
         let new_val = self.value.bind(py).call_method1("__mul__", (other_val,))?.unbind();
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }

    fn propagate_div(&self, py: Python<'_>, other: &dyn UncertaintyBackend) -> PyResult<Box<dyn UncertaintyBackend>> {
         let other_val = other.mean(py)?;
         let new_val = self.value.bind(py).call_method1("__truediv__", (other_val,))?.unbind();
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }

    fn propagate_pow(&self, py: Python<'_>, exponent: f64) -> PyResult<Box<dyn UncertaintyBackend>> {
         let new_val = self.value.bind(py).call_method1("__pow__", (exponent,))?.unbind();
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }

    fn propagate_function(&self, py: Python<'_>, func: &str) -> PyResult<Box<dyn UncertaintyBackend>> {
         let new_val = self.value.bind(py).call_method0(func)?.unbind(); 
         Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }))
    }

    fn get_model_name(&self) -> &str { "tensor" }
}
