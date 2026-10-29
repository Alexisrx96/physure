use std::collections::HashMap;
use num_rational::Rational64;
use std::hash::{Hash, Hasher};
use smallvec::SmallVec;
use crate::error::{PhysureError, PhysureResult};
use crate::units::exact::ExactScale;

pub type DimVec = SmallVec<[(String, (i64, i64)); 4]>;

/// An exponent as superscript digits: `2` is `²`, `0.5` is `⁰⋅⁵`.
fn superscript(exponent: f64) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    format!("{exponent}")
        .chars()
        .map(|c| match c {
            '.' => '⋅',
            digit => DIGITS[digit as usize - '0' as usize],
        })
        .collect()
}

/// What makes a unit a *level*: `level = factor * log10(ratio / reference)`, so `dB` is
/// `Log, 10, 1.0` and `pH` is `Log, -1, 1.0`. A level is dimensionless, a scale over plain
/// ratios: `20 dB` is the ratio 100.
#[derive(Clone, Copy, Debug)]
pub struct LogScale {
    pub factor: f64,
    pub reference: f64,
}

impl PartialEq for LogScale {
    fn eq(&self, other: &Self) -> bool {
        self.factor.to_bits() == other.factor.to_bits() && self.reference.to_bits() == other.reference.to_bits()
    }
}

impl Eq for LogScale {}

impl Hash for LogScale {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.factor.to_bits().hash(state);
        self.reference.to_bits().hash(state);
    }
}

impl LogScale {
    /// The ratio a level stands for. The same two operations as Python's
    /// `LogarithmicConverter.to_base`, in the same order, so both sides round alike.
    pub fn ratio_of(&self, level: f64) -> f64 {
        self.reference * 10f64.powf(level / self.factor)
    }

    /// The level of a positive ratio (`LogarithmicConverter.from_base`).
    pub fn level_of(&self, ratio: f64) -> f64 {
        self.factor * (ratio / self.reference).log10()
    }
}

/// A unit representation using rational exponents to avoid floating-point errors.
#[derive(Clone, Debug)]
pub struct RationalUnit {
    /// Vector of base unit names to their exponents as (numerator, denominator), maintained sorted by unit name.
    pub dimensions: DimVec,
    /// Multiplicative factor converting one of this unit to the canonical base-SI magnitude
    /// for the same `dimensions` (e.g. "m" => 1.0, "km" => 1000.0, "cm" => 0.01).
    pub scale: f64,
    /// The same factor as an exact rational, when it is known. Invariant: if this is `Some(e)`,
    /// `scale` is `e` rounded once to the nearest double. A conversion divides the exact
    /// scales and rounds the quotient once; only a unit whose scale came from a bare `f64`
    /// (or from a fractional power) has `None` and falls back to dividing the doubles.
    pub exact: Option<ExactScale>,
    /// Additive term applied *after* `scale` when converting to the canonical base-SI
    /// magnitude, i.e. `base = value * scale + offset`. Non-zero only for absolute affine
    /// units — in practice the temperature scales ("degC" => 273.15, "degF" => 255.372…).
    /// A zero offset means the unit is purely multiplicative, which is every other unit.
    pub offset: f64,
    /// The zero point as an exact rational, when it is known and not zero. Invariant: if this is
    /// `Some(e)`, `offset` is `e` rounded once. `None` with a non-zero `offset` is an offset that
    /// arrived as a bare `f64`; a zero `offset` is exactly zero whatever this holds.
    pub exact_offset: Option<ExactScale>,
    /// Set for a level (`dB`, `pH`), which converts through the logarithm of a ratio instead
    /// of through a scale. Its `scale` is 1 and it has no dimensions.
    pub log: Option<LogScale>,
    pub id: u64,
    pub display_name: Option<String>,
}

impl Eq for RationalUnit {}

impl PartialEq for RationalUnit {
    // The offset takes part in equality: "degC" and "K" share dimensions *and* scale, so
    // without it the two would compare equal and a degC->K conversion would look like a no-op.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.scale.to_bits() == other.scale.to_bits()
            && self.offset.to_bits() == other.offset.to_bits()
            && self.log == other.log
    }
}

impl Hash for RationalUnit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.scale.to_bits().hash(state);
        self.offset.to_bits().hash(state);
        self.log.hash(state);
    }
}

impl RationalUnit {
    /// Parse a rational exponent from a plain Rust (i64, i64) tuple or i64.
    pub fn parse_exponent_tuple(n: i64, den: i64) -> Option<(i64, i64)> {
        if n != 0 { Some((n, den)) } else { None }
    }

    pub fn calculate_id(dimensions: &[(String, (i64, i64))]) -> u64 {
        let mut h: u64 = 0;
        for (k, v) in dimensions {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut hasher);
            v.hash(&mut hasher);
            h ^= hasher.finish();
        }
        h
    }

    pub fn new_from_dimensions<I>(dims: I) -> Self
    where
        I: IntoIterator<Item = (String, (i64, i64))>,
    {
        let mut dimensions: DimVec = dims.into_iter().filter(|(_, (n, _))| *n != 0).collect();
        dimensions.sort_by(|a, b| a.0.cmp(&b.0));
        let id = Self::calculate_id(&dimensions);
        RationalUnit { dimensions, scale: 1.0, exact: Some(ExactScale::one()), offset: 0.0, exact_offset: None, log: None, id, display_name: None }
    }

    /// Returns a copy of this unit with a different scale factor (same dimensions/id). The
    /// factor is a bare `f64`, so the exact scale is no longer known.
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self.exact = None;
        self
    }

    /// The same dimensions measured in the coherent base units: scale 1, exactly.
    pub fn with_unit_scale(mut self) -> Self {
        self.scale = 1.0;
        self.exact = Some(ExactScale::one());
        self
    }

    /// Returns a copy of this unit with an exact scale factor; `scale` is its single rounding.
    pub fn with_exact_scale(mut self, exact: ExactScale) -> Self {
        self.scale = exact.to_f64();
        self.exact = Some(exact);
        self
    }

    /// The factor that turns a magnitude in this unit into one in `target`, which must have the
    /// same dimensions. Both exact scales are divided and the quotient rounded once; if either
    /// is unknown the doubles are divided instead.
    pub fn ratio_to(&self, target: &Self) -> f64 {
        match (&self.exact, &target.exact) {
            (Some(from), Some(to)) => from.div(to).to_f64(),
            _ => self.scale / target.scale,
        }
    }

    /// Returns a copy of this unit with an additive offset, making it an absolute affine
    /// unit (`base = value * scale + offset`), e.g. `K.with_offset(273.15)` is degC.
    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self.exact_offset = None;
        self
    }

    /// Returns a copy of this unit with an exact additive zero point; `offset` is its single rounding.
    pub fn with_exact_offset(mut self, exact: ExactScale) -> Self {
        self.offset = exact.to_f64();
        self.exact_offset = Some(exact);
        self
    }

    /// The coefficients of the conversion to `target`, which must have the same dimensions:
    /// `new = old * scale + shift`. Each is the exact value rounded once when the scales and the
    /// zero points are known exactly, so 212 degF is exactly 100 degC; otherwise it is computed
    /// from the doubles. A conversion without zero points has a zero shift.
    pub fn affine_to(&self, target: &Self) -> (f64, f64) {
        let scale = self.ratio_to(target);
        let shift = match (self.exact_zero_point(), target.exact_zero_point(), &target.exact) {
            (Some(from), Some(to), Some(divisor)) => ExactScale::shifted_quotient(from, to, divisor),
            _ => (self.offset - target.offset) / target.scale,
        };
        (scale, shift)
    }

    /// The zero point as an exact value: `Some(None)` when it is zero, `Some(Some(e))` when it is
    /// known, `None` when it is not.
    fn exact_zero_point(&self) -> Option<Option<&ExactScale>> {
        if self.offset == 0.0 {
            Some(None)
        } else {
            self.exact_offset.as_ref().map(Some)
        }
    }

    /// Returns a copy of this unit as a level: dimensionless, scale 1, and the given logarithm.
    pub fn with_log(mut self, log: LogScale) -> Self {
        self.log = Some(log);
        self
    }

    /// True for a level (`dB`, `pH`).
    pub fn is_logarithmic(&self) -> bool {
        self.log.is_some()
    }

    /// A bare number: no dimensions, scale 1, not a level. Multiplying by it changes nothing.
    pub fn is_plain_number(&self) -> bool {
        self.log.is_none() && self.dimensions.is_empty() && (self.scale - 1.0).abs() < 1e-9
    }

    /// A level stands alone: `dB/km` or `dB^2` would keep the number and lose the logarithm
    /// it is the level of. `result` is what `operands` combined into; if a level went in and
    /// no level came out, say so instead of returning a plain ratio.
    pub fn refuse_logarithmic_compound(operands: &[&Self], result: &Self) -> PhysureResult<()> {
        match operands.iter().find(|unit| unit.is_logarithmic()) {
            Some(level) if !result.is_logarithmic() => Err(PhysureError::Generic(format!(
                "the logarithmic unit '{}' cannot be part of a product, quotient or power of units; convert it to a ratio first",
                level.__repr__()
            ))),
            _ => Ok(()),
        }
    }

    /// True for absolute affine units — the temperature scales. Their arithmetic is not
    /// closed (5 degC * 2 is not 10 degC), so operations normalise them via `to_delta` first.
    pub fn is_affine(&self) -> bool {
        self.offset != 0.0
    }

    /// The purely multiplicative unit measuring *intervals* on this scale: degC => K,
    /// degF => 5/9 K. Dropping the offset is what makes a temperature *difference*
    /// meaningful, so this is the form every arithmetic operation works in.
    pub fn to_delta(&self) -> Self {
        let mut u = self.clone();
        u.offset = 0.0;
        u.exact_offset = None;
        u
    }

    pub fn dimensions_map(&self) -> HashMap<String, (i64, i64)> {
        self.dimensions.iter().cloned().collect()
    }

    pub fn dimensionless() -> Self {
        RationalUnit {
            dimensions: DimVec::new(),
            scale: 1.0,
            exact: Some(ExactScale::one()),
            offset: 0.0,
            exact_offset: None,
            log: None,
            id: 0,
            display_name: None,
        }
    }

    pub fn base(name: &str) -> Self {
        let mut u = Self::new_from_dimensions([(name.to_string(), (1, 1))]);
        u.display_name = Some(name.to_string());
        u
    }

    /// True if `other` has the same physical dimensions (ignoring scale) as `self`.
    pub fn same_dimensions(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions
    }

    pub fn get_exponent(&self, base: &str) -> Option<(i64, i64)> {
        self.dimensions
            .binary_search_by(|(k, _)| k.as_str().cmp(base))
            .ok()
            .map(|idx| self.dimensions[idx].1)
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut new_dims = DimVec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.dimensions.len() && j < other.dimensions.len() {
            let (k1, v1) = &self.dimensions[i];
            let (k2, v2) = &other.dimensions[j];
            match k1.cmp(k2) {
                std::cmp::Ordering::Less => {
                    new_dims.push((k1.clone(), *v1));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    new_dims.push((k2.clone(), *v2));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let r1 = Rational64::new(v1.0, v1.1);
                    let r2 = Rational64::new(v2.0, v2.1);
                    let res = r1 + r2;
                    if *res.numer() != 0 {
                        new_dims.push((k1.clone(), (*res.numer(), *res.denom())));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.dimensions.len() {
            new_dims.push(self.dimensions[i].clone());
            i += 1;
        }
        while j < other.dimensions.len() {
            new_dims.push(other.dimensions[j].clone());
            j += 1;
        }
        let id = Self::calculate_id(&new_dims);
        let display_name = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            self.display_name.clone()
        } else if self.dimensions.is_empty() && (self.scale - 1.0).abs() < 1e-9 {
            other.display_name.clone()
        } else {
            None
        };
        let exact = match (&self.exact, &other.exact) {
            (Some(a), Some(b)) => Some(a.mul(b)),
            _ => None,
        };
        let scale = exact.as_ref().map_or(self.scale * other.scale, ExactScale::to_f64);
        // No offset on a product: `degC * m` has no zero point to speak of. Callers that hold
        // an affine operand must `to_delta` it (and shift its magnitude) before multiplying.
        // A level times a plain number is still that level (`10 dB * 2`); with anything else it
        // is no level at all, which `refuse_logarithmic_compound` reports to whoever can fail.
        let log = if other.is_plain_number() {
            self.log
        } else if self.is_plain_number() {
            other.log
        } else {
            None
        };
        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, exact_offset: None, log, id, display_name }
    }

    pub fn div(&self, other: &Self) -> Self {
        let mut new_dims = DimVec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.dimensions.len() && j < other.dimensions.len() {
            let (k1, v1) = &self.dimensions[i];
            let (k2, v2) = &other.dimensions[j];
            match k1.cmp(k2) {
                std::cmp::Ordering::Less => {
                    new_dims.push((k1.clone(), *v1));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    new_dims.push((k2.clone(), (-v2.0, v2.1)));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let r1 = Rational64::new(v1.0, v1.1);
                    let r2 = Rational64::new(v2.0, v2.1);
                    let res = r1 - r2;
                    if *res.numer() != 0 {
                        new_dims.push((k1.clone(), (*res.numer(), *res.denom())));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.dimensions.len() {
            new_dims.push(self.dimensions[i].clone());
            i += 1;
        }
        while j < other.dimensions.len() {
            let (k2, v2) = &other.dimensions[j];
            new_dims.push((k2.clone(), (-v2.0, v2.1)));
            j += 1;
        }
        let id = Self::calculate_id(&new_dims);
        let display_name = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            self.display_name.clone()
        } else {
            None
        };
        let exact = match (&self.exact, &other.exact) {
            (Some(a), Some(b)) => Some(a.div(b)),
            _ => None,
        };
        let scale = exact.as_ref().map_or(self.scale / other.scale, ExactScale::to_f64);
        let log = if other.is_plain_number() { self.log } else { None };
        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, exact_offset: None, log, id, display_name }
    }

    pub fn pow(&self, exp_r: Rational64) -> Self {
        let mut new_dims = DimVec::new();
        for (base, (num, den)) in &self.dimensions {
            let base_r = Rational64::new(*num, *den);
            let res = base_r * exp_r;
            if *res.numer() != 0 {
                new_dims.push((base.clone(), (*res.numer(), *res.denom())));
            }
        }
        let id = Self::calculate_id(&new_dims);
        let exp_f = *exp_r.numer() as f64 / *exp_r.denom() as f64;
        // A whole power of an exact scale stays exact. A fractional one does not (Python falls
        // back to a float there too), except that 1 to any power is 1.
        let exact = match &self.exact {
            Some(e) if e.is_one() => Some(ExactScale::one()),
            Some(e) if *exp_r.denom() == 1 => e.powi(*exp_r.numer()),
            _ => None,
        };
        let scale = exact.as_ref().map_or(self.scale.powf(exp_f), ExactScale::to_f64);
        let log = if exp_r == Rational64::from_integer(1) { self.log } else { None };
        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, exact_offset: None, log, id, display_name: None }
    }

    /// Maps a dimension signature to its named SI derived unit, where one exists.
    ///
    /// Only units whose dimensions uniquely identify them are listed: SI defines
    /// several derived units that are dimensionally identical to another (e.g.
    /// becquerel and hertz are both `s^-1`; gray and sievert are both `m^2*s^-2`),
    /// and pure dimensional analysis can't disambiguate those — they're
    /// intentionally omitted rather than guessed.
    pub fn known_derived_symbol(&self) -> Option<&'static str> {
        let dims: Vec<(&str, i64, i64)> = self.dimensions.iter().map(|(k, (n, d))| (k.as_str(), *n, *d)).collect();
        match dims.as_slice() {
            [("kg", 1, 1), ("m", 1, 1), ("s", -2, 1)] => Some("N"),
            [("kg", 1, 1), ("m", 2, 1), ("s", -2, 1)] => Some("J"),
            [("kg", 1, 1), ("m", 2, 1), ("s", -3, 1)] => Some("W"),
            [("kg", 1, 1), ("m", -1, 1), ("s", -2, 1)] => Some("Pa"),
            [("A", 1, 1), ("s", 1, 1)] => Some("C"),
            [("s", -1, 1)] => Some("Hz"),
            [("A", -1, 1), ("kg", 1, 1), ("m", 1, 1), ("s", -3, 1)] => Some("N/C"),
            [("A", -1, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -3, 1)] => Some("V"),
            [("A", -2, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -3, 1)] => Some("Ω"),
            [("A", 2, 1), ("kg", -1, 1), ("m", -2, 1), ("s", 4, 1)] => Some("F"),
            [("A", 2, 1), ("kg", -1, 1), ("m", -2, 1), ("s", 3, 1)] => Some("S"),
            [("A", -1, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -2, 1)] => Some("Wb"),
            [("A", -1, 1), ("kg", 1, 1), ("s", -2, 1)] => Some("T"),
            [("A", -2, 1), ("kg", 1, 1), ("m", 2, 1), ("s", -2, 1)] => Some("H"),
            [("cd", 1, 1), ("m", -2, 1)] => Some("lx"),
            [("mol", 1, 1), ("s", -1, 1)] => Some("kat"),
            _ => None,
        }
    }

    pub fn base_repr(&self) -> String {
        if self.dimensions.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        for (base, (num, den)) in &self.dimensions {
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

    fn si_prefix_for_scale(scale: f64) -> Option<&'static str> {
        if (scale - 1.0).abs() < 1e-9 {
            Some("")
        } else if (scale - 1e3).abs() < 1e-6 {
            Some("k")
        } else if (scale - 1e6).abs() < 1e-3 {
            Some("M")
        } else if (scale - 1e9).abs() < 1.0 {
            Some("G")
        } else if (scale - 1e12).abs() < 1e3 {
            Some("T")
        } else if (scale - 1e-3).abs() < 1e-9 {
            Some("m")
        } else if (scale - 1e-6).abs() < 1e-12 {
            Some("µ")
        } else if (scale - 1e-9).abs() < 1e-15 {
            Some("n")
        } else {
            None
        }
    }

    /// The unit as a product of its symbols, the way a `CompoundUnit` prints: the symbols in
    /// alphabetical order, positive exponents above the bar and negative ones below it, `kg·m/s²`,
    /// `kg/(m·s²)`, `1/s`. It reads the dimensions as symbols, so it is meant for a unit parsed
    /// with `Parser::parse_expression_atomic`.
    pub fn symbolic_repr(&self) -> String {
        let (mut numerator, mut denominator) = (Vec::new(), Vec::new());
        for (symbol, (n, d)) in &self.dimensions {
            let exponent = *n as f64 / *d as f64;
            let shown = exponent.abs();
            let term = if shown == 1.0 { symbol.clone() } else { format!("{symbol}{}", superscript(shown)) };
            if exponent > 0.0 { numerator.push(term) } else { denominator.push(term) }
        }
        let (n, d) = (numerator.join("·"), denominator.join("·"));
        let d = if d.contains('·') { format!("({d})") } else { d };
        match (n.is_empty(), d.is_empty()) {
            (false, false) => format!("{n}/{d}"),
            (true, false) => format!("1/{d}"),
            (false, true) => n,
            (true, true) => "1".to_string(),
        }
    }

    pub fn __repr__(&self) -> String {
        if let Some(ref name) = self.display_name {
            return name.clone();
        }
        if let Some(known) = self.known_derived_symbol() {
            if let Some(prefix) = Self::si_prefix_for_scale(self.scale) {
                return format!("{}{}", prefix, known);
            }
        }
        self.base_repr()
    }

    pub fn __eq__(&self, other: &RationalUnit) -> bool {
        self.id == other.id
    }

    pub fn __hash__(&self) -> u64 {
        self.id
    }

    pub fn to_string(&self, _system: Option<()>, _use_alias: bool, _alias_preference: Option<()>) -> String {
        self.__repr__()
    }
}
