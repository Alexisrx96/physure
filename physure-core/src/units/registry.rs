use std::collections::HashMap;
use crate::units::rational::RationalUnit;

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
        let unit = RationalUnit::new_from_dimensions([(name.clone(), (1, 1))]);
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

    pub fn get_unit(&self, name: &str) -> Option<RationalUnit> {
        let resolved = self.resolve_symbol(name);
        if let Some(unit) = self.base_units.get(&resolved) {
            return Some(unit.clone());
        }
        if let Some(unit) = self.derived_units.get(&resolved) {
            return Some(unit.clone());
        }
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
