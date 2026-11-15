//! Process-wide settings read from `physure.conf`.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub(crate) const DEFAULT_PARALLEL_THRESHOLD: usize = 10_000;
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

pub(crate) const DEFAULT_MAX_LOOP_ITERATIONS: usize = 1_000_000;
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

pub(crate) const DEFAULT_MAX_PIPELINE_STEPS: usize = 1_000;
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

/// The relative tolerance of PHS `≈` when none is given (`approx(a, b)`), from `physure.conf`'s
/// `[Settings] approx_rel_tol`: `a ≈ b` is `|a - b| <= approx_rel_tol * max(|a|, |b|)` on the canonical magnitudes.
pub(crate) const DEFAULT_APPROX_REL_TOL: f64 = 1e-3;
/// The bits of an `f64`: there is no atomic float, and the bits round-trip exactly.
static APPROX_REL_TOL: AtomicU64 = AtomicU64::new(0x3F50_624D_D2F1_A9FC);

/// The relative tolerance PHS `≈` uses (see [`DEFAULT_APPROX_REL_TOL`]). Process-wide, read the same way by every
/// thread (rayon workers included), like the other settings; there is no thread-local override.
pub fn approx_rel_tol() -> f64 {
    f64::from_bits(APPROX_REL_TOL.load(Ordering::Relaxed))
}

/// Sets the process-wide `≈` tolerance, returning the one it replaced. The conf readers validate it (finite, in
/// `(0, 1)`); a value outside that range makes every `≈` an error rather than a guess.
pub fn set_approx_rel_tol(rel: f64) -> f64 {
    f64::from_bits(APPROX_REL_TOL.swap(rel.to_bits(), Ordering::Relaxed))
}

pub(crate) const DEFAULT_MAX_WHILE_ITERATIONS: usize = 10_000;
static MAX_WHILE_ITERATIONS: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_WHILE_ITERATIONS);

thread_local! {
    static WHILE_OVERRIDE: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Ceiling on how many times one PHS `while` loop may run its body before it is an error ("while loop did not
/// converge after N iterations"), from `physure.conf`'s `[Settings] max_while_iterations`. Default 10,000, the
/// ceiling the interpreter hard-coded before the setting existed. At least 1 (a ceiling of 0 would refuse every loop
/// whose condition holds once).
///
/// A [`scoped_max_while_iterations`] override on this thread (if set) takes precedence; otherwise, the process-wide
/// value from [`set_max_while_iterations`] is returned.
pub fn max_while_iterations() -> usize {
    WHILE_OVERRIDE.with(Cell::get).unwrap_or_else(|| MAX_WHILE_ITERATIONS.load(Ordering::Relaxed))
}

/// Sets the process-wide `while` ceiling, returning the one it replaced. This is what reading a `physure.conf`
/// calls; for a temporary change, use [`scoped_max_while_iterations`].
pub fn set_max_while_iterations(n: usize) -> usize {
    MAX_WHILE_ITERATIONS.swap(n, Ordering::Relaxed)
}

/// Overrides the `while` ceiling on this thread until the returned guard is dropped.
pub fn scoped_max_while_iterations(n: usize) -> MaxWhileIterationsGuard {
    MaxWhileIterationsGuard(WHILE_OVERRIDE.with(|slot| slot.replace(Some(n))))
}

/// Restores the ceiling that was in force before its [`scoped_max_while_iterations`] call, including on unwind.
pub struct MaxWhileIterationsGuard(Option<usize>);

impl Drop for MaxWhileIterationsGuard {
    fn drop(&mut self) {
        WHILE_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

/// How many samples a Monte Carlo measurement draws when it does not name a count, from `physure.conf`'s
/// `[Settings] monte_carlo_samples`: the count every such measurement drew before the setting existed.
pub(crate) const DEFAULT_MONTE_CARLO_SAMPLES: usize = 1_000;
/// The fewest samples the setting accepts: the standard uncertainty is the sample standard deviation (JCGM 101:2008
/// 7.6), which needs two.
pub const MIN_MONTE_CARLO_SAMPLES: usize = 2;
/// The most samples the setting accepts. Every Monte Carlo measurement allocates an array of this many `f64`s, so a
/// file received from elsewhere (under `phs conf verify --run` or `phs serve`) could otherwise make each literal
/// allocate without bound and the allocator abort the process, as `max_loop_iterations` documents: ten million is
/// 80 MB per measurement.
pub const MAX_MONTE_CARLO_SAMPLES: usize = 10_000_000;
static MONTE_CARLO_SAMPLES: AtomicUsize = AtomicUsize::new(DEFAULT_MONTE_CARLO_SAMPLES);

thread_local! {
    static SAMPLES_OVERRIDE: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The sample count of a Monte Carlo measurement that does not name one (`Quantity::new_scalar` with `samples:
/// None`, every Monte Carlo literal of PHS, Python's `Q_` in Monte Carlo mode). A count given explicitly wins.
///
/// A [`scoped_monte_carlo_samples`] override on this thread (if set) takes precedence; otherwise, the process-wide
/// value from [`set_monte_carlo_samples`] is returned.
pub fn monte_carlo_samples() -> usize {
    SAMPLES_OVERRIDE.with(Cell::get).unwrap_or_else(|| MONTE_CARLO_SAMPLES.load(Ordering::Relaxed))
}

/// Sets the process-wide sample count, returning the one it replaced. The conf readers validate it (from
/// [`MIN_MONTE_CARLO_SAMPLES`] to [`MAX_MONTE_CARLO_SAMPLES`]); for a temporary change, use
/// [`scoped_monte_carlo_samples`].
pub fn set_monte_carlo_samples(n: usize) -> usize {
    MONTE_CARLO_SAMPLES.swap(n, Ordering::Relaxed)
}

/// Overrides the sample count on this thread until the returned guard is dropped.
pub fn scoped_monte_carlo_samples(n: usize) -> MonteCarloSamplesGuard {
    MonteCarloSamplesGuard(SAMPLES_OVERRIDE.with(|slot| slot.replace(Some(n))))
}

/// Restores the sample count that was in force before its [`scoped_monte_carlo_samples`] call, including on unwind.
pub struct MonteCarloSamplesGuard(Option<usize>);

impl Drop for MonteCarloSamplesGuard {
    fn drop(&mut self) {
        SAMPLES_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

/// Puts every process-wide setting a conf can change back to the value compiled into the library: the three
/// ceilings above, the `while` ceiling, the `≈` tolerance, the Monte Carlo sample count, the asymmetric shape, the default degrees of freedom, the Monte Carlo seed, the propagation mode, the angle mode and the
/// `[Compatibility]` grants. Activating a frozen
/// configuration calls it before applying the file's settings, so a setting the file does not mention takes its
/// compiled default and never a value some earlier reading left behind. Thread-local overrides are not touched.
pub(crate) fn reset_to_defaults() {
    set_parallel_threshold(DEFAULT_PARALLEL_THRESHOLD);
    set_max_loop_iterations(DEFAULT_MAX_LOOP_ITERATIONS);
    set_max_pipeline_steps(DEFAULT_MAX_PIPELINE_STEPS);
    set_approx_rel_tol(DEFAULT_APPROX_REL_TOL);
    set_monte_carlo_samples(DEFAULT_MONTE_CARLO_SAMPLES);
    set_max_while_iterations(DEFAULT_MAX_WHILE_ITERATIONS);
    crate::uncertainty::shapes::set_asymmetric_shape(crate::uncertainty::ShapeKind::DEFAULT);
    crate::uncertainty::dof::set_default_dof(f64::INFINITY);
    crate::uncertainty::set_monte_carlo_seed(crate::uncertainty::MonteCarloSeed::DEFAULT);
    crate::uncertainty::set_propagation_mode(crate::uncertainty::PropagationMode::default());
    crate::units::angle_mode::set_angle_mode(crate::units::angle_mode::AngleMode::default());
    crate::units::angle_mode::set_marker_grants(true, true);
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
    fn the_stored_default_is_the_default() {
        assert_eq!(f64::from_bits(0x3F50_624D_D2F1_A9FC), DEFAULT_APPROX_REL_TOL);
    }

    #[test]
    fn set_approx_rel_tol_returns_previous_and_updates() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_approx_rel_tol(1e-6);
        assert_eq!(approx_rel_tol(), 1e-6);
        assert_eq!(set_approx_rel_tol(original), 1e-6);
        assert_eq!(DEFAULT_APPROX_REL_TOL, 1e-3);
    }

    #[test]
    fn reset_to_defaults_restores_approx_rel_tol() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let (threshold, iterations, steps) = (parallel_threshold(), max_loop_iterations(), max_pipeline_steps());
        let mode = crate::uncertainty::mode::propagation_mode();
        let original = set_approx_rel_tol(0.25);
        reset_to_defaults();
        assert_eq!(approx_rel_tol(), DEFAULT_APPROX_REL_TOL);
        set_approx_rel_tol(original);
        set_parallel_threshold(threshold);
        set_max_loop_iterations(iterations);
        set_max_pipeline_steps(steps);
        crate::uncertainty::set_propagation_mode(mode);
    }

    #[test]
    fn set_monte_carlo_samples_returns_previous_and_updates() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_monte_carlo_samples(4321);
        assert_eq!(monte_carlo_samples(), 4321);
        assert_eq!(set_monte_carlo_samples(original), 4321);
        assert_eq!(DEFAULT_MONTE_CARLO_SAMPLES, 1000);
        assert_eq!((MIN_MONTE_CARLO_SAMPLES, MAX_MONTE_CARLO_SAMPLES), (2, 10_000_000));
    }

    #[test]
    fn scoped_monte_carlo_samples_takes_precedence_and_restores_on_drop() {
        let baseline = monte_carlo_samples();
        {
            let _guard = scoped_monte_carlo_samples(17);
            assert_eq!(monte_carlo_samples(), 17);
            {
                let _inner = scoped_monte_carlo_samples(5);
                assert_eq!(monte_carlo_samples(), 5);
            }
            assert_eq!(monte_carlo_samples(), 17);
        }
        assert_eq!(monte_carlo_samples(), baseline);
    }

    #[test]
    fn reset_to_defaults_restores_monte_carlo_samples() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let original = set_monte_carlo_samples(3);
        reset_to_defaults();
        assert_eq!(monte_carlo_samples(), DEFAULT_MONTE_CARLO_SAMPLES);
        set_monte_carlo_samples(original);
    }

    #[test]
    fn max_while_iterations_is_set_scoped_and_reset() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(DEFAULT_MAX_WHILE_ITERATIONS, 10_000);
        let original = set_max_while_iterations(42);
        assert_eq!(max_while_iterations(), 42);
        {
            let _guard = scoped_max_while_iterations(7);
            assert_eq!(max_while_iterations(), 7);
            {
                let _inner = scoped_max_while_iterations(3);
                assert_eq!(max_while_iterations(), 3);
            }
            assert_eq!(max_while_iterations(), 7);
        }
        assert_eq!(max_while_iterations(), 42);
        reset_to_defaults();
        assert_eq!(max_while_iterations(), DEFAULT_MAX_WHILE_ITERATIONS);
        set_max_while_iterations(original);
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
