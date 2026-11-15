//! Uncertainty propagation in each mode, the degrees of freedom and coverage factor of the Guide's examples, and
//! two-sided Student-t coverage factors.
//!
//! # The settings these cases read, and how they are pinned
//!
//! The propagation mode is a process-wide setting that a frozen file's `[Settings]` sets, so every `unc.*` case
//! runs under a thread-local [`scoped`] mode of its own and never reads the process's: `correlated` for the
//! first-order and the GUM cases, `uncorrelated` for `unc.sum_uncorrelated`, `monte_carlo` and `unscented` for the
//! two cases named after them. The other uncertainty settings a frozen file can change are pinned the same way, at
//! their compiled defaults, for every `unc.*` case: the Monte Carlo sample count (`monte_carlo_samples = 1000`, the
//! count `unc.montecarlo_seed_1` always drew). The `stat.*` cases read no setting. The units (`m`, `s`, `nm`) are read through the
//! hooked catalog like every other case.
//!
//! # Monte Carlo
//!
//! `unc.montecarlo_seed_1` draws 1000 samples per input inside `with_seed(1, ..)`: each draw takes a fresh
//! `rand::rngs::StdRng` (rand 0.8: ChaCha12) seeded from `1 ^ n * 0x9E3779B97F4A7C15` for the `n`-th draw of the
//! scope, and normal variates come from `rand_distr::Normal` (rand_distr 0.4, ziggurat). The evaluation is on the
//! calling thread (the Monte Carlo backend uses no worker threads, so no `parallel_threshold` is involved) and the
//! mean and standard deviation are `ndarray` reductions of a fixed length, so the bits are the same on every run of
//! one build. **They can move with a dependency bump**: neither `StdRng` nor `rand_distr`'s sampling is promised
//! to be reproducible across versions; a frozen file then reports this witness as changed.

use super::units::witness_of;
use super::{case, Case, CaseError, CaseResult};
use crate::frozen::witness::WitnessValue;
use crate::math::stats::student_t_two_sided;
use crate::quantity::Quantity;
use crate::uncertainty::mode::scoped;
use crate::uncertainty::{with_seed, PropagationMode};

/// Runs `f` with the propagation mode `mode` on this thread only, and every other uncertainty setting at its
/// compiled default (see the module documentation).
fn under(mode: PropagationMode, f: impl FnOnce() -> CaseResult) -> CaseResult {
    let _mode = scoped(mode);
    let _samples = crate::settings::scoped_monte_carlo_samples(crate::settings::DEFAULT_MONTE_CARLO_SAMPLES);
    f()
}

fn measured(mean: f64, u: f64, unit: &str) -> Result<Quantity, CaseError> {
    Ok(Quantity::with_uncertainty(mean, u, unit)?)
}

/// `x = 1.5 ± 0.02 m`, `s = x + 2.5 x`: one source, so the mode decides whether its two appearances add linearly
/// (correlated) or in quadrature (uncorrelated).
fn correlated_sum() -> CaseResult {
    let x = measured(1.5, 0.02, "m")?;
    let y = x.mul(&Quantity::new(2.5, "")?)?;
    Ok(witness_of(&x.add(&y)?))
}

/// `a = 2.0 ± 0.1 m` and `b = 3.0 ± 0.2 s`, two independent measurements.
fn inputs() -> Result<(Quantity, Quantity), CaseError> {
    Ok((measured(2.0, 0.1, "m")?, measured(3.0, 0.2, "s")?))
}

/// JCGM 100:2008 Annex H.1, the end gauge, as the six non-zero contributions of its linear budget (in nm, with
/// their degrees of freedom): the calibration of the standard (25, 18), the mean difference (5.8, 24), the random
/// (3.9, 5) and systematic (6.7, 8) effects of the comparator, the temperature cycle (16.599, 2) and the difference
/// of the expansion coefficients (2.88679, 50). The engine has no products of input quantities with a zero
/// estimate, so the second-order terms of the Guide's model are not expressed; the budget is the one
/// `tests/expanded_uncertainty.rs` checks against the Guide.
fn end_gauge() -> Result<Quantity, CaseError> {
    let budget = [
        (50_000_623.0, 25.0, 18.0),
        (215.0, 5.8, 24.0),
        (0.0, 3.9, 5.0),
        (0.0, 6.7, 8.0),
        (0.0, 16.599, 2.0),
        (0.0, 2.88679, 50.0),
    ];
    let mut length: Option<Quantity> = None;
    for (mean, u, nu) in budget {
        let part = measured(mean, u, "nm")?.with_dof(nu)?;
        length = Some(match length {
            None => part,
            Some(sum) => sum.add(&part)?,
        });
    }
    Ok(length.expect("the budget is not empty"))
}

/// A dimensionless number with no uncertainty.
fn plain(value: f64) -> WitnessValue {
    WitnessValue::number(value, 0.0, "")
}

/// JCGM 100:2008 G.4.1, example 1: `Y = X1 X2 X3` with relative standard uncertainties 0.25 %, 0.57 % and 0.82 %
/// from 10, 5 and 15 observations (9, 4 and 14 degrees of freedom). The estimates are 2, 3 and 5 (the Guide gives
/// only relative uncertainties; any estimates give the same effective degrees of freedom).
fn welch_satterthwaite_g41() -> CaseResult {
    let x1 = measured(2.0, 2.0 * 0.0025, "")?.with_dof(9.0)?;
    let x2 = measured(3.0, 3.0 * 0.0057, "")?.with_dof(4.0)?;
    let x3 = measured(5.0, 5.0 * 0.0082, "")?.with_dof(14.0)?;
    Ok(plain(x1.mul(&x2)?.mul(&x3)?.effective_dof()?))
}

/// The two-sided Student-t coverage factor for coverage probability `p` and `nu` degrees of freedom.
fn t(p: f64, nu: f64) -> CaseResult {
    Ok(plain(student_t_two_sided(p, nu)?))
}

pub(super) fn cases() -> Vec<Case> {
    use PropagationMode::{Correlated, MonteCarlo, Uncorrelated, Unscented};
    vec![
        case("unc.sum_correlated", || under(Correlated, correlated_sum)),
        case("unc.sum_uncorrelated", || under(Uncorrelated, correlated_sum)),
        case("unc.product_first_order", || {
            under(Correlated, || {
                let (a, b) = inputs()?;
                Ok(witness_of(&a.mul(&b)?))
            })
        }),
        case("unc.quotient_first_order", || {
            under(Correlated, || {
                let (a, b) = inputs()?;
                Ok(witness_of(&a.div(&b)?))
            })
        }),
        case("unc.power_first_order", || {
            under(Correlated, || {
                let (a, _) = inputs()?;
                Ok(witness_of(&a.pow(3.0)?))
            })
        }),
        case("unc.montecarlo_seed_1", || {
            under(MonteCarlo, || {
                with_seed(1, || {
                    let (a, b) = inputs()?;
                    Ok(witness_of(&a.mul(&b)?))
                })
            })
        }),
        // `a²` and not `a b`: the unscented product of two quantities is propagated through their merged lineage
        // (first order, as `unc.product_first_order`), while a power goes through the sigma points themselves.
        case("unc.unscented_square", || {
            under(Unscented, || {
                let (a, _) = inputs()?;
                Ok(witness_of(&a.pow(2.0)?))
            })
        }),
        case("unc.gum_h1_uc", || under(Correlated, || Ok(witness_of(&end_gauge()?)))),
        case("unc.gum_h1_nu_eff", || under(Correlated, || Ok(plain(end_gauge()?.effective_dof()?)))),
        case("unc.expanded_k_p95", || under(Correlated, || Ok(plain(end_gauge()?.coverage_factor(0.95)?)))),
        case("unc.welch_satterthwaite_g41", || under(Correlated, welch_satterthwaite_g41)),
        case("stat.t_p95_nu1", || t(0.95, 1.0)),
        case("stat.t_p95_nu2", || t(0.95, 2.0)),
        case("stat.t_p95_nu5", || t(0.95, 5.0)),
        case("stat.t_p95_nu10", || t(0.95, 10.0)),
        case("stat.t_p95_nu30", || t(0.95, 30.0)),
        case("stat.t_p95_nu100", || t(0.95, 100.0)),
        case("stat.t_p99_nu10", || t(0.99, 10.0)),
        case("stat.t_p99_nu100", || t(0.99, 100.0)),
    ]
}
