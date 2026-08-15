//! Interchangeable distribution shapes for asymmetric uncertainties.
//!
//! A quoted pair `(σ⁻, σ⁺)` only becomes three moments once a shape is chosen. The shapes
//! here differ in how the two half-Gaussians are joined:
//!
//! * [`DimidiatedGaussian`] — equal *area*: each half carries 50% of the mass, the quoted
//!   value is the median, and σ∓ are exactly the one-sigma distances (68.27% central
//!   interval). This is the "dimidiated" model of arXiv:2411.15499 §A and the default.
//! * [`FechnerGaussian`] — equal *height*: the density is continuous at the join, the quoted
//!   value sits at the σ⁻/(σ⁻+σ⁺) percentile, and σ∓ are shape parameters. Narrower
//!   reachable skewness; opt-in.
//!
//! Moments are shape-independent once created — only construction (pair → moments) and
//! read-out (moments → pair) consult the strategy.

use std::f64::consts::{FRAC_2_PI, PI};

use super::moments::AsymmetricMoments;
use crate::error::{PhysureError, PhysureResult};

/// How a `(σ⁻, σ⁺)` pair becomes a shape, and back.
///
/// [`moments_from_sigmas`](Self::moments_from_sigmas) and
/// [`sigmas_from_moments`](Self::sigmas_from_moments) are inverses of each other for a given
/// shape, but not across shapes — the same pair means a different distribution, and therefore
/// different moments, depending on which strategy reads it.
pub trait AsymmetryShape: Send + Sync {
    /// The name this shape is selected by, e.g. via [`ShapeKind::from_name`].
    fn name(&self) -> &'static str;
    /// The moments of this shape with half-widths `sigma_minus` and `sigma_plus`.
    fn moments_from_sigmas(
        &self,
        sigma_minus: f64,
        sigma_plus: f64,
    ) -> PhysureResult<AsymmetricMoments>;
    /// The `(σ⁻, σ⁺)` pair whose instance of this shape has this variance and third moment.
    fn sigmas_from_moments(&self, variance: f64, third: f64) -> PhysureResult<(f64, f64)>;
    /// The largest standardised skewness this shape can represent as a pair of half-widths.
    fn max_skewness(&self) -> f64;
}

/// Rejects the inputs no shape can turn into a distribution: a half-width has to be a real,
/// non-negative distance, since it is measured outwards from the mode.
fn check_pair(sigma_minus: f64, sigma_plus: f64) -> PhysureResult<()> {
    if !sigma_minus.is_finite() || !sigma_plus.is_finite() || sigma_minus < 0.0 || sigma_plus < 0.0
    {
        return Err(PhysureError::Generic(format!(
            "An asymmetric uncertainty needs two finite non-negative half-widths, \
             got ({sigma_minus}, {sigma_plus})"
        )));
    }
    Ok(())
}

/// Equal-area: `k = 1/sqrt(2*pi)`, the ordinate of the standard normal at its mode. Each half
/// carries half the standard normal's mass, so joining two rescaled halves there needs no
/// further normalisation.
const K_DIMIDIATED: f64 = 0.398_942_280_401_432_7;
/// One-sided limit of the standardised skewness, reached at `sigma_minus == 0`.
const DIMIDIATED_MAX_SKEW: f64 = 1.640_560_926_866_267;

/// The equal-area split normal: two half-Gaussians of widths σ⁻ and σ⁺, each scaled so it
/// carries exactly half the total probability. The quoted value is the median, and σ∓ are the
/// ordinary one-sigma (68.27%) distances on either side of it — the reading the "dimidiated"
/// name is used for in arXiv:2411.15499 §A, and the default shape in this crate.
pub struct DimidiatedGaussian;

impl DimidiatedGaussian {
    /// Standardised skewness of the unit-scale shape `σ∓ = 1 ∓ a`. Monotonic on `(-1, 1)`, so
    /// it is what [`AsymmetryShape::sigmas_from_moments`] bisects on: the ratio of the widths
    /// fixes the shape, and only the shape (not the scale) determines skewness.
    fn standardised_skew(a: f64) -> f64 {
        let (_, v, g) = Self::raw_moments(1.0 - a, 1.0 + a);
        g / v.powf(1.5)
    }

    /// `(shift, variance, third)` about the mean for half-widths `(sm, sp)`.
    ///
    /// Each half is a truncated Gaussian rescaled by `sm` or `sp`, joined at the mode with
    /// equal area on each side. `shift` is the mean's distance from the mode (the join point);
    /// `variance` and `third` are then central moments about that mean, found by shifting the
    /// raw second and third moments about the mode.
    fn raw_moments(sm: f64, sp: f64) -> (f64, f64, f64) {
        let shift = K_DIMIDIATED * (sp - sm);
        let m2 = (sm * sm + sp * sp) / 2.0;
        let m3 = 2.0 * K_DIMIDIATED * (sp.powi(3) - sm.powi(3));
        (shift, m2 - shift * shift, m3 - 3.0 * shift * m2 + 2.0 * shift.powi(3))
    }
}

impl AsymmetryShape for DimidiatedGaussian {
    fn name(&self) -> &'static str {
        "dimidiated"
    }

    fn moments_from_sigmas(
        &self,
        sigma_minus: f64,
        sigma_plus: f64,
    ) -> PhysureResult<AsymmetricMoments> {
        check_pair(sigma_minus, sigma_plus)?;
        let (shift, variance, third) = Self::raw_moments(sigma_minus, sigma_plus);
        Ok(AsymmetricMoments { shift, variance, third })
    }

    /// The ratio `sigma_plus / sigma_minus` is found by bisecting the standardised skewness,
    /// which is monotonic in the shape parameter `a` above, then the variance sets the scale.
    /// A skewness beyond [`DimidiatedGaussian::max_skewness`] has no `(sigma-, sigma+)` form at
    /// all under this shape and is reported rather than clamped to the nearest reachable one.
    fn sigmas_from_moments(&self, variance: f64, third: f64) -> PhysureResult<(f64, f64)> {
        if !variance.is_finite() || !third.is_finite() || variance < 0.0 {
            return Err(PhysureError::Generic(format!(
                "Cannot recover half-widths from a non-finite or negative variance: \
                 ({variance}, {third})"
            )));
        }
        if variance == 0.0 {
            return if third == 0.0 {
                Ok((0.0, 0.0))
            } else {
                Err(PhysureError::Generic(
                    "A zero-variance value cannot carry a third moment".into(),
                ))
            };
        }
        let target = third / variance.powf(1.5);
        // A one-sided pair (sigma_minus == 0) sits exactly at DIMIDIATED_MAX_SKEW, but `target`
        // is computed, not the literal constant, so it can land a few ulps above it at some
        // scales even though it is not genuinely more skewed. A few ulps of slack (~9e-16
        // relative) accepts that noise without opening the door to real over-skew, which misses
        // by orders of magnitude in comparison (see skewness_ceilings_differ_and_are_enforced).
        if target.abs() > DIMIDIATED_MAX_SKEW * (1.0 + 4.0 * f64::EPSILON) {
            return Err(PhysureError::Generic(format!(
                "A skewness of {target:.4} cannot be written as a pair of half-widths: the \
                 dimidiated Gaussian tops out at {DIMIDIATED_MAX_SKEW:.4}, reached only when \
                 one side is zero"
            )));
        }
        let (mut lo, mut hi) = (-1.0_f64, 1.0_f64);
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if Self::standardised_skew(mid) < target {
                lo = mid
            } else {
                hi = mid
            }
        }
        let a = 0.5 * (lo + hi);
        let shape_variance = Self::raw_moments(1.0 - a, 1.0 + a).1;
        let s = (variance / shape_variance).sqrt();
        Ok((s * (1.0 - a), s * (1.0 + a)))
    }

    fn max_skewness(&self) -> f64 {
        DIMIDIATED_MAX_SKEW
    }
}

/// The equal-height split normal: two half-Gaussians of widths σ⁻ and σ⁺ joined so the density
/// itself, not the area, is continuous at the mode. The quoted value then sits at the
/// `σ⁻/(σ⁻+σ⁺)` percentile rather than the median, and σ∓ are shape parameters rather than
/// literal one-sigma distances. This is the "Fechner" name in arXiv:2411.15499 §A — an opt-in
/// alternative to the default [`DimidiatedGaussian`], with a narrower reachable skewness.
pub struct FechnerGaussian;

impl FechnerGaussian {
    /// The largest σ⁺/σ⁻ the inverse map searches over.
    ///
    /// Skewness rises towards a finite asymptote as the ratio grows, so the last stretch before
    /// it costs unbounded ratio for no accuracy. Past this point the one-sided closed form
    /// below is both cheaper and more accurate than continuing the search.
    const RATIO_MAX: f64 = 1.0e12;

    fn k() -> f64 {
        FRAC_2_PI.sqrt()
    }

    /// The skewness of an equal-height split normal with `sigma_plus / sigma_minus == ratio`.
    ///
    /// Scale-free, which is what makes the inverse map a one-dimensional search: the shape is
    /// fixed by the ratio alone, and the variance only sets how wide it is afterwards.
    fn skewness_of_ratio(ratio: f64) -> f64 {
        let d = ratio - 1.0;
        let variance = ratio + (1.0 - FRAC_2_PI) * d * d;
        Self::k() * d * (ratio + (4.0 / PI - 1.0) * d * d) / variance.powf(1.5)
    }
}

impl AsymmetryShape for FechnerGaussian {
    fn name(&self) -> &'static str {
        "fechner"
    }

    fn moments_from_sigmas(
        &self,
        sigma_minus: f64,
        sigma_plus: f64,
    ) -> PhysureResult<AsymmetricMoments> {
        check_pair(sigma_minus, sigma_plus)?;
        let d = sigma_plus - sigma_minus;
        let product = sigma_plus * sigma_minus;
        Ok(AsymmetricMoments {
            shift: Self::k() * d,
            variance: product + (1.0 - FRAC_2_PI) * d * d,
            third: Self::k() * d * (product + (4.0 / PI - 1.0) * d * d),
        })
    }

    /// The ratio σ⁺/σ⁻ is found by bisecting the skewness, which is monotonic in it, up to
    /// `RATIO_MAX`; past that the one-sided closed form (σ⁻ = 0) takes over, since the search
    /// would need an unbounded ratio to close the last gap. A skewness beyond
    /// [`FechnerGaussian::max_skewness`] has no `(sigma-, sigma+)` form under this shape.
    fn sigmas_from_moments(&self, variance: f64, third: f64) -> PhysureResult<(f64, f64)> {
        if !variance.is_finite() || !third.is_finite() || variance < 0.0 {
            return Err(PhysureError::Generic(format!(
                "Cannot recover half-widths from a non-finite or negative variance: \
                 ({variance}, {third})"
            )));
        }
        if variance == 0.0 {
            return if third == 0.0 {
                Ok((0.0, 0.0))
            } else {
                Err(PhysureError::Generic(format!(
                    "A zero variance cannot carry a third moment of {third}"
                )))
            };
        }

        let skew = third / variance.powf(1.5);
        let target = skew.abs();
        let max_skew = self.max_skewness();
        // Same few-ulp slack as DimidiatedGaussian, and for the same reason: a one-sided pair
        // sits exactly at max_skew, and the computed value can land a few ulps past the
        // computed constant at some scales without being genuinely more skewed.
        if target > max_skew * (1.0 + 4.0 * f64::EPSILON) {
            return Err(PhysureError::Generic(format!(
                "A skewness of {skew:.4} cannot be written as a pair of half-widths: the \
                 Fechner Gaussian tops out at {max_skew:.4}, reached only when one side is zero"
            )));
        }

        let ratio = if target >= Self::skewness_of_ratio(Self::RATIO_MAX) {
            // The one-sided limit, where the closed form is exact and the search is not.
            f64::INFINITY
        } else {
            let (mut lo, mut hi) = (1.0, Self::RATIO_MAX);
            for _ in 0..100 {
                let mid = 0.5 * (lo + hi);
                if Self::skewness_of_ratio(mid) < target {
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

    fn max_skewness(&self) -> f64 {
        Self::k() * (4.0 / PI - 1.0) / (1.0 - FRAC_2_PI).powf(1.5)
    }
}

/// Which [`AsymmetryShape`] to use, as a value that can be stored, compared, and named — the
/// trait itself is not `Copy`/`Eq` and its impls are zero-sized singletons, so this is what a
/// config field or a serialized attribute actually holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    Dimidiated,
    Fechner,
}

impl ShapeKind {
    /// The shape a bare `(σ⁻, σ⁺)` pair means when nothing has chosen one explicitly — the
    /// equal-area dimidiated Gaussian, the reading most physicists intend by the notation.
    pub const DEFAULT: ShapeKind = ShapeKind::Dimidiated;

    /// The strategy this variant names, as a `'static` reference to its zero-sized singleton.
    pub fn strategy(self) -> &'static dyn AsymmetryShape {
        match self {
            ShapeKind::Dimidiated => &DimidiatedGaussian,
            ShapeKind::Fechner => &FechnerGaussian,
        }
    }

    /// Equivalent to `self.strategy().name()`, for callers that only need the label.
    pub fn name(self) -> &'static str {
        self.strategy().name()
    }

    /// Parses the name a shape is selected by (case-insensitively), the inverse of
    /// [`ShapeKind::name`].
    pub fn from_name(name: &str) -> PhysureResult<ShapeKind> {
        match name.to_ascii_lowercase().as_str() {
            "dimidiated" => Ok(ShapeKind::Dimidiated),
            "fechner" => Ok(ShapeKind::Fechner),
            other => Err(PhysureError::Generic(format!(
                "Unknown asymmetry shape {other:?}. Expected one of: dimidiated, fechner."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimidiated_matches_reference_values() {
        let m = DimidiatedGaussian.moments_from_sigmas(1.0, 2.0).unwrap();
        assert!((m.shift - 0.398_942_280_401_432_7).abs() < 1e-15);
        assert!((m.variance - 2.340_845_056_908_104_7).abs() < 1e-15);
        assert!((m.third - 2.720_112_094_477_794_3).abs() < 1e-15);
    }

    #[test]
    fn dimidiated_symmetric_input_is_unskewed() {
        let m = DimidiatedGaussian.moments_from_sigmas(0.5, 0.5).unwrap();
        assert_eq!(m.shift, 0.0);
        assert!((m.variance - 0.25).abs() < 1e-15);
        assert_eq!(m.third, 0.0);
    }

    #[test]
    fn both_shapes_round_trip() {
        for shape in [&DimidiatedGaussian as &dyn AsymmetryShape, &FechnerGaussian] {
            for (sm, sp) in [(1.0, 1.0), (1.0, 2.0), (0.5, 1.5), (2.0, 0.7), (0.01, 1.0)] {
                let m = shape.moments_from_sigmas(sm, sp).unwrap();
                let (lo, hi) = shape
                    .sigmas_from_moments(m.variance, m.third)
                    .unwrap_or_else(|e| panic!("{} ({sm},{sp}): {e}", shape.name()));
                assert!((lo - sm).abs() < 1e-8, "{} sigma_minus {lo}", shape.name());
                assert!((hi - sp).abs() < 1e-8, "{} sigma_plus {hi}", shape.name());
            }
        }
    }

    #[test]
    fn skewness_ceilings_differ_and_are_enforced() {
        assert!((DimidiatedGaussian.max_skewness() - 1.640_560_926_866_267).abs() < 1e-9);
        assert!((FechnerGaussian.max_skewness() - 0.995_272).abs() < 1e-5);
        // Beyond a shape's ceiling the pair form does not exist; report, don't approximate.
        let too_skewed = 1.7_f64;
        assert!(DimidiatedGaussian.sigmas_from_moments(1.0, too_skewed).is_err());
    }

    #[test]
    fn negative_or_non_finite_half_widths_are_rejected_by_both() {
        for shape in [&DimidiatedGaussian as &dyn AsymmetryShape, &FechnerGaussian] {
            assert!(shape.moments_from_sigmas(-1.0, 1.0).is_err());
            assert!(shape.moments_from_sigmas(1.0, f64::NAN).is_err());
        }
    }

    #[test]
    fn shape_kind_maps_names_and_default() {
        assert_eq!(ShapeKind::DEFAULT, ShapeKind::Dimidiated);
        assert_eq!(ShapeKind::from_name("dimidiated").unwrap(), ShapeKind::Dimidiated);
        assert_eq!(ShapeKind::from_name("fechner").unwrap(), ShapeKind::Fechner);
        assert!(ShapeKind::from_name("gauss-ish").is_err());
        assert_eq!(ShapeKind::Dimidiated.strategy().name(), "dimidiated");
        assert_eq!(ShapeKind::Fechner.name(), "fechner");
    }

    #[test]
    fn one_sided_pairs_round_trip_across_scales() {
        // A one-sided pair (sigma_minus == 0) sits exactly at each shape's skewness ceiling, so
        // it is the case most exposed to the ulp-level float noise the ceiling check now
        // tolerates. sigma_plus == 1.0 alone (as in both_shapes_round_trip) happens to be a
        // scale where the noise favours acceptance; this regression sweeps decades on both
        // sides of it, several of which used to be spuriously rejected.
        for shape in [&DimidiatedGaussian as &dyn AsymmetryShape, &FechnerGaussian] {
            for sp in [1e-9, 1e-4, 1.0, 1e2, 1e9] {
                let m = shape
                    .moments_from_sigmas(0.0, sp)
                    .unwrap_or_else(|e| panic!("{} (0,{sp}): {e}", shape.name()));
                let (lo, hi) = shape
                    .sigmas_from_moments(m.variance, m.third)
                    .unwrap_or_else(|e| panic!("{} (0,{sp}): {e}", shape.name()));
                assert!(lo.abs() < sp * 1e-6, "{} sigma_minus {lo} at scale {sp}", shape.name());
                assert!(
                    (hi - sp).abs() < sp * 1e-6,
                    "{} sigma_plus {hi} at scale {sp}",
                    shape.name()
                );
            }
        }
    }
}
