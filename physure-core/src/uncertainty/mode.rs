//! The propagation mode, as read from `[Settings] propagation_mode` in `physure.conf`.
//!
//! Four values along what are really two axes, kept as one key because that is how the
//! setting has always been spelled and how the Python API exposes it:
//!
//! - `correlated` / `uncorrelated` decide whether a value is allowed to know it is the
//!   same measurement as another one. That is a property of the *operation*, so it is
//!   read in [`Lineage::combine`](crate::uncertainty::Lineage::combine).
//! - `monte_carlo` / `unscented` pick the backend a new measurement is built with, so
//!   they are read in [`Quantity::new_scalar`](crate::Quantity::new_scalar) and only
//!   when the caller did not name a backend itself.
//!
//! The file's setting is process-wide: a thread-local would be set on whichever thread
//! happened to touch the registry first, leaving every other thread on the default with
//! nothing to say so. A [`scoped`] override sits on top of it and is per-thread, which is
//! what makes it usable at all — two threads asking different questions of the same
//! process must not answer each other's.

use std::cell::Cell;
use std::str::FromStr;
use std::sync::atomic::{AtomicU8, Ordering};

/// How uncertainties combine, and what a new measurement is built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PropagationMode {
    /// Shared ancestry cancels: `x - x` is exactly zero.
    #[default]
    Correlated,
    /// Every value is treated as an independent measurement, so `x - x` is `sigma*sqrt(2)`.
    /// The opt-out for datasets where tracking provenance costs more than it is worth.
    Uncorrelated,
    /// New measurements are drawn as samples.
    MonteCarlo,
    /// New measurements are carried as sigma points.
    Unscented,
}

impl PropagationMode {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Correlated => 0,
            Self::Uncorrelated => 1,
            Self::MonteCarlo => 2,
            Self::Unscented => 3,
        }
    }

    const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::Uncorrelated,
            2 => Self::MonteCarlo,
            3 => Self::Unscented,
            _ => Self::Correlated,
        }
    }

    /// The name this mode is written with in a conf file.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Correlated => "correlated",
            Self::Uncorrelated => "uncorrelated",
            Self::MonteCarlo => "monte_carlo",
            Self::Unscented => "unscented",
        }
    }
}

impl FromStr for PropagationMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "correlated" => Ok(Self::Correlated),
            "uncorrelated" => Ok(Self::Uncorrelated),
            "monte_carlo" | "montecarlo" => Ok(Self::MonteCarlo),
            "unscented" => Ok(Self::Unscented),
            other => Err(format!(
                "unknown propagation mode '{other}', expected one of: \
                 correlated, uncorrelated, monte_carlo, unscented"
            )),
        }
    }
}

static CONFIGURED: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<PropagationMode>> = const { Cell::new(None) };
}

/// The mode currently in force: a [`scoped`] override if this thread has one, otherwise
/// whatever `physure.conf` asked for.
pub fn propagation_mode() -> PropagationMode {
    OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| PropagationMode::from_u8(CONFIGURED.load(Ordering::Relaxed)))
}

/// Sets the process-wide mode, returning the one it replaced. This is what reading a
/// `physure.conf` calls; for a temporary change, use [`scoped`].
pub fn set_propagation_mode(mode: PropagationMode) -> PropagationMode {
    PropagationMode::from_u8(CONFIGURED.swap(mode.as_u8(), Ordering::Relaxed))
}

/// Overrides the mode on this thread until the returned guard is dropped.
///
/// ```
/// use physure_core::uncertainty::{PropagationMode, propagation_mode, mode};
/// {
///     let _guard = mode::scoped(PropagationMode::Uncorrelated);
///     assert_eq!(propagation_mode(), PropagationMode::Uncorrelated);
/// }
/// assert_eq!(propagation_mode(), PropagationMode::Correlated);
/// ```
pub fn scoped(mode: PropagationMode) -> ModeGuard {
    ModeGuard(OVERRIDE.with(|slot| slot.replace(Some(mode))))
}

/// Restores the mode that was in force before its [`scoped`] call, including on unwind —
/// a mode left switched on by a panicking test would silently change every later answer.
pub struct ModeGuard(Option<PropagationMode>);

impl Drop for ModeGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| slot.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_the_conf_may_use_round_trips() {
        for mode in [
            PropagationMode::Correlated,
            PropagationMode::Uncorrelated,
            PropagationMode::MonteCarlo,
            PropagationMode::Unscented,
        ] {
            assert_eq!(mode.name().parse::<PropagationMode>().unwrap(), mode);
            assert_eq!(PropagationMode::from_u8(mode.as_u8()), mode);
        }
        assert_eq!("MonteCarlo".parse::<PropagationMode>().unwrap(), PropagationMode::MonteCarlo);
    }

    #[test]
    fn a_misspelt_mode_is_rejected_rather_than_defaulted() {
        // Falling back to correlated would report correlated results for a file that asked
        // for something else, which is the one wrong answer that looks right.
        let err = "uncorelated".parse::<PropagationMode>().unwrap_err();
        assert!(err.contains("uncorelated"), "{err}");
        assert!(err.contains("uncorrelated"), "the error should show the spelling meant: {err}");
    }
}
