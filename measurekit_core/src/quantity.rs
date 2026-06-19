use pyo3::prelude::*;
use pyo3::Bound;
use pyo3::PyResult;
use pyo3::types::{PyTuple, PyDict};
use std::collections::HashMap;
use pyo3::IntoPy;

use num_rational::Rational64;
use num_traits::FromPrimitive;

use crate::units::RationalUnit;
use crate::uncertainty::{UncertaintyBackend, GaussianBackend, MonteCarloBackend, UnscentedBackend};

#[pyclass(subclass, dict, module = "measurekit_core")]
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

// Internal helpers (NOT pymethods)
impl Quantity {
    fn _extract_value_and_unit(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<(Box<dyn UncertaintyBackend>, RationalUnit)> {
        if let Ok(other_q) = other.extract::<Quantity>() {
            Ok((dyn_clone::clone_box(&*other_q.value), other_q.unit.clone()))
        } else {
            self._to_backend(py, other)
        }
    }

    fn _to_backend(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<(Box<dyn UncertaintyBackend>, RationalUnit)> {
        if let Ok(other_q) = other.extract::<Quantity>() {
            Ok((dyn_clone::clone_box(&*other_q.value), other_q.unit.clone()))
        } else if let Ok(val) = other.extract::<f64>() {
            Ok((Box::new(GaussianBackend { mean: val, std_dev: 0.0 }), RationalUnit::new_from_dimensions(HashMap::new())))
        } else {
             use crate::uncertainty::TensorBackend;
             let val_obj = other.unbind();
             Ok((Box::new(TensorBackend { value: val_obj, uncertainty: (0.0).into_py(py) }), RationalUnit::new_from_dimensions(HashMap::new())))
        }
    }

    fn try_extract_core(
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Option<Quantity>> {
        if !args.is_empty() {
            if let Ok(core) = args.get_item(0)?.extract::<Quantity>() {
                return Ok(Some(core));
            }
        }
        if let Some(kw) = kwargs {
            if let Some(core_val) = kw.get_item("_core")? {
                if let Ok(core) = core_val.extract::<Quantity>() {
                    return Ok(Some(core));
                }
            }
        }
        Ok(None)
    }

    fn apply_kwargs(
        kw: &Bound<'_, PyDict>,
        mean_val: &mut Option<PyObject>,
        std_dev_val: &mut Option<PyObject>,
        unit: &mut Option<RationalUnit>,
        mode: &mut Option<String>,
        samples: &mut Option<usize>,
    ) -> PyResult<()> {
        if let Some(v) = kw.get_item("magnitude")?.or_else(|| kw.get_item("mean").ok().flatten()) {
            *mean_val = Some(v.unbind());
        }
        if let Some(v) = kw.get_item("uncertainty")?.or_else(|| kw.get_item("std_dev").ok().flatten()) {
            *std_dev_val = Some(v.unbind());
        }
        if let Some(v) = kw.get_item("unit")? {
            if let Ok(u) = v.extract::<RationalUnit>() { *unit = Some(u); }
        }
        if let Some(v) = kw.get_item("mode")? { *mode = v.extract::<String>().ok(); }
        if let Some(v) = kw.get_item("samples")? { *samples = v.extract::<usize>().ok(); }
        Ok(())
    }

    fn parse_quantity_args(
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<(Option<PyObject>, Option<RationalUnit>, Option<PyObject>, Option<String>, Option<usize>)> {
        let mut mean_val: Option<PyObject> = None;
        let mut std_dev_val: Option<PyObject> = None;
        let mut unit: Option<RationalUnit> = None;
        let mut mode: Option<String> = None;
        let mut samples: Option<usize> = None;

        if args.len() > 0 { mean_val = Some(args.get_item(0)?.unbind()); }
        if args.len() > 1 { unit = args.get_item(1)?.extract::<RationalUnit>().ok(); }
        if args.len() > 2 { std_dev_val = Some(args.get_item(2)?.unbind()); }
        if args.len() > 3 { mode = args.get_item(3)?.extract::<String>().ok(); }
        if args.len() > 4 { samples = args.get_item(4)?.extract::<usize>().ok(); }

        if let Some(kw) = kwargs {
            Self::apply_kwargs(kw, &mut mean_val, &mut std_dev_val, &mut unit, &mut mode, &mut samples)?;
        }

        Ok((mean_val, unit, std_dev_val, mode, samples))
    }

    fn build_backend(
        py: Python<'_>,
        mean_obj: PyObject,
        std_dev_obj: PyObject,
        mode: Option<String>,
        samples: Option<usize>,
    ) -> PyResult<Box<dyn UncertaintyBackend>> {
        let is_scalar = mean_obj.bind(py).is_instance_of::<pyo3::types::PyFloat>()
            || mean_obj.bind(py).is_instance_of::<pyo3::types::PyInt>();

        if is_scalar {
            let mean = mean_obj.bind(py).extract::<f64>()?;
            if let Ok(std_dev) = std_dev_obj.bind(py).extract::<f64>() {
                return Ok(match mode.as_deref() {
                    Some("monte_carlo") => Box::new(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
                    Some("unscented") => Box::new(UnscentedBackend::new_scalar(mean, std_dev)),
                    _ => Box::new(GaussianBackend { mean, std_dev }),
                });
            }
        }
        use crate::uncertainty::TensorBackend;
        Ok(Box::new(TensorBackend { value: mean_obj, uncertainty: std_dev_obj }))
    }
}

#[pymethods]
impl Quantity {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    pub fn new(py: Python<'_>, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        if let Some(core) = Self::try_extract_core(args, kwargs)? {
            return Ok(core);
        }
        let (mean_val, unit, std_dev_val, mode, samples) = Self::parse_quantity_args(args, kwargs)?;
        let u = unit.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("unit is required and must be a RationalUnit"))?;
        let mean_obj = mean_val.unwrap_or_else(|| (0.0).into_py(py));
        let std_dev_obj = std_dev_val.unwrap_or_else(|| (0.0).into_py(py));
        Ok(Quantity { value: Self::build_backend(py, mean_obj, std_dev_obj, mode, samples)?, unit: u })
    }

    fn __reduce__<'py>(self_: Bound<'py, Self>, py: Python<'py>) -> PyResult<(PyObject, (PyObject, PyObject, PyObject), Option<PyObject>)> {
        let cls = self_.get_type();
        let val = self_.borrow();
        let mean = val.value.mean(py)?;
        let unit = val.unit.clone().into_py(py);
        let std_dev = val.value.std_dev(py)?;
        let dict = self_.getattr("__dict__").ok().map(|d| d.into_py(py));
        Ok((cls.into(), (mean, unit, std_dev), dict))
    }

    #[getter]
    pub fn mean(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.mean(py) }

    #[getter]
    pub fn magnitude(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.mean(py) }

    #[getter]
    pub fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.std_dev(py) }

    #[getter]
    pub fn uncertainty(&self, py: Python<'_>) -> PyResult<PyObject> { self.value.std_dev(py) }

    #[getter]
    pub fn uncertainty_model(&self) -> &str { self.value.get_model_name() }

    #[getter]
    pub fn unit(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(crate::units::get_cached_unit(py, self.unit.clone())?.into_py(py))
    }

    #[getter]
    fn core_unit(&self) -> RationalUnit { self.unit.clone() }

    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._extract_value_and_unit(py, other)?;
        if self.unit != other_unit {
            return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
        }
        Ok(Quantity { value: self.value.propagate_add(py, &*other_val)?, unit: self.unit.clone() })
    }

    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        self.__add__(other)
    }

    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._extract_value_and_unit(py, other)?;
        if self.unit != other_unit {
            return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
        }
        Ok(Quantity { value: self.value.propagate_sub(py, &*other_val)?, unit: self.unit.clone() })
    }

    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._to_backend(py, other)?;
        if self.unit != other_unit {
            return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
        }
        Ok(Quantity { value: other_val.propagate_sub(py, &*self.value)?, unit: self.unit.clone() })
    }

    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._to_backend(py, other)?;
        Ok(Quantity { 
            value: self.value.propagate_mul(py, &*other_val)?, 
            unit: self.unit.mul(&other_unit)
        })
    }

    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        self.__mul__(other)
    }

    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._to_backend(py, other)?;
        Ok(Quantity { 
            value: self.value.propagate_div(py, &*other_val)?, 
            unit: self.unit.div(&other_unit)
        })
    }

    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Quantity> {
        let py = other.py();
        let (other_val, other_unit) = self._to_backend(py, other)?;
        Ok(Quantity { 
            value: other_val.propagate_div(py, &*self.value)?, 
            unit: other_unit.div(&self.unit)
        })
    }

    fn __pow__(&self, other: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<Quantity> {
        let py = other.py();
        let exp_f = other.extract::<f64>()?;
        let exp_r = Rational64::from_f64(exp_f).unwrap_or(Rational64::new(0, 1));
        Ok(Quantity { 
            value: self.value.propagate_pow(py, exp_f)?, 
            unit: self.unit.pow(exp_r)
        })
    }

    fn __neg__(&self, py: Python<'_>) -> PyResult<Quantity> {
        let zero = GaussianBackend { mean: 0.0, std_dev: 0.0 };
        Ok(Quantity { value: zero.propagate_sub(py, &*self.value)?, unit: self.unit.clone() })
    }

    fn __abs__(&self, py: Python<'_>) -> PyResult<Quantity> {
        Ok(Quantity { value: self.value.propagate_function(py, "abs")?, unit: self.unit.clone() })
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let val_repr: String = self.value.mean(py)?.bind(py).repr()?.extract()?;
        let unit_repr = self.unit.__repr__();
        Ok(format!("Quantity({}, {})", val_repr, unit_repr))
    }
}
