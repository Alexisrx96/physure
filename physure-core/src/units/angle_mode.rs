//! How a conversion reads the radian and the steradian: `[Settings] angle_mode` in `physure.conf`, and which of
//! them `[Compatibility]` lets it read as 1.
//!
//! The SI gives both units the expression 1 (Brochure, Table 4) and writes them in an expression only to say what a
//! quantity involves. `Dimensionless`, the default, lets a *conversion* identify them with 1 where the compatibility
//! rows say so; a sum or a comparison never does. `Dimension` makes the angle a dimension like any other, which is
//! what Physure did before this setting existed.
//!
//! The file's setting is process-wide, like `propagation_mode`'s, and a [`scoped`] override sits on top of it per
//! thread. A rayon worker does not inherit that override: whoever starts parallel work reads [`angle_mode`] first and
//! opens a [`scoped`] one inside each task.

use std::cell::Cell;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// How a conversion treats the radian and the steradian.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AngleMode {
    /// A conversion may read them as 1, as the SI does, where `[Compatibility]` says so.
    #[default]
    Dimensionless,
    /// They are base dimensions, compared like any other.
    Dimension,
}

impl AngleMode {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Dimensionless => 0,
            Self::Dimension => 1,
        }
    }

    const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::Dimension,
            _ => Self::Dimensionless,
        }
    }

    /// The name this mode is written with in a conf file.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dimensionless => "dimensionless",
            Self::Dimension => "dimension",
        }
    }
}

impl FromStr for AngleMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dimensionless" => Ok(Self::Dimensionless),
            "dimension" => Ok(Self::Dimension),
            other => Err(format!("unknown angle mode '{other}', expected one of: dimensionless, dimension")),
        }
    }
}

static CONFIGURED: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<AngleMode>> = const { Cell::new(None) };
}

/// The mode in force: a [`scoped`] override if this thread has one, otherwise whatever `physure.conf` asked for.
pub fn angle_mode() -> AngleMode {
    OVERRIDE.with(Cell::get).unwrap_or_else(|| AngleMode::from_u8(CONFIGURED.load(Ordering::Relaxed)))
}

/// Sets the process-wide mode, returning the one it replaced. This is what reading a `physure.conf` calls; for a
/// temporary change, use [`scoped`].
pub fn set_angle_mode(mode: AngleMode) -> AngleMode {
    AngleMode::from_u8(CONFIGURED.swap(mode.as_u8(), Ordering::Relaxed))
}

/// Overrides the mode on this thread until the returned guard is dropped; guards nest.
///
/// ```
/// use physure_core::units::angle_mode::{angle_mode, scoped, AngleMode};
/// {
///     let _guard = scoped(AngleMode::Dimension);
///     assert_eq!(angle_mode(), AngleMode::Dimension);
/// }
/// assert_eq!(angle_mode(), AngleMode::Dimensionless);
/// ```
pub fn scoped(mode: AngleMode) -> AngleModeGuard {
    AngleModeGuard(OVERRIDE.with(|slot| slot.replace(Some(mode))), PhantomData)
}

/// Restores the mode that was in force before its [`scoped`] call, including on unwind: a mode left switched on by a
/// panicking test would silently change every later answer.
#[must_use = "the override ends when the guard is dropped; bind it with `let _guard = ...`"]
pub struct AngleModeGuard(Option<AngleMode>, PhantomData<*const ()>);

impl Drop for AngleModeGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| slot.set(self.0));
    }
}

static RADIAN_READ_AS_ONE: AtomicBool = AtomicBool::new(true);
static STERADIAN_READ_AS_ONE: AtomicBool = AtomicBool::new(true);

/// Whether a conversion may read the base unit `marker` (`"rad"` or `"sr"`) as 1, as `[Compatibility]` says.
pub fn marker_converts_to_plain(marker: &str) -> bool {
    match marker {
        "rad" => RADIAN_READ_AS_ONE.load(Ordering::Relaxed),
        "sr" => STERADIAN_READ_AS_ONE.load(Ordering::Relaxed),
        _ => false,
    }
}

/// Sets which markers a conversion may read as 1: what reading the `[Compatibility]` section of the embedded conf or
/// of the `physure.conf` of the working directory calls (a system conf does not change them).
pub fn set_marker_grants(radian: bool, steradian: bool) {
    RADIAN_READ_AS_ONE.store(radian, Ordering::Relaxed);
    STERADIAN_READ_AS_ONE.store(steradian, Ordering::Relaxed);
}

/// Puts the process-wide mode, the grants and the Phase 4b settings (the Monte Carlo sample count, ..., the display rules) back as they
/// were when it was made, also on unwind. A test that
/// changes either takes `crate::settings::TEST_LOCK` first and holds one of these for as long as it does.
#[cfg(test)]
pub(crate) struct RestoreProcessState {
    mode: AngleMode,
    radian: bool,
    steradian: bool,
    monte_carlo_samples: usize,
    max_while_iterations: usize,
    asymmetric_shape: crate::uncertainty::ShapeKind,
    default_dof: f64,
    monte_carlo_seed: crate::uncertainty::MonteCarloSeed,
    display_rules: crate::settings::DisplayRules,
}

#[cfg(test)]
impl RestoreProcessState {
    pub(crate) fn capture() -> Self {
        Self {
            mode: AngleMode::from_u8(CONFIGURED.load(Ordering::Relaxed)),
            radian: marker_converts_to_plain("rad"),
            steradian: marker_converts_to_plain("sr"),
            monte_carlo_samples: crate::settings::monte_carlo_samples(),
            max_while_iterations: crate::settings::max_while_iterations(),
            asymmetric_shape: crate::uncertainty::shapes::asymmetric_shape(),
            default_dof: crate::uncertainty::dof::default_dof(),
            monte_carlo_seed: crate::uncertainty::monte_carlo_seed(),
            display_rules: *crate::settings::process_display_rules(),
        }
    }
}

#[cfg(test)]
impl Drop for RestoreProcessState {
    fn drop(&mut self) {
        set_angle_mode(self.mode);
        set_marker_grants(self.radian, self.steradian);
        crate::settings::set_monte_carlo_samples(self.monte_carlo_samples);
        crate::settings::set_max_while_iterations(self.max_while_iterations);
        crate::uncertainty::shapes::set_asymmetric_shape(self.asymmetric_shape);
        crate::uncertainty::dof::set_default_dof(self.default_dof);
        crate::uncertainty::set_monte_carlo_seed(self.monte_carlo_seed);
        crate::settings::set_display_rules(self.display_rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TEST_LOCK;

    #[test]
    fn every_spelling_the_conf_may_use_round_trips() {
        for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
            assert_eq!(mode.name().parse::<AngleMode>(), Ok(mode));
            assert_eq!(AngleMode::from_u8(mode.as_u8()), mode);
        }
        assert_eq!("  Dimension ".parse::<AngleMode>(), Ok(AngleMode::Dimension));
    }

    #[test]
    fn an_unknown_mode_names_the_choices() {
        let why = "degrees".parse::<AngleMode>().unwrap_err();
        assert!(why.contains("'degrees'"), "{why}");
        assert!(why.contains("dimensionless, dimension"), "{why}");
    }

    #[test]
    fn a_scope_nests_and_restores() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = angle_mode();
        {
            let _outer = scoped(AngleMode::Dimension);
            assert_eq!(angle_mode(), AngleMode::Dimension);
            {
                let _inner = scoped(AngleMode::Dimensionless);
                assert_eq!(angle_mode(), AngleMode::Dimensionless);
            }
            assert_eq!(angle_mode(), AngleMode::Dimension);
        }
        assert_eq!(angle_mode(), before);
    }

    #[test]
    fn a_scope_restores_on_unwind() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = angle_mode();
        let result = std::panic::catch_unwind(|| {
            let _guard = scoped(AngleMode::Dimension);
            panic!("a test that leaves a scope open");
        });
        assert!(result.is_err());
        assert_eq!(angle_mode(), before);
    }

    #[test]
    fn a_scope_belongs_to_its_thread() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let process_wide = angle_mode();
        let _guard = scoped(AngleMode::Dimension);
        let seen = std::thread::spawn(angle_mode).join().unwrap();
        assert_eq!(seen, process_wide);
    }

    #[test]
    fn the_process_wide_mode_is_set_and_the_old_one_returned() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = RestoreProcessState::capture();
        let original = set_angle_mode(AngleMode::Dimension);
        assert_eq!(angle_mode(), AngleMode::Dimension);
        assert_eq!(set_angle_mode(original), AngleMode::Dimension);
    }

    #[test]
    fn only_the_two_markers_can_be_granted() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = RestoreProcessState::capture();
        set_marker_grants(true, false);
        assert!(marker_converts_to_plain("rad"));
        assert!(!marker_converts_to_plain("sr"));
        assert!(!marker_converts_to_plain("m"));
    }
}
