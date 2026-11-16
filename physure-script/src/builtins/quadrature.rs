//! Tanh-sinh (double-exponential) quadrature with an error estimate, for the definite `integral` of PHS.
//!
//! The substitution `x = c + h_w tanh((pi/2) sinh t)` (`c` the midpoint, `h_w` the half width of `[a, b]`) turns the
//! integral into one over the whole real line whose integrand decays double-exponentially, and the trapezoidal rule
//! with step `h` is then extremely accurate; halving `h` (a "level") reuses every earlier node. Integrable endpoint
//! singularities (`1/sqrt(x)`, `ln x`, `x^-0.9` at `x = 0`) cost nothing extra, because the nodes crowd the ends.
//!
//! What makes it trustworthy here (each point was measured to matter):
//!
//! - **Abscissae as `a + dist` and `b - dist`.** The distance of a node from its end, `dist = (b - a) e / (1 + e)`
//!   with `e = exp(-pi sinh t)`, is computed directly, so a node next to `a = 0` is resolved down to the subnormal
//!   range; `c + h_w tanh(...)` would round every such node onto the end (`1/sqrt(x)` stalled at 7.5e-9 that way).
//! - **Each tail runs until it underflows, on its own** (the left one near `a`, the right one near `b`): a side stops
//!   when its weight underflows to 0 or its node would round onto the end itself, never because the integrand looks
//!   small there (a tail cut where the first samples happened to be tiny missed a peak that later levels never
//!   reached). Near `a = 0` that is down to the subnormal range; near `b = 1` at one ulp of 1. The one exception is
//!   [`Tail::Negligible`], for the end of a substitution that stands for an infinite end (`x` grows without bound
//!   there and `x^2 exp(-x)` would become `inf * 0`): that side also stops after two consecutive terms below `2^-64`
//!   of the running `sum |w f|`.
//! - **Minimum level 4 and two consecutive agreements.** A level-1 estimate of a narrow Gaussian was a confident
//!   `0`; the answer is accepted only from level 4 on, when the last two differences between levels are both within
//!   the tolerance.
//! - **The tolerance is relative**, `1e-12 max(|I|, L1)` with `L1 = sum |w f|` (the integral of `|f|` as the rule sees
//!   it), plus a floor of `64 eps L1` for the rounding of the sum itself. Never an absolute number: the integral of
//!   `x^2` over `[0, 1e-6]` (3.3e-19) must earn its digits like any other, and `sin(1000 x)` (whose integral is much
//!   smaller than its `L1`) is not refused for cancelling.
//! - **A sample that is not finite is an error**, never skipped.
//! - **A hard cap**: [`MAX_LEVEL`] levels and [`MAX_EVALUATIONS`] samples; then the best estimate is returned as not
//!   converged, and the caller refuses.
//!
//! Measured limits, which are refusals, not answers: a singularity at a **non-zero** end (`(1 - x)^-1/2` on
//! `[0, 1]`, `(x - 1)^-0.9` on `[1, 2]`: next to `x = 1` the node `1 - dist` cannot be closer than one ulp of 1, so
//! the tail beyond it is lost), a jump, a kink and `sin(1/x)` near 0 do not converge to the tolerance. A peak much
//! narrower than the spacing of the first levels' nodes can be missed entirely, and then every level agrees on a
//! wrong value: the rule cannot see what it never samples (split the interval around a known peak). The same holds
//! for a peak far out towards an infinite end.
//!
//! Reference: H. Takahasi and M. Mori, "Double exponential formulas for numerical integration", Publ. RIMS Kyoto
//! Univ. 9 (1974) 721-741, <https://doi.org/10.2977/prims/1195192451>.

use physure_core::error::{PhysureError, PhysureResult};

/// Relative tolerance of an accepted integral, against `max(|I|, L1)`.
pub(crate) const TOLERANCE: f64 = 1e-12;
/// The first level whose estimate may be accepted (level `k` has step `2^-k`).
pub(crate) const MIN_LEVEL: usize = 4;
/// The last level tried.
pub(crate) const MAX_LEVEL: usize = 12;
/// Samples one run may take before it gives up.
pub(crate) const MAX_EVALUATIONS: usize = 20_000;
/// A term below this fraction of the running `sum |w f|` is negligible (twice in a row ends a tail).
const TAIL: f64 = 1.0 / 18_446_744_073_709_551_616.0; // 2^-64
/// The rounding floor of the error, in units of `eps L1`.
const FLOOR: f64 = 64.0;

/// Where a side's nodes stop.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tail {
    /// When the weight underflows or the node rounds onto the end.
    Underflow,
    /// That, or after two consecutive negligible terms: for the end that stands for an infinite one.
    Negligible,
}

/// The outcome of a run: the estimate, its error estimate (the last difference between levels), `L1`, whether it
/// converged, the last complete level, and the coefficient `c_i` of each sample in the order they were taken, so
/// that `value = sum c_i f(x_i)` (0 for the samples of an unfinished level).
pub(crate) struct Quadrature {
    pub(crate) value: f64,
    pub(crate) error: f64,
    pub(crate) l1: f64,
    pub(crate) converged: bool,
    pub(crate) level: usize,
    pub(crate) coefficients: Vec<f64>,
}

impl Quadrature {
    pub(crate) fn evaluations(&self) -> usize {
        self.coefficients.len()
    }

    /// The sum of two pieces of one integral.
    pub(crate) fn plus(mut self, other: Quadrature) -> Quadrature {
        self.value += other.value;
        self.error += other.error;
        self.l1 += other.l1;
        self.converged &= other.converged;
        self.level = self.level.max(other.level);
        self.coefficients.extend(other.coefficients);
        self
    }
}

/// The node at `t >= 0`: its distance from the end it approaches and its weight (for step 1).
fn node(t: f64, width: f64) -> (f64, f64) {
    let e = (-std::f64::consts::PI * t.sinh()).exp();
    let dist = width * e / (1.0 + e);
    // w = (b - a)/2 * (pi/2) cosh t * sech^2((pi/2) sinh t), with sech^2 s = 4 e / (1 + e)^2 and e = exp(-2 s)
    let weight = width * std::f64::consts::PI * t.cosh() * e / ((1.0 + e) * (1.0 + e));
    (dist, weight)
}

struct Run<'a> {
    f: &'a mut dyn FnMut(f64) -> PhysureResult<f64>,
    a: f64,
    b: f64,
    raw_sum: f64,
    raw_l1: f64,
    /// The weight of each sample, in order (multiplied by the final step to give its coefficient).
    weights: Vec<f64>,
}

/// Why a level stopped before its end.
enum Stop {
    Cap,
}

impl Run<'_> {
    fn take(&mut self, x: f64, weight: f64) -> PhysureResult<f64> {
        let fx = (self.f)(x)?;
        let term = weight * fx;
        if !fx.is_finite() || !term.is_finite() {
            return Err(PhysureError::Generic(format!(
                "integral: a sample of the integrand is not finite ({fx} at {})",
                physure_core::quantity::format_float(x)
            )));
        }
        self.raw_sum += term;
        self.raw_l1 += term.abs();
        self.weights.push(weight);
        Ok(term)
    }

    /// The nodes `t = first, first + stride, ...` on one side, until the side's tail ends.
    fn side(&mut self, h: f64, first: usize, stride: usize, towards_b: bool, tail: Tail) -> PhysureResult<Result<(), Stop>> {
        let width = self.b - self.a;
        let mut negligible = 0;
        let mut j = first;
        loop {
            let (dist, weight) = node(j as f64 * h, width);
            let x = if towards_b { self.b - dist } else { self.a + dist };
            if weight == 0.0 || dist == 0.0 || x == self.a || x == self.b {
                return Ok(Ok(()));
            }
            if self.weights.len() >= MAX_EVALUATIONS {
                return Ok(Err(Stop::Cap));
            }
            let term = self.take(x, weight)?;
            negligible = if term.abs() <= TAIL * self.raw_l1 { negligible + 1 } else { 0 };
            if tail == Tail::Negligible && negligible >= 2 {
                return Ok(Ok(()));
            }
            j += stride;
        }
    }
}

/// The integral of `f` over the finite interval `[a, b]` (`a < b`), the tails near `a` and `b` ending as `tails`
/// says. An error of `f`, or a sample that is not finite, is an error; a run that does not converge returns its best
/// estimate with `converged: false`.
pub(crate) fn tanh_sinh(
    f: &mut dyn FnMut(f64) -> PhysureResult<f64>,
    a: f64,
    b: f64,
    tails: (Tail, Tail),
) -> PhysureResult<Quadrature> {
    let mut run = Run { f, a, b, raw_sum: 0.0, raw_l1: 0.0, weights: Vec::new() };
    let half = 0.5 * (b - a);
    run.take(a + half, half * std::f64::consts::FRAC_PI_2)?;

    let mut previous: Option<f64> = None;
    let mut agreed = false;
    let mut best = Quadrature { value: 0.0, error: f64::INFINITY, l1: 0.0, converged: false, level: 0, coefficients: Vec::new() };
    let mut complete = run.weights.len();
    for level in 0..=MAX_LEVEL {
        let h = 0.5f64.powi(level as i32);
        let (first, stride) = if level == 0 { (1, 1) } else { (1, 2) };
        let stopped = matches!(run.side(h, first, stride, true, tails.1)?, Err(Stop::Cap))
            || matches!(run.side(h, first, stride, false, tails.0)?, Err(Stop::Cap));
        if stopped {
            break;
        }
        complete = run.weights.len();
        let (value, l1) = (h * run.raw_sum, h * run.raw_l1);
        let error = previous.map_or(f64::INFINITY, |p| (value - p).abs());
        let within = error <= TOLERANCE * value.abs().max(l1) + FLOOR * f64::EPSILON * l1;
        best = Quadrature { value, error, l1, converged: false, level, coefficients: Vec::new() };
        if within && agreed && level >= MIN_LEVEL {
            best.converged = true;
            break;
        }
        agreed = within;
        previous = Some(value);
    }
    let step = 0.5f64.powi(best.level as i32);
    best.coefficients = run.weights.iter().enumerate().map(|(i, w)| if i < complete { step * w } else { 0.0 }).collect();
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINITE: (Tail, Tail) = (Tail::Underflow, Tail::Underflow);

    fn integrate(mut f: impl FnMut(f64) -> f64, a: f64, b: f64) -> Quadrature {
        tanh_sinh(&mut |x| Ok(f(x)), a, b, FINITE).unwrap()
    }

    /// Converged, with a relative error of at most `rel` against the closed form `expected`.
    fn assert_converges(q: &Quadrature, expected: f64, rel: f64, what: &str) {
        assert!(q.converged, "{what}: not converged, estimate {} error {:e} level {}", q.value, q.error, q.level);
        let error = (q.value - expected).abs() / expected.abs();
        assert!(error <= rel, "{what}: {} vs {expected}, relative error {error:e}", q.value);
        assert!(q.level >= MIN_LEVEL, "{what}: accepted at level {}", q.level);
    }

    #[test]
    fn integrable_singularities_at_zero_reach_full_precision() {
        assert_converges(&integrate(|x| 1.0 / x.sqrt(), 0.0, 1.0), 2.0, 1e-12, "x^-1/2");
        assert_converges(&integrate(|x| x.ln(), 0.0, 1.0), -1.0, 1e-12, "ln x");
        assert_converges(&integrate(|x| x.powf(-0.9), 0.0, 1.0), 10.0, 1e-12, "x^-0.9");
    }

    #[test]
    fn smooth_integrands_reach_full_precision() {
        assert_converges(&integrate(|x| (-x * x).exp(), 0.0, 1.0), 0.746_824_132_812_427, 1e-12, "exp(-x^2)");
        assert_converges(&integrate(f64::sin, 0.0, std::f64::consts::PI), 2.0, 1e-12, "sin");
        assert_converges(&integrate(|x| x * x, 2.0, 5.0), 39.0, 1e-12, "x^2 over [2, 5]");
    }

    #[test]
    fn a_tiny_integral_earns_its_digits_relative_to_its_own_size() {
        // 1e-18 / 3: an absolute floor of 1e-15 would have accepted any level, including level 1
        let q = integrate(|x| x * x, 0.0, 1e-6);
        assert_converges(&q, 1e-18 / 3.0, 1e-12, "x^2 over [0, 1e-6]");
    }

    #[test]
    fn an_oscillating_integrand_is_judged_against_its_l1_not_its_small_integral() {
        assert_converges(&integrate(|x| (100.0 * x).sin(), 0.0, 1.0), (1.0 - 100f64.cos()) / 100.0, 1e-11, "sin(100 x)");
        assert_converges(&integrate(|x| (1000.0 * x).sin(), 0.0, 1.0), (1.0 - 1000f64.cos()) / 1000.0, 1e-9, "sin(1000 x)");
    }

    /// Not converged: a documented refusal, with the estimate it stalled at.
    fn assert_refused(q: &Quadrature, what: &str) {
        assert!(!q.converged, "{what}: accepted {} (error estimate {:e})", q.value, q.error);
    }

    #[test]
    fn what_the_rule_cannot_resolve_is_not_accepted() {
        // measured: each stalls at the cap with the estimate and relative error noted
        // a singularity at a non-zero end: 1 - dist cannot come closer to 1 than one ulp (1.99999998, 7.7e-9)
        assert_refused(&integrate(|x| 1.0 / (1.0 - x).sqrt(), 0.0, 1.0), "(1 - x)^-1/2");
        // (9.743, 2.6e-2)
        assert_refused(&integrate(|x| (x - 1.0).powf(-0.9), 1.0, 2.0), "(x - 1)^-0.9 on [1, 2]");
        // a jump at 0.3 (0.69988, 1.7e-4) and a kink at 1/3 (2.8e-1, 1.4e-8)
        assert_refused(&integrate(|x| (x + 0.7).floor(), 0.0, 1.0), "floor(x + 0.7)");
        assert_refused(&integrate(|x| (x - 1.0 / 3.0).abs(), 0.0, 1.0), "|x - 1/3|");
        // an oscillation that never settles: before the cap, 1/x overflows next to 0 and sin(inf) is NaN
        match tanh_sinh(&mut |x| Ok((1.0 / x).sin()), 0.0, 1.0, FINITE) {
            Ok(q) => assert_refused(&q, "sin(1/x)"),
            Err(e) => assert!(e.to_string().contains("not finite"), "{e}"),
        }
    }

    #[test]
    fn jumps_placed_symmetrically_cancel_and_converge() {
        // floor(3 x) jumps at 1/3 and 2/3, mirror images about the midpoint, where the nodes are mirror images too:
        // the errors of the two jumps cancel and the rule converges to the right value (1)
        assert_converges(&integrate(|x| (3.0 * x).floor(), 0.0, 1.0), 1.0, 1e-12, "floor(3 x)");
    }

    #[test]
    fn a_peak_narrower_than_the_first_levels_spacing_is_missed() {
        // the documented failure mode: a Gaussian of width 1e-4 at 0.3 is never sampled where it is not 0, every
        // level agrees on 0, and 0 is accepted. If this starts failing the rule got better: update the help of
        // `integral` (the LSP hover, and "Definite integrals" in PHYSURE_LANGUAGE_GUIDE.md and the `phs doc` generator).
        let q = integrate(|x| (-((x - 0.3) / 1e-4).powi(2)).exp(), 0.0, 1.0);
        assert!(q.converged && q.value == 0.0, "{} {}", q.converged, q.value);
        // a width of 1e-2 is resolved (level 9, 1.77245385090551606e-2 = 1e-2 sqrt(pi) to the last digit)
        let q = integrate(|x| (-((x - 0.3) / 1e-2).powi(2)).exp(), 0.0, 1.0);
        assert_converges(&q, 1e-2 * std::f64::consts::PI.sqrt(), 1e-12, "a Gaussian of width 1e-2");
    }

    #[test]
    fn a_sample_that_is_not_finite_is_an_error() {
        // the midpoint of [-1, 1] is a node
        let error = tanh_sinh(&mut |x| Ok(1.0 / (x * x)), -1.0, 1.0, FINITE).err().expect("an error");
        assert!(error.to_string().contains("not finite"), "{error}");
        let error = tanh_sinh(&mut |_| Ok(f64::NAN), 0.0, 1.0, FINITE).err().expect("an error");
        assert!(error.to_string().contains("not finite"), "{error}");
    }

    #[test]
    fn the_evaluation_count_is_capped() {
        let q = integrate(|x| (x - 1.0 / 3.0).abs(), 0.0, 1.0);
        assert!(!q.converged && q.evaluations() == MAX_EVALUATIONS, "{}", q.evaluations());
    }

    #[test]
    fn the_coefficients_reproduce_the_value() {
        let mut samples = Vec::new();
        let q = tanh_sinh(
            &mut |x| {
                let v = (-x * x).exp();
                samples.push(v);
                Ok(v)
            },
            0.0,
            1.0,
            FINITE,
        )
        .unwrap();
        let sum: f64 = q.coefficients.iter().zip(&samples).map(|(c, f)| c * f).sum();
        assert!((sum - q.value).abs() <= 1e-15, "{sum} vs {}", q.value);
    }
}
