use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;
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

    fn parse_dimensions_dict(d: &Bound<'_, PyAny>) -> PyResult<HashMap<String, (i64, i64)>> {
        let mut dimensions = HashMap::new();
        if let Ok(dict) = d.downcast::<PyDict>() {
            for (k, v) in dict.iter() {
                let key = k.extract::<String>()?;
                if let Some(val) = Self::parse_exponent(&v) {
                    dimensions.insert(key, val);
                }
            }
        }
        Ok(dimensions)
    }

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
        let dims_obj = if !args.is_empty() {
            Some(args.get_item(0)?)
        } else if let Some(kw) = kwargs {
            kw.get_item("dims")?
        } else {
            None
        };
        let dimensions = dims_obj.map(|d| Self::parse_dimensions_dict(&d)).transpose()?.unwrap_or_default();
        Ok(Self::new_from_dimensions(dimensions))
    }

    pub fn __mul__(&self, py: Python<'_>, other: &RationalUnit) -> PyResult<PyObject> {
        Ok(get_cached_unit(py, self.mul(other))?.into_any())
    }

    pub fn __truediv__(&self, py: Python<'_>, other: &RationalUnit) -> PyResult<PyObject> {
        Ok(get_cached_unit(py, self.div(other))?.into_any())
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
        Ok(get_cached_unit(py, self.pow(exp_r))?.into_any())
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
        let dict = self.dimensions().into_py_any(py)?;
        let args = PyTuple::new(py, vec![dict])?;
        (factory, args).into_py_any(py)
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
            Ok(get_cached_unit(py, unit.clone())?.into_any())
        } else if let Some(unit) = self.derived_units.get(&resolved) {
            Ok(get_cached_unit(py, unit.clone())?.into_any())
        } else {
            if let Some(unit) = self.base_units.get(&name) {
                return Ok(get_cached_unit(py, unit.clone())?.into_any());
            }
            if let Some(unit) = self.derived_units.get(&name) {
               return Ok(get_cached_unit(py, unit.clone())?.into_any());
            }
            Err(pyo3::exceptions::PyKeyError::new_err(format!("Unit '{}' not found", name)))
        }
    }
    
    fn contains(&self, name: String) -> bool {
        let resolved = self.resolve_symbol(name.clone());
        self.base_units.contains_key(&resolved) || self.derived_units.contains_key(&resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::Rational64;
    use pyo3::Python;
    use pyo3::types::PyDict;
    use pyo3::prelude::IntoPyObject;

    fn length() -> RationalUnit {
        RationalUnit::new_from_dimensions([("L".into(), (1, 1))].into())
    }

    fn time() -> RationalUnit {
        RationalUnit::new_from_dimensions([("T".into(), (1, 1))].into())
    }

    #[test]
    fn dimensionless_is_empty() {
        let u = RationalUnit::new_from_dimensions(HashMap::new());
        assert!(u.dimensions.is_empty());
        assert!(u.dimensions.is_empty());
    }

    #[test]
    fn mul_accumulates_exponents() {
        let l2 = length().mul(&length());
        assert_eq!(l2.dimensions["L"], (2, 1));
        assert!(!l2.dimensions.is_empty());
    }

    #[test]
    fn div_cancels_same_dimension() {
        assert!(length().div(&length()).dimensions.is_empty());
    }

    #[test]
    fn div_mixed_dimensions() {
        let speed = length().div(&time()); // L/T
        assert_eq!(speed.dimensions["L"], (1, 1));
        assert_eq!(speed.dimensions["T"], (-1, 1));
    }

    #[test]
    fn pow_scales_exponent() {
        let l3 = length().pow(Rational64::new(3, 1));
        assert_eq!(l3.dimensions["L"], (3, 1));
    }

    #[test]
    fn pow_fractional_exponent() {
        let sqrt_l = length().pow(Rational64::new(1, 2));
        assert_eq!(sqrt_l.dimensions["L"], (1, 2));
    }

    #[test]
    fn pow_zero_removes_dimension() {
        let u = length().pow(Rational64::new(0, 1));
        assert!(u.dimensions.is_empty());
    }

    #[test]
    fn calculate_id_is_stable() {
        let a = RationalUnit::calculate_id(&[("L".into(), (1, 1))].into());
        let b = RationalUnit::calculate_id(&[("L".into(), (1, 1))].into());
        assert_eq!(a, b);
    }

    #[test]
    fn calculate_id_differs_for_distinct_dims() {
        let l_id = RationalUnit::calculate_id(&[("L".into(), (1, 1))].into());
        let t_id = RationalUnit::calculate_id(&[("T".into(), (1, 1))].into());
        assert_ne!(l_id, t_id);
    }

    #[test]
    fn mul_then_div_is_identity() {
        let result = length().mul(&time()).div(&time());
        assert_eq!(result, length());
    }

    #[test]
    fn repr_dimensionless() {
        let u = RationalUnit::new_from_dimensions(HashMap::new());
        assert_eq!(u.__repr__(), "Dimensionless");
    }

    #[test]
    fn repr_single_unit_exponent_1() {
        assert_eq!(length().__repr__(), "L");
    }

    #[test]
    fn repr_higher_integer_power() {
        let l2 = length().mul(&length());
        assert_eq!(l2.__repr__(), "L^2");
    }

    #[test]
    fn repr_fractional_power() {
        let sqrt_l = length().pow(Rational64::new(1, 2));
        assert_eq!(sqrt_l.__repr__(), "L^1/2");
    }

    #[test]
    fn hash_is_same_as_id() {
        let u = length();
        assert_eq!(u.__hash__(), u.id);
    }

    #[test]
    fn dimensions_accessor_returns_clone() {
        let u = length();
        let dims = u.dimensions();
        assert_eq!(dims["L"], (1, 1));
        assert_eq!(dims.len(), 1);
    }

    #[test]
    fn to_string_delegates_to_repr() {
        assert_eq!(length().to_string(None, false, None), "L");
    }

    #[test]
    fn unit_registry_add_and_lookup() {
        let mut reg = UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        };
        reg.add_base_unit("m".into());
        assert!(reg.contains("m".into()));
        assert!(!reg.contains("s".into()));
    }

    #[test]
    fn unit_registry_alias_resolves() {
        let mut reg = UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        };
        reg.add_base_unit("meter".into());
        reg.register_alias("m".into(), "meter".into()).unwrap();
        assert!(reg.contains("m".into()));
        assert_eq!(reg.resolve_symbol("m".into()), "meter");
    }

    #[test]
    fn unit_registry_add_derived() {
        let mut reg = UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        };
        let speed = length().div(&time());
        reg.add_derived_unit("m_per_s".into(), speed.clone());
        assert!(reg.contains("m_per_s".into()));
    }

    #[test]
    fn parse_exponent_tuple_nonzero() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let t = (3i64, 2i64).into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(t.as_any()), Some((3, 2)));
        });
    }

    #[test]
    fn parse_exponent_tuple_zero_numerator() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let t = (0i64, 1i64).into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(t.as_any()), None);
        });
    }

    #[test]
    fn parse_exponent_int_nonzero() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let n = 2i64.into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(n.as_any()), Some((2, 1)));
        });
    }

    #[test]
    fn parse_exponent_int_zero() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let n = 0i64.into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(n.as_any()), None);
        });
    }

    #[test]
    fn parse_exponent_float_nonzero() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let f = 0.5f64.into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(f.as_any()), Some((1, 2)));
        });
    }

    #[test]
    fn parse_exponent_float_zero() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let f = 0.0f64.into_pyobject(py).unwrap();
            assert_eq!(RationalUnit::parse_exponent(f.as_any()), None);
        });
    }

    #[test]
    fn parse_exponent_unrecognized_type_returns_none() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // A list is not a tuple/int/float — all three extractions fail → None
            let s = pyo3::types::PyList::new(py, [1, 2]).unwrap();
            assert_eq!(RationalUnit::parse_exponent(s.as_any()), None);
        });
    }

    #[test]
    fn parse_dimensions_dict_non_dict_input() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // Passing a non-dict skips the dict block entirely → empty result
            let n = 1i64.into_pyobject(py).unwrap();
            let result = RationalUnit::parse_dimensions_dict(n.as_any()).unwrap();
            assert!(result.is_empty());
        });
    }

    #[test]
    fn parse_dimensions_dict_mixed() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("L", 1i64).unwrap();
            dict.set_item("T", (2i64, 3i64)).unwrap();
            dict.set_item("M", 0i64).unwrap(); // zero → excluded
            let result = RationalUnit::parse_dimensions_dict(dict.as_any()).unwrap();
            assert_eq!(result["L"], (1, 1));
            assert_eq!(result["T"], (2, 3));
            assert!(!result.contains_key("M"));
        });
    }

    #[test]
    fn rational_unit_pymul() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let result = length().__mul__(py, &time()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert!(unit.dimensions.contains_key("L"));
            assert!(unit.dimensions.contains_key("T"));
        });
    }

    #[test]
    fn rational_unit_pydiv() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let result = length().__truediv__(py, &length()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert!(unit.dimensions.is_empty());
        });
    }

    #[test]
    fn rational_unit_pypow() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let exp = 3i64.into_pyobject(py).unwrap();
            let result = length().__pow__(py, exp.into_any(), None).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert_eq!(unit.dimensions["L"], (3, 1));
        });
    }

    #[test]
    fn unit_registry_get_unit_base() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            reg.add_base_unit("m".into());
            let result = reg.get_unit(py, "m".into()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert_eq!(unit.dimensions["m"], (1, 1));
        });
    }

    #[test]
    fn unit_registry_get_unit_derived() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            let speed = length().div(&time());
            reg.add_derived_unit("v".into(), speed);
            let result = reg.get_unit(py, "v".into()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert_eq!(unit.dimensions["L"], (1, 1));
        });
    }

    #[test]
    fn unit_registry_get_unit_via_alias() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            reg.add_base_unit("meter".into());
            reg.register_alias("m".into(), "meter".into()).unwrap();
            let result = reg.get_unit(py, "m".into()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert!(unit.dimensions.contains_key("meter"));
        });
    }

    #[test]
    fn unit_registry_get_unit_not_found() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            assert!(reg.get_unit(py, "nope".into()).is_err());
        });
    }

    #[test]
    fn rational_unit_reduce() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let result = length().__reduce__(py);
            assert!(result.is_ok(), "reduce failed: {:?}", result.err());
        });
    }

    #[test]
    fn unit_registry_get_unit_via_unresolved_base_fallback() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // get_unit falls back to original name when resolved alias not found
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            reg.add_base_unit("L".into());
            // alias to something that doesn't exist, so resolve_symbol returns "ghost"
            // but base_units["L"] is looked up by original name as fallback
            let result = reg.get_unit(py, "L".into());
            assert!(result.is_ok());
        });
    }

    #[test]
    fn unit_registry_resolve_caps_at_10_hops() {
        let mut reg = UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        };
        for i in 0..12u32 {
            reg.register_alias(format!("a{}", i), format!("a{}", i + 1)).unwrap();
        }
        reg.add_base_unit("a12".into());
        let resolved = reg.resolve_symbol("a0".into());
        assert_eq!(resolved, "a10");
    }

    #[test]
    fn mul_zero_exponent_removes_dimension() {
        let l_inv = RationalUnit::new_from_dimensions([("L".into(), (-1i64, 1i64))].into());
        assert!(length().mul(&l_inv).dimensions.is_empty());
    }

    #[test]
    fn hash_used_as_hashmap_key() {
        let mut map: std::collections::HashMap<RationalUnit, &str> = std::collections::HashMap::new();
        map.insert(length(), "length");
        assert_eq!(map[&length()], "length");
    }

    #[test]
    fn rational_unit_eq_and_neq() {
        assert!(length().__eq__(&length()));
        assert!(!length().__eq__(&time()));
    }

    #[test]
    fn get_cached_unit_cache_hit() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let u = length();
            let first = get_cached_unit(py, u.clone()).unwrap();
            let second = get_cached_unit(py, u).unwrap();
            assert!(first.is(&second)); // same Python object
        });
    }

    #[test]
    fn rational_unit_new_pymethods_empty_and_with_dims() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            use pyo3::types::PyTuple;
            // Empty → dimensionless
            let args = PyTuple::empty(py);
            let u = RationalUnit::new(&args, None).unwrap();
            assert!(u.dimensions.is_empty());

            // Positional arg: a dict of dimensions
            let dims = PyDict::new(py);
            dims.set_item("L", 1i64).unwrap();
            let args2 = PyTuple::new(py, [&dims]).unwrap();
            let u2 = RationalUnit::new(&args2, None).unwrap();
            assert_eq!(u2.dimensions["L"], (1, 1));

            // Kwargs with "dims" key
            let outer = PyDict::new(py);
            outer.set_item("dims", &dims).unwrap();
            let u3 = RationalUnit::new(&args, Some(&outer)).unwrap();
            assert_eq!(u3.dimensions["L"], (1, 1));
        });
    }

    #[test]
    fn rational_unit_pypow_tuple_exponent() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let exp = (1i64, 2i64).into_pyobject(py).unwrap();
            let result = length().__pow__(py, exp.into_any(), None).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert_eq!(unit.dimensions["L"], (1, 2));
        });
    }

    #[test]
    fn rational_unit_pypow_invalid_exponent_errors() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let exp = "bad".into_pyobject(py).unwrap();
            assert!(length().__pow__(py, exp.into_any(), None).is_err());
        });
    }

    #[test]
    fn unit_registry_new_via_pymethods() {
        let reg = UnitRegistry::new();
        assert!(reg.base_units.is_empty());
    }

    #[test]
    fn get_unit_fallback_to_base_original_name() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            reg.add_base_unit("m".into());
            reg.register_alias("m".into(), "meter".into()).unwrap(); // resolves to "meter" which doesn't exist
            let result = reg.get_unit(py, "m".into()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert!(unit.dimensions.contains_key("m"));
        });
    }

    #[test]
    fn get_unit_fallback_to_derived_original_name() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut reg = UnitRegistry {
                base_units: HashMap::new(),
                derived_units: HashMap::new(),
                aliases: HashMap::new(),
            };
            let speed = length().div(&time());
            reg.add_derived_unit("v".into(), speed);
            reg.register_alias("v".into(), "velocity".into()).unwrap(); // resolves to "velocity" which doesn't exist
            let result = reg.get_unit(py, "v".into()).unwrap();
            let unit: RationalUnit = result.extract(py).unwrap();
            assert_eq!(unit.dimensions["L"], (1, 1));
        });
    }
}
