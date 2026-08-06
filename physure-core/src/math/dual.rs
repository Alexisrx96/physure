use std::ops::{Add, Sub, Mul, Div, Neg};

/// First-Order Dual Number: x = val + der * ε where ε² = 0.
/// Provides exact machine-precision automatic differentiation in a single pass.
///
/// Reference: Clifford, W. K. (1873). Preliminary Sketch of Biquaternions.
/// Proceedings of the London Mathematical Society, 4(1), 381-395.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DualNumber {
    pub value: f64,
    pub derivative: f64,
}

impl DualNumber {
    pub fn constant(value: f64) -> Self {
        DualNumber { value, derivative: 0.0 }
    }

    pub fn variable(value: f64) -> Self {
        DualNumber { value, derivative: 1.0 }
    }

    pub fn sin(self) -> Self {
        DualNumber {
            value: self.value.sin(),
            derivative: self.value.cos() * self.derivative,
        }
    }

    pub fn cos(self) -> Self {
        DualNumber {
            value: self.value.cos(),
            derivative: -self.value.sin() * self.derivative,
        }
    }

    pub fn exp(self) -> Self {
        let ev = self.value.exp();
        DualNumber {
            value: ev,
            derivative: ev * self.derivative,
        }
    }

    pub fn ln(self) -> Self {
        DualNumber {
            value: self.value.ln(),
            derivative: self.derivative / self.value,
        }
    }

    pub fn powf(self, n: f64) -> Self {
        DualNumber {
            value: self.value.powf(n),
            derivative: n * self.value.powf(n - 1.0) * self.derivative,
        }
    }

    pub fn sqrt(self) -> Self {
        let s = self.value.sqrt();
        DualNumber {
            value: s,
            derivative: self.derivative / (2.0 * s),
        }
    }
}

impl Add for DualNumber {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        DualNumber {
            value: self.value + rhs.value,
            derivative: self.derivative + rhs.derivative,
        }
    }
}

impl Sub for DualNumber {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        DualNumber {
            value: self.value - rhs.value,
            derivative: self.derivative - rhs.derivative,
        }
    }
}

impl Mul for DualNumber {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        DualNumber {
            value: self.value * rhs.value,
            derivative: self.value * rhs.derivative + self.derivative * rhs.value,
        }
    }
}

impl Div for DualNumber {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        let val = self.value / rhs.value;
        let der = (self.derivative * rhs.value - self.value * rhs.derivative) / (rhs.value * rhs.value);
        DualNumber { value: val, derivative: der }
    }
}

impl Neg for DualNumber {
    type Output = Self;
    fn neg(self) -> Self::Output {
        DualNumber {
            value: -self.value,
            derivative: -self.derivative,
        }
    }
}

/// Second-Order Hyper-Dual Number: x = val + d1 * ε₁ + d2 * ε₂ + d12 * ε₁ε₂.
/// Enables simultaneous exact extraction of Value, 1st Derivative, and 2nd Derivative (Hessian).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HyperDualNumber {
    pub value: f64,
    pub d1: f64,
    pub d2: f64,
    pub d12: f64,
}

impl HyperDualNumber {
    pub fn constant(val: f64) -> Self {
        HyperDualNumber { value: val, d1: 0.0, d2: 0.0, d12: 0.0 }
    }

    pub fn variable(val: f64) -> Self {
        HyperDualNumber { value: val, d1: 1.0, d2: 1.0, d12: 0.0 }
    }

    pub fn hessian_var(val: f64) -> Self {
        HyperDualNumber { value: val, d1: 1.0, d2: 1.0, d12: 0.0 }
    }
}

impl Add for HyperDualNumber {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        HyperDualNumber {
            value: self.value + rhs.value,
            d1: self.d1 + rhs.d1,
            d2: self.d2 + rhs.d2,
            d12: self.d12 + rhs.d12,
        }
    }
}

impl Sub for HyperDualNumber {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        HyperDualNumber {
            value: self.value - rhs.value,
            d1: self.d1 - rhs.d1,
            d2: self.d2 - rhs.d2,
            d12: self.d12 - rhs.d12,
        }
    }
}

impl Mul for HyperDualNumber {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        HyperDualNumber {
            value: self.value * rhs.value,
            d1: self.value * rhs.d1 + self.d1 * rhs.value,
            d2: self.value * rhs.d2 + self.d2 * rhs.value,
            d12: self.value * rhs.d12 + self.d1 * rhs.d2 + self.d2 * rhs.d1 + self.d12 * rhs.value,
        }
    }
}
