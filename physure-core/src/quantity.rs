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

    pub fn new_scalar(mean: f64, std_dev: f64, unit: RationalUnit, mode: Option<&str>, samples: Option<usize>) -> Self {
        let value = match mode {
            Some("monte_carlo") => UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
            Some("unscented")   => UncertaintyValue::Unscented(UnscentedBackend::new_scalar(mean, std_dev)),
            _                   => UncertaintyValue::Gaussian(GaussianBackend { mean, std_dev }),
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
        value.propagate_mul(&UncertaintyValue::Gaussian(GaussianBackend { mean: factor, std_dev: 0.0 }))
    }

    pub fn add(&self, other: &Quantity) -> PhysureResult<Quantity> {
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
        let new_value = self.value.propagate_mul(&other.value)?;
        let new_unit = self.unit.mul(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn div(&self, other: &Quantity) -> PhysureResult<Quantity> {
        let new_value = self.value.propagate_div(&other.value)?;
        let new_unit = self.unit.div(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn pow(&self, exponent: f64) -> PhysureResult<Quantity> {
        let exp_r = Rational64::from_f64(exponent).unwrap_or(Rational64::new(0, 1));
        let new_value = self.value.propagate_pow(exponent)?;
        let new_unit = self.unit.pow(exp_r);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn sqrt(&self) -> PhysureResult<Quantity> {
        self.pow(0.5)
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

    /// This quantity's magnitude expressed in canonical base-SI terms (mean * unit.scale).
    pub fn canonical_magnitude(&self) -> f64 {
        self.value.mean() * self.unit.scale
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
        let ratio = self.unit.scale / target.scale;
        let new_value = Self::scale_value(&self.value, ratio)?;
        Ok(Quantity { value: new_value, unit: target.clone() })
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
}
