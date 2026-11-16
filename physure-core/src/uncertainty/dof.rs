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
//! not change, and a source that declares nothing has [`default_dof`] degrees of freedom (`[Settings]
//! default_dof`), infinitely many unless a configuration says otherwise: what the Guide assumes for an
//! uncertainty known exactly (a Type B estimate with a negligible uncertainty).
//!
//! The table only grows: ids are never reused, so an entry cannot come to describe another source. Every
//! declaration is stored, `+∞` included: a declared infinity must stay distinguishable from no declaration
//! at all (it is refused to change, like any declared value, and the degrees of freedom of a source that
//! declared nothing are a setting, not necessarily infinite).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use super::lineage::Lineage;
use crate::error::{PhysureError, PhysureResult};

fn table() -> &'static RwLock<HashMap<u32, f64>> {
    static TABLE: OnceLock<RwLock<HashMap<u32, f64>>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// The bits of `+∞`, the compiled default of `[Settings] default_dof` (there is no atomic float).
const INFINITY_BITS: u64 = 0x7FF0_0000_0000_0000;
static DEFAULT_DOF: AtomicU64 = AtomicU64::new(INFINITY_BITS);

thread_local! {
    static DOF_OVERRIDE: Cell<Option<f64>> = const { Cell::new(None) };
}

/// The degrees of freedom of a source that declared none, from `physure.conf`'s `[Settings] default_dof`: a number
/// greater than zero, or `+∞` (the compiled default, the value every undeclared source had before the setting
/// existed). A source that declared any value, `+∞` included, keeps its own.
///
/// A [`scoped_default_dof`] override on this thread (if set) takes precedence; otherwise, the process-wide value
/// from [`set_default_dof`] is returned.
pub fn default_dof() -> f64 {
    DOF_OVERRIDE.with(Cell::get).unwrap_or_else(|| f64::from_bits(DEFAULT_DOF.load(Ordering::Relaxed)))
}

/// Sets the process-wide default, returning the one it replaced. The conf readers validate it (greater than zero,
/// infinity allowed); for a temporary change, use [`scoped_default_dof`].
pub fn set_default_dof(nu: f64) -> f64 {
    f64::from_bits(DEFAULT_DOF.swap(nu.to_bits(), Ordering::Relaxed))
}

/// Overrides the default on this thread until the returned guard is dropped.
pub fn scoped_default_dof(nu: f64) -> DefaultDofGuard {
    DefaultDofGuard(DOF_OVERRIDE.with(|slot| slot.replace(Some(nu))))
}

/// Restores the default that was in force before its [`scoped_default_dof`] call, including on unwind.
pub struct DefaultDofGuard(Option<f64>);

impl Drop for DefaultDofGuard {
    fn drop(&mut self) {
        DOF_OVERRIDE.with(|slot| slot.set(self.0));
    }
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
            // `+∞` too: dropping it let a later finite value through the check above
            table.insert(id, nu);
            Ok(())
        }
    }
}

/// The degrees of freedom of the source `id`: what it declared, or [`default_dof`] when it declared none.
pub fn dof_of(id: u32) -> f64 {
    let table = table().read().unwrap_or_else(|poisoned| poisoned.into_inner());
    table.get(&id).copied().unwrap_or_else(default_dof)
}

impl Lineage {
    /// Declares `nu` degrees of freedom for the measurement this lineage describes, which must be
    /// exactly one source: the degrees of freedom belong to a measurement, and a result that combines
    /// several has those of its sources (declare them on each before combining).
    ///
    /// # Errors
    /// An exact lineage, one with several sources, `nu` not positive, or `nu` different from what the
    /// source already has.
    pub fn declare_dof(&self, nu: f64) -> PhysureResult<()> {
        match self.terms() {
            [(id, _)] => declare_dof(*id, nu),
            [] => Err(PhysureError::DomainError(
                "an exact quantity has no uncertainty to have degrees of freedom".to_string(),
            )),
            many => Err(PhysureError::DomainError(format!(
                "a quantity that combines {} sources has no degrees of freedom to declare: declare \
                 them on each measurement before combining",
                many.len()
            ))),
        }
    }

    /// The degrees of freedom declared for the sources of this lineage, `(source id, nu)`, for those that
    /// declared any (`+∞` included): what a serialized lineage must carry so that it can be rebuilt elsewhere.
    pub fn declared_dofs(&self) -> Vec<(u32, f64)> {
        let table = table().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.terms().iter().filter_map(|&(id, _)| table.get(&id).map(|&nu| (id, nu))).collect()
    }

    /// The effective degrees of freedom of the value this lineage describes (Welch–Satterthwaite).
    ///
    /// A source that declared none counts with [`default_dof`]. `+∞` when no source has a finite number of them,
    /// and for an exact value. Written with the
    /// share of each source in the variance, `w_i = c_i² / u_c²`, so that `nu_eff = 1 / sum_i w_i² / nu_i`
    /// does not underflow for a very small uncertainty.
    pub fn effective_dof(&self) -> f64 {
        let terms = self.terms();
        let variance: f64 = terms.iter().map(|&(_, c)| c * c).sum();
        if variance <= 0.0 {
            return f64::INFINITY;
        }
        let undeclared = default_dof();
        let table = table().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let denominator: f64 = terms
            .iter()
            .filter_map(|&(id, c)| {
                // an infinite one contributes nothing (`share² / ∞` is zero): skipped, so the sum keeps its bits
                let nu = Some(table.get(&id).copied().unwrap_or(undeclared)).filter(|nu| nu.is_finite())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the thread-local guard is used here: a process-wide default would change the degrees of freedom the
    /// crate's other unit tests read for undeclared sources (the process-wide setter is tested in
    /// `tests/uncertainty_settings.rs`).
    #[test]
    fn an_undeclared_source_has_the_default_in_force_and_a_declared_one_keeps_its_own() {
        let undeclared = Lineage::measured(4.0);
        let declared_inf = Lineage::measured(4.0);
        declared_inf.declare_dof(f64::INFINITY).unwrap();
        let declared_5 = Lineage::measured(3.0);
        declared_5.declare_dof(5.0).unwrap();
        assert_eq!(default_dof(), f64::INFINITY, "the compiled default");
        assert_eq!(dof_of(undeclared.terms()[0].0), f64::INFINITY);
        {
            let _fifty = scoped_default_dof(50.0);
            assert_eq!(default_dof(), 50.0);
            assert_eq!(dof_of(undeclared.terms()[0].0), 50.0);
            assert_eq!(undeclared.effective_dof(), 50.0);
            assert_eq!(dof_of(declared_inf.terms()[0].0), f64::INFINITY, "a declared infinity stays infinite");
            assert_eq!(declared_inf.effective_dof(), f64::INFINITY);
            assert_eq!(declared_5.effective_dof(), 5.0);
            // u_c^4 = 625: 3^4 / 5 from the declared source, 4^4 / 50 from the undeclared one
            let sum = Lineage::combine(&declared_5, 1.0, &undeclared, 1.0).effective_dof();
            assert!((sum - 625.0 / (81.0 / 5.0 + 256.0 / 50.0)).abs() < 1e-9, "{sum}");
            let with_inf = Lineage::combine(&declared_5, 1.0, &declared_inf, 1.0).effective_dof();
            assert!((with_inf - 625.0 / (81.0 / 5.0)).abs() < 1e-9, "{with_inf}");
            assert_eq!(undeclared.declared_dofs(), vec![], "the default is not a declaration");
            assert_eq!(Lineage::exact().effective_dof(), f64::INFINITY, "an exact value has none");
            let elsewhere = std::thread::spawn(default_dof).join().unwrap();
            assert_eq!(elsewhere, f64::INFINITY, "a scope is per thread");
        }
        assert_eq!(dof_of(undeclared.terms()[0].0), f64::INFINITY);
    }
}
