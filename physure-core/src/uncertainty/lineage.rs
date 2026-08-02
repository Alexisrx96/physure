//! Provenance tracking for a scalar uncertainty.
//!
//! A standard deviation on its own cannot tell you whether two quantities share a source, so
//! `x - x` came out at `sigma * sqrt(2)` instead of zero: the two operands looked like two
//! independent measurements that happened to agree. A lineage records *which* measurements a
//! value came from and with what sensitivity, so cancellation falls out of the arithmetic.
//!
//! Each term is `(measurement id, coefficient)`, where the coefficient is that measurement's
//! contribution to the current standard deviation. A freshly measured quantity has one term
//! whose coefficient is its own sigma. Propagating through an operation with partial
//! derivatives `J_k` merges the operand lineages by
//!
//! ```text
//! c_new(id) = sum_k J_k * c_k(id)
//! ```
//!
//! and the standard deviation is recovered as `sqrt(sum(c^2))`. When two lineages share no
//! ids this reduces exactly to the quadrature sum it replaces, so independent quantities keep
//! propagating as before — only shared ancestry behaves differently.

use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// Mints an id for a newly measured quantity.
///
/// Two calls never collide, so building the same numbers twice yields two independent
/// measurements — which is the physically honest reading of measuring something twice.
/// Sharing comes from cloning or reusing a value, not from equal magnitudes.
pub fn fresh_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Which measurements a value came from, and how strongly each one still moves it.
///
/// Terms are kept sorted by id so merging is a linear walk. `Leaf` holds its single term
/// inline: a measured quantity is the common case and should not pay for a heap allocation.
#[derive(Clone, Debug, PartialEq)]
pub enum Lineage {
    /// One source measurement, contributing its own standard deviation.
    Leaf([(u32, f64); 1]),
    /// Zero or more sources. Empty means exact — a constant with no uncertainty.
    Derived(Vec<(u32, f64)>),
}

impl Lineage {
    /// A newly measured quantity. A zero standard deviation is exact, so it gets no id:
    /// minting one would let two unrelated exact constants "cancel" against each other.
    pub fn measured(std_dev: f64) -> Self {
        if std_dev == 0.0 || !std_dev.is_finite() {
            Self::exact()
        } else {
            Self::Leaf([(fresh_id(), std_dev)])
        }
    }

    /// A value with no uncertainty at all.
    pub fn exact() -> Self {
        Self::Derived(Vec::new())
    }

    /// The terms, sorted by id.
    pub fn terms(&self) -> &[(u32, f64)] {
        match self {
            Self::Leaf(t) => t,
            Self::Derived(v) => v,
        }
    }

    /// `sqrt(sum(c^2))` — the standard deviation this lineage represents.
    ///
    /// The empty case is spelled out rather than left to the fold: `impl Sum for f64` uses
    /// `-0.0` as its identity so that signed zeros survive addition, and `(-0.0).sqrt()` is
    /// `-0.0`, so a fully cancelled lineage would otherwise report its standard deviation as
    /// negative zero — which prints as `-0.000000`.
    pub fn std_dev(&self) -> f64 {
        match self {
            Self::Leaf([(_, c)]) => c.abs(),
            Self::Derived(v) if v.is_empty() => 0.0,
            Self::Derived(v) => v.iter().map(|(_, c)| c * c).sum::<f64>().sqrt(),
        }
    }

    /// True when the value carries no uncertainty.
    pub fn is_exact(&self) -> bool {
        self.terms().is_empty()
    }

    /// Applies a single partial derivative, as for a one-argument function.
    pub fn scale(&self, jacobian: f64) -> Self {
        if jacobian == 0.0 || !jacobian.is_finite() {
            return Self::exact();
        }
        // Routed through `push_term` so the "no zero coefficients" invariant holds however a
        // lineage was built, not just for the ones that came out of `combine`.
        let mut out = Vec::with_capacity(self.terms().len());
        for &(id, c) in self.terms() {
            push_term(&mut out, id, c * jacobian);
        }
        Self::Derived(out)
    }

    /// Rescales every coefficient so the lineage reports `target` as its standard deviation.
    ///
    /// Needed where a backend computes a better sigma than a first-order derivative can —
    /// the unscented transform through a nonlinear function, for instance. The shape of the
    /// dependency is kept so later cancellation still works; only its size is corrected.
    pub fn rescaled_to(&self, target: f64) -> Self {
        let current = self.std_dev();
        if current == 0.0 || !current.is_finite() || !target.is_finite() {
            return Self::exact();
        }
        self.scale(target / current)
    }

    /// Merges two lineages under the chain rule: `c_new(id) = ja * a(id) + jb * b(id)`.
    ///
    /// A term that cancels to exactly zero is dropped rather than kept at 0.0, so a lineage
    /// never grows with dead sources — that is what keeps `x - x` at an empty lineage
    /// instead of one term holding zero.
    pub fn combine(a: &Lineage, ja: f64, b: &Lineage, jb: f64) -> Self {
        let (at, bt) = (a.terms(), b.terms());
        let mut out: Vec<(u32, f64)> = Vec::with_capacity(at.len() + bt.len());
        let (mut i, mut j) = (0, 0);

        while i < at.len() && j < bt.len() {
            match at[i].0.cmp(&bt[j].0) {
                std::cmp::Ordering::Less => {
                    push_term(&mut out, at[i].0, at[i].1 * ja);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    push_term(&mut out, bt[j].0, bt[j].1 * jb);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    push_term(&mut out, at[i].0, at[i].1 * ja + bt[j].1 * jb);
                    i += 1;
                    j += 1;
                }
            }
        }
        for &(id, c) in &at[i..] {
            push_term(&mut out, id, c * ja);
        }
        for &(id, c) in &bt[j..] {
            push_term(&mut out, id, c * jb);
        }

        Self::Derived(out)
    }
}

fn push_term(out: &mut Vec<(u32, f64)>, id: u32, coeff: f64) {
    if coeff != 0.0 && coeff.is_finite() {
        out.push((id, coeff));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measured_value_reports_its_own_sigma() {
        assert_eq!(Lineage::measured(0.3).std_dev(), 0.3);
        assert!(Lineage::measured(0.0).is_exact());
    }

    #[test]
    fn two_measurements_of_the_same_number_stay_independent() {
        // Physically these are two separate readings, so they must not cancel.
        let a = Lineage::measured(0.3);
        let b = Lineage::measured(0.3);
        let diff = Lineage::combine(&a, 1.0, &b, -1.0);
        assert!((diff.std_dev() - (0.09f64 + 0.09).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn a_value_cancels_against_itself() {
        let x = Lineage::measured(0.3);
        assert!(Lineage::combine(&x, 1.0, &x, -1.0).is_exact());
        assert!((Lineage::combine(&x, 1.0, &x, 1.0).std_dev() - 0.6).abs() < 1e-12);
    }

    #[test]
    fn a_cancelled_lineage_reports_positive_zero() {
        // `impl Sum for f64` folds from -0.0, and (-0.0).sqrt() is -0.0, so the empty case
        // has to be handled explicitly or a cancelled result prints as `-0.000000`.
        let x = Lineage::measured(0.3);
        let sd = Lineage::combine(&x, 1.0, &x, -1.0).std_dev();
        assert_eq!(sd, 0.0);
        assert!(!sd.is_sign_negative(), "a standard deviation must never carry a negative sign");
    }

    #[test]
    fn a_scaled_copy_still_cancels() {
        // 2x - 2x: the shared source survives being scaled, which is what proves this is
        // provenance tracking and not a special case for the literal `x - x` shape.
        let x = Lineage::measured(0.3);
        let two_x = x.scale(2.0);
        assert!(Lineage::combine(&two_x, 1.0, &two_x, -1.0).is_exact());
    }

    #[test]
    fn independent_sources_reduce_to_quadrature() {
        // The property that keeps every existing result unchanged: with no shared ids the
        // merge is exactly the quadrature sum it replaces.
        let a = Lineage::measured(0.3);
        let b = Lineage::measured(0.4);
        let sum = Lineage::combine(&a, 1.0, &b, 1.0);
        assert!((sum.std_dev() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn terms_stay_sorted_and_merged() {
        let a = Lineage::measured(1.0);
        let b = Lineage::measured(1.0);
        let c = Lineage::measured(1.0);
        let ab = Lineage::combine(&a, 1.0, &b, 1.0);
        let bc = Lineage::combine(&b, 1.0, &c, 1.0);
        let merged = Lineage::combine(&ab, 1.0, &bc, 1.0);
        // a + 2b + c, and the ids must come out ordered.
        assert_eq!(merged.terms().len(), 3);
        assert!(merged.terms().windows(2).all(|w| w[0].0 < w[1].0));
        assert!((merged.std_dev() - (1.0f64 + 4.0 + 1.0).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn rescaling_keeps_the_shape_but_fixes_the_size() {
        let x = Lineage::measured(0.3);
        let r = x.rescaled_to(0.9);
        assert!((r.std_dev() - 0.9).abs() < 1e-12);
        // Still the same source, so it still cancels against its own family.
        assert!(Lineage::combine(&r, 1.0, &x, -3.0).is_exact());
    }
}
