use pyo3::prelude::*;
use pyo3::Bound;
use pyo3::PyResult;
use std::collections::HashMap;
use num_rational::Rational64;
use std::hash::{Hash, Hasher};

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
    pub fn new(dims: Option<HashMap<String, (i64, i64)>>) -> Self {
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

    pub fn __mul__(&self, other: &RationalUnit) -> RationalUnit {
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

    pub fn __truediv__(&self, other: &RationalUnit) -> RationalUnit {
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

    #[getter]
    fn exponents(&self) -> HashMap<String, f64> {
        self.dimensions.iter().map(|(k, (n, d))| {
            (k.clone(), *n as f64 / *d as f64)
        }).collect()
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
            if *den == 1 {
                parts.push(format!("{}^{}", base, num));
            } else {
                parts.push(format!("{}^{}/{}", base, num, den));
            }
        }
        parts.join(" * ")
    }
}

/// A registry to hold unit definitions, ensuring state isolation.
#[pyclass]
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
        let unit = RationalUnit { dimensions: dims };
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
        // Simple non-recursive alias check for now to avoid cycles, 
        // or a small loop limit
        for _ in 0..10 {
            if let Some(target) = self.aliases.get(&current) {
                current = target.clone();
            } else {
                return current;
            }
        }
        current // Return last resolved
    }

    fn get_unit(&self, name: String) -> PyResult<RationalUnit> {
        let resolved = self.resolve_symbol(name.clone());

        if let Some(unit) = self.base_units.get(&resolved) {
            Ok(unit.clone())
        } else if let Some(unit) = self.derived_units.get(&resolved) {
            Ok(unit.clone())
        } else {
            // Also check keys directly in case resolve failed or name is direct key
            if let Some(unit) = self.base_units.get(&name) {
                return Ok(unit.clone());
            }
            if let Some(unit) = self.derived_units.get(&name) {
               return Ok(unit.clone());
            }
            Err(pyo3::exceptions::PyKeyError::new_err(format!("Unit '{}' not found", name)))
        }
    }
    
    fn contains(&self, name: String) -> bool {
        let resolved = self.resolve_symbol(name.clone());
        self.base_units.contains_key(&resolved) || self.derived_units.contains_key(&resolved)
    }
}
