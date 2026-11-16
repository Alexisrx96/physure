//! The special functions behind a coverage factor: `ln Γ`, the regularized incomplete beta and gamma
//! functions, and from them the two-sided Student-t and normal quantiles.
//!
//! The coverage factor `k_p` of a result with `ν` effective degrees of freedom is the number with
//! `P(|T_ν| < k_p) = p` (JCGM 100:2008, G.3.2 and Table G.2). It has no closed form, so it is found by
//! bracketing and bisection on the two-sided tail, which is monotone: bisection cannot fail, and the
//! tail is evaluated to near machine precision, so neither can the factor be far from it. No dependency
//! is needed; the algorithms are the textbook continued fractions (Numerical Recipes, `betacf` and
//! `gcf`) and the Lanczos approximation (g = 7, n = 9).

use crate::error::{PhysureError, PhysureResult};
use std::f64::consts::PI;

const MAX_ITERATIONS: usize = 3000;
const EPSILON: f64 = 1e-16;
const TINY: f64 = 1e-300;

/// Beyond this many degrees of freedom the continued fraction needs too many terms, and the Student-t
/// quantile is the normal one with its first Cornish–Fisher correction, `z + (z³ + z) / 4ν`, whose error
/// is of the order of `1/ν²`: below 1e-12 here.
const NORMAL_LIMIT: f64 = 1e6;

/// `ln Γ(x)` for `x > 0` (Lanczos, g = 7, n = 9; relative error about 1e-15).
pub fn ln_gamma(x: f64) -> f64 {
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // reflection: Γ(x) Γ(1 - x) = π / sin(πx)
        return PI.ln() - (PI * x).sin().ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let series = COEFFICIENTS
        .iter()
        .enumerate()
        .skip(1)
        .fold(COEFFICIENTS[0], |sum, (i, c)| sum + c / (x + i as f64));
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + series.ln()
}

/// The continued fraction of the incomplete beta function (modified Lentz).
fn beta_continued_fraction(a: f64, b: f64, x: f64) -> f64 {
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITERATIONS {
        let m = m as f64;
        let m2 = 2.0 * m;
        let even = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + even * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + even / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let odd = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + odd * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + odd / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < EPSILON {
            break;
        }
    }
    h
}

/// The regularized incomplete beta function `I_x(a, b)`, for `a, b > 0` and `0 <= x <= 1`.
pub fn reg_inc_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_front = ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (-x).ln_1p();
    let front = ln_front.exp();
    // the fraction converges quickly on the side of the mean where x lies; the other uses the symmetry
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_continued_fraction(a, b, x) / a
    } else {
        1.0 - front * beta_continued_fraction(b, a, 1.0 - x) / b
    }
}

/// The regularized upper incomplete gamma function `Q(a, x) = 1 - P(a, x)`, for `a > 0` and `x >= 0`.
pub fn reg_upper_gamma(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let ln_front = -x + a * x.ln() - ln_gamma(a);
    if x < a + 1.0 {
        // the series for P(a, x), then its complement
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut denominator = a;
        for _ in 0..MAX_ITERATIONS {
            denominator += 1.0;
            term *= x / denominator;
            sum += term;
            if term.abs() < sum.abs() * EPSILON {
                break;
            }
        }
        1.0 - sum * ln_front.exp()
    } else {
        // the continued fraction for Q(a, x)
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / TINY;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..=MAX_ITERATIONS {
            let i = i as f64;
            let an = -i * (i - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < TINY {
                d = TINY;
            }
            c = b + an / c;
            if c.abs() < TINY {
                c = TINY;
            }
            d = 1.0 / d;
            let delta = d * c;
            h *= delta;
            if (delta - 1.0).abs() < EPSILON {
                break;
            }
        }
        ln_front.exp() * h
    }
}

/// `P(|T_ν| > t)`: the two-sided tail of the Student-t distribution with `ν` degrees of freedom.
fn student_t_tail(t: f64, nu: f64) -> f64 {
    reg_inc_beta(nu / 2.0, 0.5, nu / (nu + t * t))
}

/// `P(|Z| > z)`: the two-sided tail of the standard normal distribution.
fn normal_tail(z: f64) -> f64 {
    reg_upper_gamma(0.5, z * z / 2.0)
}

/// The `t >= 0` at which the decreasing `tail` equals `target`: a bracket found by doubling, then bisection.
fn solve_tail(tail: impl Fn(f64) -> f64, target: f64) -> f64 {
    let mut low = 0.0;
    let mut high = 1.0;
    while tail(high) > target && high < 1e300 {
        low = high;
        high *= 2.0;
    }
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if tail(middle) > target {
            low = middle;
        } else {
            high = middle;
        }
        if high - low <= 1e-15 * high {
            break;
        }
    }
    0.5 * (low + high)
}

/// The two-sided coverage factor `k_p` of the standard normal distribution: `P(|Z| < k_p) = p`.
pub fn normal_two_sided(p: f64) -> PhysureResult<f64> {
    check_probability(p)?;
    Ok(solve_tail(normal_tail, 1.0 - p))
}

/// The two-sided coverage factor `k_p` of the Student-t distribution with `nu` degrees of freedom:
/// `P(|T_ν| < k_p) = p`, `nu` real (the Guide truncates the effective degrees of freedom to an integer
/// before this, which is the caller's choice) and `+∞` for the normal distribution.
///
/// # Errors
/// `p` outside the open interval (0, 1), or `nu` not positive, is a `DomainError`.
pub fn student_t_two_sided(p: f64, nu: f64) -> PhysureResult<f64> {
    check_probability(p)?;
    if nu.is_nan() || nu <= 0.0 {
        return Err(PhysureError::DomainError(format!("degrees of freedom must be positive, got {nu}")));
    }
    if nu.is_infinite() {
        return normal_two_sided(p);
    }
    if nu > NORMAL_LIMIT {
        let z = normal_two_sided(p)?;
        return Ok(z + (z * z * z + z) / (4.0 * nu));
    }
    Ok(solve_tail(|t| student_t_tail(t, nu), 1.0 - p))
}

fn check_probability(p: f64) -> PhysureResult<()> {
    if p > 0.0 && p < 1.0 {
        Ok(())
    } else {
        Err(PhysureError::DomainError(format!("a coverage probability must lie in (0, 1), got {p}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, relative: f64) -> bool {
        ((a - b) / b).abs() < relative
    }

    #[test]
    fn ln_gamma_is_the_log_of_the_factorial_and_reflects_below_one_half() {
        assert!(close(ln_gamma(5.0), 24f64.ln(), 1e-14));
        assert!(close(ln_gamma(0.5), PI.sqrt().ln(), 1e-14));
        assert!(close(ln_gamma(100.0), 359.134_205_369_575_4, 1e-14));
        // Γ(0.25) = 3.6256099082...
        assert!(close(ln_gamma(0.25), 3.625_609_908_221_908f64.ln(), 1e-13));
    }

    #[test]
    fn the_incomplete_beta_has_its_known_values() {
        assert_eq!(reg_inc_beta(2.0, 3.0, 0.0), 0.0);
        assert_eq!(reg_inc_beta(2.0, 3.0, 1.0), 1.0);
        // I_x(1, 1) = x and I_x(a, b) + I_{1-x}(b, a) = 1
        assert!(close(reg_inc_beta(1.0, 1.0, 0.3), 0.3, 1e-14));
        assert!(close(reg_inc_beta(2.0, 5.0, 0.2) + reg_inc_beta(5.0, 2.0, 0.8), 1.0, 1e-14));
        // I_{1/2}(a, a) = 1/2
        assert!(close(reg_inc_beta(7.5, 7.5, 0.5), 0.5, 1e-13));
    }

    #[test]
    fn the_incomplete_gamma_has_its_known_values() {
        assert_eq!(reg_upper_gamma(0.5, 0.0), 1.0);
        // Q(1, x) = e^-x, on both sides of the switch between the series and the fraction
        assert!(close(reg_upper_gamma(1.0, 0.5), (-0.5f64).exp(), 1e-14));
        assert!(close(reg_upper_gamma(1.0, 4.0), (-4.0f64).exp(), 1e-14));
        // Q(1/2, z²/2) = erfc(z/√2): the two-sided normal tail at 1.96 is 0.0499957902964
        assert!(close(reg_upper_gamma(0.5, 1.96 * 1.96 / 2.0), 0.049_995_790_296_4, 1e-9));
    }

    #[test]
    fn the_normal_quantile_has_its_known_values() {
        assert!(close(normal_two_sided(0.95).unwrap(), 1.959_963_984_540_054, 1e-12));
        assert!(close(normal_two_sided(0.6827).unwrap(), 1.0, 1e-3));
        assert!(normal_two_sided(0.0).is_err() && normal_two_sided(1.0).is_err());
    }

    #[test]
    fn many_degrees_of_freedom_approach_the_normal_quantile_from_above() {
        let normal = normal_two_sided(0.95).unwrap();
        let big = student_t_two_sided(0.95, 5.0e6).unwrap();
        let bigger = student_t_two_sided(0.95, 5.0e9).unwrap();
        assert!(big > normal && big - normal < 1e-6);
        assert!(bigger > normal && bigger - normal < big - normal);
        // across the limit the two formulas agree
        let below = student_t_two_sided(0.95, 0.999_999e6).unwrap();
        let above = student_t_two_sided(0.95, 1.000_001e6).unwrap();
        assert!((below - above).abs() < 1e-9, "{below} vs {above}");
    }
}
