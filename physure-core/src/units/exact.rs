//! Exact rational scale factors for units.
//!
//! A unit's `scale` is an `f64`: the nearest double to the decimal that `physure.conf`
//! writes. A conversion divides two such scales, and every rounding on the way shows in the
//! last digit (`1 ppm => percent` used to be `9.999999999999999e-05`). `ExactScale` keeps the
//! value as the rational the catalog states, so a ratio is computed exactly and rounded once,
//! which is what Python's `Fraction` arithmetic does.
//!
//! Three representations, so that the common cases cost almost nothing:
//!
//! * `One`, every coherent SI unit: no allocation, no arithmetic.
//! * `Small`, a reduced fraction of two `u128`: `km`, `mile`, `lb`, `g/cm^3`, nearly every unit
//!   a person writes. Plain integer arithmetic with overflow checks.
//! * `Big`, an unbounded `BigRational`: the rest. A prefix such as quecto (10^-30) times a scale
//!   written with 15 digits already needs a denominator beyond 128 bits, and the catalog
//!   contains such units, so the fallback is exact too instead of dropping to `f64`.
//!
//! A value is always held in the smallest representation that fits, so equal values compare
//! equal.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::sync::Arc;

/// Largest power of ten, and largest integer exponent, an exact scale will build. Beyond this
/// the result is "not exact" (`None`) and callers fall back to `f64`, rather than allocate
/// integers of unbounded size on behalf of a hostile `1e999999999`.
const MAX_EXPONENT: i64 = 400;

/// Integers up to this size convert to `f64` without rounding, so dividing two of them with the
/// FPU is already the correctly rounded quotient.
const EXACT_IN_F64: u128 = 1 << 53;

/// A positive rational number: the value of a unit in the coherent base units of its dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactScale(Repr);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Repr {
    One,
    /// numerator, denominator: reduced, both non-zero, not both 1.
    Small(u128, u128),
    Big(Arc<BigRational>),
}

impl ExactScale {
    pub fn one() -> Self {
        ExactScale(Repr::One)
    }

    pub fn is_one(&self) -> bool {
        matches!(self.0, Repr::One)
    }

    fn small(numer: u128, denom: u128) -> Self {
        if numer == 1 && denom == 1 {
            Self::one()
        } else {
            ExactScale(Repr::Small(numer, denom))
        }
    }

    /// The smallest representation that holds `ratio` (which is already reduced).
    fn from_ratio(ratio: BigRational) -> Self {
        match (ratio.numer().to_u128(), ratio.denom().to_u128()) {
            (Some(numer), Some(denom)) => Self::small(numer, denom),
            _ => ExactScale(Repr::Big(Arc::new(ratio))),
        }
    }

    fn to_big(&self) -> BigRational {
        match &self.0 {
            Repr::One => BigRational::one(),
            Repr::Small(numer, denom) => BigRational::new(BigInt::from(*numer), BigInt::from(*denom)),
            Repr::Big(ratio) => ratio.as_ref().clone(),
        }
    }

    /// Reads a decimal such as `1e-5`, `0.0254`, `6.02214076e+23` or a fraction `3/7` as
    /// Python's `Fraction(text)` does. `None` for zero, negative values, and anything that is
    /// not a finite decimal, so the caller keeps the `f64` it already has.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some((numer, denom)) = parse_small_decimal(text) {
            return Some(Self::small(numer, denom));
        }
        let ratio = match text.split_once('/') {
            Some((numer, denom)) => {
                let numer: BigInt = numer.trim().parse().ok()?;
                let denom: BigInt = denom.trim().parse().ok()?;
                if denom.is_zero() {
                    return None;
                }
                BigRational::new(numer, denom)
            }
            None => parse_decimal(text)?,
        };
        ratio.is_positive().then(|| Self::from_ratio(ratio))
    }

    /// The exact value of a double's shortest round-trip decimal (`0.001` for `1e-3`), for
    /// scales that reach the registry as `f64` rather than as the text of the catalog.
    pub fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        Self::parse(&format!("{value:e}"))
    }

    pub fn mul(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (Repr::One, _) => other.clone(),
            (_, Repr::One) => self.clone(),
            (Repr::Small(an, ad), Repr::Small(bn, bd)) => match mul_small((*an, *ad), (*bn, *bd)) {
                Some((numer, denom)) => Self::small(numer, denom),
                None => Self::from_ratio(self.to_big() * other.to_big()),
            },
            _ => Self::from_ratio(self.to_big() * other.to_big()),
        }
    }

    /// `self / other`; `other` is never zero, because `parse` refuses zero.
    pub fn div(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (_, Repr::One) => self.clone(),
            (Repr::One, Repr::Small(numer, denom)) => Self::small(*denom, *numer),
            (Repr::Small(an, ad), Repr::Small(bn, bd)) => match mul_small((*an, *ad), (*bd, *bn)) {
                Some((numer, denom)) => Self::small(numer, denom),
                None => Self::from_ratio(self.to_big() / other.to_big()),
            },
            _ => Self::from_ratio(self.to_big() / other.to_big()),
        }
    }

    /// `self` raised to an integer power; `None` when the exponent is too large to build.
    pub fn powi(&self, exponent: i64) -> Option<Self> {
        if exponent.abs() > MAX_EXPONENT {
            return None;
        }
        let power = exponent.unsigned_abs();
        match &self.0 {
            Repr::One => Some(Self::one()),
            Repr::Small(numer, denom) => {
                let (numer, denom) = if exponent < 0 { (*denom, *numer) } else { (*numer, *denom) };
                match (u32::try_from(power).ok().and_then(|p| numer.checked_pow(p)), u32::try_from(power).ok().and_then(|p| denom.checked_pow(p))) {
                    (Some(n), Some(d)) => Some(Self::small(n, d)),
                    _ => Some(Self::big_pow(self, exponent)),
                }
            }
            Repr::Big(_) => Some(Self::big_pow(self, exponent)),
        }
    }

    fn big_pow(&self, exponent: i64) -> Self {
        let base = self.to_big();
        let base = if exponent < 0 { base.recip() } else { base };
        Self::from_ratio(num_traits::pow(base, exponent.unsigned_abs() as usize))
    }

    /// `(plus - minus) / divisor` rounded once; a missing operand is zero. The shift of an
    /// affine conversion, whose operands (the zero points) may be zero or differ in sign.
    pub fn shifted_quotient(plus: Option<&Self>, minus: Option<&Self>, divisor: &Self) -> f64 {
        let plus = plus.map_or_else(BigRational::zero, Self::to_big);
        let minus = minus.map_or_else(BigRational::zero, Self::to_big);
        ((plus - minus) / divisor.to_big()).to_f64().unwrap_or(f64::NAN)
    }

    /// The nearest `f64`, rounded once (ties to even).
    pub fn to_f64(&self) -> f64 {
        match &self.0 {
            Repr::One => 1.0,
            Repr::Small(numer, denom) if *numer <= EXACT_IN_F64 && *denom <= EXACT_IN_F64 => {
                *numer as f64 / *denom as f64
            }
            Repr::Small(..) | Repr::Big(_) => self.to_big().to_f64().unwrap_or(f64::NAN),
        }
    }
}

/// The product of two reduced fractions, reduced, or `None` if it does not fit in `u128`.
/// Cross-reducing before multiplying keeps the result reduced without a final gcd.
fn mul_small((an, ad): (u128, u128), (bn, bd): (u128, u128)) -> Option<(u128, u128)> {
    let g1 = gcd(an, bd);
    let g2 = gcd(bn, ad);
    let numer = (an / g1).checked_mul(bn / g2)?;
    let denom = (ad / g2).checked_mul(bd / g1)?;
    Some((numer, denom))
}

/// Binary gcd of two non-zero integers.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return a << shift;
        }
    }
}

/// A plain positive decimal whose reduced fraction fits in two `u128`, read without
/// allocating; `None` for anything else, which `parse` then reads the slow way (and refuses,
/// if it is zero, negative or not a decimal).
fn parse_small_decimal(text: &str) -> Option<(u128, u128)> {
    let (mantissa, exponent) = match text.find(|c| c == 'e' || c == 'E') {
        Some(at) => (&text[..at], text[at + 1..].parse::<i32>().ok()?),
        None => (text, 0),
    };
    let mantissa = mantissa.strip_prefix('+').unwrap_or(mantissa);
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    let mut numer: u128 = 0;
    for byte in int_part.bytes().chain(frac_part.bytes()) {
        if !byte.is_ascii_digit() {
            return None;
        }
        numer = numer.checked_mul(10)?.checked_add(u128::from(byte - b'0'))?;
    }
    if numer == 0 {
        return None;
    }
    let power = exponent.checked_sub(i32::try_from(frac_part.len()).ok()?)?;
    let (numer, denom) = if power >= 0 {
        (numer.checked_mul(10u128.checked_pow(power as u32)?)?, 1)
    } else {
        (numer, 10u128.checked_pow(power.unsigned_abs())?)
    };
    let divisor = gcd(numer, denom);
    Some((numer / divisor, denom / divisor))
}

fn parse_decimal(text: &str) -> Option<BigRational> {
    let (mantissa, exponent) = match text.find(|c| c == 'e' || c == 'E') {
        Some(at) => (&text[..at], text[at + 1..].parse::<i64>().ok()?),
        None => (text, 0),
    };
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().chain(frac_part.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let power_of_ten = exponent.checked_sub(frac_part.len() as i64)?;
    if power_of_ten.abs() > MAX_EXPONENT {
        return None;
    }
    let mut numer: BigInt = format!("{int_part}{frac_part}").parse().ok()?;
    if negative {
        numer = -numer;
    }
    let ten_power = num_traits::pow(BigInt::from(10), power_of_ten.unsigned_abs() as usize);
    Some(if power_of_ten >= 0 {
        BigRational::from_integer(numer * ten_power)
    } else {
        BigRational::new(numer, ten_power)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(text: &str) -> ExactScale {
        ExactScale::parse(text).unwrap_or_else(|| panic!("{text:?} should parse"))
    }

    fn is_small(scale: &ExactScale) -> bool {
        matches!(scale.0, Repr::Small(..))
    }

    fn is_big(scale: &ExactScale) -> bool {
        matches!(scale.0, Repr::Big(_))
    }

    #[test]
    fn parses_decimals_exponents_and_fractions() {
        assert!(exact("1").is_one());
        assert!(exact("1.0").is_one());
        assert!(exact("1e0").is_one());
        assert_eq!(exact("0.5"), exact("1/2"));
        assert_eq!(exact("1e3"), exact("1000"));
        assert_eq!(exact("1E-3"), exact("1/1000"));
        assert_eq!(exact("+2.5e+1"), exact("25"));
        assert_eq!(exact(" 0.0254 "), exact("127/5000"));
    }

    #[test]
    fn refuses_what_is_not_a_positive_finite_decimal() {
        for text in ["", "e5", ".", "abc", "0", "0.0", "-1", "1/0", "nan", "inf", "1e", "1.2.3", "0x10", "1e999999"] {
            assert!(ExactScale::parse(text).is_none(), "{text:?} should not parse");
        }
    }

    #[test]
    fn from_f64_reads_the_shortest_decimal_of_the_double() {
        assert_eq!(ExactScale::from_f64(0.001).unwrap(), exact("1/1000"));
        assert_eq!(ExactScale::from_f64(1.0).unwrap(), ExactScale::one());
        assert_eq!(ExactScale::from_f64(0.0174532925).unwrap(), exact("0.0174532925"));
        assert!(ExactScale::from_f64(0.0).is_none());
        assert!(ExactScale::from_f64(f64::NAN).is_none());
        assert!(ExactScale::from_f64(-2.0).is_none());
    }

    #[test]
    fn arithmetic_is_exact() {
        let kilo = exact("1e3");
        let milli = exact("1e-3");
        assert!(kilo.mul(&milli).is_one());
        assert!(kilo.div(&kilo).is_one());
        assert_eq!(milli.div(&kilo), exact("1e-6"));
        assert_eq!(kilo.powi(2).unwrap(), exact("1e6"));
        assert_eq!(kilo.powi(-2).unwrap(), exact("1e-6"));
        assert!(kilo.powi(0).unwrap().is_one());
        assert!(ExactScale::one().powi(7).unwrap().is_one());
        assert!(kilo.powi(401).is_none());
    }

    /// `1 ppm => percent`: the doubles 1e-6 and 0.01 divide to `9.999999999999999e-05`, the
    /// rationals the catalog states divide to exactly `1e-4`.
    #[test]
    fn a_ratio_is_rounded_once() {
        let ppm = exact("1e-6");
        let percent = exact("0.01");
        assert_ne!(1e-6_f64 / 0.01_f64, 1e-4);
        assert_eq!(ppm.div(&percent).to_f64(), 1e-4);
        assert_eq!(exact("0.1").div(&exact("0.01")).to_f64(), 10.0);
    }

    #[test]
    fn to_f64_rounds_to_nearest_even_beyond_what_i128_can_hold() {
        // quecto (1e-30) times a 15-digit scale: the denominator is 10^50.
        let tiny = exact("1e-30").mul(&exact("4.84813681109536e-06"));
        assert!(is_big(&tiny));
        assert_eq!(tiny.to_f64(), 4.84813681109536e-36);
        // 4.8e24 still fits in u128, but not in 53 bits: rounded through the exact division
        let big = exact("1e30").mul(&exact("4.84813681109536e-06"));
        assert!(is_small(&big));
        assert_eq!(big.to_f64(), 4.84813681109536e24);
    }

    #[test]
    fn a_value_is_held_in_the_smallest_representation_that_fits() {
        assert!(is_small(&exact("1e3")));
        assert!(is_small(&exact("0.0254")));
        assert!(is_big(&exact("1e50")));
        assert!(is_big(&exact("1e-50")));
        // leaving the u128 range and coming back normalises, so equal values compare equal
        let there = exact("1e50").mul(&exact("1e3"));
        assert!(is_big(&there));
        let back = there.div(&exact("1e50"));
        assert!(is_small(&back));
        assert_eq!(back, exact("1e3"));
        assert!(exact("1e50").div(&exact("1e50")).is_one());
    }

    #[test]
    fn small_arithmetic_that_overflows_falls_back_to_big_and_stays_exact() {
        let a = exact("1e20");
        let b = exact("1e20");
        let product = a.mul(&b);
        assert!(is_big(&product));
        assert_eq!(product, exact("1e40"));
        assert_eq!(a.powi(3).unwrap(), exact("1e60"));
        assert_eq!(a.powi(-3).unwrap(), exact("1e-60"));
        assert_eq!(exact("1e-20").div(&exact("1e20")), exact("1e-40"));
    }

    #[test]
    fn a_fraction_is_reduced_across_the_operands() {
        let third = exact("1/3");
        let nine = exact("9");
        assert_eq!(third.mul(&nine), exact("3"));
        assert_eq!(nine.div(&third), exact("27"));
        assert_eq!(exact("2/3").mul(&exact("3/2")), ExactScale::one());
    }

    /// The allocation-free reading and the `BigInt` one are the same number.
    #[test]
    fn the_fast_reading_of_a_decimal_agrees_with_the_general_one() {
        for text in ["1", "0.0254", "1e-5", "6.02214076e+23", "4046.8564224", "0.017453292519943295", "1e38", "1e-38", "123456789012345678901234567890"] {
            let fast = ExactScale::parse(text).unwrap_or_else(|| panic!("{text:?} should parse"));
            let general = ExactScale::from_ratio(parse_decimal(text).unwrap());
            assert_eq!(fast, general, "{text}");
        }
        // beyond u128 it falls back, and still parses
        assert!(parse_small_decimal("1e39").is_none());
        assert!(ExactScale::parse("1e39").is_some());
    }

    #[test]
    fn a_shift_is_rounded_once_from_the_exact_zero_points() {
        let kelvin_of_degc = exact("273.15");
        let kelvin_of_degf = exact("45967/180");
        let degf = exact("5/9");
        // degC -> degF: (273.15 - 45967/180) / (5/9) = 32 exactly
        assert_eq!(ExactScale::shifted_quotient(Some(&kelvin_of_degc), Some(&kelvin_of_degf), &degf), 32.0);
        // degF -> degC: (45967/180 - 273.15) / 1 = -160/9
        assert_eq!(ExactScale::shifted_quotient(Some(&kelvin_of_degf), Some(&kelvin_of_degc), &ExactScale::one()), -160.0 / 9.0);
        assert_eq!(ExactScale::shifted_quotient(None, None, &degf), 0.0);
        assert_eq!(ExactScale::shifted_quotient(Some(&kelvin_of_degc), None, &ExactScale::one()), 273.15);
    }

    #[test]
    fn to_f64_of_large_small_values_goes_through_the_exact_division() {
        // numerator and denominator both above 2^53, so the FPU quotient would round twice
        let scale = exact("0.30480000000000001");
        assert!(is_small(&scale));
        assert_eq!(scale.to_f64(), 0.30480000000000001);
    }
}
