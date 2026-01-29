use pyo3::prelude::*;
use pyo3::Bound;
use pyo3::PyResult;
use pyo3::types::{PyTuple, PyDict};
use std::collections::HashMap;
use num_rational::Rational64;
use num_traits::FromPrimitive;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::sync::OnceLock;

static UNIT_CACHE: OnceLock<Mutex<HashMap<u64, Py<RationalUnit>>>> = OnceLock::new();

pub(crate) fn get_cached_unit(py: Python<'_>, unit: RationalUnit) -> PyResult<Py<RationalUnit>> {
    let mutex = UNIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = mutex.lock().unwrap();
    if let Some(existing) = cache.get(&unit.id) {
        return Ok(existing.clone_ref(py));
    }
    let id = unit.id;
    let py_unit = Py::new(py, unit)?;
    cache.insert(id, py_unit.clone_ref(py));
    Ok(py_unit)
}

/// A unit representation using rational exponents to avoid floating-point errors.
#[pyclass(subclass, dict, module = "measurekit_core")]
#[derive(Clone, Debug, Eq)]
pub struct RationalUnit {
    /// Map of base unit names to their exponents as (numerator, denominator).
    #[pyo3(get)]
    pub dimensions: HashMap<String, (i64, i64)>,
    #[pyo3(get)]
    pub id: u64,
}

impl PartialEq for RationalUnit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for RationalUnit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl RationalUnit {
    pub fn calculate_id(dimensions: &HashMap<String, (i64, i64)>) -> u64 {
        let mut h: u64 = 0;
        for (k, v) in dimensions {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut hasher);
            v.hash(&mut hasher);
            h ^= hasher.finish();
        }
        h
    }

    pub fn new_from_dimensions(dimensions: HashMap<String, (i64, i64)>) -> Self {
        let id = Self::calculate_id(&dimensions);
        RationalUnit { dimensions, id }
    }

    // Internal Rust-only arithmetic
    pub fn mul(&self, other: &Self) -> Self {
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
        Self::new_from_dimensions(new_dims)
    }

    pub fn div(&self, other: &Self) -> Self {
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
        Self::new_from_dimensions(new_dims)
    }

    pub fn pow(&self, exp_r: Rational64) -> Self {
        let mut new_dims = HashMap::new();
        for (base, (num, den)) in &self.dimensions {
            let base_r = Rational64::new(*num, *den);
            let res = base_r * exp_r;
            if *res.numer() != 0 {
                new_dims.insert(base.clone(), (*res.numer(), *res.denom()));
            }
        }
        Self::new_from_dimensions(new_dims)
    }
}

#[pymethods]
impl RationalUnit {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    pub fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut dimensions = HashMap::new();
        let dims_obj = if !args.is_empty() {
            Some(args.get_item(0)?)
        } else if let Some(kw) = kwargs {
            kw.get_item("dims")?
        } else {
            None
        };

        if let Some(d) = dims_obj {
            if let Ok(dict) = d.downcast::<PyDict>() {
                for (k, v) in dict.iter() {
                    let key = k.extract::<String>()?;
                    if let Ok((n, den)) = v.extract::<(i64, i64)>() {
                        if n != 0 {
                            dimensions.insert(key, (n, den));
                        }
                    } else if let Ok(n) = v.extract::<i64>() {
                        if n != 0 {
                            dimensions.insert(key, (n, 1));
                        }
                    } else if let Ok(f) = v.extract::<f64>() {
                        if f != 0.0 {
                            let r = Rational64::from_f64(f).unwrap_or(Rational64::new(0, 1));
                            dimensions.insert(key, (*r.numer(), *r.denom()));
                        }
                    }
                }
            }
        }
        Ok(Self::new_from_dimensions(dimensions))
    }

    pub fn __mul__(&self, py: Python<'_>, other: &RationalUnit) -> PyResult<PyObject> {
        Ok(get_cached_unit(py, self.mul(other))?.into_py(py))
    }

    pub fn __truediv__(&self, py: Python<'_>, other: &RationalUnit) -> PyResult<PyObject> {
        Ok(get_cached_unit(py, self.div(other))?.into_py(py))
    }

    pub fn __pow__(&self, py: Python<'_>, exponent: Bound<'_, PyAny>, _modulo: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let exp_r = if let Ok(val) = exponent.extract::<i64>() {
            Rational64::new(val, 1)
        } else if let Ok(vals) = exponent.extract::<(i64, i64)>() {
            Rational64::new(vals.0, vals.1)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Exponent must be an integer or a tuple (numerator, denominator)",
            ));
        };
        Ok(get_cached_unit(py, self.pow(exp_r))?.into_py(py))
    }

    fn __eq__(&self, other: &RationalUnit) -> bool {
        self.dimensions == other.dimensions
    }

    fn __hash__(&self) -> u64 {
        self.id
    }



    pub fn __repr__(&self) -> String {
        if self.dimensions.is_empty() {
            return "Dimensionless".to_string();
        }
        let mut parts = Vec::new();
        let mut keys: Vec<&String> = self.dimensions.keys().collect();
        keys.sort();
        for base in keys {
            let (num, den) = self.dimensions.get(base).unwrap();
            if *num == 1 && *den == 1 {
                parts.push(base.clone());
            } else if *den == 1 {
                parts.push(format!("{}^{}", base, num));
            } else {
                parts.push(format!("{}^{}/{}", base, num, den));
            }
        }
        parts.join(" * ")
    }

    #[getter]
    pub fn dimensions(&self) -> HashMap<String, (i64, i64)> {
        self.dimensions.clone()
    }

    pub fn __reduce__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let factory = py.import("measurekit.domain.measurement.units")?.getattr("reconstruct_compound_unit")?;
        let dict = self.dimensions().to_object(py);
        let args = PyTuple::new(py, vec![dict])?;
        Ok((factory, args).to_object(py))
    }

    #[pyo3(signature = (_system = None, _use_alias = false, _alias_preference = None))]
    pub fn to_string(&self, _system: Option<Bound<'_, PyAny>>, _use_alias: bool, _alias_preference: Option<Bound<'_, PyAny>>) -> String {
        self.__repr__()
    }
}

/// A registry to hold unit definitions, ensuring state isolation.
#[pyclass(module = "measurekit_core")]
pub struct UnitRegistry {
    base_units: HashMap<String, RationalUnit>,
    derived_units: HashMap<String, RationalUnit>,
    aliases: HashMap<String, String>,
}

#[pymethods]
impl UnitRegistry {
    #[new]
    fn new() -> Self {
        UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    fn add_base_unit(&mut self, name: String) {
        let mut dims = HashMap::new();
        dims.insert(name.clone(), (1, 1));
        let unit = RationalUnit::new_from_dimensions(dims);
        self.base_units.insert(name, unit);
    }

    fn add_derived_unit(&mut self, name: String, definition: RationalUnit) {
        self.derived_units.insert(name, definition);
    }

    fn register_alias(&mut self, alias: String, symbol: String) -> PyResult<()> {
        self.aliases.insert(alias, symbol);
        Ok(())
    }

    fn resolve_symbol(&self, name: String) -> String {
        let mut current = name;
        for _ in 0..10 {
            if let Some(target) = self.aliases.get(&current) {
                current = target.clone();
            } else {
                return current;
            }
        }
        current
    }

    fn get_unit(&self, py: Python<'_>, name: String) -> PyResult<PyObject> {
        let resolved = self.resolve_symbol(name.clone());
        if let Some(unit) = self.base_units.get(&resolved) {
            Ok(get_cached_unit(py, unit.clone())?.into_py(py))
        } else if let Some(unit) = self.derived_units.get(&resolved) {
            Ok(get_cached_unit(py, unit.clone())?.into_py(py))
        } else {
            if let Some(unit) = self.base_units.get(&name) {
                return Ok(get_cached_unit(py, unit.clone())?.into_py(py));
            }
            if let Some(unit) = self.derived_units.get(&name) {
               return Ok(get_cached_unit(py, unit.clone())?.into_py(py));
            }
            Err(pyo3::exceptions::PyKeyError::new_err(format!("Unit '{}' not found", name)))
        }
    }
    
    fn contains(&self, name: String) -> bool {
        let resolved = self.resolve_symbol(name.clone());
        self.base_units.contains_key(&resolved) || self.derived_units.contains_key(&resolved)
    }
}
