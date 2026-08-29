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

const DEFAULT_MAX_LOOP_ITERATIONS: usize = 1_000_000;
static MAX_LOOP_ITERATIONS: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_LOOP_ITERATIONS);

thread_local! {
    static LOOP_OVERRIDE: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Ceiling on how many elements a single `for`-expression range, or a `parallel_map` input
/// vector, may materialize into a `Vec<PhsValue>` at once. Set from `physure.conf`'s
/// `[Settings] max_loop_iterations`. Unlike [`parallel_threshold`], `parallel_map` *does*
/// respect this ceiling: both it and the `for`-expression's range branch allocate one
/// `Vec<PhsValue>` sized by a value an untrusted caller can pick directly (a range endpoint,
/// or a vector's element count), so both carry the identical unbounded-allocation risk.
///
/// Default is 1,000,000 -- the exact largest size an adversarial audit of `phs serve` measured
/// as fast and safe: `sum(for i in 0..1_000_000 { i })` completed in 0.7s, while
/// `n = 100_000_000` tried to allocate ~49.6 GB of `PhsValue`s and made the global allocator
/// `abort()` the whole process (not a catchable panic), killing every other connected client
/// instantly. Every range-heavy test in this workspace stays far under a million (the largest,
/// `physure-script`'s `test_interpreter_for_expr_large_scale`, uses 100,000), so this leaves
/// two orders of magnitude of headroom for legitimate workloads while still refusing the
/// exploit long before memory pressure builds. A workload that genuinely needs more raises
/// `max_loop_iterations` in its `physure.conf`.
///
/// A [`scoped_max_loop_iterations`] override on this thread (if set) takes precedence;
/// otherwise, the process-wide value from [`set_max_loop_iterations`] is returned.
pub fn max_loop_iterations() -> usize {
    LOOP_OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| MAX_LOOP_ITERATIONS.load(Ordering::Relaxed))
}

/// Sets the process-wide loop-iteration ceiling, returning the one it replaced. This is what
/// reading a `physure.conf` calls; for a temporary change, use [`scoped_max_loop_iterations`].
pub fn set_max_loop_iterations(n: usize) -> usize {
    MAX_LOOP_ITERATIONS.swap(n, Ordering::Relaxed)
}

/// Overrides the loop-iteration ceiling on this thread until the returned guard is dropped.
pub fn scoped_max_loop_iterations(n: usize) -> MaxLoopIterationsGuard {
    MaxLoopIterationsGuard(LOOP_OVERRIDE.with(|slot| slot.replace(Some(n))))
}

/// Restores the ceiling that was in force before its [`scoped_max_loop_iterations`] call,
/// including on unwind -- a ceiling left switched on by a panicking test would silently change
/// every later answer.
pub struct MaxLoopIterationsGuard(Option<usize>);

impl Drop for MaxLoopIterationsGuard {
    fn drop(&mut self) {
        LOOP_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

const DEFAULT_MAX_PIPELINE_STEPS: usize = 1_000;
static MAX_PIPELINE_STEPS: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_PIPELINE_STEPS);

thread_local! {
    static PIPELINE_OVERRIDE: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Ceiling on how many steps a single `PhsPipeline::execute()` call may run. Checked once at
/// the very start of `execute()`, before any step runs, so an oversized pipeline is rejected
/// cleanly instead of partially executing. Set from `physure.conf`'s
/// `[Settings] max_pipeline_steps`.
///
/// Default is 1,000. An adversarial audit of `phs serve` sent a single ~4.6 MB HTTP request
/// (well under any existing body-size cap) describing a pipeline of 50,000 trivial steps, and
/// the server accepted and ran it to completion, pinning one CPU core for 3+ minutes (roughly
/// 3.6 ms/step). At 1,000 steps that same per-step cost bounds the worst case to a few
/// seconds, comfortably above any pipeline in this workspace's own tests (the largest today
/// has 2 steps). A workload that genuinely needs more raises `max_pipeline_steps` in its
/// `physure.conf`.
///
/// A [`scoped_max_pipeline_steps`] override on this thread (if set) takes precedence;
/// otherwise, the process-wide value from [`set_max_pipeline_steps`] is returned.
pub fn max_pipeline_steps() -> usize {
    PIPELINE_OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| MAX_PIPELINE_STEPS.load(Ordering::Relaxed))
}

/// Sets the process-wide pipeline-step ceiling, returning the one it replaced. This is what
/// reading a `physure.conf` calls; for a temporary change, use [`scoped_max_pipeline_steps`].
pub fn set_max_pipeline_steps(n: usize) -> usize {
    MAX_PIPELINE_STEPS.swap(n, Ordering::Relaxed)
}

/// Overrides the pipeline-step ceiling on this thread until the returned guard is dropped.
pub fn scoped_max_pipeline_steps(n: usize) -> MaxPipelineStepsGuard {
    MaxPipelineStepsGuard(PIPELINE_OVERRIDE.with(|slot| slot.replace(Some(n))))
}

/// Restores the ceiling that was in force before its [`scoped_max_pipeline_steps`] call,
/// including on unwind.
pub struct MaxPipelineStepsGuard(Option<usize>);

impl Drop for MaxPipelineStepsGuard {
    fn drop(&mut self) {
        PIPELINE_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parallel_threshold_returns_previous_and_updates() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_parallel_threshold(42);
        assert_eq!(parallel_threshold(), 42);
        let previous = set_parallel_threshold(original);
        assert_eq!(previous, 42);
    }

    #[test]
    fn scoped_override_takes_precedence_and_restores_on_drop() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let baseline = parallel_threshold();
        {
            let _guard = scoped(99);
            assert_eq!(parallel_threshold(), 99);
            {
                let _inner_guard = scoped(55);
                assert_eq!(parallel_threshold(), 55);
            }
            assert_eq!(parallel_threshold(), 99);
        }
        assert_eq!(parallel_threshold(), baseline);
    }

    #[test]
    fn set_max_loop_iterations_returns_previous_and_updates() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_max_loop_iterations(42);
        assert_eq!(max_loop_iterations(), 42);
        let previous = set_max_loop_iterations(original);
        assert_eq!(previous, 42);
    }

    #[test]
    fn scoped_max_loop_iterations_takes_precedence_and_restores_on_drop() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let baseline = max_loop_iterations();
        {
            let _guard = scoped_max_loop_iterations(99);
            assert_eq!(max_loop_iterations(), 99);
            {
                let _inner_guard = scoped_max_loop_iterations(55);
                assert_eq!(max_loop_iterations(), 55);
            }
            assert_eq!(max_loop_iterations(), 99);
        }
        assert_eq!(max_loop_iterations(), baseline);
    }

    #[test]
    fn set_max_pipeline_steps_returns_previous_and_updates() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_max_pipeline_steps(7);
        assert_eq!(max_pipeline_steps(), 7);
        let previous = set_max_pipeline_steps(original);
        assert_eq!(previous, 7);
    }

    #[test]
    fn scoped_max_pipeline_steps_takes_precedence_and_restores_on_drop() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let baseline = max_pipeline_steps();
        {
            let _guard = scoped_max_pipeline_steps(3);
            assert_eq!(max_pipeline_steps(), 3);
            {
                let _inner_guard = scoped_max_pipeline_steps(1);
                assert_eq!(max_pipeline_steps(), 1);
            }
            assert_eq!(max_pipeline_steps(), 3);
        }
        assert_eq!(max_pipeline_steps(), baseline);
    }
}
