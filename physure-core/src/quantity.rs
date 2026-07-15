use num_rational::Rational64;
use num_traits::FromPrimitive;

use crate::units::RationalUnit;
use crate::uncertainty::{UncertaintyBackend, GaussianBackend, MonteCarloBackend, UnscentedBackend};

pub struct Quantity {
    pub value: Box<dyn UncertaintyBackend>,
    pub unit: RationalUnit,
}

impl Clone for Quantity {
    fn clone(&self) -> Self {
        Quantity {
            value: dyn_clone::clone_box(&*self.value),
            unit: self.unit.clone(),
        }
    }
}

impl Quantity {
    pub fn new_scalar(mean: f64, std_dev: f64, unit: RationalUnit, mode: Option<&str>, samples: Option<usize>) -> Self {
        let backend: Box<dyn UncertaintyBackend> = match mode {
            Some("monte_carlo") => Box::new(MonteCarloBackend::from_stats(mean, std_dev, samples.unwrap_or(1000))),
            Some("unscented")   => Box::new(UnscentedBackend::new_scalar(mean, std_dev)),
            _                   => Box::new(GaussianBackend { mean, std_dev }),
        };
        Quantity { value: backend, unit }
    }

    pub fn from_backend(value: Box<dyn UncertaintyBackend>, unit: RationalUnit) -> Self {
        Quantity { value, unit }
    }

    pub fn add(&self, other: &Quantity) -> Result<Quantity, String> {
        if self.unit != other.unit {
            return Err("Unit mismatch in addition".into());
        }
        let new_value = self.value.propagate_add(&*other.value)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn sub(&self, other: &Quantity) -> Result<Quantity, String> {
        if self.unit != other.unit {
            return Err("Unit mismatch in subtraction".into());
        }
        let new_value = self.value.propagate_sub(&*other.value)?;
        Ok(Quantity { value: new_value, unit: self.unit.clone() })
    }

    pub fn mul(&self, other: &Quantity) -> Result<Quantity, String> {
        let new_value = self.value.propagate_mul(&*other.value)?;
        let new_unit = self.unit.mul(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn div(&self, other: &Quantity) -> Result<Quantity, String> {
        let new_value = self.value.propagate_div(&*other.value)?;
        let new_unit = self.unit.div(&other.unit);
        Ok(Quantity { value: new_value, unit: new_unit })
    }

    pub fn pow(&self, exponent: f64) -> Result<Quantity, String> {
        let exp_r = Rational64::from_f64(exponent).unwrap_or(Rational64::new(0, 1));
        let new_value = self.value.propagate_pow(exponent)?;
        let new_unit = self.unit.pow(exp_r);
        Ok(Quantity { value: new_value, unit: new_unit })
    }
}
