use std::collections::{HashMap, HashSet};
use num_rational::Rational64;
use crate::error::{PhysureError, PhysureResult};
use crate::units::exact::ExactScale;
use crate::units::rational::{LogScale, RationalUnit, UnitKind};

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

/// A unit's base symbols with their exponents, sorted: the key of a recipe unit.
pub type DimKey = Vec<(String, (i64, i64))>;

/// How a number in one unit becomes a number in another (see `UnitRegistry::conversion`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Conversion {
    /// `new = old * scale + shift`: a plain factor (the shift is zero), or two temperature
    /// scales, whose zero points differ.
    Affine { scale: f64, shift: f64 },
    /// A level is involved: `from` turns the number into the plain ratio both stand for and `to`
    /// turns that ratio into the number.
    Level { from: Stage, to: Stage },
}

/// One side of a conversion through a ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// A plain unit: `ratio = number * scale`.
    Scale(f64),
    /// A level: `ratio = reference * 10^(number / factor)`, and back `number = factor *
    /// log10(ratio / reference)`.
    Level(LogScale),
}

impl Stage {
    fn of(unit: &RationalUnit) -> Self {
        match unit.log {
            Some(log) => Stage::Level(log),
            None => Stage::Scale(unit.scale),
        }
    }
}

/// What the registry says about a unit expression.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitFacts {
    /// The physical dimension, by dimension symbol: `kg·m/s²` is `L: 1, M: 1, T: -2`.
    pub dimension: Vec<(String, (i64, i64))>,
    /// `Point` for an absolute temperature scale (degC, degF), `Delta` for everything else.
    pub kind: UnitKind,
    /// False for an offset or a logarithmic unit.
    pub is_linear: bool,
    /// The shortest name the registry knows for the unit: `J/s` is `W`, `kilometer` is `km`.
    pub alias: String,
    /// The unit with every recipe unit (`N`, `J`, `Hz`...) replaced by its base symbols.
    pub simplified: String,
}

/// A registry to hold unit definitions and rich metadata, ensuring state isolation.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitRegistry {
    pub base_units: HashMap<String, RationalUnit>,
    pub derived_units: HashMap<String, RationalUnit>,
    pub aliases: HashMap<String, String>,
    pub prefixes: HashMap<String, f64>,
    /// The exact value of each prefix factor, for the prefixes whose factor is known exactly.
    pub prefix_exact: HashMap<String, ExactScale>,
    /// The spelled-out name of each prefix (`kilo`) and the symbol it stands for (`k`), so
    /// that `kilometer` reads as `km`.
    pub prefix_names: HashMap<String, String>,
    /// Every name of a unit, in the order the catalog lists them, under its symbol.
    pub symbol_aliases: HashMap<String, Vec<String>>,
    /// The dimension symbol of each base unit (`m` is `L`, `kg` is `M`).
    pub base_dimension: HashMap<String, String>,
    /// The base unit of each dimension symbol (`L` is `m`, `$` is `USD`): the first one declared,
    /// kept so that a text parsed later can declare units of a dimension without declaring its base
    /// unit again, and cannot make a second base unit of it by accident.
    pub dimension_base: HashMap<String, String>,
    /// The units the catalog defines by a recipe at scale 1 (`N = kg*m/s^2`): the ones that
    /// `simplify` replaces by their base symbols. A unit at another scale stays what it is.
    pub recipes: HashSet<String>,
    /// The names of the recipe units, under the base symbols they expand to.
    pub recipe_aliases: HashMap<DimKey, Vec<String>>,
    /// The names of the units that take no prefix (`noprefix` in the catalog): `mUSD`, `kh` and
    /// `kmi` are not units, however many of the names of the unit they scale.
    pub no_prefix: HashSet<String>,
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
            prefix_exact: HashMap::new(),
            prefix_names: HashMap::new(),
            symbol_aliases: HashMap::new(),
            base_dimension: HashMap::new(),
            dimension_base: HashMap::new(),
            recipes: HashSet::new(),
            no_prefix: HashSet::new(),
            recipe_aliases: HashMap::new(),
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
        self.add_prefix_exact(symbol, factor, ExactScale::from_f64(factor));
    }

    /// Registers a prefix with its factor as the double and, when known, as the exact value
    /// that the catalog states.
    pub fn add_prefix_exact(&mut self, symbol: String, factor: f64, exact: Option<ExactScale>) {
        match exact {
            Some(e) => self.prefix_exact.insert(symbol.clone(), e),
            None => self.prefix_exact.remove(&symbol),
        };
        self.prefixes.insert(symbol, factor);
    }

    /// Registers the spelled-out name of a prefix that is already registered by its symbol.
    pub fn add_prefix_name(&mut self, name: String, symbol: String) {
        self.prefix_names.insert(name, symbol);
    }

    /// Registers a level of a plain ratio: `add_log_unit("dB", 10.0, 1.0)` is `10 * log10(ratio / 1)`.
    pub fn add_log_unit(&mut self, name: String, factor: f64, reference: f64) {
        self.add_level(name, RationalUnit::dimensionless(), factor, reference);
    }

    /// Registers a level of a quantity: `dBW` is `10 * log10(P / 1 W)`, a level with the dimensions
    /// of `quantity` (its scale and zero point are ignored) whose reference is in base units.
    pub fn add_level(&mut self, name: String, quantity: RationalUnit, factor: f64, reference: f64) {
        let mut u = quantity.with_unit_scale().with_offset(0.0).with_log(LogScale { factor, reference });
        u.display_name = Some(name.clone());
        self.derived_units.insert(name, u);
    }

    /// Records the names of a unit, in the order the catalog lists them.
    pub fn note_aliases(&mut self, symbol: &str, names: &[String]) {
        let known = self.symbol_aliases.entry(symbol.to_string()).or_default();
        for name in names {
            if !known.contains(name) {
                known.push(name.clone());
            }
        }
    }

    /// Records that `symbol` is a recipe unit, and what it expands to.
    pub fn note_recipe(&mut self, symbol: &str, expansion: &[(String, (i64, i64))], names: &[String]) {
        self.recipes.insert(symbol.to_string());
        let known = self.recipe_aliases.entry(expansion.to_vec()).or_default();
        for name in names {
            if !known.contains(name) {
                known.push(name.clone());
            }
        }
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
        self.add_scaled_unit_exact(name, base, scale, ExactScale::from_f64(scale));
    }

    /// Like `add_scaled_unit`, with the factor also given as the exact value the catalog states.
    pub fn add_scaled_unit_exact(&mut self, name: String, base: &str, scale: f64, exact: Option<ExactScale>) {
        let base_unit = self.get_unit(base).expect("base unit must be registered first");
        let mut u = Self::scaled(base_unit, scale, exact);
        u.display_name = Some(name.clone());
        self.derived_units.insert(name, u);
    }

    /// `base` rescaled by `factor`: exact when both the base and the factor are, otherwise the
    /// product of the doubles.
    fn scaled(base: RationalUnit, factor: f64, exact: Option<ExactScale>) -> RationalUnit {
        match (&base.exact, exact) {
            (Some(b), Some(e)) => {
                let combined = b.mul(&e);
                base.with_exact_scale(combined)
            }
            _ => {
                let product = base.scale * factor;
                base.with_scale(product)
            }
        }
    }

    /// Registers an absolute affine unit against an already-registered `base`, e.g.
    /// `add_affine_unit("degC", "K", 1.0, 273.15)`: one degC is `scale` base units wide and
    /// its zero sits `offset` base units above the base unit's zero.
    pub fn add_affine_unit(&mut self, name: String, base: &str, scale: f64, offset: f64) {
        self.add_affine_unit_exact(name, base, scale, ExactScale::from_f64(scale), offset, ExactScale::from_f64(offset));
    }

    /// Like `add_affine_unit`, with the scale also given as the exact value the catalog states.
    pub fn add_affine_unit_exact(
        &mut self,
        name: String,
        base: &str,
        scale: f64,
        exact: Option<ExactScale>,
        offset: f64,
        exact_offset: Option<ExactScale>,
    ) {
        let base_unit = self.get_unit(base).expect("base unit must be registered first");
        let mut u = Self::scaled(base_unit, scale, exact).with_offset(offset);
        if let Some(e) = exact_offset {
            u = u.with_exact_offset(e);
        }
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

    /// The registered symbol closest to `name`, for a "did you mean" hint when a unit
    /// lookup fails. Suggestions are capped at roughly one edit per four characters so an
    /// unrelated word (`foobar`) gets no hint at all, while a plausible typo
    /// (`metre` → `meter`, `Km` → `km`) does.
    pub fn closest_symbol(&self, name: &str) -> Option<String> {
        let budget = 1 + name.chars().count() / 4;
        let lowercase = name.to_lowercase();

        // Prefixed symbols ("km", "kPa") are synthesised by `get_unit` from a prefix and a
        // base, never stored as keys, so the edit-distance search below cannot see them.
        // Case-only slips are the common way to miss one, so retry those first.
        let first_char_lowered: String =
            name.chars().enumerate().map(|(i, c)| if i == 0 { c.to_ascii_lowercase() } else { c }).collect();
        for variant in [&lowercase, &first_char_lowered] {
            if variant.as_str() != name && self.get_unit(variant).is_some() {
                return Some(variant.clone());
            }
        }

        self.base_units
            .keys()
            .chain(self.derived_units.keys())
            .chain(self.aliases.keys())
            .map(|candidate| (edit_distance(name, candidate), candidate))
            .filter(|(distance, _)| *distance <= budget)
            // Ties go to the candidate that shares the most of the typed prefix, so "wat"
            // suggests "watt" and not "kat" — both are one edit away from it.
            .min_by_key(|(distance, candidate)| {
                let shared = lowercase
                    .chars()
                    .zip(candidate.to_lowercase().chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                (*distance, std::cmp::Reverse(shared), candidate.len())
            })
            .map(|(_, candidate)| candidate.clone())
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
            self.split_prefix(name).map(|(p_sym, p_factor, base_u)| {
                let mut prefixed = Self::scaled(base_u, p_factor, self.prefix_exact.get(&p_sym).cloned());
                prefixed.display_name = Some(name.to_string());
                prefixed
            })
        };
        if let Some(ref mut unit) = u {
            if unit.display_name.is_none() {
                unit.display_name = Some(name.to_string());
            }
        }
        u
    }

    /// Splits `name` into a registered prefix symbol and its factor, if `name` is a known
    /// prefix immediately followed by a base or derived unit symbol (e.g. `"km"` -> `Some(("k",
    /// 1000.0, <the "m" RationalUnit>))`; `"km/h"` or any other compound expression -> `None`,
    /// since this only recognizes a single prefixed symbol, not an arbitrary unit expression).
    /// Factored out of `get_unit`'s own prefix-matching so `Inspection`'s reverse lookup (Track
    /// C) reuses this exact rule instead of re-deriving it.
    ///
    /// Returns `(prefix symbol, prefix factor, the unprefixed remainder's own RationalUnit)`.
    ///
    /// A prefix is written by its symbol (`km`) or by its name (`kilometer`; the symbol that
    /// comes back is still `k`). When several prefixes fit, the longest one that leaves a unit
    /// wins, so the answer does not depend on the order a `HashMap` iterates in: `dam` is
    /// deca + `m`, never deci + `am`.
    /// Carrying the resolved unit forward means `get_unit`'s caller doesn't have to redo the
    /// same `resolve_symbol` + four-way lookup a second time just to fetch what this method
    /// already found while checking whether `name` was a known prefix+unit combination.
    pub fn split_prefix(&self, name: &str) -> Option<(String, f64, RationalUnit)> {
        self.split_prefix_of(name).map(|(symbol, factor, _, unit)| (symbol, factor, unit))
    }

    /// `split_prefix` that also says which spelling of the unit followed the prefix.
    fn split_prefix_of(&self, name: &str) -> Option<(String, f64, String, RationalUnit)> {
        let symbols = self.prefixes.iter().map(|(symbol, factor)| (symbol.as_str(), symbol.as_str(), *factor));
        let names = self.prefix_names.iter().filter_map(|(long, symbol)| {
            self.prefixes.get(symbol).map(|factor| (long.as_str(), symbol.as_str(), *factor))
        });
        let mut best: Option<(usize, &str, f64, &str, &RationalUnit)> = None;
        for (spelled, symbol, factor) in symbols.chain(names) {
            if name.len() <= spelled.len() || !name.starts_with(spelled) {
                continue;
            }
            if best.is_some_and(|(length, ..)| length >= spelled.len()) {
                continue;
            }
            // A prefix scales a unit; it has nothing to scale on a level (`mdB`, `kpH`), and a unit
            // that the catalog marks `noprefix` takes none.
            let rest = &name[spelled.len()..];
            if self.no_prefix.contains(rest) {
                continue;
            }
            if let Some(unit) = self.unit_by_symbol(rest).filter(|unit| !unit.is_logarithmic()) {
                best = Some((spelled.len(), symbol, factor, &name[spelled.len()..], unit));
            }
        }
        best.map(|(_, symbol, factor, rest, unit)| (symbol.to_string(), factor, rest.to_string(), unit.clone()))
    }

    /// The unit with every recipe unit replaced by its base symbols (what Python's `get_unit`
    /// hands back for `J/s`: `kg·m²/s³`). `atomic` is a unit parsed with
    /// `Parser::parse_expression_atomic`, whose dimensions are the symbols the user wrote.
    pub fn simplify(&self, atomic: &RationalUnit) -> RationalUnit {
        let zero = Rational64::from_integer(0);
        let mut sums: HashMap<String, Rational64> = HashMap::new();
        for (symbol, (n, d)) in &atomic.dimensions {
            let exponent = Rational64::new(*n, *d);
            let resolved = self.resolve_symbol(symbol);
            if self.derived_units.get(&resolved).is_some_and(|unit| unit.is_plain_number()) {
                // the unit one (`unity`) is nothing to write
                continue;
            }
            let recipe = if self.recipes.contains(&resolved) { self.get_unit(&resolved) } else { None };
            match recipe {
                Some(recipe) => {
                    for (base, (bn, bd)) in &recipe.dimensions {
                        *sums.entry(base.clone()).or_insert(zero) += Rational64::new(*bn, *bd) * exponent;
                    }
                }
                None => *sums.entry(symbol.clone()).or_insert(zero) += exponent,
            }
        }
        RationalUnit::new_from_dimensions(
            sums.into_iter().filter(|(_, e)| *e != zero).map(|(symbol, e)| (symbol, (*e.numer(), *e.denom()))),
        )
    }

    /// The physical dimension of a unit written as symbols, by dimension symbol.
    pub fn dimension_of(&self, atomic: &RationalUnit) -> PhysureResult<Vec<(String, (i64, i64))>> {
        let zero = Rational64::from_integer(0);
        let mut sums: HashMap<String, Rational64> = HashMap::new();
        for (symbol, (n, d)) in &atomic.dimensions {
            let unit = self.get_unit(symbol).ok_or_else(|| PhysureError::UnknownUnit {
                suggestion: self.closest_symbol(symbol),
                symbol: symbol.clone(),
            })?;
            for (base, (bn, bd)) in &unit.dimensions {
                let dimension = self.base_dimension.get(base).unwrap_or(base).clone();
                *sums.entry(dimension).or_insert(zero) += Rational64::new(*bn, *bd) * Rational64::new(*n, *d);
            }
        }
        let mut dimension: Vec<_> =
            sums.into_iter().filter(|(_, e)| *e != zero).map(|(symbol, e)| (symbol, (*e.numer(), *e.denom()))).collect();
        dimension.sort();
        Ok(dimension)
    }

    /// `Point` for a single absolute temperature scale, `Delta` for anything else.
    pub fn kind_of(&self, atomic: &RationalUnit) -> UnitKind {
        match atomic.dimensions.as_slice() {
            [(symbol, (1, 1))] if self.get_unit(symbol).is_some_and(|unit| unit.is_affine()) => UnitKind::Point,
            _ => UnitKind::Delta,
        }
    }

    /// False when any symbol of the unit is an offset or a logarithmic unit.
    pub fn is_linear_of(&self, atomic: &RationalUnit) -> bool {
        atomic
            .dimensions
            .iter()
            .all(|(symbol, _)| self.get_unit(symbol).map_or(true, |unit| !unit.is_affine() && !unit.is_logarithmic()))
    }

    /// The name a unit is shown by (`to_string(use_alias=True)` in Python): a registered symbol by its
    /// shortest name, any other name as written, a prefixed one by the prefix symbol and the symbol of
    /// what it prefixes (`kilometer` is `km`), a recipe unit written out in base symbols by the shortest name of the
    /// recipe (`kg·m²/s³` is `W`), and anything else written out.
    pub fn alias_of(&self, atomic: &RationalUnit) -> String {
        let unit = self.simplify(atomic);
        let found = match unit.dimensions.as_slice() {
            [(symbol, (1, 1))] => self.alias_of_symbol(symbol),
            dimensions => self.recipe_aliases.get(dimensions).and_then(|names| shortest(names)).cloned(),
        };
        found.unwrap_or_else(|| unit.symbolic_repr())
    }

    /// How a unit symbol is shown: the shortest name when it is the unit's symbol (the first name the
    /// catalog lists), as written when it is any other name, and a prefixed one by the prefix symbol
    /// and the symbol of what it prefixes.
    fn alias_of_symbol(&self, name: &str) -> Option<String> {
        let canonical = self.resolve_symbol(name);
        if self.base_units.contains_key(&canonical) || self.derived_units.contains_key(&canonical) {
            // The unit written by its symbol is shown by the shortest of its names (`deg` is `°`); one
            // written by any other name as it was written: `gallon` stays `gallon`.
            let names = self.symbol_aliases.get(&canonical)?;
            return if names.first().is_some_and(|symbol| symbol == name) {
                shortest(names).cloned()
            } else {
                Some(name.to_string())
            };
        }
        let (prefix, _, rest, _) = self.split_prefix_of(name)?;
        let base = self.symbol_aliases.get(&self.resolve_symbol(&rest))?.first()?;
        Some(format!("{prefix}{base}"))
    }

    /// Dimension, kind, linearity, alias and simplified form of a unit expression.
    pub fn facts(&self, expression: &str) -> PhysureResult<UnitFacts> {
        let atomic = crate::units::parser::Parser::parse_expression_atomic(expression)?;
        let unit = self.simplify(&atomic);
        Ok(UnitFacts {
            dimension: self.dimension_of(&unit)?,
            kind: self.kind_of(&unit),
            is_linear: self.is_linear_of(&unit),
            alias: self.alias_of(&unit),
            simplified: unit.symbolic_repr(),
        })
    }

    /// The unit that a unit written as symbols stands for: each symbol replaced by what the
    /// registry says it is, with its exact scale (`km/h` is 5/18 m/s). `atomic` is a unit parsed
    /// with `Parser::parse_expression_atomic`. A symbol the registry does not know is an error, and
    /// so is a level inside a compound (`RationalUnit::refuse_logarithmic_compound`).
    pub fn expand(&self, atomic: &RationalUnit) -> PhysureResult<RationalUnit> {
        // A single unit is what the registry says it is, zero point and level included: a product
        // would keep only its scale.
        if let [(symbol, (1, 1))] = atomic.dimensions.iter().collect::<Vec<_>>().as_slice() {
            return self.get_unit(symbol).ok_or_else(|| PhysureError::UnknownUnit {
                suggestion: self.closest_symbol(symbol),
                symbol: symbol.to_string(),
            });
        }
        let mut expanded = RationalUnit::dimensionless();
        for (symbol, (n, d)) in &atomic.dimensions {
            let unit = self.get_unit(symbol).ok_or_else(|| PhysureError::UnknownUnit {
                suggestion: self.closest_symbol(symbol),
                symbol: symbol.clone(),
            })?;
            let power = unit.pow(Rational64::new(*n, *d));
            let product = expanded.mul(&power);
            RationalUnit::refuse_logarithmic_compound(&[&expanded, &power], &product)?;
            expanded = product;
        }
        Ok(expanded)
    }

    /// The factor that turns a magnitude in `from` into one in `to`, two units written as symbols
    /// of the same dimension: the ratio of their exact scales, rounded once. The zero point of a
    /// temperature scale is not part of a factor.
    pub fn factor_between(&self, from: &RationalUnit, to: &RationalUnit) -> PhysureResult<f64> {
        let (from, to) = (self.expand(from)?, self.expand(to)?);
        if from.is_logarithmic() || to.is_logarithmic() {
            return Err(PhysureError::Generic(format!(
                "a level ('{}' to '{}') has no factor: it converts through the logarithm of a ratio",
                from.__repr__(),
                to.__repr__()
            )));
        }
        if !from.same_dimensions(&to) {
            return Err(PhysureError::UnitMismatch { expected: from.__repr__(), actual: to.__repr__() });
        }
        Ok(from.ratio_to(&to))
    }

    /// The unit that measures a dimension in the base units of this registry: each dimension symbol
    /// is replaced by the base unit declared for it (`L` is `m`, `M` is `kg`), to its power. It is
    /// written as symbols, like a unit parsed with `Parser::parse_expression_atomic`.
    pub fn unit_of_dimension(&self, dimension: &[(String, (i64, i64))]) -> PhysureResult<RationalUnit> {
        let mut symbols = Vec::with_capacity(dimension.len());
        for (symbol, exponent) in dimension {
            let base = self.dimension_base.get(symbol).ok_or_else(|| {
                PhysureError::Generic(format!("no base unit is declared for the dimension '{symbol}'"))
            })?;
            symbols.push((base.clone(), *exponent));
        }
        Ok(RationalUnit::new_from_dimensions(symbols))
    }

    /// How a number in `from` becomes a number in `to`, two units written as symbols of the same
    /// dimension: the plan the core itself follows in `Quantity::convert_to`. A caller that holds
    /// its numbers elsewhere (NumPy, torch) applies the plan with its own operations and gets the
    /// answer of the core, rounded the same way.
    pub fn conversion(&self, from: &RationalUnit, to: &RationalUnit) -> PhysureResult<Conversion> {
        let (from, to) = (self.expand(from)?, self.expand(to)?);
        if !from.same_dimensions(&to) {
            return Err(PhysureError::UnitMismatch { expected: from.__repr__(), actual: to.__repr__() });
        }
        if from.is_logarithmic() || to.is_logarithmic() {
            if from.log == to.log && from.scale == to.scale {
                // the same level under another name (decibel, dB): nothing to convert
                return Ok(Conversion::Affine { scale: 1.0, shift: 0.0 });
            }
            return Ok(Conversion::Level { from: Stage::of(&from), to: Stage::of(&to) });
        }
        if from.is_affine() || to.is_affine() {
            let (scale, shift) = from.affine_to(&to);
            return Ok(Conversion::Affine { scale, shift });
        }
        Ok(Conversion::Affine { scale: from.ratio_to(&to), shift: 0.0 })
    }

    /// The base or derived unit that `symbol` names, directly or through an alias.
    fn unit_by_symbol(&self, symbol: &str) -> Option<&RationalUnit> {
        let resolved = self.resolve_symbol(symbol);
        self.base_units
            .get(&resolved)
            .or_else(|| self.derived_units.get(&resolved))
            .or_else(|| self.base_units.get(symbol))
            .or_else(|| self.derived_units.get(symbol))
    }

    /// Every name the registry holds for a unit, sorted: base units, derived units and aliases.
    /// A prefixed name (`km`) is not held, it is read from `k` and `m`.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .base_units
            .keys()
            .chain(self.derived_units.keys())
            .chain(self.aliases.keys())
            .cloned()
            .collect();
        names.sort();
        names.dedup();
        names
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

/// Levenshtein distance over `char`s — unit symbols are full of non-ASCII (Å, Ω, °, µ),
/// so a byte-wise distance would score them wrong.
/// The shortest of the names, the first listed among equals.
fn shortest(names: &[String]) -> Option<&String> {
    names.iter().min_by_key(|name| name.chars().count())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b_chars.len()).collect();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, a_char) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, &b_char) in b_chars.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != b_char);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b_chars.len()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_prefix_recognizes_a_registered_prefix_over_a_known_unit() {
        let (reg, _) = crate::units::conf::build_registry_from_conf();
        let (symbol, factor, _unit) = reg.split_prefix("km").expect("km should split as k + m");
        assert_eq!(symbol, "k");
        assert_eq!(factor, 1000.0);
    }

    #[test]
    fn a_prefix_can_be_spelled_out() {
        let (reg, _) = crate::units::conf::build_registry_from_conf();
        let (symbol, factor, unit) = reg.split_prefix("kilometer").expect("kilometer is kilo + meter");
        assert_eq!((symbol.as_str(), factor), ("k", 1000.0));
        assert_eq!(unit.scale, 1.0);
        assert_eq!(reg.get_unit("kilometer").unwrap().scale, 1000.0);
        assert_eq!(reg.get_unit("attoampere").unwrap().scale, 1e-18);
        assert_eq!(reg.get_unit("kilometer").unwrap().ratio_to(&reg.get_unit("km").unwrap()), 1.0);
    }

    #[test]
    fn the_longest_prefix_that_leaves_a_unit_wins() {
        use super::UnitRegistry;
        let mut reg = UnitRegistry::new();
        reg.add_base_unit("m".to_string());
        reg.add_base_unit("am".to_string());
        reg.add_prefix("d".to_string(), 0.1);
        reg.add_prefix("da".to_string(), 10.0);
        // deci + "am" and deca + "m" both leave a unit; a HashMap would pick either
        for _ in 0..20 {
            let (symbol, factor, unit) = reg.split_prefix("dam").unwrap();
            assert_eq!((symbol.as_str(), factor), ("da", 10.0));
            assert_eq!(unit.dimensions, reg.get_unit("m").unwrap().dimensions);
        }
    }

    #[test]
    fn split_prefix_returns_none_for_a_plain_unregistered_remainder() {
        let (reg, _) = crate::units::conf::build_registry_from_conf();
        assert!(reg.split_prefix("qzzz").is_none());
    }

    #[test]
    fn split_prefix_result_carries_the_resolved_unit_so_get_unit_does_not_look_it_up_twice() {
        let (reg, _) = crate::units::conf::build_registry_from_conf();
        let (symbol, factor, unit) = reg.split_prefix("km").expect("km should split as k + m");
        assert_eq!(symbol, "k");
        assert_eq!(factor, 1000.0);
        // The returned unit is the *unprefixed* remainder ("m"), scale 1.0 -- get_unit is what
        // applies `factor` on top of it, not split_prefix's job.
        assert_eq!(unit.scale, 1.0);
    }
}
