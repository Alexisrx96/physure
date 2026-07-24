use std::collections::HashMap;
use crate::units::rational::RationalUnit;

/// A registry to hold unit definitions, ensuring state isolation.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UnitMetadata {
    pub category: Option<String>,
    pub latex_symbol: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ConstantMetadata {
    pub value: String,
    pub description: Option<String>,
    pub latex_symbol: Option<String>,
}

/// A registry to hold unit definitions and rich metadata, ensuring state isolation.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitRegistry {
    pub base_units: HashMap<String, RationalUnit>,
    pub derived_units: HashMap<String, RationalUnit>,
    pub aliases: HashMap<String, String>,
    pub prefixes: HashMap<String, f64>,
    pub unit_meta: HashMap<String, UnitMetadata>,
    pub categories: HashMap<String, Vec<String>>,
    pub constants_meta: HashMap<String, ConstantMetadata>,
}

impl UnitRegistry {
    pub fn new() -> Self {
        UnitRegistry {
            base_units: HashMap::new(),
            derived_units: HashMap::new(),
            aliases: HashMap::new(),
            prefixes: HashMap::new(),
            unit_meta: HashMap::new(),
            categories: HashMap::new(),
            constants_meta: HashMap::new(),
        }
    }

    pub fn get_unit_latex(&self, name: &str) -> Option<&str> {
        let resolved = self.resolve_symbol(name);
        self.unit_meta.get(&resolved).and_then(|m| m.latex_symbol.as_deref())
    }

    pub fn get_category_units(&self, category: &str) -> Option<&Vec<String>> {
        self.categories.get(category)
    }

    pub fn add_prefix(&mut self, symbol: String, factor: f64) {
        self.prefixes.insert(symbol, factor);
    }

    pub fn add_base_unit(&mut self, name: String) {
        let unit = RationalUnit::new_from_dimensions([(name.clone(), (1, 1))]);
        self.base_units.insert(name, unit);
    }

    pub fn add_derived_unit(&mut self, name: String, mut definition: RationalUnit) {
        definition.display_name = Some(name.clone());
        self.derived_units.insert(name, definition);
    }

    /// Registers a unit that shares dimensions with `base` (which must already be registered)
    /// but has its own scale factor relative to it, e.g. `add_scaled_unit("km", "m", 1000.0)`.
    pub fn add_scaled_unit(&mut self, name: String, base: &str, scale: f64) {
        let base_unit = self.get_unit(base).expect("base unit must be registered first");
        let new_scale = base_unit.scale * scale;
        let mut u = base_unit.with_scale(new_scale);
        u.display_name = Some(name.clone());
        self.derived_units.insert(name, u);
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
                break;
            }
        }
        current
    }

    pub fn get_unit(&self, name: &str) -> Option<RationalUnit> {
        let resolved = self.resolve_symbol(name);
        let mut u = if let Some(unit) = self.base_units.get(&resolved) {
            Some(unit.clone())
        } else if let Some(unit) = self.derived_units.get(&resolved) {
            Some(unit.clone())
        } else if let Some(unit) = self.base_units.get(name) {
            Some(unit.clone())
        } else if let Some(unit) = self.derived_units.get(name) {
            Some(unit.clone())
        } else {
            let mut prefix_match = None;
            for (p_sym, p_factor) in &self.prefixes {
                if name.starts_with(p_sym) && name.len() > p_sym.len() {
                    let rest = &name[p_sym.len()..];
                    let rest_resolved = self.resolve_symbol(rest);
                    let base_opt = self
                        .base_units
                        .get(&rest_resolved)
                        .or_else(|| self.derived_units.get(&rest_resolved))
                        .or_else(|| self.base_units.get(rest))
                        .or_else(|| self.derived_units.get(rest));
                    if let Some(base_u) = base_opt {
                        let new_scale = base_u.scale * p_factor;
                        let mut prefixed = base_u.clone().with_scale(new_scale);
                        prefixed.display_name = Some(name.to_string());
                        prefix_match = Some(prefixed);
                        break;
                    }
                }
            }
            prefix_match
        };
        if let Some(ref mut unit) = u {
            if unit.display_name.is_none() {
                unit.display_name = Some(name.to_string());
            }
        }
        u
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

        // Pure synonyms (same magnitude, different spelling)
        reg.register_alias("meter".into(), "m".into());
        reg.register_alias("meters".into(), "m".into());
        reg.register_alias("second".into(), "s".into());
        reg.register_alias("seconds".into(), "s".into());
        reg.register_alias("gram".into(), "g".into());
        reg.register_alias("µC".into(), "uC".into());

        // Scaled (prefixed) units — real conversion factors relative to their base
        reg.add_scaled_unit("g".into(), "kg", 0.001);
        reg.add_scaled_unit("cm".into(), "m", 0.01);
        reg.add_scaled_unit("km".into(), "m", 1000.0);
        reg.add_scaled_unit("mm".into(), "m", 0.001);
        reg.add_scaled_unit("nm".into(), "m", 1e-9);
        reg.add_scaled_unit("h".into(), "s", 3600.0);
        reg.add_scaled_unit("min".into(), "s", 60.0);
        reg.add_scaled_unit("ms".into(), "s", 0.001);
        reg.add_scaled_unit("ns".into(), "s", 1e-9);
        reg.add_scaled_unit("nN".into(), "N", 1e-9);
        reg.add_scaled_unit("kN".into(), "N", 1000.0);
        reg.add_scaled_unit("kPa".into(), "Pa", 1000.0);
        reg.add_scaled_unit("nC".into(), "C", 1e-9);
        reg.add_scaled_unit("uC".into(), "C", 1e-6);
        reg.add_scaled_unit("pC".into(), "C", 1e-12);

        reg
    }

    /// Pre-bakes Imperial unit definitions for instant lookup.
    pub fn build_default_imperial() -> Self {
        let mut reg = Self::build_default_si();

        reg.add_scaled_unit("in".into(), "m", 0.0254);
        reg.add_scaled_unit("ft".into(), "m", 0.3048);
        reg.add_scaled_unit("yd".into(), "m", 0.9144);
        reg.add_scaled_unit("mi".into(), "m", 1609.344);
        reg.add_scaled_unit("lb".into(), "kg", 0.45359237);
        reg.add_scaled_unit("oz".into(), "kg", 0.028349523125);

        reg.register_alias("inch".into(), "in".into());
        reg.register_alias("inches".into(), "in".into());
        reg.register_alias("foot".into(), "ft".into());
        reg.register_alias("feet".into(), "ft".into());
        reg.register_alias("yard".into(), "yd".into());
        reg.register_alias("yards".into(), "yd".into());
        reg.register_alias("mile".into(), "mi".into());
        reg.register_alias("miles".into(), "mi".into());
        reg.register_alias("pound".into(), "lb".into());
        reg.register_alias("pounds".into(), "lb".into());
        reg.register_alias("lbs".into(), "lb".into());
        reg.register_alias("ounce".into(), "oz".into());
        reg.register_alias("ounces".into(), "oz".into());

        reg
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        Self::new()
    }
}
