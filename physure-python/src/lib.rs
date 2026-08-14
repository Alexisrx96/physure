// physure-python/src/lib.rs
// ─────────────────────────────────────────────────────────────────────────────
// PyO3 Thin Wrapper — THE ONLY PLACE WHERE PyO3 IS ALLOWED.
//
// This file:
//   1. Wraps physure_core structs in #[pyclass] Python types.
//   2. Exposes TensorBackend (needs PyObject, cannot live in the pure core).
//   3. Implements Buffer Protocol helpers for zero-copy batch operations.
//   4. Registers the Python module (physure._core).
//
// HARD RULE: No physics math lives here. All computation delegates to
//            physure_core::*. This is strictly a translation layer.
// ─────────────────────────────────────────────────────────────────────────────

#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use pyo3::IntoPyObjectExt;
use pyo3::buffer::PyBuffer;
use numpy::PyUntypedArrayMethods;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use num_rational::Rational64;
use num_traits::FromPrimitive;

use ::physure_core::{
    RationalUnit, UnitRegistry, Quantity, PruningConfig, CovarianceStore,
    GaussianBackend, MonteCarloBackend, UnscentedBackend, UncertaintyBackend, UncertaintyValue,
    PhysureResult, PhysureError,
    DimVector, UnitConverter, UnitDefinition, UnitKind,
};
use ::physure_core::uncertainty::lineage::{self, Lineage};
use ::physure_core::uncertainty::moments::{self, AsymmetricMoments, MomentsBackend};
use physure_script::symbolic::Expr;

// ── Unit cache (Python object interning) ───────────────────────────────────
// Avoids allocating duplicate Python wrappers for the same RationalUnit.
// `RationalUnit::id` is derived from the dimensions alone, so it cannot be the whole key:
// J, kJ and kg·m²/s² share one id but differ in scale and display name, and keying on it
// handed back whichever of them was cached first.
static UNIT_CACHE: OnceLock<Mutex<HashMap<(u64, u64, Option<String>), PyObject>>> = OnceLock::new();

fn get_cached_unit(py: Python<'_>, unit: RationalUnit) -> PyResult<PyObject> {
    let mutex = UNIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = mutex.lock().unwrap();
    let key = (unit.id, unit.scale.to_bits(), unit.display_name.clone());
    if let Some(existing) = cache.get(&key) {
        return Ok(existing.clone_ref(py));
    }
    let py_unit = PyRationalUnit(unit).into_py_any(py)?;
    cache.insert(key, py_unit.clone_ref(py));
    Ok(py_unit)
}

// ── TensorBackend (lives here because it holds PyObject) ───────────────────
// This backend handles numpy/torch/jax arrays as Quantity magnitudes.
// It CANNOT live in physure-core because it holds Python-owned objects.
struct TensorBackend {
    pub value: PyObject,
    pub uncertainty: PyObject,
}

impl Clone for TensorBackend {
    fn clone(&self) -> Self {
        Python::with_gil(|py| TensorBackend {
            value: self.value.clone_ref(py),
            uncertainty: self.uncertainty.clone_ref(py),
        })
    }
}

impl UncertaintyBackend for TensorBackend {
    fn mean(&self) -> f64 {
        // For tensor backends, mean() is only meaningful for a single-element
        // array. Multi-element magnitudes are not supported by this scalar
        // uncertainty-propagation path; panic loudly rather than return NaN.
        Python::with_gil(|py| {
            self.value
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .expect("TensorBackend::mean: cannot extract a scalar via `.item()` — likely a multi-element magnitude, but could also indicate a non-numeric or unsupported value type; array-valued Rust uncertainty backends are not yet supported")
        })
    }

    fn std_dev(&self) -> f64 {
        Python::with_gil(|py| {
            self.uncertainty
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .expect("TensorBackend::std_dev: cannot extract a scalar via `.item()` — likely a multi-element magnitude, but could also indicate a non-numeric or unsupported value type; array-valued Rust uncertainty backends are not yet supported")
        })
    }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let other_val = other.mean().into_py_any(py).map_err(|e| PhysureError::Generic(e.to_string()))?;
            let new_val = self.value.bind(py)
                .call_method1("__add__", (other_val,))
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let other_val = other.mean().into_py_any(py).map_err(|e| PhysureError::Generic(e.to_string()))?;
            let new_val = self.value.bind(py)
                .call_method1("__sub__", (other_val,))
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let other_val = other.mean().into_py_any(py).map_err(|e| PhysureError::Generic(e.to_string()))?;
            let new_val = self.value.bind(py)
                .call_method1("__mul__", (other_val,))
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let other_val = other.mean().into_py_any(py).map_err(|e| PhysureError::Generic(e.to_string()))?;
            let new_val = self.value.bind(py)
                .call_method1("__truediv__", (other_val,))
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let new_val = self.value.bind(py)
                .call_method1("__pow__", (exponent,))
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Python::with_gil(|py| {
            let new_val = self.value.bind(py)
                .call_method0(func)
                .map_err(|e| PhysureError::Generic(e.to_string()))?
                .unbind();
            Ok(Box::new(TensorBackend { value: new_val, uncertainty: self.uncertainty.clone_ref(py) }) as Box<dyn UncertaintyBackend>)
        })
    }

    fn get_model_name(&self) -> &str { "tensor" }
}

// ── PyRationalUnit — Python-facing wrapper for RationalUnit ─────────────────
#[pyclass(name = "RationalUnit", subclass, dict, module = "physure._core")]
#[derive(Clone)]
pub struct PyRationalUnit(pub RationalUnit);

#[pymethods]
impl PyRationalUnit {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let dims_obj = if !args.is_empty() {
            Some(args.get_item(0)?)
        } else if let Some(kw) = kwargs {
            kw.get_item("dims")?
        } else {
            None
        };
        let dimensions = dims_obj
            .map(|d| parse_dimensions_dict(&d))
            .transpose()?
            .unwrap_or_default();
        Ok(PyRationalUnit(RationalUnit::new_from_dimensions(dimensions)))
    }

    #[getter]
    fn dimensions(&self) -> HashMap<String, (i64, i64)> {
        self.0.dimensions_map()
    }

    #[getter]
    fn id(&self) -> u64 {
        self.0.id
    }

    fn __mul__(&self, py: Python<'_>, other: &PyRationalUnit) -> PyResult<PyObject> {
        get_cached_unit(py, self.0.mul(&other.0))
    }

    fn __truediv__(&self, py: Python<'_>, other: &PyRationalUnit) -> PyResult<PyObject> {
        get_cached_unit(py, self.0.div(&other.0))
    }

    fn __pow__(&self, py: Python<'_>, exponent: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let exp_r = if let Ok(val) = exponent.extract::<i64>() {
            Rational64::new(val, 1)
        } else if let Ok(vals) = exponent.extract::<(i64, i64)>() {
            Rational64::new(vals.0, vals.1)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Exponent must be an integer or a tuple (numerator, denominator)",
            ));
        };
        get_cached_unit(py, self.0.pow(exp_r))
    }

    fn __eq__(&self, other: &PyRationalUnit) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 { self.0.id }
    fn __repr__(&self) -> String { self.0.__repr__() }

    #[pyo3(signature = (_system = None, _use_alias = false, _alias_preference = None))]
    fn to_string(&self, _system: Option<Bound<'_, PyAny>>, _use_alias: bool, _alias_preference: Option<Bound<'_, PyAny>>) -> String {
        self.0.__repr__()
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<PyObject> {
        // For pickle support: reconstruct from dimensions dict
        let cls = py.get_type::<PyRationalUnit>();
        let dims = self.dimensions().into_py_any(py)?;
        let args = PyTuple::new(py, vec![dims])?;
        (cls, args).into_py_any(py)
    }
}

fn parse_dimensions_dict(d: &Bound<'_, PyAny>) -> PyResult<HashMap<String, (i64, i64)>> {
    use pyo3::types::PyDict;
    let mut dimensions = HashMap::new();
    if let Ok(dict) = d.downcast::<PyDict>() {
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
            if let Some(val) = parse_exponent(&v) {
                dimensions.insert(key, val);
            }
        }
    }
    Ok(dimensions)
}

fn parse_exponent(v: &Bound<'_, PyAny>) -> Option<(i64, i64)> {
    if let Ok((n, den)) = v.extract::<(i64, i64)>() {
        if n != 0 { return Some((n, den)); }
    } else if let Ok(n) = v.extract::<i64>() {
        if n != 0 { return Some((n, 1)); }
    } else if let Ok(f) = v.extract::<f64>() {
        if f != 0.0 {
            let r = Rational64::from_f64(f).unwrap_or(Rational64::new(0, 1));
            return Some((*r.numer(), *r.denom()));
        }
    }
    None
}

// ── PyUnitRegistry ──────────────────────────────────────────────────────────
#[pyclass(name = "UnitRegistry", module = "physure._core")]
pub struct PyUnitRegistry(UnitRegistry);

#[pymethods]
impl PyUnitRegistry {
    #[new]
    fn new() -> Self { PyUnitRegistry(UnitRegistry::new()) }

    fn add_base_unit(&mut self, name: String) {
        self.0.add_base_unit(name);
    }

    fn add_derived_unit(&mut self, name: String, definition: &PyRationalUnit) {
        self.0.add_derived_unit(name, definition.0.clone());
    }

    fn register_alias(&mut self, alias: String, symbol: String) {
        self.0.register_alias(alias, symbol);
    }

    fn get_unit(&self, py: Python<'_>, name: String) -> PyResult<PyObject> {
        match self.0.get_unit(&name) {
            Some(unit) => get_cached_unit(py, unit),
            None => Err(pyo3::exceptions::PyKeyError::new_err(format!("Unit '{}' not found", name))),
        }
    }

    fn contains(&self, name: String) -> bool {
        self.0.contains(&name)
    }

    #[staticmethod]
    fn default_si() -> Self {
        PyUnitRegistry(UnitRegistry::build_default_si())
    }

    #[staticmethod]
    fn default_imperial() -> Self {
        PyUnitRegistry(UnitRegistry::build_default_imperial())
    }

    fn get_prefixes(&self) -> HashMap<String, f64> {
        self.0.prefixes.clone()
    }

    #[staticmethod]
    fn from_conf() -> Self {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_from_conf();
        PyUnitRegistry(reg)
    }

    fn get_categories(&self) -> HashMap<String, Vec<String>> {
        self.0.categories.clone()
    }

    fn get_constants_meta(&self) -> HashMap<String, (String, Option<String>, Option<String>)> {
        self.0.constants_meta.iter().map(|(k, v)| {
            (k.clone(), (v.value.clone(), v.description.clone(), v.latex_symbol.clone()))
        }).collect()
    }
}

// ── PyQuantity ──────────────────────────────────────────────────────────────
#[pyclass(name = "Quantity", subclass, dict, module = "physure._core")]
pub struct PyQuantity(pub Quantity);

impl Clone for PyQuantity {
    fn clone(&self) -> Self { PyQuantity(self.0.clone()) }
}

#[pymethods]
impl PyQuantity {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(py: Python<'_>, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        // Fast path: wrapping an existing PyQuantity core
        if !args.is_empty() {
            if let Ok(q) = args.get_item(0)?.extract::<PyQuantity>() {
                return Ok(q);
            }
        }

        let mut mean_val: Option<PyObject> = None;
        let mut std_dev_val: Option<PyObject> = None;
        let mut unit: Option<PyRationalUnit> = None;
        let mut mode: Option<String> = None;
        let mut samples: Option<usize> = None;

        // Positional args: (magnitude, unit, uncertainty, mode, samples)
        if args.len() > 0 { mean_val = Some(args.get_item(0)?.unbind()); }
        if args.len() > 1 { unit = args.get_item(1)?.extract::<PyRationalUnit>().ok(); }
        if args.len() > 2 { std_dev_val = Some(args.get_item(2)?.unbind()); }
        if args.len() > 3 { mode = args.get_item(3)?.extract::<String>().ok(); }
        if args.len() > 4 { samples = args.get_item(4)?.extract::<usize>().ok(); }

        // Keyword args override positionals
        if let Some(kw) = kwargs {
            if let Some(v) = kw.get_item("magnitude")?.or_else(|| kw.get_item("mean").ok().flatten()) {
                mean_val = Some(v.unbind());
            }
            if let Some(v) = kw.get_item("uncertainty")?.or_else(|| kw.get_item("std_dev").ok().flatten()) {
                std_dev_val = Some(v.unbind());
            }
            if let Some(v) = kw.get_item("unit")? {
                unit = v.extract::<PyRationalUnit>().ok();
            }
            if let Some(v) = kw.get_item("mode")? { mode = v.extract::<String>().ok(); }
            if let Some(v) = kw.get_item("samples")? { samples = v.extract::<usize>().ok(); }
        }

        let u = unit.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("unit is required and must be a RationalUnit")
        })?;

        let mean_obj = mean_val.unwrap_or_else(|| Python::with_gil(|p| 0.0_f64.into_py_any(p).unwrap()));
        let std_dev_obj = std_dev_val.unwrap_or_else(|| Python::with_gil(|p| 0.0_f64.into_py_any(p).unwrap()));

        let backend = build_backend(py, mean_obj, std_dev_obj, mode, samples)?;
        Ok(PyQuantity(Quantity::from_value(backend, u.0)))
    }

    #[getter]
    fn mean(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.0.value.mean().into_py_any(py)
    }

    #[getter]
    fn magnitude(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.0.value.mean().into_py_any(py)
    }

    #[getter]
    fn std_dev(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.0.value.std_dev().into_py_any(py)
    }

    #[getter]
    fn uncertainty(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.0.value.std_dev().into_py_any(py)
    }

    #[getter]
    fn uncertainty_model(&self) -> &str {
        self.0.value.get_model_name()
    }

    #[getter]
    fn unit(&self, py: Python<'_>) -> PyResult<PyObject> {
        get_cached_unit(py, self.0.unit.clone())
    }

    #[getter]
    fn core_unit(&self) -> PyRationalUnit {
        PyRationalUnit(self.0.unit.clone())
    }

    fn __add__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        let (other_val, other_unit) = extract_value_and_unit(py, &other)?;
        if self.0.unit != other_unit {
            return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
        }
        let new_val = self.0.value.propagate_add(&other_val)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn __radd__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        self.__add__(py, other)
    }

    fn __sub__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        let (other_val, other_unit) = extract_value_and_unit(py, &other)?;
        if self.0.unit != other_unit {
            return Err(pyo3::exceptions::PyTypeError::new_err("Unit mismatch"));
        }
        let new_val = self.0.value.propagate_sub(&other_val)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn __mul__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        let (other_val, other_unit) = to_backend(py, &other)?;
        let new_val = self.0.value.propagate_mul(&other_val)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.mul(&other_unit))))
    }

    fn __rmul__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        self.__mul__(py, other)
    }

    fn __truediv__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyQuantity> {
        let (other_val, other_unit) = to_backend(py, &other)?;
        let new_val = self.0.value.propagate_div(&other_val)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.div(&other_unit))))
    }

    fn __pow__(&self, _py: Python<'_>, other: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<PyQuantity> {
        let exp_f = other.extract::<f64>()?;
        let exp_r = Rational64::from_f64(exp_f).unwrap_or(Rational64::new(0, 1));
        let new_val = self.0.value.propagate_pow(exp_f)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.pow(exp_r))))
    }

    fn __neg__(&self) -> PyResult<PyQuantity> {
        let zero = UncertaintyValue::Gaussian(GaussianBackend::exact(0.0));
        let new_val = zero.propagate_sub(&self.0.value)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn __abs__(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("abs")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn sin(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("sin")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn cos(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("cos")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn tan(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("tan")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn exp(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("exp")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn log(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("log")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn sqrt(&self) -> PyResult<PyQuantity> {
        let new_q = self.0.sqrt()
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(new_q))
    }

    #[pyo3(signature = (other, rel_tol = 0.1, abs_tol = 1e-5))]
    fn approx_eq(&self, other: &PyQuantity, rel_tol: f64, abs_tol: f64) -> bool {
        self.0.approx_eq(&other.0, rel_tol, abs_tol)
    }

    fn __eq__(&self, other: Bound<'_, PyAny>) -> bool {
        if let Ok(q) = other.extract::<PyQuantity>() {
            return self.0.unit == q.0.unit && self.0.value.mean() == q.0.value.mean();
        }
        if let Ok(f) = other.extract::<f64>() {
            return self.0.unit.same_dimensions(&RationalUnit::dimensionless())
                && self.0.value.mean() == f;
        }
        false
    }

    fn tanh(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("tanh")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }


    /// Matches the pure-Python `Quantity.__repr__`, uncertainty included: the results of
    /// `Interpreter.evaluate()` are inspected through `repr` (a list, a notebook cell), and
    /// dropping the `± 0.05` there made an uncertain measurement look exact.
    fn __repr__(&self) -> String {
        let std_dev = self.0.value.std_dev();
        if std_dev > 0.0 {
            format!(
                "Quantity({}, {}, uncertainty={})",
                self.0.value.mean(),
                self.0.unit.__repr__(),
                std_dev
            )
        } else {
            format!("Quantity({}, {})", self.0.value.mean(), self.0.unit.__repr__())
        }
    }

    /// The measurement as a reader sees it (`0.25 kPa`), delegated to the core's `Display`
    /// so the REPL, `print()` and the `phs` CLI all render a quantity the same way.
    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __reduce__<'py>(self_: Bound<'py, Self>, py: Python<'py>) -> PyResult<(PyObject, (PyObject, PyObject, PyObject))> {
        let cls = self_.get_type();
        let val = self_.borrow();
        let mean = val.0.value.mean().into_py_any(py)?;
        let unit_obj = PyRationalUnit(val.0.unit.clone()).into_py_any(py)?;
        let std_dev = val.0.value.std_dev().into_py_any(py)?;
        Ok((cls.into(), (mean, unit_obj, std_dev)))
    }
}

// ── PyPruningConfig ─────────────────────────────────────────────────────────
#[pyclass(name = "PruningConfig", module = "physure._core")]
#[derive(Clone, Copy)]
pub struct PyPruningConfig(pub PruningConfig);

#[pymethods]
impl PyPruningConfig {
    #[new]
    #[pyo3(signature = (max_age = 100, enabled = false, corr_threshold = 1e-6))]
    fn new(max_age: usize, enabled: bool, corr_threshold: f64) -> Self {
        PyPruningConfig(PruningConfig { max_age, enabled, corr_threshold })
    }

    #[getter] fn max_age(&self) -> usize { self.0.max_age }
    #[setter] fn set_max_age(&mut self, v: usize) { self.0.max_age = v; }
    #[getter] fn enabled(&self) -> bool { self.0.enabled }
    #[setter] fn set_enabled(&mut self, v: bool) { self.0.enabled = v; }
    #[getter] fn corr_threshold(&self) -> f64 { self.0.corr_threshold }
    #[setter] fn set_corr_threshold(&mut self, v: f64) { self.0.corr_threshold = v; }

    fn __getstate__(&self) -> (usize, bool, f64) { (self.0.max_age, self.0.enabled, self.0.corr_threshold) }
    fn __setstate__(&mut self, state: (usize, bool, f64)) {
        self.0.max_age = state.0; self.0.enabled = state.1; self.0.corr_threshold = state.2;
    }
}

// ── PyCovarianceStore ───────────────────────────────────────────────────────
#[pyclass(name = "CovarianceStore", module = "physure._core")]
pub struct PyCovarianceStore(pub CovarianceStore);

#[pymethods]
impl PyCovarianceStore {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PyPruningConfig>) -> Self {
        let cfg = config.map(|c| c.0).unwrap_or_default();
        PyCovarianceStore(CovarianceStore::new(cfg))
    }

    fn register_variable(&mut self, var_id: u64, variance: numpy::PyReadonlyArrayDyn<'_, f64>) {
        let slice = variance.as_slice().unwrap();
        let shape = variance.shape();
        self.0.register_variable_slice(var_id, slice, shape);
    }

    fn register_diagonal(&mut self, var_id: u64, variance_diag: numpy::PyReadonlyArrayDyn<'_, f64>) {
        let slice = variance_diag.as_slice().unwrap();
        self.0.register_diagonal_slice(var_id, slice);
    }

    fn propagate(
        &mut self,
        out_id: u64,
        input_ids: Vec<u64>,
        jacobians: Vec<numpy::PyReadonlyArrayDyn<'_, f64>>,
    ) {
        let j_slices: Vec<(&[f64], &[usize])> = jacobians
            .iter()
            .map(|j| (j.as_slice().unwrap(), j.shape()))
            .collect();
        self.0.propagate_slices(out_id, input_ids, j_slices);
    }

    #[pyo3(name = "get_block_csr")]
    fn get_block_csr_py<'py>(
        &self,
        py: Python<'py>,
        id1: u64,
        id2: u64,
    ) -> PyResult<Option<(
        Bound<'py, numpy::PyArray1<f64>>,
        Bound<'py, numpy::PyArray1<i32>>,
        Bound<'py, numpy::PyArray1<i32>>,
        (usize, usize),
    )>> {
        if let Some(mat) = self.0.get_block_internal(id1, id2) {
            let csr = if mat.is_csr() { mat } else { mat.to_csr() };
            let shape = (csr.rows(), csr.cols());
            let (indptr, indices, data) = csr.into_raw_storage();

            let py_data = numpy::PyArray1::from_vec(py, data);
            let py_indices = numpy::PyArray1::from_vec(py, indices.iter().map(|&x| x as i32).collect());
            let py_indptr = numpy::PyArray1::from_vec(py, indptr.iter().map(|&x| x as i32).collect());

            Ok(Some((py_data, py_indices, py_indptr, shape)))
        } else {
            Ok(None)
        }
    }

    fn prune(&mut self) {
        self.0.prune();
    }
}

// ── Zero-copy Buffer Protocol helpers ───────────────────────────────────────

/// Convert a batch of values in-place using the C Buffer Protocol.
/// Accepts any Python object that supports the buffer protocol with f64 items:
///   - `bytearray` (raw), `memoryview`, `numpy.ndarray` (dtype=float64)
///
/// This is the core of the zero-copy FFI strategy: Python never copies data.
/// Rust reads & writes directly to Python-owned memory.
#[pyfunction]
fn batch_to_si_inplace(py: Python<'_>, buf: PyBuffer<f64>, factor: f64) -> PyResult<()> {
    let slice = buf.as_slice(py).ok_or_else(|| pyo3::exceptions::PyBufferError::new_err("Invalid buffer"))?;
    let data = unsafe {
        let ptr = slice.as_ptr() as *mut f64;
        std::slice::from_raw_parts_mut(ptr, slice.len())
    };
    ::physure_core::batch_to_si(data, factor);
    Ok(())
}

/// Euler integration step over flat position/velocity buffers.
/// Both buffers must be f64 and same length.
#[pyfunction]
fn step_euler_inplace(
    py: Python<'_>,
    positions: PyBuffer<f64>,
    velocities: PyBuffer<f64>,
    dt: f64,
) -> PyResult<()> {
    let pos_slice = positions.as_slice(py).ok_or_else(|| pyo3::exceptions::PyBufferError::new_err("Invalid positions buffer"))?;
    let vel_slice = velocities.as_slice(py).ok_or_else(|| pyo3::exceptions::PyBufferError::new_err("Invalid velocities buffer"))?;
    let pos = unsafe {
        let ptr = pos_slice.as_ptr() as *mut f64;
        std::slice::from_raw_parts_mut(ptr, pos_slice.len())
    };
    let vel = unsafe {
        let ptr = vel_slice.as_ptr() as *const f64;
        std::slice::from_raw_parts(ptr, vel_slice.len())
    };
    ::physure_core::step_euler(pos, vel, dt);
    Ok(())
}

/// Scale and shift buffer elements in-place: y = val * scale + shift.
#[pyfunction]
fn batch_scale_and_shift_inplace(
    py: Python<'_>,
    buf: PyBuffer<f64>,
    scale: f64,
    shift: f64,
) -> PyResult<()> {
    let slice = buf.as_slice(py).ok_or_else(|| pyo3::exceptions::PyBufferError::new_err("Invalid buffer"))?;
    let data = unsafe {
        let ptr = slice.as_ptr() as *mut f64;
        std::slice::from_raw_parts_mut(ptr, slice.len())
    };
    ::physure_core::batch_scale_and_shift(data, scale, shift);
    Ok(())
}

/// Parse unit string expression directly via native Rust parser.
///
/// Uses the atomic (non-registry-expanding) parser so named/derived units (e.g. "N", "J",
/// "km") are preserved as written rather than expanded into their base-SI decomposition —
/// this is what Python-facing `CompoundUnit` construction expects.
#[pyfunction]
fn parse_unit_expression(py: Python<'_>, expr: &str) -> PyResult<PyObject> {
    let unit = ::physure_core::units::Parser::parse_expression_atomic(expr)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    get_cached_unit(py, unit)
}

/// Evaluate dual number auto-differentiation operation in native Rust.
#[pyfunction]
fn eval_dual_number(val: f64, der: f64, op: &str) -> PyResult<(f64, f64)> {
    let d = ::physure_core::math::DualNumber { value: val, derivative: der };
    let res = match op {
        "sin" => d.sin(),
        "cos" => d.cos(),
        "exp" => d.exp(),
        "ln"  => d.ln(),
        "sqrt" => d.sqrt(),
        _ => return Err(pyo3::exceptions::PyValueError::new_err(format!("Unsupported op: {}", op))),
    };
    Ok((res.value, res.derivative))
}

/// 2nd-order Hessian Non-Linear Uncertainty Propagation in native Rust.
#[pyfunction]
fn propagate_hessian_uncertainty(
    f_mean: f64,
    jacobian: numpy::PyReadonlyArray1<'_, f64>,
    hessian: numpy::PyReadonlyArray2<'_, f64>,
    covariance: numpy::PyReadonlyArray2<'_, f64>,
) -> PyResult<(f64, f64)> {
    let j_slice = jacobian.as_slice().unwrap();
    let h_slice = hessian.as_slice().unwrap();
    let cov_slice = covariance.as_slice().unwrap();
    let shape = hessian.shape();

    let mean_out = ::physure_core::math::HessianPropagation::propagate_mean_slices(f_mean, h_slice, cov_slice, shape[0], shape[1]);
    let var_out = ::physure_core::math::HessianPropagation::propagate_variance_slices(j_slice, h_slice, cov_slice, shape[0], shape[1]);

    Ok((mean_out, var_out))
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn build_backend(
    py: Python<'_>,
    mean_obj: PyObject,
    std_dev_obj: PyObject,
    mode: Option<String>,
    samples: Option<usize>,
) -> PyResult<UncertaintyValue> {
    let is_scalar = mean_obj.bind(py).is_instance_of::<pyo3::types::PyFloat>()
        || mean_obj.bind(py).is_instance_of::<pyo3::types::PyInt>();

    if is_scalar {
        let mean = mean_obj.bind(py).extract::<f64>()?;
        if let Ok(std_dev) = std_dev_obj.bind(py).extract::<f64>() {
            return Ok(match mode.as_deref() {
                Some("monte_carlo") => UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
                Some("unscented")   => UncertaintyValue::Unscented(UnscentedBackend::new_scalar(mean, std_dev)),
                _                   => UncertaintyValue::Gaussian(GaussianBackend::new(mean, std_dev)),
            });
        }
    }
    Ok(UncertaintyValue::Custom(Box::new(TensorBackend { value: mean_obj, uncertainty: std_dev_obj })))
}

fn extract_value_and_unit(
    py: Python<'_>,
    other: &Bound<'_, PyAny>,
) -> PyResult<(UncertaintyValue, RationalUnit)> {
    if let Ok(q) = other.extract::<PyQuantity>() {
        return Ok((q.0.value.clone(), q.0.unit.clone()));
    }
    to_backend(py, other)
}

fn to_backend(
    py: Python<'_>,
    other: &Bound<'_, PyAny>,
) -> PyResult<(UncertaintyValue, RationalUnit)> {
    if let Ok(q) = other.extract::<PyQuantity>() {
        return Ok((q.0.value.clone(), q.0.unit.clone()));
    }
    if let Ok(val) = other.extract::<f64>() {
        // A bare Python number is exact, not a measurement, so it gets no source id.
        return Ok((UncertaintyValue::Gaussian(GaussianBackend::exact(val)), RationalUnit::dimensionless()));
    }
    let val_obj = other.clone().unbind();
    let uncertainty = 0.0_f64.into_py_any(py)?;
    Ok((UncertaintyValue::Custom(Box::new(TensorBackend { value: val_obj, uncertainty })), RationalUnit::dimensionless()))
}

#[pyfunction]
fn to_arrow_record_batch(py: Python<'_>, quantities: Vec<PyRef<'_, PyQuantity>>) -> PyResult<PyObject> {
    let raw: Vec<Quantity> = quantities.iter().map(|q| q.0.clone()).collect();
    let bytes = ::physure_core::serialization::quantities_to_arrow(&raw)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(pyo3::types::PyBytes::new(py, &bytes).into_py_any(py)?)
}

// ── PyDimVector ─────────────────────────────────────────────────────────────
/// Python-visible wrapper around the native SI dimension vector.
/// Exposed as `physure._core.DimVector`.
#[pyclass(name = "DimVector", module = "physure._core")]
#[derive(Clone)]
struct PyDimVector(DimVector);

#[pymethods]
impl PyDimVector {
    #[new]
    fn new(pairs: &Bound<'_, pyo3::types::PyDict>) -> PyResult<Self> {
        // Collect owned Strings first, then borrow within the same scope.
        let owned: Vec<(String, i64)> = pairs
            .iter()
            .map(|(k, v)| -> PyResult<(String, i64)> {
                Ok((k.extract::<String>()?, v.extract::<i64>()?))
            })
            .collect::<PyResult<_>>()?;
        DimVector::from_pairs(owned.iter().map(|(s, e)| (s.as_str(), *e)))
            .map(PyDimVector)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    #[staticmethod]
    fn dimensionless() -> Self {
        PyDimVector(DimVector::DIMENSIONLESS)
    }

    #[staticmethod]
    fn from_pairs(pairs: Vec<(String, i64)>) -> PyResult<Self> {
        DimVector::from_pairs(pairs.iter().map(|(s, e)| (s.as_str(), *e)))
            .map(PyDimVector)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    fn is_dimensionless(&self) -> bool {
        self.0.is_dimensionless()
    }

    fn __mul__(&self, other: &PyDimVector) -> PyDimVector {
        PyDimVector(self.0.mul(&other.0))
    }

    fn __truediv__(&self, other: &PyDimVector) -> PyDimVector {
        PyDimVector(self.0.div(&other.0))
    }

    fn __pow__(&self, exp: i32, _modulo: Option<i32>) -> PyDimVector {
        PyDimVector(self.0.pow(exp))
    }

    fn __eq__(&self, other: &PyDimVector) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut h);
        h.finish()
    }

    fn __repr__(&self) -> String {
        format!("DimVector(\"{}\")", self.0)
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    /// Return non-zero exponents as a Python dict {symbol: exponent}.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let d = pyo3::types::PyDict::new(py);
        for (sym, exp) in self.0.to_pairs() {
            d.set_item(sym, exp as i32)?;
        }
        Ok(d)
    }

    #[getter]
    fn vector(&self) -> Vec<i32> {
        self.0.0.iter().map(|&x| x as i32).collect()
    }
}

// ── PyUnitDefinition ─────────────────────────────────────────────────────────
/// Python-visible wrapper around the native UnitDefinition.
/// Exposed as `physure._core.UnitDefinition`.
#[pyclass(name = "UnitDefinition", module = "physure._core")]
#[derive(Clone)]
struct PyUnitDefinition(UnitDefinition);

#[pymethods]
impl PyUnitDefinition {
    #[new]
    #[pyo3(signature = (symbol, dimension, converter_kind, scale=1.0, offset=0.0, factor=10.0, reference=1.0, name=None, kind="delta", allow_prefixes=true))]
    fn new(
        symbol: String,
        dimension: &PyDimVector,
        converter_kind: &str,
        scale: f64,
        offset: f64,
        factor: f64,
        reference: f64,
        name: Option<String>,
        kind: &str,
        allow_prefixes: bool,
    ) -> PyResult<Self> {
        let converter = match converter_kind {
            "linear" => UnitConverter::linear(scale),
            "offset" => UnitConverter::offset(scale, offset),
            "logarithmic" => UnitConverter::logarithmic(factor, reference),
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    format!("Unknown converter_kind: {other}. Expected 'linear', 'offset', or 'logarithmic'"),
                ));
            }
        };
        let mut def = UnitDefinition::new(symbol, dimension.0, converter)
            .with_kind(UnitKind::from_str(kind))
            .with_allow_prefixes(allow_prefixes);
        if let Some(n) = name {
            def = def.with_name(n);
        }
        Ok(PyUnitDefinition(def))
    }

    #[getter]
    fn symbol(&self) -> &str {
        &self.0.symbol
    }

    #[getter]
    fn dimension(&self) -> PyDimVector {
        PyDimVector(self.0.dimension)
    }

    #[getter]
    fn name(&self) -> Option<&str> {
        self.0.name.as_deref()
    }

    #[getter]
    fn allow_prefixes(&self) -> bool {
        self.0.allow_prefixes
    }

    #[getter]
    fn kind(&self) -> &str {
        match self.0.kind {
            UnitKind::Point => "point",
            UnitKind::Delta => "delta",
        }
    }

    #[getter]
    fn is_linear(&self) -> bool {
        self.0.converter.is_linear()
    }

    fn scale(&self) -> Option<f64> {
        self.0.scale()
    }

    fn offset(&self) -> f64 {
        self.0.offset()
    }

    fn to_base(&self, value: f64) -> f64 {
        self.0.converter.to_base(value)
    }

    fn from_base(&self, value: f64) -> f64 {
        self.0.converter.from_base(value)
    }

    fn convert_to(&self, value: f64, target: &PyUnitDefinition) -> f64 {
        self.0.converter.convert_value(value, &target.0.converter)
    }

    fn __repr__(&self) -> String {
        format!("UnitDefinition('{}', {})", self.0.symbol, self.0.dimension)
    }
}

// ── pyfunction: convert_units ─────────────────────────────────────────────
/// Fast batch unit conversion: converts `data` in-place from `src` to `dst`.
#[pyfunction]
fn convert_units_inplace(
    data: &Bound<'_, pyo3::types::PyAny>,
    src: &PyUnitDefinition,
    dst: &PyUnitDefinition,
) -> PyResult<()> {
    let buf: PyBuffer<f64> = PyBuffer::get(data)?;
    let slice = unsafe {
        std::slice::from_raw_parts_mut(
            buf.buf_ptr() as *mut f64,
            buf.len_bytes() / std::mem::size_of::<f64>(),
        )
    };
    src.0.converter.convert_batch(slice, &dst.0.converter);
    Ok(())
}

// ── pyfunction: dim_vector_from_dict ──────────────────────────────────────
/// Create a DimVector from a Python dict {symbol: exponent}.
#[pyfunction]
fn dim_vector_from_dict(pairs: &Bound<'_, pyo3::types::PyDict>) -> PyResult<PyDimVector> {
    let mut vec_pairs: Vec<(String, i64)> = Vec::new();
    for (k, v) in pairs.iter() {
        let sym: String = k.extract()?;
        let exp: i64 = v.extract()?;
        vec_pairs.push((sym, exp));
    }
    DimVector::from_pairs(vec_pairs.iter().map(|(s, e)| (s.as_str(), *e)))
        .map(PyDimVector)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
}

// ── PyExpr ──────────────────────────────────────────────────────────────────
/// Native Rust symbolic math AST wrapper (`physure._core.Expr`).
#[pyclass(name = "Expr", module = "physure._core")]
#[derive(Clone)]
struct PyExpr(Expr);

#[pymethods]
impl PyExpr {
    #[staticmethod]
    fn number(v: f64) -> Self {
        PyExpr(Expr::number(v))
    }

    #[staticmethod]
    fn symbol(s: String) -> Self {
        PyExpr(Expr::symbol(s))
    }

    #[staticmethod]
    fn quantity(name: String, unit: &PyRationalUnit) -> Self {
        PyExpr(Expr::quantity(name, &unit.0))
    }

    #[staticmethod]
    fn sin(e: &PyExpr) -> Self {
        PyExpr(Expr::sin(&e.0))
    }

    #[staticmethod]
    fn cos(e: &PyExpr) -> Self {
        PyExpr(Expr::cos(&e.0))
    }

    #[staticmethod]
    fn ln(e: &PyExpr) -> Self {
        PyExpr(Expr::ln(&e.0))
    }

    #[staticmethod]
    fn exp(e: &PyExpr) -> Self {
        PyExpr(Expr::exp(&e.0))
    }

    fn __add__(&self, other: &PyExpr) -> PyResult<PyExpr> {
        self.0.add(&other.0)
            .map(PyExpr)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __sub__(&self, other: &PyExpr) -> PyResult<PyExpr> {
        self.0.sub(&other.0)
            .map(PyExpr)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __mul__(&self, other: &PyExpr) -> PyExpr {
        PyExpr(self.0.mul(&other.0))
    }

    fn __truediv__(&self, other: &PyExpr) -> PyExpr {
        PyExpr(self.0.div(&other.0))
    }

    fn __pow__(&self, other: &PyExpr, _modulo: Option<&Bound<'_, PyAny>>) -> PyExpr {
        PyExpr(self.0.pow(&other.0))
    }

    fn simplify(&self) -> PyExpr {
        PyExpr(self.0.simplify())
    }

    fn factor(&self) -> PyExpr {
        PyExpr(self.0.factor())
    }

    #[pyo3(signature = (var, n=1))]
    fn diff(&self, var: &str, n: usize) -> PyResult<PyExpr> {
        self.0.diff(var, n)
            .map(PyExpr)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn integrate(&self, var: &str) -> PyResult<PyExpr> {
        self.0.integrate(var)
            .map(PyExpr)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn unit(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match self.0.unit() {
            Ok(Some(u)) => get_cached_unit(py, u).map(Some),
            Ok(None) => Ok(None),
            Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
        }
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    fn __eq__(&self, other: &PyExpr) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut h);
        h.finish()
    }
}

#[pyfunction]
fn tokenize_phs_expression(_py: Python<'_>, stmt: &str) -> PyResult<Vec<(String, String, usize)>> {
    let lexer = ::physure_script::PhsLexer::new(stmt);
    let tokens = lexer.tokenize()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let result = tokens.into_iter().map(|t| {
        let kind_str = match t.kind {
            ::physure_script::TokenKind::Number(_) => "NUMBER",
            ::physure_script::TokenKind::Ident(_) => "IDENT",
            ::physure_script::TokenKind::StringLiteral(_) => "STRING",
            ::physure_script::TokenKind::Op(_) => "OP",
            ::physure_script::TokenKind::Sup(_) => "SUP",
            ::physure_script::TokenKind::Sqrt => "SQRT",
        };
        (kind_str.to_string(), t.value, t.pos)
    }).collect();

    Ok(result)
}

/// Converts a PHS interpreter result into a Python object, recursing into vectors.
fn phs_value_to_py(py: Python<'_>, val: ::physure_script::PhsValue) -> PyResult<PyObject> {
    Ok(match val {
        ::physure_script::PhsValue::None => py.None(),
        ::physure_script::PhsValue::Number(n) => n.into_py_any(py)?,
        ::physure_script::PhsValue::Bool(b) => b.into_py_any(py)?,
        ::physure_script::PhsValue::String(s) => s.into_py_any(py)?,
        ::physure_script::PhsValue::Quantity(q) => PyQuantity(q).into_py_any(py)?,
        ::physure_script::PhsValue::Function(f) => f.name.into_py_any(py)?,
        ::physure_script::PhsValue::Sigma(k) => k.into_py_any(py)?,
        ::physure_script::PhsValue::SigmaBound(q, _) => PyQuantity(q).into_py_any(py)?,
        ::physure_script::PhsValue::Plot(p) => p.ascii.into_py_any(py)?,
        ::physure_script::PhsValue::Equation(l, r) => format!("{} = {}", l.to_phs_string(), r.to_phs_string()).into_py_any(py)?,
        ::physure_script::PhsValue::Vector(v) => {
            let items: PyResult<Vec<PyObject>> = v.into_iter().map(|item| phs_value_to_py(py, item)).collect();
            items?.into_py_any(py)?
        }
        ::physure_script::PhsValue::Matrix(m) => PyQuantityMatrix(m).into_py_any(py)?,
        ::physure_script::PhsValue::Range(start, end) => format!("{} .. {}", start, end).into_py_any(py)?,
    })
}

/// Converts a Python object passed from a host-registered function back into a PhsValue.
/// Order matters: bool must be checked before f64, since Python bools coerce to floats.
fn py_to_phs_value(obj: &Bound<'_, PyAny>) -> PyResult<::physure_script::PhsValue> {
    use ::physure_script::PhsValue;
    if obj.is_none() {
        Ok(PhsValue::None)
    } else if let Ok(q) = obj.extract::<PyQuantity>() {
        Ok(PhsValue::Quantity(q.0))
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(PhsValue::Bool(b))
    } else if let Ok(n) = obj.extract::<f64>() {
        Ok(PhsValue::Number(n))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(PhsValue::String(s))
    } else if let Ok(items) = obj.try_iter() {
        let values = items
            .map(|item| py_to_phs_value(&item?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(PhsValue::Vector(values))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Cannot convert Python value {obj} to a PHS value"
        )))
    }
}

#[pyclass(name = "Interpreter")]
pub struct PyInterpreter {
    inner: ::physure_script::PhsInterpreter,
}

#[pymethods]
impl PyInterpreter {
    /// `base_dir`, if given, resolves `import` statements relative to that directory
    /// instead of the process's current working directory.
    #[new]
    #[pyo3(signature = (base_dir=None))]
    fn new(base_dir: Option<String>) -> PyResult<Self> {
        let inner = match base_dir {
            Some(dir) => ::physure_script::PhsInterpreter::with_base_dir(dir),
            None => ::physure_script::PhsInterpreter::default(),
        };
        Ok(PyInterpreter { inner })
    }

    /// Registers a Python callable as a PHS function named `name`. The callable
    /// receives PHS argument values converted to plain Python objects (Quantity,
    /// float, bool, str, None, or a list) and must return a value convertible back.
    fn register_function(&mut self, callback: Py<PyAny>, name: String) {
        self.inner.register_fn(name, move |args: &[::physure_script::PhsValue]| {
            Python::with_gil(|py| {
                let py_args = args
                    .iter()
                    .cloned()
                    .map(|v| phs_value_to_py(py, v))
                    .collect::<PyResult<Vec<_>>>()
                    .map_err(|e| ::physure_core::error::PhysureError::Generic(e.to_string()))?;
                let tuple = PyTuple::new(py, py_args)
                    .map_err(|e| ::physure_core::error::PhysureError::Generic(e.to_string()))?;
                let result = callback
                    .call1(py, tuple)
                    .map_err(|e| ::physure_core::error::PhysureError::Generic(e.to_string()))?;
                py_to_phs_value(result.bind(py))
                    .map_err(|e| ::physure_core::error::PhysureError::Generic(e.to_string()))
            })
        });
    }

    fn evaluate(&mut self, py: Python<'_>, source: &str) -> PyResult<Vec<PyObject>> {
        let results = py.allow_threads(|| self.inner.eval_str(source))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        results.into_iter().map(|val| phs_value_to_py(py, val)).collect()
    }

    fn deriv(&self, expression: &str, var: &str) -> PyResult<String> {
        let call_expr = format!("use deriv from calc\nderiv(\"{}\", \"{}\")", expression, var);
        let mut interp = self.inner.clone();
        let res = pyo3::Python::with_gil(|py| py.allow_threads(|| interp.eval_str(&call_expr)))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(res.last().map(|v| v.to_string()).unwrap_or_default())
    }

    fn integral(&self, expression: &str, var: &str) -> PyResult<String> {
        let call_expr = format!("use integral from calc\nintegral(\"{}\", \"{}\")", expression, var);
        let mut interp = self.inner.clone();
        let res = pyo3::Python::with_gil(|py| py.allow_threads(|| interp.eval_str(&call_expr)))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(res.last().map(|v| v.to_string()).unwrap_or_default())
    }

    fn solve(&self, equation: &str, var: &str) -> PyResult<String> {
        let call_expr = format!("use solve from calc\nsolve(\"{}\", \"{}\")", equation, var);
        let mut interp = self.inner.clone();
        let res = pyo3::Python::with_gil(|py| py.allow_threads(|| interp.eval_str(&call_expr)))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(match res.last() {
            Some(::physure_script::PhsValue::Equation(_, rhs)) => rhs.to_phs_string(),
            Some(v) => v.to_string(),
            None => String::new(),
        })
    }

    fn get_fn_params(&self, name: &str) -> PyResult<Option<Vec<String>>> {
        if let Some(::physure_script::PhsValue::Function(f)) = self.inner.env.get(name) {
            Ok(Some(f.params.clone()))
        } else {
            Ok(None)
        }
    }

    /// Re-scans `ext/` for native plugins whose file changed since load (or the
    /// last reload) and installs their updated functions, without restarting
    /// the process. No-op if this interpreter wasn't constructed with `base_dir`.
    /// Returns the names of functions (re)installed.
    fn reload_native_ext(&mut self) -> Vec<String> {
        self.inner.reload_native_ext()
    }
}

#[pyfunction]
fn evaluate_phs_native(py: Python<'_>, source: &str) -> PyResult<Vec<PyObject>> {
    let results = ::physure_script::eval_phs(source)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let mut py_results = Vec::new();
    for val in results {
        py_results.push(phs_value_to_py(py, val)?);
    }
    Ok(py_results)
}

#[pyfunction]
fn transpile_phs_native(_py: Python<'_>, source: &str, target_str: &str) -> PyResult<String> {
    let program = ::physure_script::parse_phs(source)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    
    let target = match target_str {
        "python" | "py" | "Python" | "PYTHON" => ::physure_script::Target::Python,
        "rust" | "rs" | "Rust" | "RUST" => ::physure_script::Target::Rust,
        "java" | "Java" | "JAVA" => ::physure_script::Target::Java,
        s if s.to_lowercase().starts_with("java:") => {
            let class_name = &s[5..];
            ::physure_script::Target::JavaWithClass(class_name.to_string())
        }
        _ => return Err(pyo3::exceptions::PyValueError::new_err(format!("Unsupported target: {}", target_str))),
    };

    let code = ::physure_script::transpile(&program, target)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    
    Ok(code)
}

#[pyclass(name = "QuantityVector")]
#[derive(Clone)]
pub struct PyQuantityVector(pub physure_core::QuantityVector);

#[pymethods]
impl PyQuantityVector {
    #[new]
    pub fn new(components: Vec<PyQuantity>) -> Self {
        let comps = components.into_iter().map(|q| q.0).collect();
        PyQuantityVector(physure_core::QuantityVector::new(comps))
    }

    pub fn dot(&self, other: &PyQuantityVector) -> PyResult<PyQuantity> {
        let res = self.0.dot(&other.0).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantity(res))
    }

    pub fn cross(&self, other: &PyQuantityVector) -> PyResult<PyQuantityVector> {
        let res = self.0.cross(&other.0).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantityVector(res))
    }

    pub fn norm(&self) -> PyResult<PyQuantity> {
        let res = self.0.norm().map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantity(res))
    }

    pub fn unit_vector(&self) -> PyResult<PyQuantityVector> {
        let res = self.0.unit_vector().map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantityVector(res))
    }

    pub fn __repr__(&self) -> String {
        let comps: Vec<String> = self.0.components.iter().map(|q| q.to_string()).collect();
        format!("QuantityVector([{}])", comps.join(", "))
    }
}

#[pyclass(name = "QuantityMatrix")]
#[derive(Clone)]
pub struct PyQuantityMatrix(pub physure_core::QuantityMatrix);

#[pymethods]
impl PyQuantityMatrix {
    #[new]
    pub fn new(data: Vec<Vec<PyQuantity>>) -> PyResult<Self> {
        let raw_data = data.into_iter().map(|row| row.into_iter().map(|q| q.0).collect()).collect();
        let mat = physure_core::QuantityMatrix::new(raw_data).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantityMatrix(mat))
    }

    pub fn transpose(&self) -> PyQuantityMatrix {
        PyQuantityMatrix(self.0.transpose())
    }

    pub fn matmul(&self, other: &PyQuantityMatrix) -> PyResult<PyQuantityMatrix> {
        let res = self.0.matmul(&other.0).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantityMatrix(res))
    }

    pub fn det(&self) -> PyResult<PyQuantity> {
        let res = self.0.det().map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyQuantity(res))
    }

    pub fn __repr__(&self) -> String {
        let rows: Vec<String> = self.0.data.iter().map(|r| {
            let items: Vec<String> = r.iter().map(|q| q.to_string()).collect();
            format!("[{}]", items.join(", "))
        }).collect();
        format!("QuantityMatrix([{}])", rows.join(", "))
    }
}

// ── PyLineage ─────────────────────────────────────────────────────────────────
// Provenance for one scalar uncertainty: which measurements it came from, and how
// strongly each one still moves it.
//
// Exported so Python does not need a second implementation of the merge rule. Two
// implementations mean two answers, and the whole point of the lineage is that PHS,
// Rust and Python agree on what `x - x` is.
#[pyclass(name = "Lineage", module = "physure._core")]
#[derive(Clone)]
pub struct PyLineage(pub Lineage);

#[pymethods]
impl PyLineage {
    /// A newly measured quantity. Passing `source_id` reuses an existing source
    /// instead of minting one, so a caller that names its measurements keeps them
    /// correlated across calls. A zero or non-finite `std_dev` is exact and gets no id.
    #[new]
    #[pyo3(signature = (std_dev, source_id = None))]
    fn new(std_dev: f64, source_id: Option<u32>) -> Self {
        match source_id {
            Some(id) => PyLineage(Lineage::measured_with_id(id, std_dev)),
            None => PyLineage(Lineage::measured(std_dev)),
        }
    }

    /// A value with no uncertainty. It is not a source, so two exact constants never
    /// cancel against each other.
    #[staticmethod]
    fn exact() -> Self {
        PyLineage(Lineage::exact())
    }

    /// Rebuilds from `(source_id, coefficient)` pairs as produced by `terms()`.
    #[staticmethod]
    fn from_terms(terms: Vec<(u32, f64)>) -> Self {
        PyLineage(Lineage::from_terms(terms))
    }

    /// Mints a source id that no other lineage will reuse.
    #[staticmethod]
    fn fresh_id() -> u32 {
        lineage::fresh_id()
    }

    /// Merges two lineages under the chain rule: `c_new(id) = ja*a(id) + jb*b(id)`.
    #[staticmethod]
    fn combine(a: &PyLineage, ja: f64, b: &PyLineage, jb: f64) -> Self {
        PyLineage(Lineage::combine(&a.0, ja, &b.0, jb))
    }

    /// Applies a single partial derivative, as for a one-argument function.
    fn scale(&self, jacobian: f64) -> Self {
        PyLineage(self.0.scale(jacobian))
    }

    /// Keeps the dependency shape but resizes it to report `target` as its sigma.
    fn rescaled_to(&self, target: f64) -> Self {
        PyLineage(self.0.rescaled_to(target))
    }

    /// The `(source_id, coefficient)` pairs, sorted by id.
    fn terms(&self) -> Vec<(u32, f64)> {
        self.0.terms().to_vec()
    }

    #[getter]
    fn std_dev(&self) -> f64 {
        self.0.std_dev()
    }

    #[getter]
    fn is_exact(&self) -> bool {
        self.0.is_exact()
    }

    fn __len__(&self) -> usize {
        self.0.terms().len()
    }

    fn __eq__(&self, other: &PyLineage) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        // f64 is not Hash, so the coefficients go in by bit pattern. Terms are kept
        // sorted, so equal lineages always hash the same.
        let mut h: u64 = 1469598103934665603;
        for &(id, coeff) in self.0.terms() {
            for byte in id.to_le_bytes().iter().chain(coeff.to_bits().to_le_bytes().iter()) {
                h ^= u64::from(*byte);
                h = h.wrapping_mul(1099511628211);
            }
        }
        h
    }

    fn __repr__(&self) -> String {
        format!("Lineage(std_dev={}, terms={:?})", self.0.std_dev(), self.0.terms())
    }

    // Rebuilt through `from_terms` rather than `__new__`, which would mint a new
    // source id and leave the restored value uncorrelated with its own siblings.
    // `from_terms` also reserves the ids it sees, so a lineage restored in a fresh
    // process cannot collide with one minted there later.
    fn __reduce__(&self, py: Python<'_>) -> PyResult<(PyObject, (Vec<(u32, f64)>,))> {
        let from_terms = py.get_type::<PyLineage>().getattr("from_terms")?;
        Ok((from_terms.unbind(), (self.0.terms().to_vec(),)))
    }
}

// ── PyAsymmetricMoments / PyMomentsBackend ───────────────────────────────────
// The conversion between a quoted `(σ⁻, σ⁺)` pair and the three moments that propagate.
//
// Exported for the same reason as `Lineage`: a second implementation of the forward map
// and its bisection inverse would drift from this one, and the two would disagree about
// the skew of the same measurement. Propagation is deliberately not exposed, because the
// core does not have it yet — a value built here can be measured and reported, and any
// arithmetic on it raises rather than answering symmetrically.
#[pyclass(name = "AsymmetricMoments", module = "physure._core")]
#[derive(Clone, Copy)]
pub struct PyAsymmetricMoments(pub AsymmetricMoments);

#[pymethods]
impl PyAsymmetricMoments {
    /// The moments of the dimidiated Gaussian with these half-widths, both measured
    /// outwards from the quoted value.
    #[staticmethod]
    fn from_sigmas(sigma_minus: f64, sigma_plus: f64) -> PyResult<Self> {
        moments::moments_from_sigmas(sigma_minus, sigma_plus)
            .map(PyAsymmetricMoments)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// A value with no spread, and so no shape to be asymmetric.
    #[staticmethod]
    fn exact() -> Self {
        PyAsymmetricMoments(AsymmetricMoments::exact())
    }

    /// `mean − mode`: how far the mean sits from the number the experimenter wrote.
    #[getter]
    fn shift(&self) -> f64 {
        self.0.shift
    }

    #[getter]
    fn variance(&self) -> f64 {
        self.0.variance
    }

    /// The third central moment. Zero for any symmetric distribution, which is what lets a
    /// symmetric caller fall through to the path it already had.
    #[getter]
    fn third(&self) -> f64 {
        self.0.third
    }

    #[getter]
    fn std_dev(&self) -> f64 {
        self.0.std_dev()
    }

    #[getter]
    fn skewness(&self) -> f64 {
        self.0.skewness()
    }

    /// The `(σ⁻, σ⁺)` pair these moments describe. Raises when the skew is past what a pair
    /// of half-widths can express rather than returning the closest shape available.
    fn sigmas(&self) -> PyResult<(f64, f64)> {
        self.0
            .sigmas()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __eq__(&self, other: &PyAsymmetricMoments) -> bool {
        self.0 == other.0
    }

    fn __repr__(&self) -> String {
        format!(
            "AsymmetricMoments(shift={}, variance={}, third={})",
            self.0.shift, self.0.variance, self.0.third
        )
    }
}

/// An asymmetric measurement: the mean, the spread with its provenance, and the third moment.
#[pyclass(name = "MomentsBackend", module = "physure._core")]
#[derive(Clone)]
pub struct PyMomentsBackend(pub MomentsBackend);

#[pymethods]
impl PyMomentsBackend {
    /// A quantity quoted as `value +sigma_plus -sigma_minus`. `value` is the mode, so the
    /// mean lands a little to the long-tailed side of it.
    #[staticmethod]
    fn measured(value: f64, sigma_minus: f64, sigma_plus: f64) -> PyResult<Self> {
        MomentsBackend::measured(value, sigma_minus, sigma_plus)
            .map(PyMomentsBackend)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// The mean, which for a skewed measurement is not the quoted value. See `mode`.
    #[getter]
    fn mean(&self) -> f64 {
        self.0.mean
    }

    #[getter]
    fn third(&self) -> f64 {
        self.0.third
    }

    #[getter]
    fn std_dev(&self) -> f64 {
        self.0.sigma.std_dev()
    }

    /// The provenance of the spread, shared with the Gaussian backend so an asymmetric value
    /// still cancels against itself.
    #[getter]
    fn sigma(&self) -> PyLineage {
        PyLineage(self.0.sigma.clone())
    }

    fn moments(&self) -> PyAsymmetricMoments {
        PyAsymmetricMoments(self.0.moments())
    }

    fn sigmas(&self) -> PyResult<(f64, f64)> {
        self.0
            .sigmas()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// The value to quote the half-widths around.
    fn mode(&self) -> PyResult<f64> {
        self.0
            .mode()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        match self.0.sigmas() {
            Ok((lo, hi)) => format!("MomentsBackend(mean={}, -{}, +{})", self.0.mean, lo, hi),
            Err(_) => format!("MomentsBackend(mean={}, third={})", self.0.mean, self.0.third),
        }
    }
}

/// The largest skewness a pair of half-widths can express, reached only when one side is zero.
#[pyfunction]
fn max_skewness() -> f64 {
    moments::max_skewness()
}

// ── Module Registration ──────────────────────────────────────────────────────
#[pymodule(name = "_core")]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLineage>()?;
    m.add_class::<PyAsymmetricMoments>()?;
    m.add_class::<PyMomentsBackend>()?;
    m.add_function(wrap_pyfunction!(max_skewness, m)?)?;
    m.add_class::<PyRationalUnit>()?;
    m.add_class::<PyUnitRegistry>()?;
    m.add_class::<PyQuantity>()?;
    m.add_class::<PyQuantityVector>()?;
    m.add_class::<PyQuantityMatrix>()?;
    m.add_class::<PyInterpreter>()?;
    m.add_class::<PyPruningConfig>()?;
    m.add_class::<PyCovarianceStore>()?;
    m.add_class::<PyDimVector>()?;
    m.add_class::<PyUnitDefinition>()?;
    m.add_class::<PyExpr>()?;
    m.add_function(wrap_pyfunction!(batch_to_si_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(step_euler_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(batch_scale_and_shift_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(parse_unit_expression, m)?)?;
    m.add_function(wrap_pyfunction!(eval_dual_number, m)?)?;
    m.add_function(wrap_pyfunction!(propagate_hessian_uncertainty, m)?)?;
    m.add_function(wrap_pyfunction!(to_arrow_record_batch, m)?)?;
    m.add_function(wrap_pyfunction!(convert_units_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(dim_vector_from_dict, m)?)?;
    m.add_function(wrap_pyfunction!(tokenize_phs_expression, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate_phs_native, m)?)?;
    m.add_function(wrap_pyfunction!(transpile_phs_native, m)?)?;
    m.add_function(wrap_pyfunction!(export_mesh_3d_native, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[pyfunction]
#[pyo3(name = "export_mesh_3d_native")]
fn export_mesh_3d_native(
    title: String,
    x_label: String,
    y_label: String,
    z_label: String,
    x_grid: Vec<f64>,
    y_grid: Vec<f64>,
    z_grid: Vec<f64>,
    rows: usize,
    cols: usize,
    format: String,
) -> PyResult<PyObject> {
    let mesh = physure_core::Mesh3DData::new(
        title, x_label, y_label, z_label, x_grid, y_grid, z_grid, rows, cols,
    );
    let bytes = mesh
        .export_format(&format)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    Python::with_gil(|py| {
        Ok(pyo3::types::PyBytes::new(py, &bytes).into())
    })
}
