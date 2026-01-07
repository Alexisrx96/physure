use pyo3::prelude::*;
use pyo3::Bound;
use pyo3::PyResult;
use num_rational::Rational64;
use std::collections::HashMap;

use crate::units::RationalUnit;
use crate::uncertainty::{UncertaintyBackend, GaussianBackend, MonteCarloBackend, UnscentedBackend};

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
    pub fn mean(&self) -> f64 { self.value.mean() }

    #[getter]
    pub fn std_dev(&self) -> f64 { self.value.std_dev() }

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
