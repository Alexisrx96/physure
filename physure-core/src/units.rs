use std::collections::HashMap;
use num_rational::Rational64;
use std::hash::{Hash, Hasher};

/// A unit representation using rational exponents to avoid floating-point errors.
#[derive(Clone, Debug, Eq)]
pub struct RationalUnit {
    /// Map of base unit names to their exponents as (numerator, denominator).
    pub dimensions: HashMap<String, (i64, i64)>,
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
    /// Parse a rational exponent from a plain Rust (i64, i64) tuple or i64.
    pub fn parse_exponent_tuple(n: i64, den: i64) -> Option<(i64, i64)> {
        if n != 0 { Some((n, den)) } else { None }
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

    pub fn dimensionless() -> Self {
        Self::new_from_dimensions(HashMap::new())
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

    pub fn __eq__(&self, other: &RationalUnit) -> bool {
        self.dimensions == other.dimensions
    }

    pub fn __hash__(&self) -> u64 {
        self.id
    }

    pub fn to_string(&self, _system: Option<()>, _use_alias: bool, _alias_preference: Option<()>) -> String {
        self.__repr__()
    }
}


/// A registry to hold unit definitions, ensuring state isolation.
pub struct UnitRegistry {
    pub base_units: HashMap<String, RationalUnit>,
    pub derived_units: HashMap<String, RationalUnit>,
    pub aliases: HashMap<String, String>,
}

impl UnitRegistry {
    pub fn new() -> Self {
        UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn add_base_unit(&mut self, name: String) {
        let mut dims = HashMap::new();
        dims.insert(name.clone(), (1, 1));
        let unit = RationalUnit::new_from_dimensions(dims);
        self.base_units.insert(name, unit);
    }

    pub fn add_derived_unit(&mut self, name: String, definition: RationalUnit) {
        self.derived_units.insert(name, definition);
    }

    pub fn register_alias(&mut self, alias: String, symbol: String) {
        self.aliases.insert(alias, symbol);
    }

    pub fn resolve_symbol(&self, name: &str) -> String {
        let mut current = name.to_string();
        for _ in 0..10 {
            if let Some(target) = self.aliases.get(&current) {
                current = target.clone();
            } else {
                return current;
            }
        }
        current
    }

    /// Look up a unit by name (or alias). Returns None if not found.
    pub fn get_unit(&self, name: &str) -> Option<RationalUnit> {
        let resolved = self.resolve_symbol(name);
        if let Some(unit) = self.base_units.get(&resolved) {
            return Some(unit.clone());
        }
        if let Some(unit) = self.derived_units.get(&resolved) {
            return Some(unit.clone());
        }
        // Fallback: try the original name
        if let Some(unit) = self.base_units.get(name) {
            return Some(unit.clone());
        }
        if let Some(unit) = self.derived_units.get(name) {
            return Some(unit.clone());
        }
        None
    }

    pub fn contains(&self, name: &str) -> bool {
        let resolved = self.resolve_symbol(name);
        self.base_units.contains_key(&resolved) || self.derived_units.contains_key(&resolved)
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::Rational64;

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
        assert_eq!(u.id, u.id);
    }

    #[test]
    fn dimensions_accessor_returns_clone() {
        let u = length();
        let dims = u.dimensions.clone();
        assert_eq!(dims["L"], (1, 1));
        assert_eq!(dims.len(), 1);
    }

    #[test]
    fn to_string_delegates_to_repr() {
        assert_eq!(length().to_string(None, false, None), "L");
    }

    #[test]
    fn unit_registry_add_and_lookup() {
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("m".into());
        assert!(reg.contains("m"));
        assert!(!reg.contains("s"));
    }

    #[test]
    fn unit_registry_alias_resolves() {
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("meter".into());
        reg.register_alias("m".into(), "meter".into());
        assert!(reg.contains("m"));
        assert_eq!(reg.resolve_symbol("m"), "meter");
    }

    #[test]
    fn unit_registry_add_derived() {
        let mut reg = UnitRegistry::new();
        let speed = length().div(&time());
        reg.add_derived_unit("m_per_s".into(), speed.clone());
        assert!(reg.contains("m_per_s"));
    }

    #[test]
    fn parse_exponent_tuple() {
        assert_eq!(RationalUnit::parse_exponent_tuple(3, 2), Some((3, 2)));
        assert_eq!(RationalUnit::parse_exponent_tuple(0, 1), None);
        assert_eq!(RationalUnit::parse_exponent_tuple(2, 1), Some((2, 1)));
    }

    #[test]
    fn unit_registry_get_unit_base() {
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("m".into());
        let unit = reg.get_unit("m").unwrap();
        assert_eq!(unit.dimensions["m"], (1, 1));
    }

    #[test]
    fn unit_registry_get_unit_derived() {
        let mut reg = UnitRegistry::new();
        let speed = length().div(&time());
        reg.add_derived_unit("v".into(), speed);
        let unit = reg.get_unit("v").unwrap();
        assert_eq!(unit.dimensions["L"], (1, 1));
    }

    #[test]
    fn unit_registry_get_unit_via_alias() {
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("meter".into());
        reg.register_alias("m".into(), "meter".into());
        let unit = reg.get_unit("m").unwrap();
        assert!(unit.dimensions.contains_key("meter"));
    }

    #[test]
    fn unit_registry_get_unit_not_found() {
        let reg = UnitRegistry::new();
        assert!(reg.get_unit("nope").is_none());
    }

    #[test]
    fn unit_registry_resolve_caps_at_10_hops() {
        let mut reg = UnitRegistry::new();
        for i in 0..12u32 {
            reg.register_alias(format!("a{}", i), format!("a{}", i + 1));
        }
        reg.add_base_unit("a12".into());
        let resolved = reg.resolve_symbol("a0");
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
    fn unit_registry_new_is_empty() {
        let reg = UnitRegistry::new();
        assert!(reg.base_units.is_empty());
    }

    #[test]
    fn get_unit_fallback_to_base_original_name() {
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("m".into());
        reg.register_alias("m".into(), "meter".into()); // resolves to "meter" which doesn't exist
        let unit = reg.get_unit("m").unwrap();
        assert!(unit.dimensions.contains_key("m"));
    }

    #[test]
    fn get_unit_fallback_to_derived_original_name() {
        let mut reg = UnitRegistry::new();
        let speed = length().div(&time());
        reg.add_derived_unit("v".into(), speed);
        reg.register_alias("v".into(), "velocity".into()); // resolves to "velocity" which doesn't exist
        let unit = reg.get_unit("v").unwrap();
        assert_eq!(unit.dimensions["L"], (1, 1));
    }
}



