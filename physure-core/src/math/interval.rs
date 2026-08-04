use std::ops::{Add, Sub, Mul, Div, Neg};

/// Closed Real Interval [min, max] representing rigorous numerical physical bounds.
///
/// References:
///     - Moore, R. E. (1966). Interval Analysis. Prentice-Hall.
///     - IEEE Std 1788-2015: IEEE Standard for Interval Arithmetic.
///       DOI: https://doi.org/10.1109/IEEESTD.2015.7140721
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub min: f64,
    pub max: f64,
}

impl Interval {
    pub fn new(min: f64, max: f64) -> Self {
        if min <= max {
            Interval { min, max }
        } else {
            Interval { min: max, max: min }
        }
    }

    pub fn point(val: f64) -> Self {
        Interval { min: val, max: val }
    }

    pub fn midpoint(&self) -> f64 {
        (self.min + self.max) / 2.0
    }

    pub fn width(&self) -> f64 {
        self.max - self.min
    }

    pub fn radius(&self) -> f64 {
        self.width() / 2.0
    }

    pub fn contains(&self, val: f64) -> bool {
        self.min <= val && val <= self.max
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.min <= other.max && other.min <= self.max
    }

    pub fn abs(&self) -> Self {
        if self.min >= 0.0 {
            *self
        } else if self.max <= 0.0 {
            Interval { min: -self.max, max: -self.min }
        } else {
            Interval { min: 0.0, max: self.min.abs().max(self.max.abs()) }
        }
    }

    pub fn powi(&self, n: i32) -> Self {
        if n % 2 == 1 {
            Interval::new(self.min.powi(n), self.max.powi(n))
        } else if self.min >= 0.0 {
            Interval::new(self.min.powi(n), self.max.powi(n))
        } else if self.max <= 0.0 {
            Interval::new(self.max.powi(n), self.min.powi(n))
        } else {
            Interval::new(0.0, self.min.abs().max(self.max.abs()).powi(n))
        }
    }

    pub fn sqrt(&self) -> Option<Self> {
        if self.max < 0.0 {
            None
        } else {
            let min = if self.min < 0.0 { 0.0 } else { self.min.sqrt() };
            Some(Interval::new(min, self.max.sqrt()))
        }
    }
}

impl Add for Interval {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Interval {
            min: self.min + rhs.min,
            max: self.max + rhs.max,
        }
    }
}

impl Sub for Interval {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Interval {
            min: self.min - rhs.max,
            max: self.max - rhs.min,
        }
    }
}

impl Mul for Interval {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        let p1 = self.min * rhs.min;
        let p2 = self.min * rhs.max;
        let p3 = self.max * rhs.min;
        let p4 = self.max * rhs.max;
        let min = p1.min(p2).min(p3.min(p4));
        let max = p1.max(p2).max(p3.max(p4));
        Interval { min, max }
    }
}

impl Div for Interval {
    type Output = Option<Self>;
    fn div(self, rhs: Self) -> Self::Output {
        if rhs.contains(0.0) {
            None
        } else {
            let inv = Interval { min: 1.0 / rhs.max, max: 1.0 / rhs.min };
            Some(self * inv)
        }
    }
}

impl Neg for Interval {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Interval {
            min: -self.max,
            max: -self.min,
        }
    }
}
