//! Asymmetric uncertainties, carried as moments.
//!
//! An experimenter who writes `12.3 +0.5 -0.4 pb` is describing a distribution that is not
//! symmetric about the quoted value, and the two numbers are not two standard deviations —
//! they are the parameters of a shape. Averaging them, which is what a symmetric model has to
//! do, throws away the asymmetry, and the asymmetry is often the interesting part.
//!
//! The shape used here is the *dimidiated* Gaussian: two half-Gaussians of widths σ⁻ and σ⁺
//! joined at the mode. It is the reading most physicists intend by that notation, and its
//! first three moments have a closed form, so the pair converts to moments and back without a
//! fit. Moments are what propagation needs, because for a linear combination they simply add:
//! the mean, the variance and the third central moment of `Σ cₖ xₖ` are `Σ cₖ μₖ`, `Σ cₖ² σₖ²`
//! and `Σ cₖ³ μ₃ₖ` over independent sources. The `cₖ` are exactly what [`Lineage`] already
//! records, so correlated sources cancel here for the same reason they cancel for σ.
//!
//! See R. Barlow, *Asymmetric Errors*, PHYSTAT 2003, for the conventions.

use std::f64::consts::{FRAC_2_PI, PI};

use super::lineage::Lineage;
use super::trait_def::UncertaintyBackend;
use crate::error::{PhysureError, PhysureResult};

/// The largest σ⁺/σ⁻ the inverse map searches over.
///
/// Skewness rises towards a finite asymptote as the ratio grows, so the last stretch before it
/// costs unbounded ratio for no accuracy. Past this point the one-sided closed form below is
/// both cheaper and more accurate than continuing the search.
const RATIO_MAX: f64 = 1.0e12;

/// The first three moments of an asymmetric measurement.
///
/// `variance` and `third` are central — taken about the mean, not about the quoted value.
/// `shift` is what separates the two, and it is why the distinction matters: a skewed
/// measurement's mean is not the number the experimenter wrote down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AsymmetricMoments {
    /// `mean − mode`: how far the mean sits from the quoted value.
    pub shift: f64,
    /// The variance about the mean.
    pub variance: f64,
    /// The third central moment about the mean. Zero for any symmetric distribution.
    pub third: f64,
}

impl AsymmetricMoments {
    /// The moments of an exact value.
    pub fn exact() -> Self {
        AsymmetricMoments { shift: 0.0, variance: 0.0, third: 0.0 }
    }

    /// The standard deviation, i.e. what a symmetric model would report on its own.
    pub fn std_dev(&self) -> f64 {
        self.variance.max(0.0).sqrt()
    }

    /// The dimensionless skewness `μ₃ / σ³`, or zero for an exact value.
    pub fn skewness(&self) -> f64 {
        if self.variance <= 0.0 { 0.0 } else { self.third / self.variance.powf(1.5) }
    }

    /// The `(σ⁻, σ⁺)` pair that reproduces these moments. See [`sigmas_from_moments`].
    pub fn sigmas(&self) -> PhysureResult<(f64, f64)> {
        sigmas_from_moments(self.variance, self.third)
    }
}

fn k() -> f64 {
    FRAC_2_PI.sqrt()
}

/// The moments of the dimidiated Gaussian with half-widths `sigma_minus` and `sigma_plus`.
///
/// Both widths measure outwards from the mode, so both are non-negative; a negative one is a
/// sign error at the call site rather than a distribution, and is rejected.
///
/// ```
/// use physure_core::uncertainty::moments::moments_from_sigmas;
/// // Equal halves are an ordinary Gaussian: no shift, no skew.
/// let m = moments_from_sigmas(0.5, 0.5).unwrap();
/// assert_eq!(m.shift, 0.0);
/// assert_eq!(m.third, 0.0);
/// assert!((m.variance - 0.25).abs() < 1e-12);
/// ```
pub fn moments_from_sigmas(sigma_minus: f64, sigma_plus: f64) -> PhysureResult<AsymmetricMoments> {
    if !sigma_minus.is_finite() || !sigma_plus.is_finite() || sigma_minus < 0.0 || sigma_plus < 0.0 {
        return Err(PhysureError::Generic(format!(
            "An asymmetric uncertainty needs two finite non-negative half-widths measured outwards \
             from the value, got ({}, {})",
            sigma_minus, sigma_plus
        )));
    }
    let d = sigma_plus - sigma_minus;
    let product = sigma_plus * sigma_minus;
    Ok(AsymmetricMoments {
        shift: k() * d,
        variance: product + (1.0 - FRAC_2_PI) * d * d,
        third: k() * d * (product + (4.0 / PI - 1.0) * d * d),
    })
}

/// The skewness of a dimidiated Gaussian with `sigma_plus / sigma_minus == ratio`.
///
/// Scale-free, which is what makes the inverse map a one-dimensional search: the shape is
/// fixed by the ratio alone, and the variance only sets how wide it is afterwards.
fn skewness_of_ratio(ratio: f64) -> f64 {
    let d = ratio - 1.0;
    let variance = ratio + (1.0 - FRAC_2_PI) * d * d;
    k() * d * (ratio + (4.0 / PI - 1.0) * d * d) / variance.powf(1.5)
}

/// The largest skewness a dimidiated Gaussian can have, reached in the one-sided limit σ⁻ → 0.
///
/// It is a little under 1, so a distribution more skewed than this cannot be written as a pair
/// of half-widths at all. That is a property of the shape, not a limitation of the search, and
/// [`sigmas_from_moments`] reports it rather than returning the closest thing it can find.
pub fn max_skewness() -> f64 {
    k() * (4.0 / PI - 1.0) / (1.0 - FRAC_2_PI).powf(1.5)
}

/// The `(σ⁻, σ⁺)` pair whose dimidiated Gaussian has this variance and third moment.
///
/// The inverse of [`moments_from_sigmas`]. Only two of the three moments are needed: the pair
/// fixes the shape and the width, and `shift` then says where the mode sits relative to the
/// mean, so recovering it from the pair is exact rather than a third constraint to satisfy.
///
/// The ratio σ⁺/σ⁻ is found by bisection on the skewness, which is monotonic in it, and the
/// variance scales the result. A skewness beyond [`max_skewness`] is not representable and is
/// rejected — silently returning the most skewed shape available would understate the tail by
/// an unbounded amount.
///
/// ```
/// use physure_core::uncertainty::moments::{moments_from_sigmas, sigmas_from_moments};
/// let m = moments_from_sigmas(0.4, 0.5).unwrap();
/// let (lo, hi) = sigmas_from_moments(m.variance, m.third).unwrap();
/// assert!((lo - 0.4).abs() < 1e-9 && (hi - 0.5).abs() < 1e-9);
/// ```
pub fn sigmas_from_moments(variance: f64, third: f64) -> PhysureResult<(f64, f64)> {
    if !variance.is_finite() || !third.is_finite() || variance < 0.0 {
        return Err(PhysureError::Generic(format!(
            "Cannot recover half-widths from a non-finite or negative variance: ({}, {})",
            variance, third
        )));
    }
    if variance == 0.0 {
        // No spread at all leaves nothing for a third moment to describe.
        return if third == 0.0 {
            Ok((0.0, 0.0))
        } else {
            Err(PhysureError::Generic(format!(
                "A zero variance cannot carry a third moment of {}",
                third
            )))
        };
    }

    let skew = third / variance.powf(1.5);
    let target = skew.abs();
    if target > max_skewness() {
        return Err(PhysureError::Generic(format!(
            "A skewness of {:.4} cannot be written as a pair of half-widths: the dimidiated \
             Gaussian tops out at {:.4}, reached only when one side is zero",
            skew,
            max_skewness()
        )));
    }

    let ratio = if target >= skewness_of_ratio(RATIO_MAX) {
        // The one-sided limit, where the closed form is exact and the search is not.
        f64::INFINITY
    } else {
        let (mut lo, mut hi) = (1.0, RATIO_MAX);
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if skewness_of_ratio(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    };

    let (narrow, wide) = if ratio.is_finite() {
        let d = ratio - 1.0;
        let narrow = (variance / (ratio + (1.0 - FRAC_2_PI) * d * d)).sqrt();
        (narrow, ratio * narrow)
    } else {
        (0.0, (variance / (1.0 - FRAC_2_PI)).sqrt())
    };

    Ok(if skew < 0.0 { (wide, narrow) } else { (narrow, wide) })
}

/// A measured asymmetric value, held as the moments a propagation model needs.
///
/// A container, not a model. It carries the mean, the spread with its provenance, and the
/// third moment, and stops there. How a third moment combines when two values meet — what a
/// shared source does to the skew, what a jacobian does to it — is the propagation design, and
/// it is deliberately left open here rather than settled by the first thing that compiles.
#[derive(Clone, Debug)]
pub struct MomentsBackend {
    /// The mean, not the quoted value. See [`MomentsBackend::mode`].
    pub mean: f64,
    /// The standard deviation and its provenance, exactly as the Gaussian backend holds it.
    pub sigma: Lineage,
    /// The third central moment about the mean.
    pub third: f64,
}

impl MomentsBackend {
    /// A newly measured asymmetric quantity, quoted as `value +sigma_plus -sigma_minus`.
    ///
    /// `value` is the mode — the number the experimenter wrote — so the mean lands a little to
    /// the long-tailed side of it. Each call mints a fresh source id, as measuring twice gives
    /// two independent measurements.
    pub fn measured(value: f64, sigma_minus: f64, sigma_plus: f64) -> PhysureResult<Self> {
        let m = moments_from_sigmas(sigma_minus, sigma_plus)?;
        Ok(MomentsBackend { mean: value + m.shift, sigma: Lineage::measured(m.std_dev()), third: m.third })
    }

    /// The moments of this value.
    pub fn moments(&self) -> AsymmetricMoments {
        let std_dev = self.sigma.std_dev();
        AsymmetricMoments { shift: 0.0, variance: std_dev * std_dev, third: self.third }
            .with_shift_from_sigmas()
    }

    /// The `(σ⁻, σ⁺)` pair to report this value as.
    pub fn sigmas(&self) -> PhysureResult<(f64, f64)> {
        let std_dev = self.sigma.std_dev();
        sigmas_from_moments(std_dev * std_dev, self.third)
    }

    /// The mode: the value to quote the half-widths around.
    pub fn mode(&self) -> PhysureResult<f64> {
        Ok(self.mean - self.moments().shift)
    }
}

impl AsymmetricMoments {
    /// Fills in the shift implied by the pair this variance and third moment describe.
    ///
    /// The shift is not free once the other two are fixed, so computing it here keeps a value's
    /// mode consistent with the pair it will be printed as.
    fn with_shift_from_sigmas(self) -> Self {
        match self.sigmas() {
            Ok((lo, hi)) => AsymmetricMoments { shift: k() * (hi - lo), ..self },
            // Too skewed for the pair form. The variance and third moment are still exactly
            // what they were; only the mode is unavailable, and reporting the mean as the mode
            // is the symmetric answer, which is the one that does not invent a number.
            Err(_) => self,
        }
    }
}

// TODO: moment propagation is not implemented — see the asymmetric-uncertainty work. Until it
// is, every arithmetic path refuses. Falling back to the Gaussian rules would look like it
// worked and quietly report a symmetric answer for a measurement whose whole point is that it
// is not symmetric, which is the failure this model exists to prevent.
impl UncertaintyBackend for MomentsBackend {
    fn mean(&self) -> f64 {
        self.mean
    }

    fn std_dev(&self) -> f64 {
        self.sigma.std_dev()
    }

    fn propagate_add(&self, _other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented("addition"))
    }

    fn propagate_sub(&self, _other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented("subtraction"))
    }

    fn propagate_mul(&self, _other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented("multiplication"))
    }

    fn propagate_div(&self, _other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented("division"))
    }

    fn propagate_pow(&self, _exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented("exponentiation"))
    }

    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Err(not_implemented(func))
    }

    fn get_model_name(&self) -> &str {
        "moments"
    }
}

pub(super) fn not_implemented(op: &str) -> PhysureError {
    PhysureError::Generic(format!(
        "Asymmetric uncertainties can be measured and reported but not yet propagated: {} is \
         not implemented for the moments model",
        op
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(sigma_minus: f64, sigma_plus: f64) {
        let m = moments_from_sigmas(sigma_minus, sigma_plus).unwrap();
        let (lo, hi) = m.sigmas().unwrap();
        assert!(
            (lo - sigma_minus).abs() < 1e-9 && (hi - sigma_plus).abs() < 1e-9,
            "({}, {}) came back as ({}, {})",
            sigma_minus,
            sigma_plus,
            lo,
            hi
        );
    }

    #[test]
    fn equal_halves_are_an_ordinary_gaussian() {
        let m = moments_from_sigmas(0.3, 0.3).unwrap();
        assert_eq!(m.shift, 0.0);
        assert_eq!(m.third, 0.0);
        assert!((m.std_dev() - 0.3).abs() < 1e-15);
    }

    #[test]
    fn the_mean_leans_towards_the_long_tail() {
        let up = moments_from_sigmas(0.4, 0.5).unwrap();
        let down = moments_from_sigmas(0.5, 0.4).unwrap();
        assert!(up.shift > 0.0 && up.third > 0.0);
        assert!(down.shift < 0.0 && down.third < 0.0);
        // Mirroring the pair mirrors the distribution, so only the odd moments change sign.
        assert!((up.variance - down.variance).abs() < 1e-15);
        assert!((up.shift + down.shift).abs() < 1e-15);
        assert!((up.third + down.third).abs() < 1e-15);
    }

    #[test]
    fn the_pair_survives_the_trip_through_moments() {
        for &(lo, hi) in &[(0.4, 0.5), (0.5, 0.4), (0.1, 1.0), (1.0, 0.1), (2.0, 2.0), (0.0, 0.0)] {
            round_trip(lo, hi);
        }
    }

    #[test]
    fn a_one_sided_measurement_is_the_most_skewed_shape_there_is() {
        let m = moments_from_sigmas(0.0, 1.0).unwrap();
        assert!((m.skewness() - max_skewness()).abs() < 1e-12);
        let (lo, hi) = m.sigmas().unwrap();
        assert!(lo.abs() < 1e-9 && (hi - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_skew_no_pair_can_reach_is_refused() {
        // Just past the asymptote — a real distribution, but not one of these.
        let err = sigmas_from_moments(1.0, max_skewness() + 0.01).unwrap_err();
        assert!(err.to_string().contains("cannot be written as a pair"), "{}", err);
    }

    #[test]
    fn skewness_only_depends_on_the_ratio() {
        let small = moments_from_sigmas(0.04, 0.05).unwrap();
        let large = moments_from_sigmas(40.0, 50.0).unwrap();
        assert!((small.skewness() - large.skewness()).abs() < 1e-12);
    }

    #[test]
    fn a_negative_half_width_is_a_sign_error() {
        assert!(moments_from_sigmas(-0.1, 0.5).is_err());
        assert!(sigmas_from_moments(-1.0, 0.0).is_err());
        assert!(sigmas_from_moments(0.0, 0.5).is_err());
    }

    #[test]
    fn a_measured_value_keeps_its_provenance_like_any_other() {
        // The lineage half is already settled, so an asymmetric measurement is a source in the
        // same sense a symmetric one is: one id, cancelling against itself.
        let x = MomentsBackend::measured(12.3, 0.4, 0.5).unwrap();
        assert_eq!(x.sigma.terms().len(), 1);
        assert_eq!(Lineage::combine(&x.sigma, 1.0, &x.sigma, -1.0).std_dev(), 0.0);
    }

    #[test]
    fn propagation_refuses_rather_than_answering_symmetrically() {
        let x = MomentsBackend::measured(12.3, 0.4, 0.5).unwrap();
        let y = MomentsBackend::measured(1.0, 0.4, 0.5).unwrap();
        let Err(err) = x.propagate_add(&y) else { panic!("answered instead of refusing") };
        assert!(err.to_string().contains("not yet propagated"), "{err}");
        assert!(x.propagate_pow(2.0).is_err());
        assert!(x.propagate_function("sin").is_err());
    }

    #[test]
    fn the_quoted_value_is_the_mode_not_the_mean() {
        let x = MomentsBackend::measured(12.3, 0.4, 0.5).unwrap();
        assert!(x.mean > 12.3, "the long tail is upwards, so the mean is above the mode");
        assert!((x.mode().unwrap() - 12.3).abs() < 1e-9);
        let (lo, hi) = x.sigmas().unwrap();
        assert!((lo - 0.4).abs() < 1e-9 && (hi - 0.5).abs() < 1e-9);
    }
}
