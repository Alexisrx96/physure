use num_rational::Rational64;
use num_traits::FromPrimitive;

use crate::error::{PhysureError, PhysureResult};
use crate::units::RationalUnit;
use crate::uncertainty::{
    UncertaintyBackend, UncertaintyValue, GaussianBackend, MonteCarloBackend, UnscentedBackend,
};

#[derive(Clone)]
pub struct Quantity {
    pub value: UncertaintyValue,
    pub unit: RationalUnit,
}

impl std::fmt::Debug for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Quantity")
            .field("mean", &self.value.mean())
            // A debug dump that hides the uncertainty turns every test failure and log line
            // about an uncertain quantity into a report about a different measurement.
            .field("std_dev", &self.value.std_dev())
            .field("unit", &self.unit)
            .finish()
    }
}

impl PartialEq for Quantity {
    fn eq(&self, other: &Self) -> bool {
        self.unit == other.unit && (self.value.mean() - other.value.mean()).abs() < 1e-9
    }
}

/// `n` written as a fraction, when one applies: `1.5` is `3/2`, or `1 1/2` with `mixed`.
///
/// `None` when no small fraction says the same thing — π and the irrationals have no
/// fraction to give, and the decimal is the honest rendering. The caller falls back to it.
pub fn format_fraction(n: f64, mixed: bool) -> Option<String> {
    // ponytail: the denominator ceiling. Raise it if ten-thousandths start reading as
    // decimals; every value above it is one nobody was going to read as a fraction anyway.
    const MAX_DENOM: i64 = 10_000;
    if !n.is_finite() {
        return None;
    }
    // Rounding debris is not a fraction. `0.1 + 0.2` is 0.30000000000000004, whose exact
    // ratio is 1125899906842624/3752999689475413, and `25 m/s => km/h` lands on
    // 89.99999999999999 rather than 90. An f64 carries 15 decimal digits and the debris
    // always falls past them, so cut there first — the same cut `format_float` makes.
    let clean: f64 = format!("{:.14e}", n).parse().unwrap_or(n);
    let ratio = Rational64::approximate_float(clean)?;
    let (numer, denom) = (*ratio.numer(), *ratio.denom());
    if denom > MAX_DENOM {
        return None;
    }
    // A magnitude too small to reach the ceiling approximates to 0/1, and 0 is a different
    // number: 1e-30 kg is not no mass at all.
    if numer == 0 && n != 0.0 {
        return None;
    }
    if denom == 1 {
        return Some(numer.to_string());
    }
    let whole = numer / denom;
    if !mixed || whole == 0 {
        return Some(format!("{}/{}", numer, denom));
    }
    Some(format!("{} {}/{}", whole, (numer % denom).abs(), denom))
}

pub fn format_float(n: f64) -> String {
    if n == 0.0 {
        return "0.0".to_string();
    }
    let abs_n = n.abs();
    if abs_n < 1e-4 || abs_n >= 1e16 {
        format!("{:e}", n)
    } else {
        // `{}` prints the shortest string that round-trips the f64, which is honest but
        // unreadable once a conversion has left rounding debris: 25 m/s => km/h comes out
        // as 89.99999999999999. An f64 carries 15 decimal digits exactly and the debris
        // always lands past them, so round to 15 significant digits and keep the result
        // only when it is genuinely shorter.
        let s = {
            let exact = format!("{}", n);
            let rounded = format!("{:.*}", (15 - (abs_n.log10().floor() as i32 + 1)).clamp(0, 17) as usize, n);
            let trimmed = if rounded.contains('.') {
                rounded.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                rounded
            };
            if trimmed.len() < exact.len() && trimmed.parse::<f64>().is_ok() {
                trimmed
            } else {
                exact
            }
        };
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            format!("{}.0", s)
        } else {
            s
        }
    }
}

impl std::fmt::Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Multiplying/dividing quantities can produce an anonymous (unnamed) compound
        // unit whose scale isn't 1.0 (e.g. Ohm * milliamp). There's no literal symbol
        // for such a unit, so fold the scale into the value and display in canonical
        // (scale-1) terms instead of printing the raw, unscaled magnitude.
        let (val, std_dev, unit_str) = if self.unit.display_name.is_none() && (self.unit.scale - 1.0).abs() > 1e-9 {
            let mut canonical_unit = self.unit.clone();
            canonical_unit.scale = 1.0;
            (self.canonical_magnitude(), self.value.std_dev() * self.unit.scale, canonical_unit.__repr__())
        } else {
            (self.value.mean(), self.value.std_dev(), self.unit.__repr__())
        };
        // An uncertainty the user declared has to survive to the output: printing
        // "10.0 m" for `10.0 +/- 0.5 m` throws away the half of the measurement that
        // says how much to trust the other half.
        let val_str = if std_dev > 0.0 {
            format!("{} ± {}", format_float(val), format_float(std_dev))
        } else {
            format_float(val)
        };
        if unit_str.is_empty() || unit_str == "Dimensionless" {
            write!(f, "{}", val_str)
        } else {
            write!(f, "{} {}", val_str, unit_str)
        }
    }
}

impl Quantity {
    /// Creates a simple Quantity from a magnitude and a unit expression string (e.g. Quantity::new(10.0, "m/s")).
    pub fn new(mean: f64, unit_expr: &str) -> PhysureResult<Self> {
        let clean_unit = unit_expr.trim().replace(" / ", "/").replace(" * ", "*");
        if clean_unit.is_empty() {
            return Ok(Self::new_scalar(mean, 0.0, crate::units::RationalUnit::dimensionless(), None, None));
        }
        let unit = crate::units::parser::Parser::parse_expression(&clean_unit)?;
        Ok(Self::new_scalar(mean, 0.0, unit, None, None))
    }

    /// A measured scalar.
    ///
    /// `mode` names the backend explicitly; `None` defers to `[Settings] propagation_mode`
    /// in `physure.conf`, which is how PHS gets to honour the file without every call site
    /// having to read it. An exact value stays Gaussian whatever the setting says: there is
    /// no distribution to sample or to place sigma points on, and drawing a thousand
    /// identical samples for every plain number in a script is a cost with nothing behind it.
    pub fn new_scalar(mean: f64, std_dev: f64, unit: RationalUnit, mode: Option<&str>, samples: Option<usize>) -> Self {
        let resolved = match mode {
            Some(named) => named,
            None if std_dev == 0.0 => "gaussian",
            None => crate::uncertainty::propagation_mode().name(),
        };
        let value = match resolved {
            "monte_carlo" => UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
            "unscented"   => UncertaintyValue::Unscented(UnscentedBackend::new_scalar(mean, std_dev)),
            _             => UncertaintyValue::Gaussian(GaussianBackend::new(mean, std_dev)),
        };
        Quantity { value, unit }
    }

    pub fn from_backend(backend: Box<dyn UncertaintyBackend>, unit: RationalUnit) -> Self {
        Quantity {
            value: UncertaintyValue::Custom(backend),
            unit,
        }
    }

    pub fn from_value(value: UncertaintyValue, unit: RationalUnit) -> Self {
        Quantity { value, unit }
    }

    /// Rescales an uncertainty value by a plain multiplicative constant (mean and std_dev alike),
    /// reusing the existing propagate_mul machinery with a zero-uncertainty scalar.
    fn scale_value(value: &UncertaintyValue, factor: f64) -> PhysureResult<UncertaintyValue> {
        // Multiplying by one is not free: the unscented backend rebuilds its sigma points
        // from (mean, std_dev) and the Monte Carlo backend allocates a second array, so an
        // identity rescale would quietly resample a value that nobody asked to change.
        if factor == 1.0 {
            return Ok(value.clone());
        }
        // `exact`, not `new`: a conversion factor is not a measurement, so it must not mint a
        // source id. Minting one would leave a term that never cancels, and `x - x.to("cm")`
        // would stop coming out at zero.
        value.propagate_mul(&UncertaintyValue::Gaussian(GaussianBackend::exact(factor)))
    }

    /// Shifts an uncertainty value by an exact additive constant. A zero point carries no
    /// uncertainty of its own, so `exact` is used for the same reason as in `scale_value`:
    /// minting a source id here would leave a term that never cancels.
    fn shift_value(value: &UncertaintyValue, offset: f64) -> PhysureResult<UncertaintyValue> {
        if offset == 0.0 {
            return Ok(value.clone());
        }
        value.propagate_add(&UncertaintyValue::Gaussian(GaussianBackend::exact(offset)))
    }

    pub fn add(&self, other: &Quantity) -> PhysureResult<Quantity> {
        // An absolute temperature has no additive algebra of its own — `20 degC + 5 degC` is
        // only meaningful once both sides are on an interval scale. Normalising to K first
        // keeps every operation below purely multiplicative, as the rest of this file assumes.
        if self.unit.is_affine() || other.unit.is_affine() {
            return self.to_delta()?.add(&other.to_delta()?);
        }
        if !self.unit.same_dimensions(&other.unit) {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: other.unit.__repr__(),
            });
        }
        let other_value = if self.unit.scale != other.unit.scale {
            Self::scale_value(&other.value, other.unit.scale / self.unit.scale)?
        } else {
            other.value.clone()
        };
        let new_value = self.value.propagate_add(&other_value)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn sub(&self, other: &Quantity) -> PhysureResult<Quantity> {
        // `T2 - T1` across Celsius/Fahrenheit is the calorimetry case: both sides go to K, so
        // the difference comes out as a true interval instead of subtracting two zero points.
        if self.unit.is_affine() || other.unit.is_affine() {
            return self.to_delta()?.sub(&other.to_delta()?);
        }
        if !self.unit.same_dimensions(&other.unit) {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: other.unit.__repr__(),
            });
        }
        let other_value = if self.unit.scale != other.unit.scale {
            Self::scale_value(&other.value, other.unit.scale / self.unit.scale)?
        } else {
            other.value.clone()
        };
        let new_value = self.value.propagate_sub(&other_value)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn mul(&self, other: &Quantity) -> PhysureResult<Quantity> {
        if self.unit.is_affine() || other.unit.is_affine() {
            return self.to_delta()?.mul(&other.to_delta()?);
        }
        let new_value = self.value.propagate_mul(&other.value)?;
        let new_unit = self.unit.mul(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn div(&self, other: &Quantity) -> PhysureResult<Quantity> {
        if self.unit.is_affine() || other.unit.is_affine() {
            return self.to_delta()?.div(&other.to_delta()?);
        }
        let new_value = self.value.propagate_div(&other.value)?;
        let new_unit = self.unit.div(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn pow(&self, exponent: f64) -> PhysureResult<Quantity> {
        if self.unit.is_affine() {
            return self.to_delta()?.pow(exponent);
        }
        let m = self.value.mean();
        // `powf` computes a negative base via `exp(exponent * ln(base))`, which is only
        // defined for a positive base -- so a negative base with a fractional exponent
        // (`(-4)^0.5`) comes back NaN even where a real answer exists (e.g. the real cube
        // root of -8), and quietly handing that NaN back is the confident-wrong-answer this
        // library exists to prevent. An integer exponent stays exact (`(-2)^3 = -8`).
        if m < 0.0 && exponent.fract() != 0.0 {
            return Err(PhysureError::DomainError(format!(
                "{m}^{exponent} cannot be computed for a negative base with a non-integer exponent"
            )));
        }
        let exp_r = Rational64::from_f64(exponent).unwrap_or(Rational64::new(0, 1));
        let new_value = self.value.propagate_pow(exponent)?;
        let new_unit = self.unit.pow(exp_r);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    /// Relabels this quantity with a different unit, keeping the magnitude and the full
    /// uncertainty backend (lineage, Monte Carlo draws, ...) exactly as they are.
    ///
    /// This is not a conversion -- `5` relabeled to `m` is `5 m`, not `5` rescaled by some
    /// factor into `m`. It exists for the one place that legitimately needs to *assign* a
    /// unit to a bare number rather than convert an already-dimensioned one: a PHS function
    /// parameter declared `(x: m)` under `@implicit_units`, called with a plain `5`.
    pub fn with_unit(&self, unit: RationalUnit) -> Quantity {
        Quantity { value: self.value.clone(), unit }
    }

    pub fn sqrt(&self) -> PhysureResult<Quantity> {
        let m = self.value.mean();
        if m < 0.0 {
            return Err(PhysureError::DomainError(format!(
                "sqrt of a negative value ({m}) is undefined for real numbers"
            )));
        }
        self.pow(0.5)
    }

    /// Folding the magnitude to its absolute value moves where the measurement sits, not how
    /// well it is known, so the unit and the standard deviation both carry over untouched.
    pub fn abs(&self) -> PhysureResult<Quantity> {
        let new_value = self.value.propagate_function("abs")?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    /// Moves the mean to the integer below it without touching anything else.
    ///
    /// Flooring is a statement about where the measurement sits, not about how well it is
    /// known, so the unit and the standard deviation both survive.
    pub fn floor(&self) -> PhysureResult<Quantity> {
        self.shift_mean_to(self.value.mean().floor())
    }

    /// The `floor` counterpart: the integer above the mean, same unit, same uncertainty.
    pub fn ceil(&self) -> PhysureResult<Quantity> {
        self.shift_mean_to(self.value.mean().ceil())
    }

    /// Slides the whole distribution so its mean lands on `target`.
    ///
    /// The shift is applied as an addition of an exact constant rather than by rebuilding
    /// the quantity, because that is the only form that keeps a Monte Carlo cloud or a set
    /// of sigma points intact. Rounding each sample instead would be worse than rebuilding:
    /// every sample of `9.81 ± 0.05` floors to 9, so the result would come back with a
    /// standard deviation of zero — an uncertain measurement printed as if it were exact.
    fn shift_mean_to(&self, target: f64) -> PhysureResult<Quantity> {
        let offset = UncertaintyValue::Gaussian(GaussianBackend::exact(target - self.value.mean()));
        let new_value = self.value.propagate_add(&offset)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn sin(&self) -> PhysureResult<Quantity> {
        self.trig("sin")
    }

    pub fn cos(&self) -> PhysureResult<Quantity> {
        self.trig("cos")
    }

    pub fn tan(&self) -> PhysureResult<Quantity> {
        self.trig("tan")
    }

    /// A trigonometric function takes an angle and returns a pure ratio.
    ///
    /// Two arguments are admissible: an angle, which is converted to radians by its own
    /// scale (that scale is defined against the radian, so it is exactly the degrees →
    /// radians factor for `deg`), and a dimensionless value, which is read as radians —
    /// the same reading a bare number gets. A length or a mass is a dimensional error:
    /// `sin(9.81 m/s^2)` has no meaning, and quietly answering -0.379 for it is the
    /// confident wrong answer this library exists to prevent.
    ///
    /// The result is dimensionless, and the uncertainty rides through the core's derivative
    /// propagation (σ_sin = |cos x|·σ_x), which is also what keeps a Monte Carlo or
    /// unscented value from being flattened into a Gaussian on the way out.
    fn trig(&self, func: &str) -> PhysureResult<Quantity> {
        let radians = self.as_radians(func)?;
        let new_value = radians.propagate_function(func)?;
        Ok(Quantity { value: new_value, unit: RationalUnit::dimensionless() })
    }

    /// This quantity's value in radians, or an error if it is not an angle at all.
    fn as_radians(&self, func: &str) -> PhysureResult<UncertaintyValue> {
        if !self.unit.dimensions.is_empty() && !self.unit.same_dimensions(&RationalUnit::base("rad")) {
            return Err(PhysureError::Generic(format!(
                "{func} expects an angle or a dimensionless value, got '{}'",
                self.unit.__repr__()
            )));
        }
        // Folding in the scale is what turns `90 deg` into π/2 and `50 %` into 0.5; for a
        // plain radian or a bare dimensionless value the factor is 1.0 and nothing moves.
        Self::scale_value(&self.value, self.unit.scale)
    }

    pub fn exp(&self) -> PhysureResult<Quantity> {
        self.transcendental("exp", "exp")
    }

    /// The natural logarithm. The core calls it `log`; PHS and the rest of the world call
    /// that one `ln`, so the name is translated here rather than at every call site.
    pub fn ln(&self) -> PhysureResult<Quantity> {
        self.transcendental("ln", "log")
    }

    /// The base-10 logarithm, obtained from the natural one: log10 x = ln x / ln 10.
    /// Dividing by an exact constant is a plain rescale, so the relative uncertainty — and
    /// the backend — come through unchanged, which no separate log10 kernel would give us.
    pub fn log10(&self) -> PhysureResult<Quantity> {
        let natural = self.transcendental("log", "log")?;
        let value = Self::scale_value(&natural.value, std::f64::consts::LN_10.recip())?;
        Ok(Quantity { value, unit: natural.unit })
    }

    /// Applies a transcendental function to a dimensionless magnitude.
    ///
    /// `exp`, `ln` and `log` are power series in their argument: `1 + x + x²/2 + …` can only
    /// be summed when every term carries the same unit, which is to say when `x` carries
    /// none. `ln(5 m)` is a physics error, so it is reported as one instead of being
    /// computed from the bare number 5.
    fn transcendental(&self, name: &str, core_func: &str) -> PhysureResult<Quantity> {
        if !self.unit.dimensions.is_empty() {
            return Err(PhysureError::Generic(format!(
                "{name} expects a dimensionless value, got '{}'",
                self.unit.__repr__()
            )));
        }
        // A dimensionless unit can still carry a scale (`%`, `ppm`), and the series is in
        // the pure ratio: ln(50 %) has to be ln(0.5), not ln(50).
        let magnitude = Self::scale_value(&self.value, self.unit.scale)?;
        // `exp` is defined everywhere; `ln`/`log10` (both routed here as `core_func == "log"`)
        // are not defined at or below zero -- `ln(0)` silently answering `-inf` and `ln(-5)`
        // answering `NaN` are exactly the confident-wrong-answer this library exists to
        // prevent, so both are reported as errors instead.
        if core_func == "log" && magnitude.mean() <= 0.0 {
            return Err(PhysureError::DomainError(format!(
                "{name} of a non-positive value ({}) is undefined for real numbers",
                magnitude.mean()
            )));
        }
        let new_value = magnitude.propagate_function(core_func)?;
        Ok(Quantity { value: new_value, unit: RationalUnit::dimensionless() })
    }

    pub fn approx_eq(&self, other: &Quantity, rel_tol: f64, abs_tol: f64) -> bool {
        if !self.unit.same_dimensions(&other.unit) {
            return false;
        }
        let self_mag = self.canonical_magnitude();
        let other_mag = other.canonical_magnitude();
        let diff = (self_mag - other_mag).abs();
        let tol = abs_tol.max(rel_tol * self_mag.abs().max(other_mag.abs()));
        diff <= tol
    }

    /// Backing implementation for PHS's `assert(actual, expected)` builtin: passes when the
    /// two quantities have compatible dimensions and their magnitudes agree once converted
    /// to a common scale, within a fixed tolerance chosen to catch real bugs while
    /// tolerating floating-point drift across languages.
    pub fn phs_assert(&self, other: &Quantity) -> PhysureResult<()> {
        const REL_TOL: f64 = 1e-9;
        const ABS_TOL: f64 = 1e-12;
        if self.approx_eq(other, REL_TOL, ABS_TOL) {
            return Ok(());
        }
        if !self.unit.same_dimensions(&other.unit) {
            return Err(PhysureError::AssertionFailed {
                kind: "assert",
                message: format!("{} and {} have incompatible dimensions", self, other),
            });
        }
        Err(PhysureError::AssertionFailed {
            kind: "assert",
            message: format!("{} != {}", self, other),
        })
    }

    /// Backing implementation for PHS's `exact_assert(actual, expected)` builtin: passes
    /// only when both operands carry the literal same unit — aliases like `m`/`meter` still
    /// match, since `RationalUnit`'s `PartialEq` already ignores the display alias — and the
    /// magnitudes are bit-exact. No conversion, no tolerance.
    pub fn phs_exact_assert(&self, other: &Quantity) -> PhysureResult<()> {
        if self.unit == other.unit && self.value.mean().to_bits() == other.value.mean().to_bits() {
            return Ok(());
        }
        Err(PhysureError::AssertionFailed {
            kind: "exact_assert",
            message: format!("{} != {}", self, other),
        })
    }

    /// This quantity's magnitude expressed in canonical base-SI terms (mean * unit.scale).
    pub fn canonical_magnitude(&self) -> f64 {
        self.value.mean() * self.unit.scale
    }

    /// The measurement quoted in the base units it is built from: `2 kΩ` reads
    /// `2000 A^-2 * kg * m^2 * s^-3`. The scale rides into the magnitude, so the physical
    /// value is the same one `Display` prints — only the terms change. This is what `x: base`
    /// spells in PHS.
    pub fn base_display(&self) -> String {
        let mut base_unit = self.unit.clone();
        base_unit.scale = 1.0;
        base_unit.display_name = None;
        let std_dev = self.value.std_dev() * self.unit.scale;
        let val_str = if std_dev > 0.0 {
            format!("{} ± {}", format_float(self.canonical_magnitude()), format_float(std_dev))
        } else {
            format_float(self.canonical_magnitude())
        };
        let unit_str = base_unit.base_repr();
        if unit_str.is_empty() {
            val_str
        } else {
            format!("{} {}", val_str, unit_str)
        }
    }

    /// Converts this quantity to an equivalent one expressed in `target`'s unit/scale.
    /// Errors if `target` has different physical dimensions.
    pub fn convert_to(&self, target: &RationalUnit) -> PhysureResult<Quantity> {
        if !self.unit.same_dimensions(target) {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: target.__repr__(),
            });
        }
        // Affine scales (degC, degF) cannot be converted with a single ratio: go through the
        // canonical base magnitude so the zero points are subtracted, not scaled.
        if self.unit.is_affine() || target.is_affine() {
            let base = Self::shift_value(
                &Self::scale_value(&self.value, self.unit.scale)?,
                self.unit.offset,
            )?;
            let new_value =
                Self::scale_value(&Self::shift_value(&base, -target.offset)?, 1.0 / target.scale)?;
            return Ok(Quantity { value: new_value, unit: target.clone() });
        }
        let ratio = self.unit.scale / target.scale;
        let new_value = Self::scale_value(&self.value, ratio)?;
        Ok(Quantity { value: new_value, unit: target.clone() })
    }

    /// This quantity restated in its canonical base unit, so that arithmetic is meaningful:
    /// `20 degC` becomes `293.15 K`. A non-affine quantity is returned untouched, which is
    /// every quantity except a temperature written on the Celsius or Fahrenheit scale.
    fn to_delta(&self) -> PhysureResult<Quantity> {
        if !self.unit.is_affine() {
            return Ok(self.clone());
        }
        let mut base = self.unit.to_delta().with_scale(1.0);
        // Drop the affine name so the result renders as "K" and not as the scale it came from.
        base.display_name = None;
        self.convert_to(&base)
    }

    pub fn with_uncertainty(mean: f64, std_dev: f64, unit_expr: &str) -> PhysureResult<Self> {
        let clean_unit = unit_expr.trim().replace(" / ", "/").replace(" * ", "*");
        if clean_unit.is_empty() {
            return Ok(Self::new_scalar(mean, std_dev, crate::units::RationalUnit::dimensionless(), None, None));
        }
        let unit = crate::units::parser::Parser::parse_expression(&clean_unit)?;
        Ok(Self::new_scalar(mean, std_dev, unit, None, None))
    }

    pub fn powi(&self, exp: i32) -> PhysureResult<Quantity> {
        self.pow(exp as f64)
    }

    pub fn powf(&self, exp: f64) -> PhysureResult<Quantity> {
        self.pow(exp)
    }

    pub fn to(&self, target_unit: &str) -> PhysureResult<Quantity> {
        let dummy = Quantity::new(1.0, target_unit)?;
        self.convert_to(&dummy.unit)
    }
}

impl std::ops::Add for Quantity {
    type Output = Quantity;
    fn add(self, rhs: Quantity) -> Self::Output {
        Quantity::add(&self, &rhs).unwrap()
    }
}

impl<'a, 'b> std::ops::Add<&'b Quantity> for &'a Quantity {
    type Output = Quantity;
    fn add(self, rhs: &'b Quantity) -> Self::Output {
        Quantity::add(self, rhs).unwrap()
    }
}

impl<'a> std::ops::Add<&'a Quantity> for Quantity {
    type Output = Quantity;
    fn add(self, rhs: &'a Quantity) -> Self::Output {
        Quantity::add(&self, rhs).unwrap()
    }
}

impl<'a> std::ops::Add<Quantity> for &'a Quantity {
    type Output = Quantity;
    fn add(self, rhs: Quantity) -> Self::Output {
        Quantity::add(self, &rhs).unwrap()
    }
}

impl std::ops::Sub for Quantity {
    type Output = Quantity;
    fn sub(self, rhs: Quantity) -> Self::Output {
        Quantity::sub(&self, &rhs).unwrap()
    }
}

impl<'a, 'b> std::ops::Sub<&'b Quantity> for &'a Quantity {
    type Output = Quantity;
    fn sub(self, rhs: &'b Quantity) -> Self::Output {
        Quantity::sub(self, rhs).unwrap()
    }
}

impl<'a> std::ops::Sub<&'a Quantity> for Quantity {
    type Output = Quantity;
    fn sub(self, rhs: &'a Quantity) -> Self::Output {
        Quantity::sub(&self, rhs).unwrap()
    }
}

impl<'a> std::ops::Sub<Quantity> for &'a Quantity {
    type Output = Quantity;
    fn sub(self, rhs: Quantity) -> Self::Output {
        Quantity::sub(self, &rhs).unwrap()
    }
}

impl std::ops::Mul for Quantity {
    type Output = Quantity;
    fn mul(self, rhs: Quantity) -> Self::Output {
        Quantity::mul(&self, &rhs).unwrap()
    }
}

impl<'a, 'b> std::ops::Mul<&'b Quantity> for &'a Quantity {
    type Output = Quantity;
    fn mul(self, rhs: &'b Quantity) -> Self::Output {
        Quantity::mul(self, rhs).unwrap()
    }
}

impl<'a> std::ops::Mul<&'a Quantity> for Quantity {
    type Output = Quantity;
    fn mul(self, rhs: &'a Quantity) -> Self::Output {
        Quantity::mul(&self, rhs).unwrap()
    }
}

impl<'a> std::ops::Mul<Quantity> for &'a Quantity {
    type Output = Quantity;
    fn mul(self, rhs: Quantity) -> Self::Output {
        Quantity::mul(self, &rhs).unwrap()
    }
}

impl std::ops::Div for Quantity {
    type Output = Quantity;
    fn div(self, rhs: Quantity) -> Self::Output {
        Quantity::div(&self, &rhs).unwrap()
    }
}

impl<'a, 'b> std::ops::Div<&'b Quantity> for &'a Quantity {
    type Output = Quantity;
    fn div(self, rhs: &'b Quantity) -> Self::Output {
        Quantity::div(self, rhs).unwrap()
    }
}

impl<'a> std::ops::Div<&'a Quantity> for Quantity {
    type Output = Quantity;
    fn div(self, rhs: &'a Quantity) -> Self::Output {
        Quantity::div(&self, rhs).unwrap()
    }
}

impl<'a> std::ops::Div<Quantity> for &'a Quantity {
    type Output = Quantity;
    fn div(self, rhs: Quantity) -> Self::Output {
        Quantity::div(self, &rhs).unwrap()
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::RationalUnit;

    fn ohm_unit() -> RationalUnit {
        RationalUnit::new_from_dimensions([
            ("A".to_string(), (-2, 1)),
            ("kg".to_string(), (1, 1)),
            ("m".to_string(), (2, 1)),
            ("s".to_string(), (-3, 1)),
        ])
    }

    fn milliamp_unit() -> RationalUnit {
        RationalUnit::new_from_dimensions([("A".to_string(), (1, 1))]).with_scale(0.001)
    }

    #[test]
    fn display_folds_anonymous_compound_scale_and_aliases_to_known_symbol() {
        // 560 Ohm * 428 mA should display as "239.68 V", not "239680 A^-1 * kg * m^2 * s^-3".
        let r = Quantity::new_scalar(560.0, 0.0, ohm_unit(), None, None);
        let i = Quantity::new_scalar(428.0, 0.0, milliamp_unit(), None, None);
        let v = r.mul(&i).unwrap();
        assert!((v.canonical_magnitude() - 239.68).abs() < 1e-9);
        let s = v.to_string();
        assert!(s.ends_with(" V"), "expected canonical Volt display, got {s}");
        let printed_value: f64 = s.trim_end_matches(" V").parse().unwrap();
        assert!((printed_value - 239.68).abs() < 1e-6, "printed value {printed_value} != 239.68");
    }

    #[test]
    fn format_fraction_answers_only_when_a_fraction_applies() {
        assert_eq!(format_fraction(1.5, false).as_deref(), Some("3/2"));
        assert_eq!(format_fraction(1.5, true).as_deref(), Some("1 1/2"));
        assert_eq!(format_fraction(-1.5, true).as_deref(), Some("-1 1/2"));
        // Below 1 there is no whole part to quote, and `0 1/2` is nobody's notation.
        assert_eq!(format_fraction(0.5, true).as_deref(), Some("1/2"));
        // A whole number is a whole number, not `4/1`.
        assert_eq!(format_fraction(4.0, true).as_deref(), Some("4"));
        assert_eq!(format_fraction(0.0, false).as_deref(), Some("0"));
        // The debris cut: the exact ratio of 0.30000000000000004 is
        // 1125899906842624/3752999689475413, which is true and useless.
        assert_eq!(format_fraction(0.1 + 0.2, false).as_deref(), Some("3/10"));
        assert_eq!(format_fraction(25.0 * 3.6, false).as_deref(), Some("90"));
        // Past the ceiling, and past anything a reader would take for a fraction.
        assert_eq!(format_fraction(std::f64::consts::PI, false), None);
        // 1e-30 kg approximates to 0/1, and no mass at all is a different measurement.
        assert_eq!(format_fraction(1e-30, false), None);
        assert_eq!(format_fraction(f64::NAN, false), None);
        assert_eq!(format_fraction(f64::INFINITY, false), None);
    }

    #[test]
    fn sqrt_of_a_negative_quantity_is_a_domain_error_not_nan() {
        let q = Quantity::new(-4.0, "m^2").unwrap();
        let err = q.sqrt().unwrap_err();
        assert!(matches!(err, PhysureError::DomainError(_)), "expected DomainError, got {err:?}");
    }

    #[test]
    fn pow_of_a_negative_base_with_a_fractional_exponent_is_a_domain_error_not_nan() {
        let q = Quantity::new(-4.0, "").unwrap();
        let err = q.pow(0.5).unwrap_err();
        assert!(matches!(err, PhysureError::DomainError(_)), "expected DomainError, got {err:?}");
    }

    #[test]
    fn pow_of_a_negative_base_with_an_integer_exponent_still_works() {
        let q = Quantity::new(-2.0, "m").unwrap();
        let cubed = q.pow(3.0).unwrap();
        assert!((cubed.value.mean() - (-8.0)).abs() < 1e-9);
    }

    #[test]
    fn ln_of_a_non_positive_value_is_a_domain_error_not_nan_or_inf() {
        let zero = Quantity::new(0.0, "").unwrap();
        assert!(matches!(zero.ln().unwrap_err(), PhysureError::DomainError(_)));
        let negative = Quantity::new(-5.0, "").unwrap();
        assert!(matches!(negative.ln().unwrap_err(), PhysureError::DomainError(_)));
    }

    #[test]
    fn log10_of_a_non_positive_value_is_a_domain_error() {
        let negative = Quantity::new(-5.0, "").unwrap();
        assert!(matches!(negative.log10().unwrap_err(), PhysureError::DomainError(_)));
    }

    #[test]
    fn with_unit_relabels_without_rescaling_or_losing_uncertainty() {
        let q = Quantity::new_scalar(2.5, 0.1, RationalUnit::dimensionless(), None, None);
        let relabeled = q.with_unit(RationalUnit::base("m"));
        assert_eq!(relabeled.value.mean(), 2.5);
        assert_eq!(relabeled.value.std_dev(), 0.1);
        assert_eq!(relabeled.unit, RationalUnit::base("m"));
    }

    #[test]
    fn phs_assert_passes_for_equal_dimension_and_magnitude() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        assert!(a.phs_assert(&b).is_ok());
    }

    #[test]
    fn phs_assert_fails_on_dimension_mismatch() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.0, "s").unwrap();
        let err = a.phs_assert(&b).unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }

    #[test]
    fn phs_assert_fails_when_magnitude_differs_beyond_tolerance() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.1, "m").unwrap();
        assert!(a.phs_assert(&b).is_err());
    }

    #[test]
    fn phs_exact_assert_passes_for_alias_units_with_same_magnitude() {
        // "m" and "meter" share the same id/scale/offset in physure.conf; only the
        // display alias differs, which `exact_assert` deliberately ignores.
        let a = Quantity::new(5.0, "m").unwrap();
        let b = Quantity::new(5.0, "meter").unwrap();
        assert!(a.phs_exact_assert(&b).is_ok());
    }

    #[test]
    fn phs_exact_assert_fails_when_conversion_would_be_required() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        let err = a.phs_exact_assert(&b).unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "exact_assert", .. }));
    }

    #[test]
    fn phs_exact_assert_fails_on_bit_inexact_magnitude() {
        let a = Quantity::new(5.0, "m").unwrap();
        let b = Quantity::new(5.0 + 1e-12, "m").unwrap();
        assert!(a.phs_exact_assert(&b).is_err());
    }

    /// Affine conversion is not a single ratio: the zero point has to be added on the way to
    /// the base unit and taken off again on the way out. A degC and a Kelvin are the same
    /// size, so a scale-only conversion between them is the identity and silently wrong.
    #[test]
    fn affine_conversion_moves_the_zero_point() {
        let celsius = RationalUnit::base("K").with_offset(273.15);
        let kelvin = RationalUnit::base("K");
        let fahrenheit = RationalUnit::base("K").with_scale(5.0 / 9.0).with_offset(255.37222222222223);

        let boiling = Quantity::new_scalar(100.0, 0.0, celsius.clone(), None, None);
        assert!((boiling.convert_to(&kelvin).unwrap().value.mean() - 373.15).abs() < 1e-9);
        assert!((boiling.convert_to(&fahrenheit).unwrap().value.mean() - 212.0).abs() < 1e-9);
        // Converting a scale onto itself must not shift the value twice.
        assert!((boiling.convert_to(&celsius).unwrap().value.mean() - 100.0).abs() < 1e-9);

        // A difference is an interval: both sides normalise to K, so the zero points cancel
        // instead of being subtracted twice. 120 degF - 20 degC = 28.888... K, not -244 K.
        let hot = Quantity::new_scalar(120.0, 0.0, fahrenheit, None, None);
        let warm = Quantity::new_scalar(20.0, 0.0, celsius.clone(), None, None);
        let delta = hot.sub(&warm).unwrap();
        assert!((delta.value.mean() - 28.888888888888889).abs() < 1e-9, "got {}", delta.value.mean());
        assert!(!delta.unit.is_affine(), "a difference of temperatures is an interval");

        // Multiplying an absolute temperature normalises it first: the product of an affine
        // unit has no zero point to inherit, so the result must be plain Kelvin-dimensioned.
        let doubled = warm.mul(&Quantity::new_scalar(2.0, 0.0, RationalUnit::dimensionless(), None, None)).unwrap();
        assert!((doubled.value.mean() - 586.3).abs() < 1e-9, "got {}", doubled.value.mean());
        assert!(!doubled.unit.is_affine());
    }
}
