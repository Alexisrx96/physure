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
};

// ── Unit cache (Python object interning) ───────────────────────────────────
// Avoids allocating duplicate Python wrappers for the same RationalUnit.
static UNIT_CACHE: OnceLock<Mutex<HashMap<u64, PyObject>>> = OnceLock::new();

fn get_cached_unit(py: Python<'_>, unit: RationalUnit) -> PyResult<PyObject> {
    let mutex = UNIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = mutex.lock().unwrap();
    if let Some(existing) = cache.get(&unit.id) {
        return Ok(existing.clone_ref(py));
    }
    let id = unit.id;
    let py_unit = PyRationalUnit(unit).into_py_any(py)?;
    cache.insert(id, py_unit.clone_ref(py));
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
        // For tensor backends, mean() is not a meaningful scalar.
        // Use mean_object() via Python for full tensor access.
        Python::with_gil(|py| {
            self.value
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .unwrap_or(f64::NAN)
        })
    }

    fn std_dev(&self) -> f64 {
        Python::with_gil(|py| {
            self.uncertainty
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .unwrap_or(0.0)
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
        let zero = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.0, std_dev: 0.0 });
        let new_val = zero.propagate_sub(&self.0.value)
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn __abs__(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("abs")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
    }

    fn __repr__(&self) -> String {
        format!("Quantity({}, {})", self.0.value.mean(), self.0.unit.__repr__())
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
                _                   => UncertaintyValue::Gaussian(GaussianBackend { mean, std_dev }),
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
        return Ok((UncertaintyValue::Gaussian(GaussianBackend { mean: val, std_dev: 0.0 }), RationalUnit::dimensionless()));
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

// ── Module Registration ──────────────────────────────────────────────────────
#[pymodule(name = "_core")]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRationalUnit>()?;
    m.add_class::<PyUnitRegistry>()?;
    m.add_class::<PyQuantity>()?;
    m.add_class::<PyPruningConfig>()?;
    m.add_class::<PyCovarianceStore>()?;
    m.add_function(wrap_pyfunction!(batch_to_si_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(step_euler_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(to_arrow_record_batch, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
