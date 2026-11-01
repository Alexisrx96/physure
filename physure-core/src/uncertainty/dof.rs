//! The degrees of freedom of the sources of a value, and from them its effective degrees of freedom.
//!
//! A standard uncertainty that comes from `n` observations carries `n - 1` degrees of freedom: how well
//! the uncertainty itself is known. JCGM 100:2008 (4.3.2, 4.3.9, G.4) keeps them per input quantity and
//! combines them for the result by the Welch–Satterthwaite formula,
//!
//! ```text
//! nu_eff = u_c^4 / sum_i (c_i u_i)^4 / nu_i
//! ```
//!
//! A [`Lineage`] already is that decomposition: its terms are `(source id, c_i u_i)`, one per independent
//! source, merged when a source is shared. So the only thing missing is the `nu_i`, and it is kept
//! **once per source**, here, rather than inside every term of every derived value: the arithmetic does
//! not change, and a source that declares nothing has infinitely many degrees of freedom, which is what
//! the Guide assumes for an uncertainty known exactly (a Type B estimate with a negligible uncertainty).
//!
//! The table only grows: ids are never reused, so an entry cannot come to describe another source, and
//! only a source that declares a finite value is stored.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use super::lineage::Lineage;
use crate::error::{PhysureError, PhysureResult};

fn table() -> &'static RwLock<HashMap<u32, f64>> {
    static TABLE: OnceLock<RwLock<HashMap<u32, f64>>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Declares that the source `id` has `nu` degrees of freedom (`+∞` for an uncertainty known exactly).
///
/// A source's degrees of freedom are declared once: the same value again changes nothing, and another
/// one is an error, because every copy of a value shares its sources and a later call must not change
/// what an earlier result already reported.
///
/// # Errors
/// `nu` not positive (or NaN), or different from what the source already has.
pub fn declare_dof(id: u32, nu: f64) -> PhysureResult<()> {
    if nu.is_nan() || nu <= 0.0 {
        return Err(PhysureError::DomainError(format!("degrees of freedom must be positive, got {nu}")));
    }
    let mut table = table().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    match table.get(&id).copied() {
        Some(existing) if existing != nu => Err(PhysureError::DomainError(format!(
            "source {id} already has {existing} degrees of freedom and cannot be given {nu}"
        ))),
        Some(_) => Ok(()),
        None => {
            if nu.is_finite() {
                table.insert(id, nu);
            }
            Ok(())
        }
    }
}

/// The degrees of freedom declared for the source `id`: `+∞` when it declared none.
pub fn dof_of(id: u32) -> f64 {
    let table = table().read().unwrap_or_else(|poisoned| poisoned.into_inner());
    table.get(&id).copied().unwrap_or(f64::INFINITY)
}

impl Lineage {
    /// The effective degrees of freedom of the value this lineage describes (Welch–Satterthwaite).
    ///
    /// `+∞` when no source has a finite number of them, and for an exact value. Written with the
    /// share of each source in the variance, `w_i = c_i² / u_c²`, so that `nu_eff = 1 / sum_i w_i² / nu_i`
    /// does not underflow for a very small uncertainty.
    pub fn effective_dof(&self) -> f64 {
        let terms = self.terms();
        let variance: f64 = terms.iter().map(|&(_, c)| c * c).sum();
        if variance <= 0.0 {
            return f64::INFINITY;
        }
        let table = table().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let denominator: f64 = terms
            .iter()
            .filter_map(|&(id, c)| {
                let nu = table.get(&id).copied()?;
                let share = c * c / variance;
                Some(share * share / nu)
            })
            .sum();
        if denominator > 0.0 {
            1.0 / denominator
        } else {
            f64::INFINITY
        }
    }
}
