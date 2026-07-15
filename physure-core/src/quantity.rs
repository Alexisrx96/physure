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

impl Quantity {
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

    pub fn add(&self, other: &Quantity) -> PhysureResult<Quantity> {
        if self.unit != other.unit {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: other.unit.__repr__(),
            });
        }
        let new_value = self.value.propagate_add(&other.value)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn sub(&self, other: &Quantity) -> PhysureResult<Quantity> {
        if self.unit != other.unit {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: other.unit.__repr__(),
            });
        }
        let new_value = self.value.propagate_sub(&other.value)?;
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
}
