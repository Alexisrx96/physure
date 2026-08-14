//! Asymmetric uncertainties, carried as moments.
//!
//! An experimenter who writes `12.3 +0.5 -0.4 pb` is describing a distribution that is not
//! symmetric about the quoted value, and the two numbers are not two standard deviations —
//! they are the parameters of a shape. Averaging them, which is what a symmetric model has to
//! do, throws away the asymmetry, and the asymmetry is often the interesting part.
//!
//! Turning a pair into a shape is a choice, made by [`AsymmetryShape`](super::shapes::AsymmetryShape). The default is
//! the *dimidiated* Gaussian — two half-Gaussians of widths σ⁻ and σ⁺ joined so each carries
//! equal area, with the quoted value at the median — because that is the reading most
//! physicists intend by the notation. The *Fechner* (equal-height) Gaussian is available as an
//! opt-in alternative. Both names and both closed forms are in arXiv:2411.15499 §A. Either way
//! the pair converts to moments and back without a fit, and moments are what propagation needs:
//! for a linear combination they simply add, so the mean, the variance and the third central
//! moment of `Σ cₖ xₖ` are `Σ cₖ μₖ`, `Σ cₖ² σₖ²` and `Σ cₖ³ μ₃ₖ` over independent sources. The
//! `cₖ` are exactly what [`Lineage`] already records, so correlated sources cancel here for the
//! same reason they cancel for σ.
//!
//! See R. Barlow, *Asymmetric Errors*, PHYSTAT 2003, for the conventions.

use super::lineage::{Lineage, fresh_id};
use super::mode::{PropagationMode, propagation_mode};
use super::shapes::ShapeKind;
use super::trait_def::UncertaintyBackend;
use crate::error::{PhysureError, PhysureResult};

/// The first three moments of an asymmetric measurement.
///
/// `variance` and `third` are central — taken about the mean, not about the quoted value.
/// `shift` is what separates the two, and it is why the distinction matters: a skewed
/// measurement's mean is not the number the experimenter wrote down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AsymmetricMoments {
    /// `mean − mode`: how far the mean sits from the quoted value. Under the default shape
    /// the join point (the mode) is also the median, so `shift` is equally the mean's distance
    /// from the median there — see [`shapes::DimidiatedGaussian`](super::shapes::DimidiatedGaussian).
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

/// The moments of the default shape ([`ShapeKind::DEFAULT`]) with half-widths `sigma_minus`
/// and `sigma_plus`.
///
/// Both widths measure outwards from the mode, so both are non-negative; a negative one is a
/// sign error at the call site rather than a distribution, and is rejected. To use a shape
/// other than the default, call [`AsymmetryShape::moments_from_sigmas`](super::shapes::AsymmetryShape::moments_from_sigmas)
/// on a [`ShapeKind::strategy`] directly.
///
/// ```
/// use physure_core::uncertainty::moments::moments_from_sigmas;
/// // Equal halves are an ordinary Gaussian under any shape: no shift, no skew.
/// let m = moments_from_sigmas(0.5, 0.5).unwrap();
/// assert_eq!(m.shift, 0.0);
/// assert_eq!(m.third, 0.0);
/// assert!((m.variance - 0.25).abs() < 1e-12);
/// ```
pub fn moments_from_sigmas(sigma_minus: f64, sigma_plus: f64) -> PhysureResult<AsymmetricMoments> {
    ShapeKind::DEFAULT.strategy().moments_from_sigmas(sigma_minus, sigma_plus)
}

/// The largest skewness the default shape ([`ShapeKind::DEFAULT`]) can represent as a pair of
/// half-widths, reached in the one-sided limit σ⁻ → 0.
///
/// A distribution more skewed than this cannot be written as a pair under the default shape at
/// all. That is a property of the shape, not a limitation of the search, and
/// [`sigmas_from_moments`] reports it rather than returning the closest thing it can find.
pub fn max_skewness() -> f64 {
    ShapeKind::DEFAULT.strategy().max_skewness()
}

/// The `(σ⁻, σ⁺)` pair whose default-shape ([`ShapeKind::DEFAULT`]) distribution has this
/// variance and third moment.
///
/// The inverse of [`moments_from_sigmas`]. Only two of the three moments are needed: the pair
/// fixes the shape and the width, and `shift` then says where the mode sits relative to the
/// mean, so recovering it from the pair is exact rather than a third constraint to satisfy. A
/// skewness beyond [`max_skewness`] is not representable and is rejected — silently returning
/// the most skewed shape available would understate the tail by an unbounded amount.
///
/// ```
/// use physure_core::uncertainty::moments::{moments_from_sigmas, sigmas_from_moments};
/// let m = moments_from_sigmas(0.4, 0.5).unwrap();
/// let (lo, hi) = sigmas_from_moments(m.variance, m.third).unwrap();
/// assert!((lo - 0.4).abs() < 1e-9 && (hi - 0.5).abs() < 1e-9);
/// ```
pub fn sigmas_from_moments(variance: f64, third: f64) -> PhysureResult<(f64, f64)> {
    ShapeKind::DEFAULT.strategy().sigmas_from_moments(variance, third)
}

/// One noise source as seen from a derived value: how sensitive the value is to it, and the
/// source's own intrinsic central moments.
///
/// The split is what makes the third moment cancel the way [`Lineage`]'s standard deviation
/// already does. Sensitivities are signed and linear, so they can be merged per source the
/// same way `Lineage` merges coefficients; the variance and third moment cannot be merged that
/// way; they only make sense multiplied by the *square* and *cube* of the merged sensitivity.
/// Folding `a^3 * mu_3` per operand instead of per source is what silently breaks `2x - x`:
/// each occurrence of `x` folds its own `a^3`, giving `2^3*mu_3 + (-1)^3*mu_3 = 7*mu_3` for a
/// value that is exactly `x` and should report `x`'s own `mu_3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceMoments {
    /// This source's contribution to the value's linear sensitivity — what `Lineage`'s
    /// coefficient is, but kept separate from the moments below so the two can be merged with
    /// different arithmetic.
    pub sensitivity: f64,
    /// The source's own variance, before the sensitivity is applied.
    pub variance: f64,
    /// The source's own third central moment, before the sensitivity is applied.
    pub third: f64,
}

/// Provenance for an asymmetric measurement, the way [`Lineage`] is provenance for a symmetric
/// one — except each term carries its source's intrinsic `(variance, third)` alongside the
/// signed sensitivity, rather than a single pre-multiplied number.
///
/// Terms are kept sorted by id, like `Lineage`, so merging is a linear walk; a term whose
/// sensitivity has cancelled to zero is dropped rather than kept at zero, so a fully cancelled
/// lineage — `x - x` — comes out with no terms rather than one dead one.
///
/// Ids are minted from [`fresh_id`], the same counter `Lineage` uses. An id therefore names
/// either a `Lineage` source or a `MomentLineage` source, never both, which is what lets a
/// symmetric value promoted into the moments world ([`MomentLineage::from_lineage`]) keep its
/// id and still cancel against itself later: there is no way for that id to collide with one a
/// `MomentLineage` minted directly, and no ambiguity about which arithmetic a shared id implies.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MomentLineage {
    terms: Vec<(u32, SourceMoments)>,
}

impl MomentLineage {
    /// A value with no uncertainty at all.
    pub fn exact() -> Self {
        Self { terms: Vec::new() }
    }

    /// A newly measured asymmetric quantity, carrying its own variance and third moment.
    /// Mints a fresh source id: measuring twice gives two independent measurements, exactly as
    /// [`Lineage::measured`] does.
    ///
    /// A source with no variance and no third moment is exact, so it gets no id — minting one
    /// would let two unrelated exact constants "cancel" against each other later.
    pub fn measured(variance: f64, third: f64) -> Self {
        Self::measured_with_id(fresh_id(), variance, third)
    }

    /// A measured quantity tied to a caller-supplied source id rather than a fresh one, the
    /// moments analogue of [`Lineage::measured_with_id`].
    pub fn measured_with_id(id: u32, variance: f64, third: f64) -> Self {
        if variance == 0.0 && third == 0.0 {
            return Self::exact();
        }
        Self { terms: vec![(id, SourceMoments { sensitivity: 1.0, variance, third })] }
    }

    /// Lifts a symmetric, lineage-tracked value into the moments world. `(id, c)` becomes
    /// `(id, {sensitivity: c, variance: 1, third: 0})`: only `sensitivity^2 * variance` and
    /// `sensitivity^3 * third` are ever read back out, so this reproduces the lineage's own
    /// variance exactly (`c^2 * 1 == c^2`) and stays skew-free (`third` is 0) — while keeping
    /// the ids, so a later reuse of the same source still cancels against this one.
    pub fn from_lineage(l: &Lineage) -> Self {
        Self {
            terms: l
                .terms()
                .iter()
                .map(|&(id, c)| (id, SourceMoments { sensitivity: c, variance: 1.0, third: 0.0 }))
                .collect(),
        }
    }

    /// Rebuilds a moments lineage from raw terms, sorting by id and dropping any term whose
    /// sensitivity is zero — the invariants the rest of this type relies on.
    pub fn from_terms(mut terms: Vec<(u32, SourceMoments)>) -> Self {
        terms.sort_by_key(|t| t.0);
        terms.retain(|t| t.1.sensitivity != 0.0);
        Self { terms }
    }

    /// The terms, sorted by id.
    pub fn terms(&self) -> &[(u32, SourceMoments)] {
        &self.terms
    }

    /// `Σ sensitivity² * variance` over the sources — the variance this lineage represents.
    pub fn variance(&self) -> f64 {
        self.terms.iter().map(|(_, s)| s.sensitivity * s.sensitivity * s.variance).sum()
    }

    /// `Σ sensitivity³ * third` over the sources — the third central moment this lineage
    /// represents. This is the sum a bare scalar accumulator cannot reproduce, because it needs
    /// the merged, signed sensitivity of each source, not a per-operand contribution.
    pub fn third(&self) -> f64 {
        self.terms.iter().map(|(_, s)| s.sensitivity.powi(3) * s.third).sum()
    }

    /// The standard deviation this lineage represents.
    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Applies a single partial derivative, as for a one-argument function. The jacobian
    /// multiplies every source's sensitivity directly, so it lands cubed in [`Self::third`] and
    /// squared in [`Self::variance`] the next time either is read — which is why negating a
    /// value flips the sign of its skew but not of its variance.
    pub fn scale(&self, jacobian: f64) -> Self {
        if jacobian == 0.0 {
            return Self::exact();
        }
        Self {
            terms: self
                .terms
                .iter()
                .map(|&(id, s)| (id, SourceMoments { sensitivity: s.sensitivity * jacobian, ..s }))
                .collect(),
        }
    }

    /// Merges two moments lineages under the chain rule, the moments analogue of
    /// [`Lineage::combine`].
    ///
    /// In `uncorrelated` mode every value is a fresh, independent measurement, mirroring
    /// [`Lineage::combine`]'s own `combine_independent` arm: the variances and third moments are
    /// folded through the jacobian powers first (`Σ j² V` and `Σ j³ μ₃`), and the result is one
    /// freshly minted source carrying them — no id from either operand survives, so nothing
    /// downstream can later cancel against them.
    ///
    /// In `correlated` mode, sensitivities are merged per source *first* — `ja * a(id) + jb *
    /// b(id)`, exactly as `Lineage::combine` merges coefficients — and only then are the merged
    /// sensitivities squared and cubed against the source's own `(variance, third)`. A shared id
    /// names the same measurement on both sides, so its intrinsic moments must already agree;
    /// that invariant is asserted in debug builds rather than silently trusted, since a
    /// violation would mean two different sources had been given the same id upstream.
    pub fn combine(a: &Self, ja: f64, b: &Self, jb: f64) -> Self {
        if propagation_mode() == PropagationMode::Uncorrelated {
            let variance = ja * ja * a.variance() + jb * jb * b.variance();
            let third = ja.powi(3) * a.third() + jb.powi(3) * b.third();
            return Self::measured(variance, third);
        }

        let (at, bt) = (a.terms(), b.terms());
        let mut out: Vec<(u32, SourceMoments)> = Vec::with_capacity(at.len() + bt.len());
        let (mut i, mut j) = (0, 0);

        fn push(out: &mut Vec<(u32, SourceMoments)>, id: u32, s: SourceMoments) {
            if s.sensitivity != 0.0 {
                out.push((id, s));
            }
        }

        while i < at.len() && j < bt.len() {
            match at[i].0.cmp(&bt[j].0) {
                std::cmp::Ordering::Less => {
                    push(&mut out, at[i].0, SourceMoments { sensitivity: at[i].1.sensitivity * ja, ..at[i].1 });
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    push(&mut out, bt[j].0, SourceMoments { sensitivity: bt[j].1.sensitivity * jb, ..bt[j].1 });
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    debug_assert_eq!(at[i].1.variance, bt[j].1.variance);
                    debug_assert_eq!(at[i].1.third, bt[j].1.third);
                    push(
                        &mut out,
                        at[i].0,
                        SourceMoments {
                            sensitivity: at[i].1.sensitivity * ja + bt[j].1.sensitivity * jb,
                            ..at[i].1
                        },
                    );
                    i += 1;
                    j += 1;
                }
            }
        }
        for &(id, s) in &at[i..] {
            push(&mut out, id, SourceMoments { sensitivity: s.sensitivity * ja, ..s });
        }
        for &(id, s) in &bt[j..] {
            push(&mut out, id, SourceMoments { sensitivity: s.sensitivity * jb, ..s });
        }

        Self { terms: out }
    }
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
    /// mode consistent with the pair it will be printed as. Recomputing it through
    /// [`moments_from_sigmas`] — the default shape's own forward map — rather than a
    /// shape-specific formula keeps this correct for whichever shape [`ShapeKind::DEFAULT`]
    /// names.
    fn with_shift_from_sigmas(self) -> Self {
        match self.sigmas().and_then(|(lo, hi)| moments_from_sigmas(lo, hi)) {
            Ok(m) => AsymmetricMoments { shift: m.shift, ..self },
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

    // -- MomentLineage -----------------------------------------------------------------

    #[test]
    fn same_source_reused_cancels_in_both_moments() {
        // 2x - x == x: net sensitivity 1, so variance AND third both return to the source's own.
        let x = MomentLineage::measured_with_id(7, 4.0, 2.5);
        let two_x_minus_x = MomentLineage::combine(&x, 2.0, &x, -1.0);
        assert!((two_x_minus_x.variance() - 4.0).abs() < 1e-12);
        assert!((two_x_minus_x.third() - 2.5).abs() < 1e-12); // a scalar accumulator says 7*2.5
    }

    #[test]
    fn x_minus_x_is_exact_and_prunes_the_source() {
        let x = MomentLineage::measured_with_id(9, 4.0, 2.5);
        let zero = MomentLineage::combine(&x, 1.0, &x, -1.0);
        assert_eq!(zero.terms().len(), 0);
        assert_eq!(zero.variance(), 0.0);
        assert_eq!(zero.third(), 0.0);
    }

    #[test]
    fn independent_sources_add_with_jacobian_powers() {
        let a = MomentLineage::measured(1.0, 0.5); // fresh ids: independent
        let b = MomentLineage::measured(2.0, -0.25);
        let m = MomentLineage::combine(&a, 2.0, &b, -3.0);
        assert!((m.variance() - (4.0 * 1.0 + 9.0 * 2.0)).abs() < 1e-12); // Σ a²V
        assert!((m.third() - (8.0 * 0.5 + -27.0 * -0.25)).abs() < 1e-12); // Σ a³μ₃, signs kept
    }

    #[test]
    fn scale_keeps_the_sign_in_the_third_power() {
        let a = MomentLineage::measured(1.0, 0.5);
        let neg = a.scale(-1.0);
        assert!((neg.variance() - 1.0).abs() < 1e-12);
        assert!((neg.third() + 0.5).abs() < 1e-12); // J³ flips it
    }

    #[test]
    fn from_lineage_preserves_ids_and_variance() {
        let l = Lineage::measured_with_id(3, 0.5); // one source, amplitude 0.5
        let m = MomentLineage::from_lineage(&l);
        assert_eq!(m.terms().len(), 1);
        assert_eq!(m.terms()[0].0, 3);
        assert!((m.variance() - 0.25).abs() < 1e-12); // matches l.std_dev()²
        assert_eq!(m.third(), 0.0);
    }

    #[test]
    fn uncorrelated_mode_collapses_like_lineage_does() {
        // Mirror Lineage::combine_independent: no id bookkeeping survives.
        let _guard = crate::uncertainty::mode::scoped(PropagationMode::Uncorrelated);
        let x = MomentLineage::measured_with_id(7, 4.0, 2.5);
        let m = MomentLineage::combine(&x, 2.0, &x, -1.0);
        // Treated as independent: variance 4·4 + 1·4 = 20, third 8·2.5 − 1·2.5 = 17.5.
        assert!((m.variance() - 20.0).abs() < 1e-12);
        assert!((m.third() - 17.5).abs() < 1e-12);
    }

    #[test]
    fn cancellation_and_addition_hold_across_orders_of_magnitude() {
        // Task 1 shipped a bug from comparing a computed float to a hardcoded literal with a
        // strict `>`, wrong at most scales but invisible at 1.0. Guard against the same class of
        // mistake here by exercising both the cancelling and additive paths well away from 1.0.
        for scale in [1e-6, 1e-3, 1.0, 1e3, 1e6, 1e9] {
            let x = MomentLineage::measured_with_id(42, 4.0 * scale, 2.5 * scale);
            let two_x_minus_x = MomentLineage::combine(&x, 2.0, &x, -1.0);
            assert!(
                (two_x_minus_x.variance() - 4.0 * scale).abs() < 1e-9 * scale.max(1.0),
                "variance mismatch at scale {scale}"
            );
            assert!(
                (two_x_minus_x.third() - 2.5 * scale).abs() < 1e-9 * scale.max(1.0),
                "third mismatch at scale {scale}"
            );

            let a = MomentLineage::measured(1.0 * scale, 0.5 * scale);
            let b = MomentLineage::measured(2.0 * scale, -0.25 * scale);
            let m = MomentLineage::combine(&a, 2.0, &b, -3.0);
            assert!(
                (m.variance() - (4.0 * scale + 18.0 * scale)).abs() < 1e-9 * scale.max(1.0),
                "independent variance mismatch at scale {scale}"
            );
        }
    }
}
