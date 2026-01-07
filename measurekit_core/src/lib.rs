use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3::{Bound, PyResult};
use std::collections::HashMap;
use num_rational::{Rational64, Ratio};
use std::hash::{Hash, Hasher};
use ndarray::{Array1, Array2};
use rand::prelude::*;
use rand_distr::{Normal, Distribution, StandardNormal};
use nalgebra::{DMatrix, DVector};
use dyn_clone::DynClone;
use sprs::{CsMatI, TriMatI};
use arrow::array::{Float64Array, StringArray, ArrayRef, Array};
use arrow::record_batch::RecordBatch;
use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

/// A unit representation using rational exponents to avoid floating-point errors.
#[pyclass]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RationalUnit {
    /// Map of base unit names to their exponents as (numerator, denominator).
    #[pyo3(get)]
    pub dimensions: HashMap<String, (i64, i64)>,
}

impl Hash for RationalUnit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Commutative hashing (XOR) avoids the need to sort keys (O(N) instead of O(N log N))
        let mut h: u64 = 0;
        for (k, v) in &self.dimensions {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut hasher);
            v.hash(&mut hasher);
            h ^= hasher.finish();
        }
        h.hash(state);
    }
}

#[pymethods]
impl RationalUnit {
    #[new]
    #[pyo3(signature = (dims = None))]
    fn new(dims: Option<HashMap<String, (i64, i64)>>) -> Self {
        let mut dimensions = HashMap::new();
        if let Some(d) = dims {
            for (k, (n, d_val)) in d {
                let r = Rational64::new(n, d_val);
                if *r.numer() != 0 {
                    dimensions.insert(k, (*r.numer(), *r.denom()));
                }
            }
        }
        RationalUnit { dimensions }
    }

    fn __mul__(&self, other: &RationalUnit) -> RationalUnit {
        let mut new_dims = self.dimensions.clone();
        for (base, (num, den)) in &other.dimensions {
            let current = new_dims.get(base).copied().unwrap_or((0, 1));
            let r1 = Rational64::new(current.0, current.1);
            let r2 = Rational64::new(*num, *den);
            let res = r1 + r2;
            if *res.numer() == 0 {
                new_dims.remove(base);
            } else {
                new_dims.insert(base.clone(), (*res.numer(), *res.denom()));
            }
        }
        RationalUnit { dimensions: new_dims }
    }

    fn __truediv__(&self, other: &RationalUnit) -> RationalUnit {
        let mut new_dims = self.dimensions.clone();
        for (base, (num, den)) in &other.dimensions {
            let current = new_dims.get(base).copied().unwrap_or((0, 1));
            let r1 = Rational64::new(current.0, current.1);
            let r2 = Rational64::new(*num, *den);
            let res = r1 - r2;
            if *res.numer() == 0 {
                new_dims.remove(base);
            } else {
                new_dims.insert(base.clone(), (*res.numer(), *res.denom()));
            }
        }
        RationalUnit { dimensions: new_dims }
    }

    fn __pow__(&self, exponent: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<RationalUnit> {
        let exp_r = if let Ok(val) = exponent.extract::<i64>() {
            Rational64::new(val, 1)
        } else if let Ok(vals) = exponent.extract::<(i64, i64)>() {
            Rational64::new(vals.0, vals.1)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Exponent must be an integer or a tuple (numerator, denominator)",
            ));
        };

        let mut new_dims = HashMap::new();
        for (base, (num, den)) in &self.dimensions {
            let base_r = Rational64::new(*num, *den);
            let res = base_r * exp_r;
            if *res.numer() != 0 {
                new_dims.insert(base.clone(), (*res.numer(), *res.denom()));
            }
        }
        Ok(RationalUnit {
            dimensions: new_dims,
        })
    }

    fn __eq__(&self, other: &RationalUnit) -> bool {
        self.dimensions == other.dimensions
    }

    fn __hash__(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut keys: Vec<&String> = self.dimensions.keys().collect();
        keys.sort();
        for k in keys {
            k.hash(&mut h);
            self.dimensions.get(k).unwrap().hash(&mut h);
        }
        h.finish()
    }

    fn __repr__(&self) -> String {
        if self.dimensions.is_empty() {
            return "Dimensionless".to_string();
        }
        let mut parts = Vec::new();
        let mut keys: Vec<&String> = self.dimensions.keys().collect();
        keys.sort();
        for base in keys {
            let (num, den) = self.dimensions.get(base).unwrap();
            if *den == 1 {
                parts.push(format!("{}^{}", base, num));
            } else {
                parts.push(format!("{}^{}/{}", base, num, den));
            }
        }
        parts.join(" * ")
    }
}

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
    mean: f64,
    std_dev: f64,
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
    fn from_stats(mean: f64, std_dev: f64, n_samples: usize) -> Self {
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
    fn new_scalar(mean: f64, std_dev: f64) -> Self {
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

#[pyclass]
pub struct QuantityInner {
    pub value: Box<dyn UncertaintyBackend>,
    pub unit: RationalUnit,
}

impl Clone for QuantityInner {
    fn clone(&self) -> Self {
        QuantityInner {
            value: dyn_clone::clone_box(&*self.value),
            unit: self.unit.clone(),
        }
    }
}

#[pymethods]
impl QuantityInner {
    #[new]
    #[pyo3(signature = (mean, std_dev, unit, mode = None, samples = None))]
    fn new(mean: f64, std_dev: f64, unit: RationalUnit, mode: Option<String>, samples: Option<usize>) -> Self {
        let backend: Box<dyn UncertaintyBackend> = match mode.as_deref() {
            Some("monte_carlo") => Box::new(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
            Some("unscented") => Box::new(UnscentedBackend::new_scalar(mean, std_dev)),
            _ => Box::new(GaussianBackend { mean, std_dev }),
        };
        QuantityInner { value: backend, unit }
    }

    #[getter]
    fn mean(&self) -> f64 { self.value.mean() }

    #[getter]
    fn std_dev(&self) -> f64 { self.value.std_dev() }

    #[getter]
    fn unit(&self) -> RationalUnit { self.unit.clone() }

    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(other_qi) = other.extract::<QuantityInner>() {
            if self.unit != other_qi.unit {
                return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
            }
            Ok(QuantityInner { value: self.value.propagate_add(&*other_qi.value), unit: self.unit.clone() })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner { value: self.value.propagate_add(&o), unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for add"))
        }
    }

    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        self.__add__(other)
    }

    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(other_qi) = other.extract::<QuantityInner>() {
            if self.unit != other_qi.unit {
                return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
            }
            Ok(QuantityInner { value: self.value.propagate_sub(&*other_qi.value), unit: self.unit.clone() })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner { value: self.value.propagate_sub(&o), unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for sub"))
        }
    }

    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner { value: o.propagate_sub(&*self.value), unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for rsub"))
        }
    }

    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(other_qi) = other.extract::<QuantityInner>() {
            Ok(QuantityInner {
                value: self.value.propagate_mul(&*other_qi.value),
                unit: self.unit.__mul__(&other_qi.unit),
            })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner { value: self.value.propagate_mul(&o), unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for mul"))
        }
    }

    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        self.__mul__(other)
    }

    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(other_qi) = other.extract::<QuantityInner>() {
            Ok(QuantityInner {
                value: self.value.propagate_div(&*other_qi.value),
                unit: self.unit.__truediv__(&other_qi.unit),
            })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner { value: self.value.propagate_div(&o), unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for div"))
        }
    }

    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<QuantityInner> {
        if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(QuantityInner {
                value: o.propagate_div(&*self.value),
                unit: RationalUnit::new(None).__truediv__(&self.unit),
            })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for rtruediv"))
        }
    }

    fn __float__(&self) -> f64 { self.mean() }
    fn __int__(&self) -> i64 { self.mean() as i64 }

    fn __pow__(&self, exponent: f64, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<QuantityInner> {
        let mut new_dims = HashMap::new();
        let ratio = if exponent.fract() == 0.0 {
             Rational64::new(exponent as i64, 1)
        } else {
             num_rational::Ratio::<i64>::approximate_float(exponent).unwrap_or(num_rational::Ratio::new(0, 1))
        };

        for (base, (num, den)) in &self.unit.dimensions {
            let res = Rational64::new(*num, *den) * ratio;
            if *res.numer() != 0 {
                new_dims.insert(base.clone(), (*res.numer(), *res.denom()));
            }
        }

        Ok(QuantityInner {
            value: self.value.propagate_pow(exponent),
            unit: RationalUnit { dimensions: new_dims },
        })
    }

    fn propagate_function(&self, func: String) -> QuantityInner {
        QuantityInner {
            value: self.value.propagate_function(&func),
            unit: self.unit.clone(),
        }
    }

    fn __repr__(&self) -> String {
        format!("{:.4} +/- {:.4} {}", self.mean(), self.std_dev(), self.unit.__repr__())
    }
}

#[pyclass]
#[derive(Clone, Copy)]
pub struct PruningConfig {
    #[pyo3(get, set)]
    pub max_age: usize,
    #[pyo3(get, set)]
    pub enabled: bool,
}

#[pymethods]
impl PruningConfig {
    #[new]
    #[pyo3(signature = (max_age = 100, enabled = false))]
    fn new(max_age: usize, enabled: bool) -> Self {
        PruningConfig { max_age, enabled }
    }
}

#[pyclass]
pub struct CovarianceStore {
    matrix: CsMatI<f64, usize>,
    last_updated: Vec<usize>,
    current_step: usize,
    config: PruningConfig,
    next_idx: usize,
}

#[pymethods]
impl CovarianceStore {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PruningConfig>) -> Self {
        CovarianceStore {
            matrix: CsMatI::new_csc((0, 0), vec![0], vec![], vec![]),
            last_updated: Vec::new(),
            current_step: 0,
            config: config.unwrap_or(PruningConfig { max_age: 100, enabled: false }),
            next_idx: 0,
        }
    }

    fn allocate(&mut self, size: usize) -> (usize, usize) {
        let start = self.next_idx;
        let end = start + size;
        self.next_idx = end;
        self.last_updated.resize(end, self.current_step);
        (start, end)
    }

    fn update_covariance(&mut self, _out_idx: (usize, usize), in_indices: Vec<(usize, usize)>) {
        self.current_step += 1;
        for (start, end) in in_indices {
            for i in start..end {
                self.last_updated[i] = self.current_step;
            }
        }
        if self.config.enabled {
            self.prune();
        }
    }

    fn prune(&mut self) {
        let max_age = self.config.max_age;
        let mut to_zero = Vec::new();
        for (i, &last) in self.last_updated.iter().enumerate() {
            if self.current_step - last > max_age {
                to_zero.push(i);
            }
        }
        if to_zero.is_empty() { return; }
        
        // Zero out elements associated with pruned variables
        let mut new_tri = TriMatI::new(self.matrix.shape());
        for (&val, (r, c)) in self.matrix.iter() {
            if !to_zero.contains(&r) && !to_zero.contains(&c) {
                new_tri.add_triplet(r, c, val);
            }
        }
        self.matrix = new_tri.to_csr();
    }
}

#[pyfunction]
pub fn to_arrow_record_batch(quantities: Bound<'_, PyList>) -> PyResult<Vec<u8>> {
    let len = quantities.len();
    let mut means = Vec::with_capacity(len);
    let mut std_devs = Vec::with_capacity(len);
    
    // Use Dictionary encoding for units (very efficient for repeated units)
    let mut unit_values = Vec::new();
    let mut unit_indices = Vec::with_capacity(len);
    let mut unit_repr_cache: HashMap<RationalUnit, u32> = HashMap::new();

    for q_any in quantities.iter() {
        // Optimization: downcast and borrow instead of extract (zero allocation)
        let q_bound = q_any.downcast::<QuantityInner>()?;
        let q = q_bound.borrow();
        
        means.push(q.mean());
        std_devs.push(q.std_dev());
        
        // Use the RationalUnit itself as the cache key (efficient Hash/Eq)
        let idx = *unit_repr_cache.entry(q.unit.clone()).or_insert_with(|| {
            let i = unit_values.len() as u32;
            unit_values.push(q.unit.__repr__());
            i
        });
        unit_indices.push(Some(idx));
    }

    let mean_array = Float64Array::from(means);
    let std_dev_array = Float64Array::from(std_devs);
    
    // Build DictionaryArray for units (reduces memory and overhead)
    let keys = arrow::array::UInt32Array::from(unit_indices);
    let values = StringArray::from(unit_values);
    let unit_array = arrow::array::DictionaryArray::<arrow::datatypes::UInt32Type>::try_new(
        keys,
        Arc::new(values) as ArrayRef
    ).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow dict error: {}", e)))?;

    let schema = Schema::new(vec![
        Field::new("mean", DataType::Float64, false),
        Field::new("std_dev", DataType::Float64, false),
        Field::new("unit", unit_array.data_type().clone(), false),
    ]);

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(mean_array) as ArrayRef,
            Arc::new(std_dev_array) as ArrayRef,
            Arc::new(unit_array) as ArrayRef,
        ],
    ).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow error: {}", e)))?;

    let mut buffer = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &batch.schema()).unwrap();
        writer.write(&batch).unwrap();
        writer.finish().unwrap();
    }
    Ok(buffer)
}

/// A registry to hold unit definitions, ensuring state isolation.
#[pyclass]
pub struct UnitRegistry {
    base_units: HashMap<String, RationalUnit>,
    derived_units: HashMap<String, RationalUnit>,
}

#[pymethods]
impl UnitRegistry {
    #[new]
    fn new() -> Self {
        UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
        }
    }

    fn add_base_unit(&mut self, name: String) {
        let mut dims = HashMap::new();
        dims.insert(name.clone(), (1, 1));
        let unit = RationalUnit { dimensions: dims };
        self.base_units.insert(name, unit);
    }

    fn add_derived_unit(&mut self, name: String, definition: RationalUnit) {
        self.derived_units.insert(name, definition);
    }

    fn get_unit(&self, name: String) -> PyResult<RationalUnit> {
        if let Some(unit) = self.base_units.get(&name) {
            Ok(unit.clone())
        } else if let Some(unit) = self.derived_units.get(&name) {
            Ok(unit.clone())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(name))
        }
    }
}

#[pymodule]
fn measurekit_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RationalUnit>()?;
    m.add_class::<UnitRegistry>()?;
    m.add_class::<QuantityInner>()?;
    m.add_class::<PruningConfig>()?;
    m.add_class::<CovarianceStore>()?;
    m.add_function(wrap_pyfunction!(to_arrow_record_batch, m)?)?;
    Ok(())
}
