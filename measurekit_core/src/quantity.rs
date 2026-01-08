use pyo3::prelude::*;
use pyo3::Bound;
use pyo3::PyResult;
use num_rational::Rational64;
use pyo3::types::{PyTuple, PyDict};
use std::collections::HashMap;

use crate::units::RationalUnit;
use crate::uncertainty::{UncertaintyBackend, GaussianBackend, MonteCarloBackend, UnscentedBackend};

#[pyclass(subclass, module = "measurekit_core")]
pub struct Quantity {
    pub value: Box<dyn UncertaintyBackend>,
    pub unit: RationalUnit,
}

impl Clone for Quantity {
    fn clone(&self) -> Self {
        Quantity {
            value: dyn_clone::clone_box(&*self.value),
            unit: self.unit.clone(),
        }
    }
}

#[pymethods]
impl Quantity {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(py: Python<'_>, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        // 1. Check for Copy Constructor pattern (magnitude=CoreQuantity)
        let mut existing_core = None;
        if args.len() > 0 {
             existing_core = args.get_item(0)?.extract::<Quantity>().ok();
        }
        if existing_core.is_none() {
            if let Some(kw) = kwargs {
                if let Some(mag) = kw.get_item("magnitude").ok().flatten() {
                    existing_core = mag.extract::<Quantity>().ok();
                }
            }
        }
        if let Some(core) = existing_core {
            return Ok(core.clone());
        }

        let mut mean_val: Option<PyObject> = None;
        let mut std_dev_val: Option<PyObject> = None;
        let mut unit = None;
        let mut mode = None;
        let mut samples = None;

        // Extract from positional args
        if args.len() > 0 {
            mean_val = Some(args.get_item(0)?.unbind());
        }
        if args.len() > 1 {
            std_dev_val = Some(args.get_item(1)?.unbind());
        }
        if args.len() > 2 {
            // Only extract if it's actually a RationalUnit
            unit = args.get_item(2)?.extract::<RationalUnit>().ok();
        }
        if args.len() > 3 {
            mode = args.get_item(3)?.extract::<String>().ok();
        }
        if args.len() > 4 {
            samples = args.get_item(4)?.extract::<usize>().ok();
        }

        // Override with keyword args
        if let Some(kw) = kwargs {
            if let Some(v) = kw.get_item("mean")?.or_else(|| kw.get_item("magnitude").ok().flatten()) {
                mean_val = Some(v.unbind());
            }
            if let Some(v) = kw.get_item("std_dev")?.or_else(|| kw.get_item("uncertainty").ok().flatten()) {
                 std_dev_val = Some(v.unbind());
            }
            if let Some(v) = kw.get_item("unit")? {
                if let Ok(u) = v.extract::<RationalUnit>() {
                    unit = Some(u);
                }
            }
            if let Some(v) = kw.get_item("mode")? {
                mode = v.extract::<String>().ok();
            }
            if let Some(v) = kw.get_item("samples")? {
                samples = v.extract::<usize>().ok();
            }
        }

        let u = unit.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("unit is required and must be a RationalUnit"))?;
        
        // Detect Backend Type
        // If mean_val is float -> Gaussian/MC/US
        // If mean_val is Array/Tensor -> TensorBackend
        let mean_obj = mean_val.unwrap_or(0.0.to_object(py));
        let std_dev_obj = std_dev_val.unwrap_or(0.0.to_object(py));
        
        let is_float = mean_obj.bind(py).extract::<f64>().is_ok();
        
        let backend: Box<dyn UncertaintyBackend> = if is_float {
             let mean = mean_obj.bind(py).extract::<f64>()?;
             let std_dev = std_dev_obj.bind(py).extract::<f64>().unwrap_or(0.0);
             match mode.as_deref() {
                Some("monte_carlo") => Box::new(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
                Some("unscented") => Box::new(UnscentedBackend::new_scalar(mean, std_dev)),
                _ => Box::new(GaussianBackend { mean, std_dev }),
            }
        } else {
            // Tensor Backend
            use crate::uncertainty::TensorBackend;
            Box::new(TensorBackend { value: mean_obj, uncertainty: std_dev_obj })
        };

        Ok(Quantity { value: backend, unit: u })
    }

    #[getter]
    pub fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.mean(py) }

    #[getter]
    pub fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.std_dev(py) }

    #[getter]
    fn core_unit(&self) -> RationalUnit { self.unit.clone() }

    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(other_qi) = other.extract::<Quantity>() {
            if self.unit != other_qi.unit {
                return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
            }
            Ok(Quantity { value: self.value.propagate_add(py, &*other_qi.value)?, unit: self.unit.clone() })
        } else if let Ok(val) = other.extract::<f64>() {
            // Scalar fallback
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity { value: self.value.propagate_add(py, &o)?, unit: self.unit.clone() })
        } else {
             // Assume Tensor/Array
             // Create a temporary wrapped backend for 'other'
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             let o = TensorBackend { value: val_obj, uncertainty: 0.0.to_object(py) }; // zero unc
             Ok(Quantity { value: self.value.propagate_add(py, &o)?, unit: self.unit.clone() })
        }
    }

    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        self.__add__(other)
    }

    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(other_qi) = other.extract::<Quantity>() {
            if self.unit != other_qi.unit {
                return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
            }
            Ok(Quantity { value: self.value.propagate_sub(py, &*other_qi.value)?, unit: self.unit.clone() })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity { value: self.value.propagate_sub(py, &o)?, unit: self.unit.clone() })
        } else {
             // Assume Tensor/Array
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             let o = TensorBackend { value: val_obj, uncertainty: 0.0.to_object(py) }; 
             Ok(Quantity { value: self.value.propagate_sub(py, &o)?, unit: self.unit.clone() })
        }
    }

    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity { value: o.propagate_sub(py, &*self.value)?, unit: self.unit.clone() })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Invalid operand for rsub"))
        }
    }

    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(other_qi) = other.extract::<Quantity>() {
            Ok(Quantity {
                value: self.value.propagate_mul(py, &*other_qi.value)?,
                unit: self.unit.__mul__(&other_qi.unit),
            })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity { value: self.value.propagate_mul(py, &o)?, unit: self.unit.clone() })
        } else {
             // Assume Tensor/Array
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             let o = TensorBackend { value: val_obj, uncertainty: 0.0.to_object(py) }; 
             Ok(Quantity { value: self.value.propagate_mul(py, &o)?, unit: self.unit.clone() })
        }
    }

    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        self.__mul__(other)
    }

    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(other_qi) = other.extract::<Quantity>() {
            Ok(Quantity {
                value: self.value.propagate_div(py, &*other_qi.value)?,
                unit: self.unit.__truediv__(&other_qi.unit),
            })
        } else if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity { value: self.value.propagate_div(py, &o)?, unit: self.unit.clone() })
        } else {
             // Assume Tensor/Array
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             let o = TensorBackend { value: val_obj, uncertainty: 0.0.to_object(py) }; 
             Ok(Quantity { value: self.value.propagate_div(py, &o)?, unit: self.unit.clone() })
        }
    }

    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        if let Ok(val) = other.extract::<f64>() {
            let o = GaussianBackend { mean: val, std_dev: 0.0 };
            Ok(Quantity {
                value: o.propagate_div(py, &*self.value)?,
                unit: RationalUnit::new(None).__truediv__(&self.unit),
            })
        } else {
             // Assume Tensor
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             let o = TensorBackend { value: val_obj, uncertainty: 0.0.to_object(py) }; 
             Ok(Quantity {
                value: o.propagate_div(py, &*self.value)?,
                unit: RationalUnit::new(None).__truediv__(&self.unit),
            })
        }
    }

    fn __float__(&self, py: Python<'_>) -> PyResult<f64> { self.value.mean(py)?.bind(py).extract() }
    fn __int__(&self, py: Python<'_>) -> PyResult<i64> { self.value.mean(py)?.bind(py).extract() }

    fn __pow__(&self, other: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<Quantity> {
        // Exponent should be a number for unit logic, but can be a PyObject for values if backend supports it.
        // MeasureKit usually assumes scalar exponent for units.
        let exponent: f64 = other.extract()?;
        let py = other.py();

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

        Ok(Quantity {
            value: self.value.propagate_pow(py, exponent)?,
            unit: RationalUnit { 
                id: RationalUnit::calculate_id(&new_dims), 
                dimensions: new_dims 
            },
        })
    }

    fn __neg__(&self) -> PyResult<Quantity> {
        // We don't have python here in signature, but we need it.
        // Use Python::with_gil
        Python::with_gil(|py| {
            let neg_one = GaussianBackend { mean: -1.0, std_dev: 0.0 };
            Ok(Quantity { 
                value: self.value.propagate_mul(py, &neg_one)?, 
                unit: self.unit.clone() 
            })
        })
    }

    fn __pos__(&self) -> Quantity {
        self.clone()
    }

    fn __abs__(&self) -> PyResult<Quantity> {
         Python::with_gil(|py| {
            self.propagate_function(py, "abs".to_string())
        })
    }

    fn propagate_function(&self, py: Python<'_>, func: String) -> PyResult<Quantity> {
        Ok(Quantity {
            value: self.value.propagate_function(py, &func)?,
            unit: self.unit.clone(),
        })
    }

    pub fn to_unit(&self, target_unit: RationalUnit, factor: f64) -> PyResult<Quantity> {
        Python::with_gil(|py| {
            Ok(Quantity {
                value: self.value.propagate_mul(py, &GaussianBackend { mean: factor, std_dev: 0.0 })?,
                unit: target_unit,
            })
        })
    }

    fn __repr__(&self) -> PyResult<String> {
        Python::with_gil(|py| {
            let m: f64 = self.value.mean(py)?.bind(py).extract().unwrap_or(0.0);
            let s: f64 = self.value.std_dev(py)?.bind(py).extract().unwrap_or(0.0);
            Ok(format!("{:.4} +/- {:.4} {}", m, s, self.unit.__repr__()))
        })
    }
}
