/// Native dimension vector for physure-core.
///
/// A `DimVector` stores the exponents of the 9 canonical SI base dimensions
/// as a fixed-length array of `i8`:
///
/// ```text
/// index: 0=L  1=M  2=T  3=I  4=O  5=N  6=J  7=A  8=$
/// ```
///
/// Using `i8` keeps the struct at 9 bytes (zero-copy-friendly) while covering
/// all practical exponent ranges (–127 … +127).
use std::fmt;

/// Canonical SI base-dimension order shared across the whole codebase.
pub const SI_ORDER: [&str; 9] = ["L", "M", "T", "I", "O", "N", "J", "A", "$"];

/// Maps a dimension symbol to its canonical index.
pub fn dim_index(symbol: &str) -> Option<usize> {
    SI_ORDER.iter().position(|&s| s == symbol)
}

/// Fixed-length exponent vector for a physical dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DimVector(pub [i8; 9]);

impl DimVector {
    /// The dimensionless (zero) vector.
    pub const DIMENSIONLESS: DimVector = DimVector([0; 9]);

    /// Construct from an iterator of (symbol, exponent) pairs.
    pub fn from_pairs<'a>(
        pairs: impl IntoIterator<Item = (&'a str, i64)>,
    ) -> Result<Self, String> {
        let mut v = [0i8; 9];
        for (sym, exp) in pairs {
            match dim_index(sym) {
                Some(i) => {
                    v[i] = exp
                        .try_into()
                        .map_err(|_| format!("Exponent {exp} out of i8 range for {sym}"))?;
                }
                None => {
                    return Err(format!("Unknown base dimension symbol: {sym}"));
                }
            }
        }
        Ok(DimVector(v))
    }

    /// Returns `true` if all exponents are zero (dimensionless).
    pub fn is_dimensionless(&self) -> bool {
        self.0.iter().all(|&x| x == 0)
    }

    /// Multiply two dimensions: add their exponent vectors.
    pub fn mul(&self, other: &DimVector) -> DimVector {
        let mut out = [0i8; 9];
        for i in 0..9 {
            out[i] = self.0[i].saturating_add(other.0[i]);
        }
        DimVector(out)
    }

    /// Divide two dimensions: subtract their exponent vectors.
    pub fn div(&self, other: &DimVector) -> DimVector {
        let mut out = [0i8; 9];
        for i in 0..9 {
            out[i] = self.0[i].saturating_sub(other.0[i]);
        }
        DimVector(out)
    }

    /// Raise to an integer power: scale all exponents.
    pub fn pow(&self, exp: i32) -> DimVector {
        let mut out = [0i8; 9];
        for i in 0..9 {
            out[i] = ((self.0[i] as i32) * exp) as i8;
        }
        DimVector(out)
    }

    /// Raise to a rational power (num/den). Returns `None` if any result is
    /// not exactly representable as an integer.
    pub fn pow_rational(&self, num: i32, den: i32) -> Option<DimVector> {
        let mut out = [0i8; 9];
        for i in 0..9 {
            let val = (self.0[i] as i32) * num;
            if val % den != 0 {
                return None;
            }
            out[i] = (val / den) as i8;
        }
        Some(DimVector(out))
    }

    /// Return the non-zero components as a `Vec<(&str, i8)>`.
    pub fn to_pairs(&self) -> Vec<(&'static str, i8)> {
        SI_ORDER
            .iter()
            .enumerate()
            .filter_map(|(i, &sym)| {
                if self.0[i] != 0 {
                    Some((sym, self.0[i]))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the analytical string representation, e.g. `"L·T⁻²"`.
    pub fn analytical_repr(&self) -> String {
        let pairs = self.to_pairs();
        if pairs.is_empty() {
            return "Dimensionless".to_string();
        }
        pairs
            .iter()
            .map(|(sym, exp)| {
                if *exp == 1 {
                    sym.to_string()
                } else {
                    format!("{}{}", sym, to_superscript(*exp as i32))
                }
            })
            .collect::<Vec<_>>()
            .join("·")
    }
}

impl fmt::Display for DimVector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.analytical_repr())
    }
}

/// Converts an integer to Unicode superscript string (e.g. -2 → "⁻²").
pub fn to_superscript(n: i32) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut result = String::new();
    if n < 0 {
        result.push('⁻');
    }
    let s = n.unsigned_abs().to_string();
    for c in s.chars() {
        let idx = c as usize - '0' as usize;
        result.push(DIGITS[idx]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimensionless() {
        assert!(DimVector::DIMENSIONLESS.is_dimensionless());
    }

    #[test]
    fn test_from_pairs() {
        let v = DimVector::from_pairs([("L", 1), ("T", -2)]).unwrap();
        assert_eq!(v.0[0], 1); // L
        assert_eq!(v.0[2], -2); // T
    }

    #[test]
    fn test_mul_div() {
        let length = DimVector::from_pairs([("L", 1)]).unwrap();
        let time = DimVector::from_pairs([("T", 1)]).unwrap();
        let velocity = length.div(&time);
        assert_eq!(velocity.0[0], 1);  // L¹
        assert_eq!(velocity.0[2], -1); // T⁻¹
    }

    #[test]
    fn test_pow() {
        let area = DimVector::from_pairs([("L", 1)]).unwrap().pow(2);
        assert_eq!(area.0[0], 2);
    }

    #[test]
    fn test_analytical_repr() {
        let v = DimVector::from_pairs([("L", 1), ("T", -2)]).unwrap();
        assert_eq!(v.to_string(), "L·T⁻²");
    }

    #[test]
    fn test_to_superscript() {
        assert_eq!(to_superscript(2), "²");
        assert_eq!(to_superscript(-1), "⁻¹");
        assert_eq!(to_superscript(0), "⁰");
    }
}
