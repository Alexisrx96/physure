use crate::error::{PhysureError, PhysureResult};

/// Conversion models supported natively by physure-core.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitConverter {
    Linear { scale: f64 },
    Offset { scale: f64, offset: f64 },
    Logarithmic { factor: f64, reference: f64 },
}

impl UnitConverter {
    pub fn linear(scale: f64) -> Self {
        UnitConverter::Linear { scale }
    }

    pub fn offset(scale: f64, offset: f64) -> Self {
        UnitConverter::Offset { scale, offset }
    }

    pub fn logarithmic(factor: f64, reference: f64) -> Self {
        UnitConverter::Logarithmic { factor, reference }
    }

    pub fn is_linear(&self) -> bool {
        matches!(self, UnitConverter::Linear { .. })
    }

    pub fn to_base(&self, value: f64) -> f64 {
        match self {
            UnitConverter::Linear { scale } => value * scale,
            UnitConverter::Offset { scale, offset } => (value * scale) + offset,
            UnitConverter::Logarithmic { factor, reference } => reference * (10.0_f64.powf(value / factor)),
        }
    }

    pub fn from_base(&self, value: f64) -> f64 {
        match self {
            UnitConverter::Linear { scale } => value / scale,
            UnitConverter::Offset { scale, offset } => (value - offset) / scale,
            UnitConverter::Logarithmic { factor, reference } => factor * (value / reference).log10(),
        }
    }

    pub fn to_base_derivative(&self, value: f64) -> f64 {
        match self {
            UnitConverter::Linear { scale } => *scale,
            UnitConverter::Offset { scale, .. } => *scale,
            UnitConverter::Logarithmic { factor, .. } => self.to_base(value) * std::f64::consts::LN_10 / factor,
        }
    }

    pub fn from_base_derivative(&self, value: f64) -> f64 {
        match self {
            UnitConverter::Linear { scale } => 1.0 / scale,
            UnitConverter::Offset { scale, .. } => 1.0 / scale,
            UnitConverter::Logarithmic { factor, .. } => factor / (value * std::f64::consts::LN_10),
        }
    }

    pub fn convert_value(&self, value: f64, target: &UnitConverter) -> f64 {
        let base_val = self.to_base(value);
        target.from_base(base_val)
    }

    pub fn convert_batch(&self, data: &mut [f64], target: &UnitConverter) {
        for val in data.iter_mut() {
            let base_val = self.to_base(*val);
            *val = target.from_base(base_val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_converter() {
        let conv = UnitConverter::linear(1000.0); // km to m
        assert_eq!(conv.to_base(5.0), 5000.0);
        assert_eq!(conv.from_base(5000.0), 5.0);
    }

    #[test]
    fn test_offset_converter() {
        let cel = UnitConverter::offset(1.0, 273.15); // Celsius to Kelvin
        assert_eq!(cel.to_base(0.0), 273.15);
        assert_eq!(cel.from_base(273.15), 0.0);
    }

    #[test]
    fn test_convert_between() {
        let cel = UnitConverter::offset(1.0, 273.15);
        let fah = UnitConverter::offset(5.0 / 9.0, 273.15 - (32.0 * 5.0 / 9.0));
        let res = cel.convert_value(100.0, &fah);
        assert!((res - 212.0).abs() < 1e-6);
    }
}
