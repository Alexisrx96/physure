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

    /// Pre-bakes the standard SI base units, prefixes, and derived units for instant startup.
    pub fn build_default_si() -> Self {
        let mut reg = Self::new();
        // SI Base Units
        for &base in &["m", "s", "kg", "A", "K", "mol", "cd"] {
            reg.add_base_unit(base.to_string());
        }

        // SI Derived Units
        let m = reg.get_unit("m").unwrap();
        let s = reg.get_unit("s").unwrap();
        let kg = reg.get_unit("kg").unwrap();
        let a = reg.get_unit("A").unwrap();
        let _k = reg.get_unit("K").unwrap();

        let hz = s.pow(num_rational::Rational64::new(-1, 1));
        let n = kg.mul(&m).div(&s.pow(num_rational::Rational64::new(2, 1)));
        let pa = n.div(&m.pow(num_rational::Rational64::new(2, 1)));
        let j = n.mul(&m);
        let w = j.div(&s);
        let c = a.mul(&s);
        let v = w.div(&a);
        let f = c.div(&v);
        let ohm = v.div(&a);

        reg.add_derived_unit("Hz".into(), hz);
        reg.add_derived_unit("N".into(), n);
        reg.add_derived_unit("Pa".into(), pa);
        reg.add_derived_unit("J".into(), j);
        reg.add_derived_unit("W".into(), w);
        reg.add_derived_unit("C".into(), c);
        reg.add_derived_unit("V".into(), v);
        reg.add_derived_unit("F".into(), f);
        reg.add_derived_unit("ohm".into(), ohm.clone());
        reg.register_alias("Ω".into(), "ohm".into());

        // Standard aliases
        reg.register_alias("meter".into(), "m".into());
        reg.register_alias("meters".into(), "m".into());
        reg.register_alias("second".into(), "s".into());
        reg.register_alias("seconds".into(), "s".into());
        reg.register_alias("gram".into(), "g".into());

        reg
    }

    /// Pre-bakes Imperial unit definitions for instant lookup.
    pub fn build_default_imperial() -> Self {
        let mut reg = Self::build_default_si();
        let m = reg.get_unit("m").unwrap();
        let kg = reg.get_unit("kg").unwrap();

        let inch = m.clone(); // Dimensional equivalent to m
        let ft = inch.clone();
        let yd = inch.clone();
        let mi = inch.clone();
        let lb = kg.clone();

        reg.add_derived_unit("in".into(), inch);
        reg.add_derived_unit("ft".into(), ft);
        reg.add_derived_unit("yd".into(), yd);
        reg.add_derived_unit("mi".into(), mi);
        reg.add_derived_unit("lb".into(), lb);

        reg.register_alias("inch".into(), "in".into());
        reg.register_alias("foot".into(), "ft".into());
        reg.register_alias("yard".into(), "yd".into());
        reg.register_alias("mile".into(), "mi".into());
        reg.register_alias("pound".into(), "lb".into());

        reg
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        Self::new()
    }
}
