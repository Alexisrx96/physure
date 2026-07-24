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
        let s = format!("{}", n);
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            format!("{}.0", s)
        } else {
            s
        }
    }
}

impl std::fmt::Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unit_str = self.unit.__repr__();
        let val_str = format_float(self.value.mean());
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

    pub fn to(&self, target_unit: &str) -> PhysureResult<Quantity> {
        let dummy = Quantity::new(1.0, target_unit)?;
        self.convert_to(&dummy.unit)
    }
}


