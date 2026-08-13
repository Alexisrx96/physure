//! Process-wide settings read from `physure.conf`.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

const DEFAULT_PARALLEL_THRESHOLD: usize = 10_000;
static PARALLEL_THRESHOLD: AtomicUsize = AtomicUsize::new(DEFAULT_PARALLEL_THRESHOLD);

thread_local! {
    static OVERRIDE: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Minimum element count at which a `for`-expression switches from sequential
/// evaluation to `rayon`-parallel evaluation. Set from `physure.conf`'s
/// `[Settings] parallel_threshold`; `parallel_map` ignores this and is always parallel.
///
/// A [`scoped`] override on this thread (if set) takes precedence; otherwise, the
/// process-wide value from `set_parallel_threshold` is returned.
pub fn parallel_threshold() -> usize {
    OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| PARALLEL_THRESHOLD.load(Ordering::Relaxed))
}

/// Sets the process-wide threshold, returning the one it replaced. This is what reading
/// a `physure.conf` calls; for a temporary change, use [`scoped`].
pub fn set_parallel_threshold(n: usize) -> usize {
    PARALLEL_THRESHOLD.swap(n, Ordering::Relaxed)
}

/// Overrides the threshold on this thread until the returned guard is dropped.
pub fn scoped(n: usize) -> ThresholdGuard {
    ThresholdGuard(OVERRIDE.with(|slot| slot.replace(Some(n))))
}

/// Restores the threshold that was in force before its [`scoped`] call, including on unwind —
/// a threshold left switched on by a panicking test would silently change every later answer.
pub struct ThresholdGuard(Option<usize>);

impl Drop for ThresholdGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| slot.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parallel_threshold_returns_previous_and_updates() {
        let original = set_parallel_threshold(42);
        assert_eq!(parallel_threshold(), 42);
        let previous = set_parallel_threshold(original);
        assert_eq!(previous, 42);
    }

    #[test]
    fn scoped_override_takes_precedence_and_restores_on_drop() {
        let baseline = parallel_threshold();
        {
            let _guard = scoped(999);
            assert_eq!(parallel_threshold(), 999);
        }
        assert_eq!(parallel_threshold(), baseline);
    }
}
