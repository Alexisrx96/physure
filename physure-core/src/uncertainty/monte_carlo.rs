//! The Monte Carlo backend and where its random draws come from.
//!
//! # The seed policy (`[Settings] monte_carlo_seed`, conf freeze Phase 4b)
//!
//! Every draw (one [`MonteCarloBackend::from_stats`] or [`MonteCarloBackend::ensure_samples`] call) takes a fresh
//! `StdRng` seeded from `seed ^ n * 0x9E3779B97F4A7C15`, where `n` counts the draws of its stream. Which stream a
//! draw belongs to:
//!
//! * inside an **evaluation unit** (an [`EvaluationStream::run`] scope: one program run, one REPL session, one
//!   `phs serve` request, one interpreter of a host, a [`with_seed`] scope), the unit's stream. It starts at the
//!   setting's seed (or an explicit one) and advances with every draw, so two measurements of one program get
//!   independent samples and the same program gives the same samples on every run;
//! * outside any unit (Python's `Q_`, a rayon worker), the **stream of the thread**, created at the thread's first
//!   draw from [`mix`]`(seed, k)` where `k` counts the threads of the process that drew so far: independent across
//!   threads, reproducible for one thread doing the same operations in the same order, **not** reproducible across
//!   threads (which thread is `k` depends on scheduling).
//!
//! `monte_carlo_seed = random` makes every unit and every thread stream draw from operating-system entropy (the
//! behaviour before the setting existed). The setting is read when a unit starts or a thread stream is created: an
//! activation of another configuration (or `frozen::reset_cache`) never restarts a live stream, so a long-running
//! host (the LSP, a REPL) never hands a new measurement the samples of an older one.
//!
//! Two properties of the per-draw mixing that are kept because changing them would change every seeded bit:
//! `seed s` draw `n + 1` equals `seed s ^ 0x9E3779B97F4A7C15` draw `n` (different seeds, shifted streams), and a
//! draw of a zero standard deviation (which needs no random number) still advances the stream.

use ndarray::Array1;
use rand::prelude::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use rand_distr::{Normal, Distribution};
use crate::error::{PhysureError, PhysureResult};
use super::trait_def::UncertaintyBackend;

/// The odd constant of the per-draw mixing (`seed ^ n * GOLDEN`), the 64-bit golden ratio.
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// What `[Settings] monte_carlo_seed` says: a fixed seed (the compiled default is 0) or operating-system entropy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonteCarloSeed {
    Fixed(u64),
    Random,
}

impl MonteCarloSeed {
    /// The compiled default: a fixed seed of 0, so an unseeded program gives the same answer on every run.
    pub const DEFAULT: MonteCarloSeed = MonteCarloSeed::Fixed(0);

    /// The text of the setting: the number, or `random`.
    pub fn name(self) -> String {
        match self {
            MonteCarloSeed::Fixed(seed) => seed.to_string(),
            MonteCarloSeed::Random => "random".to_string(),
        }
    }
}

impl std::str::FromStr for MonteCarloSeed {
    type Err = String;

    /// A whole number from 0 to 2^64 - 1, or the word `random`.
    fn from_str(raw: &str) -> Result<Self, String> {
        match raw {
            "random" => Ok(MonteCarloSeed::Random),
            _ => raw
                .parse::<u64>()
                .map(MonteCarloSeed::Fixed)
                .map_err(|_| format!("invalid monte_carlo_seed '{raw}': a whole number from 0 to {}, or random", u64::MAX)),
        }
    }
}

static CONFIGURED_SEED: Mutex<MonteCarloSeed> = Mutex::new(MonteCarloSeed::DEFAULT);

thread_local! {
    static SEED_OVERRIDE: Cell<Option<MonteCarloSeed>> = const { Cell::new(None) };
}

/// The seed setting in force: a [`scoped_monte_carlo_seed`] override on this thread, or the process-wide value from
/// [`set_monte_carlo_seed`].
pub fn monte_carlo_seed() -> MonteCarloSeed {
    SEED_OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| *CONFIGURED_SEED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Sets the process-wide seed setting, returning the one it replaced. Live streams are not restarted (see the module
/// documentation): units started and thread streams created afterwards read it.
pub fn set_monte_carlo_seed(seed: MonteCarloSeed) -> MonteCarloSeed {
    std::mem::replace(&mut *CONFIGURED_SEED.lock().unwrap_or_else(|e| e.into_inner()), seed)
}

/// Overrides the seed setting on this thread until the returned guard is dropped.
pub fn scoped_monte_carlo_seed(seed: MonteCarloSeed) -> MonteCarloSeedGuard {
    MonteCarloSeedGuard(SEED_OVERRIDE.with(|slot| slot.replace(Some(seed))))
}

/// Restores the seed setting that was in force before its [`scoped_monte_carlo_seed`] call, including on unwind.
pub struct MonteCarloSeedGuard(Option<MonteCarloSeed>);

impl Drop for MonteCarloSeedGuard {
    fn drop(&mut self) {
        SEED_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

/// Where the next draws of a stream come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    /// `draws` draws taken so far from the stream of `seed`.
    Seeded { seed: u64, draws: u64 },
    /// Operating-system entropy, every draw.
    Entropy,
}

impl Stream {
    /// The generator of the next draw, and the stream after it.
    fn draw(self) -> (StdRng, Stream) {
        match self {
            Stream::Seeded { seed, draws } => {
                (StdRng::seed_from_u64(seed ^ draws.wrapping_mul(GOLDEN)), Stream::Seeded { seed, draws: draws + 1 })
            }
            Stream::Entropy => (StdRng::from_rng(thread_rng()).expect("thread_rng does not fail"), Stream::Entropy),
        }
    }
}

thread_local! {
    /// The stream of the evaluation unit open on this thread, if one is.
    static UNIT: Cell<Option<Stream>> = const { Cell::new(None) };
    /// The thread's own stream (for draws outside any unit), created at its first draw.
    static THREAD_STREAM: Cell<Option<Stream>> = const { Cell::new(None) };
}

/// How many threads of this process created their own stream so far: the `k` of [`mix`]`(seed, k)`.
static THREADS_THAT_DREW: AtomicU64 = AtomicU64::new(0);

/// `splitmix64`'s finaliser: a bijection of `u64` whose output bits each depend on every input bit.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(GOLDEN);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The seed of the stream number `key` derived from `base`: `splitmix64(base ^ splitmix64(key))`. Non-linear, so
/// unlike `base ^ key * c` it does not make one derived stream a shifted copy of another (the per-draw mixing is
/// linear in the draw counter).
pub fn mix(base: u64, key: u64) -> u64 {
    splitmix64(base ^ splitmix64(key))
}

/// The stream a thread starts with at its first draw outside any unit, from the setting in force.
fn new_thread_stream() -> Stream {
    match monte_carlo_seed() {
        MonteCarloSeed::Fixed(base) => Stream::Seeded { seed: mix(base, THREADS_THAT_DREW.fetch_add(1, Ordering::Relaxed)), draws: 0 },
        MonteCarloSeed::Random => Stream::Entropy,
    }
}

/// The random stream of one evaluation unit (see the module documentation): a position in a seeded stream, or
/// entropy. A host that keeps an evaluation alive across calls (an interpreter of the LSP, a REPL session, a WASM or
/// Java handle) keeps one of these with it and runs every call in it, so the calls continue one stream instead of
/// each starting it again (which would give a new measurement the samples of an older one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationStream(Stream);

impl EvaluationStream {
    /// The start of the stream of `seed`.
    pub fn seeded(seed: u64) -> Self {
        EvaluationStream(Stream::Seeded { seed, draws: 0 })
    }

    /// Operating-system entropy for every draw.
    pub fn entropy() -> Self {
        EvaluationStream(Stream::Entropy)
    }

    /// The start of the stream the seed setting in force names ([`monte_carlo_seed`]).
    pub fn from_setting() -> Self {
        match monte_carlo_seed() {
            MonteCarloSeed::Fixed(seed) => Self::seeded(seed),
            MonteCarloSeed::Random => Self::entropy(),
        }
    }

    /// The seed, `None` for entropy.
    pub fn seed(&self) -> Option<u64> {
        match self.0 {
            Stream::Seeded { seed, .. } => Some(seed),
            Stream::Entropy => None,
        }
    }

    /// How many draws were taken from this stream so far, `None` for entropy.
    pub fn draws(&self) -> Option<u64> {
        match self.0 {
            Stream::Seeded { draws, .. } => Some(draws),
            Stream::Entropy => None,
        }
    }

    /// Runs `f` as an evaluation unit at this position: every draw `f` makes on this thread comes from this stream,
    /// which then stands where `f` left it (also on unwind). A unit already open on this thread is replaced for the
    /// duration and put back after.
    pub fn run<R>(&mut self, f: impl FnOnce() -> R) -> R {
        struct Scope<'a> {
            stream: &'a mut EvaluationStream,
            previous: Option<Stream>,
        }
        impl Drop for Scope<'_> {
            fn drop(&mut self) {
                if let Some(now) = UNIT.with(|unit| unit.replace(self.previous)) {
                    self.stream.0 = now;
                }
            }
        }
        let previous = UNIT.with(|unit| unit.replace(Some(self.0)));
        let _scope = Scope { stream: self, previous };
        f()
    }
}

/// Whether an evaluation unit is open on this thread.
pub fn in_evaluation_unit() -> bool {
    UNIT.with(|unit| unit.get().is_some())
}

/// Runs `f` with every Monte Carlo draw on this thread taken from the stream of `seed`, from its start, so the same
/// sequence of operations gives the same samples (an explicit seed: `phs eval --seed`, the witnesses). Draws made on
/// another thread (a rayon worker) are not part of it; PHS keeps a loop in `monte_carlo` mode on one thread.
pub fn with_seed<R>(seed: u64, f: impl FnOnce() -> R) -> R {
    EvaluationStream::seeded(seed).run(f)
}

/// Runs `f` as an evaluation unit of the seed setting in force ([`EvaluationStream::from_setting`]), unless a unit is
/// already open on this thread: then `f` continues it (an explicit `--seed` of an enclosing scope wins, and nested
/// evaluations do not restart the stream).
pub fn with_evaluation_seed<R>(f: impl FnOnce() -> R) -> R {
    if in_evaluation_unit() {
        f()
    } else {
        EvaluationStream::from_setting().run(f)
    }
}

/// The generator for the next draw: from the evaluation unit open on this thread, or else the thread's own stream.
fn next_rng() -> StdRng {
    if let Some(stream) = UNIT.with(Cell::get) {
        let (rng, next) = stream.draw();
        UNIT.with(|unit| unit.set(Some(next)));
        return rng;
    }
    THREAD_STREAM.with(|slot| {
        let (rng, next) = slot.get().unwrap_or_else(new_thread_stream).draw();
        slot.set(Some(next));
        rng
    })
}

/// Monte Carlo uncertainty propagation backend.
///
/// Implements probability distribution propagation compliant with GUM Supplement 1
/// (JCGM 101:2008: Evaluation of measurement data — Supplement 1 to the GUM — Propagation
/// of distributions using a Monte Carlo method).
#[derive(Clone)]
pub struct MonteCarloBackend {
    pub samples: Array1<f64>,
}

impl MonteCarloBackend {
    /// Draws `n_samples` values of a normal distribution with this mean and standard deviation.
    ///
    /// Refuses fewer than two samples: the standard uncertainty is the sample standard deviation (JCGM 101:2008
    /// 7.6), which needs `M >= 2`. One sample would report a confident zero uncertainty and none a mean of `0.0`
    /// (audit B8). Refuses more than [`crate::settings::MAX_MONTE_CARLO_SAMPLES`] too: the draw allocates that many
    /// `f64`s, and a count from a user must not allocate without bound (audit T9).
    pub fn from_stats(mean: f64, std_dev: f64, n_samples: usize) -> PhysureResult<Self> {
        if n_samples < 2 {
            return Err(PhysureError::DomainError(format!(
                "a Monte Carlo evaluation needs at least 2 samples to estimate a standard uncertainty, got {n_samples}"
            )));
        }
        let max = crate::settings::MAX_MONTE_CARLO_SAMPLES;
        if n_samples > max {
            return Err(PhysureError::DomainError(format!(
                "a Monte Carlo evaluation takes at most {max} samples, got {n_samples}"
            )));
        }
        let mut rng = next_rng();
        if std_dev == 0.0 {
            return Ok(MonteCarloBackend { samples: Array1::from_elem(n_samples, mean) });
        }
        let dist = Normal::new(mean, std_dev).map_err(|e| PhysureError::DomainError(e.to_string()))?;
        Ok(MonteCarloBackend { samples: Array1::from_shape_fn(n_samples, |_| dist.sample(&mut rng)) })
    }

    /// Draws a fresh, independent sample array matching `other`'s moments.
    ///
    /// Only for operands that carry no samples of their own (a Gaussian, an Unscented, a
    /// custom backend) or whose array has a different length and therefore cannot be paired
    /// elementwise. When the other operand *is* a Monte Carlo backend of the same length,
    /// its own samples must be reused instead — see
    /// `UncertaintyValue::mc_operand_samples` in `trait_def.rs`, which is the live path for
    /// `Quantity` arithmetic.
    pub fn ensure_samples(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Array1<f64>> {
        let n = self.samples.len();
        let mut rng = next_rng();
        let m = other.mean();
        let s = other.std_dev();
        if s == 0.0 {
            return Ok(Array1::from_elem(n, m));
        }
        let dist = Normal::new(m, s).map_err(|e| PhysureError::Generic(e.to_string()))?;
        Ok(Array1::from_shape_fn(n, |_| dist.sample(&mut rng)))
    }
}

// NOTE: the `propagate_*` methods below resample the other operand unconditionally, which
// loses correlation between quantities that share samples. They are not the path `Quantity`
// arithmetic takes: `UncertaintyValue::propagate_*` in `trait_def.rs` matches
// `Self::MonteCarlo(..)` as the left operand first and reuses the right operand's array
// there. These impls remain only to satisfy the `dyn UncertaintyBackend` trait object, so
// fixing them too would be dead code — fix `trait_def.rs` if the behaviour needs to change.
impl UncertaintyBackend for MonteCarloBackend {
    fn mean(&self) -> f64 { self.samples.mean().unwrap_or(0.0) }
    /// The sample standard deviation, `sqrt(sum (x_r - mean)^2 / (M - 1))` (JCGM 101:2008 7.6, eqs. (13) and
    /// (14)). [`MonteCarloBackend::from_stats`] refuses `M < 2`; an array built by hand with fewer samples has no
    /// standard deviation to estimate and gives NaN, never a confident zero.
    fn std_dev(&self) -> f64 {
        if self.samples.len() < 2 {
            return f64::NAN;
        }
        self.samples.std(1.0)
    }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples + &other_samples }))
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples - &other_samples }))
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples * &other_samples }))
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let other_samples = self.ensure_samples(other)?;
        Ok(Box::new(MonteCarloBackend { samples: &self.samples / &other_samples }))
    }
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        Ok(Box::new(MonteCarloBackend { samples: self.samples.mapv(|x| x.powf(exponent)) }))
    }
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_samples = match func {
            "sin" => self.samples.mapv(|x| x.sin()),
            "cos" => self.samples.mapv(|x| x.cos()),
            "exp" => self.samples.mapv(|x| x.exp()),
            "log" => self.samples.mapv(|x| x.ln()),
            "log10" => self.samples.mapv(|x| x.log10()),
            "pow10" => self.samples.mapv(|x| 10f64.powf(x)),
            "abs" => self.samples.mapv(|x| x.abs()),
            "tan" => self.samples.mapv(|x| x.tan()),
            "tanh" => self.samples.mapv(|x| x.tanh()),
            "erf" => self.samples.mapv(crate::math::erf),
            _ => self.samples.clone(),
        };
        Ok(Box::new(MonteCarloBackend { samples: new_samples }))
    }

    fn get_model_name(&self) -> &str { "monte_carlo" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_samples_and_another_seed_differs() {
        let draw = |seed| with_seed(seed, || MonteCarloBackend::from_stats(10.0, 1.0, 64).unwrap().samples.to_vec());
        assert_eq!(draw(7), draw(7));
        assert_ne!(draw(7), draw(8));
    }

    #[test]
    fn successive_draws_in_one_scope_differ_but_replay_exactly() {
        let two = |seed| {
            with_seed(seed, || {
                (MonteCarloBackend::from_stats(0.0, 1.0, 8).unwrap().samples.to_vec(), MonteCarloBackend::from_stats(0.0, 1.0, 8).unwrap().samples.to_vec())
            })
        };
        let (a, b) = two(3);
        assert_ne!(a, b);
        assert_eq!(two(3), (a, b));
    }

    #[test]
    fn resampling_an_operand_is_seeded_too() {
        let other = MonteCarloBackend::from_stats(5.0, 0.5, 16).unwrap();
        let resample = |seed| with_seed(seed, || MonteCarloBackend::from_stats(1.0, 0.1, 16).unwrap().ensure_samples(&other).unwrap().to_vec());
        assert_eq!(resample(11), resample(11));
        assert_ne!(resample(11), resample(12));
    }

    /// The standard uncertainty of a Monte Carlo result is the sample standard deviation, `1 / (M - 1)` (JCGM
    /// 101:2008 7.6, eqs. (13) and (14)): for the samples 1, 2, 3, 4 the mean is 2.5, the squared deviations sum
    /// to 5, and the uncertainty is `sqrt(5 / 3)`, not the population `sqrt(5 / 4)` (audit B8).
    #[test]
    fn the_standard_uncertainty_is_the_sample_standard_deviation() {
        let mc = MonteCarloBackend { samples: Array1::from_vec(vec![1.0, 2.0, 3.0, 4.0]) };
        assert_eq!(mc.mean(), 2.5);
        let expected = (5.0_f64 / 3.0).sqrt();
        assert!((mc.std_dev() - expected).abs() <= 2.0 * f64::EPSILON * expected, "{} vs {expected}", mc.std_dev());
    }

    /// A seeded draw replays to the same bits, and its standard uncertainty is the one computed here from its own
    /// samples with `1 / (M - 1)`: derived independently of the backend.
    #[test]
    fn a_seeded_draw_replays_its_bits_and_its_uncertainty_is_the_sample_deviation_of_its_samples() {
        let draw = || with_seed(1, || MonteCarloBackend::from_stats(6.0, 0.5, 1000).unwrap());
        let (a, b) = (draw(), draw());
        assert_eq!(a.mean().to_bits(), b.mean().to_bits());
        assert_eq!(a.std_dev().to_bits(), b.std_dev().to_bits());
        let samples = a.samples.to_vec();
        let m = samples.len() as f64;
        let mean = samples.iter().sum::<f64>() / m;
        let sum_sq: f64 = samples.iter().map(|x| (x - mean) * (x - mean)).sum();
        let expected = (sum_sq / (m - 1.0)).sqrt();
        assert!((a.std_dev() - expected).abs() <= 1e-13 * expected, "{} vs {expected}", a.std_dev());
    }

    /// Fewer than two samples cannot estimate a standard deviation: refused, where `M = 0` used to give a mean of
    /// `0.0` and a NaN uncertainty and `M = 1` a confident zero uncertainty. Two samples are the least accepted.
    #[test]
    fn fewer_than_two_samples_are_refused() {
        for m in [0, 1] {
            let err = MonteCarloBackend::from_stats(5.0, 0.1, m).err().expect("refused");
            assert!(matches!(err, PhysureError::DomainError(_)), "{err:?}");
            assert!(err.to_string().contains("at least 2 samples"), "{err}");
        }
        assert_eq!(MonteCarloBackend::from_stats(5.0, 0.1, 2).unwrap().samples.len(), 2);
        let built = crate::Quantity::try_new_scalar(5.0, 0.1, crate::RationalUnit::dimensionless(), Some("monte_carlo"), Some(1));
        assert!(built.is_err());
        let empty = MonteCarloBackend { samples: Array1::from_vec(vec![]) };
        assert!(empty.std_dev().is_nan());
        let one = MonteCarloBackend { samples: Array1::from_vec(vec![3.0]) };
        assert!(one.std_dev().is_nan(), "one sample has no deviation to report, not a zero");
    }

    /// More samples than the setting allows is refused too: every draw allocates that many `f64`s, so an explicit
    /// count from a user (Python's `samples=`) must not allocate without bound.
    #[test]
    fn more_than_the_maximum_samples_are_refused() {
        let max = crate::settings::MAX_MONTE_CARLO_SAMPLES;
        let err = MonteCarloBackend::from_stats(0.0, 1.0, max + 1).err().expect("refused");
        assert!(matches!(err, PhysureError::DomainError(_)), "{err:?}");
        assert!(err.to_string().contains("at most 10000000 samples"), "{err}");
        assert_eq!(MonteCarloBackend::from_stats(0.0, 1.0, 2).unwrap().samples.len(), 2);
    }

    fn sample_count(q: &crate::Quantity) -> usize {
        match &q.value {
            crate::uncertainty::UncertaintyValue::MonteCarlo(mc) => mc.samples.len(),
            other => panic!("not Monte Carlo: {}", other.get_model_name()),
        }
    }

    /// `[Settings] monte_carlo_samples` is the count of a Monte Carlo measurement that does not name one; a count
    /// given explicitly wins. The compiled default is 1000, the count every such measurement drew before the
    /// setting existed, and a seeded draw under it gives the bits it gave then.
    #[test]
    fn a_measurement_that_names_no_sample_count_draws_the_setting_in_force() {
        let unit = crate::RationalUnit::dimensionless;
        let draw = |samples| crate::Quantity::new_scalar(5.0, 0.1, unit(), Some("monte_carlo"), samples);
        {
            let _default = crate::settings::scoped_monte_carlo_samples(crate::settings::DEFAULT_MONTE_CARLO_SAMPLES);
            assert_eq!(sample_count(&draw(None)), 1000);
            let explicit = with_seed(9, || draw(Some(1000)));
            let defaulted = with_seed(9, || draw(None));
            assert_eq!(explicit.value.mean().to_bits(), defaulted.value.mean().to_bits());
            assert_eq!(explicit.value.std_dev().to_bits(), defaulted.value.std_dev().to_bits());
        }
        let _seven = crate::settings::scoped_monte_carlo_samples(7);
        assert_eq!(sample_count(&draw(None)), 7);
        assert_eq!(sample_count(&draw(Some(12))), 12, "an explicit count wins");
        let _mode = crate::uncertainty::mode::scoped(crate::uncertainty::PropagationMode::MonteCarlo);
        let by_mode = crate::Quantity::new_scalar(5.0, 0.1, unit(), None, None);
        assert_eq!(sample_count(&by_mode), 7, "a measurement made Monte Carlo by the propagation mode too");
    }

    fn samples() -> Vec<f64> {
        MonteCarloBackend::from_stats(0.0, 1.0, 64).unwrap().samples.to_vec()
    }

    /// Outside any unit a thread draws from its own stream, which advances (two measurements differ) and is not
    /// disturbed by a unit opened and closed in between.
    #[test]
    fn outside_a_unit_the_thread_stream_advances_and_a_unit_leaves_no_trace() {
        std::thread::spawn(|| {
            let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
            let a = samples();
            with_seed(1, samples);
            let b = samples();
            assert_ne!(a, b);
            assert!(!in_evaluation_unit());
        })
        .join()
        .unwrap();
    }

    /// The stream of a thread is the stream of `mix(seed, k)` for the `k`-th thread of the process that drew: the
    /// seed setting decides it, two threads never start from the same state (the reviewer measured that two threads
    /// each starting the default stream at 0 draw identical samples), and `random` is entropy.
    #[test]
    fn two_threads_draw_from_different_streams_derived_from_the_setting() {
        let first_two = |seed| {
            std::thread::spawn(move || {
                let _seed = scoped_monte_carlo_seed(seed);
                (samples(), samples())
            })
            .join()
            .unwrap()
        };
        let (a, b) = (first_two(MonteCarloSeed::Fixed(5)), first_two(MonteCarloSeed::Fixed(5)));
        assert_ne!(a.0, b.0, "two threads under the same seed setting share no stream");
        assert_ne!(a.0, a.1);
        // each is the stream mix(5, k) for its own k, from its start
        let derived = |samples: &(Vec<f64>, Vec<f64>)| {
            (0..THREADS_THAT_DREW.load(Ordering::Relaxed)).find(|&k| with_seed(mix(5, k), || (self::samples(), self::samples())) == *samples)
        };
        let (ka, kb) = (derived(&a).expect("a derived stream"), derived(&b).expect("a derived stream"));
        assert_ne!(ka, kb);
        // never the stream of the seed itself, which a unit of the same seed draws (no correlation with a unit)
        assert_ne!(a.0, with_seed(5, samples));
        let (c, d) = (first_two(MonteCarloSeed::Random), first_two(MonteCarloSeed::Random));
        assert_ne!(c.0, d.0);
        assert!(derived(&c).is_none(), "random is entropy, not a derived stream");
    }

    #[test]
    fn mix_is_not_a_shift_of_the_per_draw_mixing() {
        // `base ^ k * GOLDEN` would make stream k + 1 the stream k shifted by one draw
        let shifted = |seed| with_seed(seed, || (samples(), samples()));
        assert_ne!(shifted(mix(0, 1)).0, shifted(mix(0, 0)).1);
        // the documented property of the per-draw mixing itself, kept: seed G draw 0 is seed 0 draw 1
        assert_eq!(shifted(GOLDEN).0, shifted(0).1);
        assert_eq!(mix(0, 0), splitmix64(splitmix64(0)));
        assert_ne!(mix(0, 0), mix(0, 1));
    }

    /// An evaluation unit of the setting in force is the stream of its seed from the start; nested units continue the
    /// enclosing one (an explicit seed wins); `random` gives a different stream each time.
    #[test]
    fn a_unit_of_the_setting_is_the_stream_of_its_seed_and_does_not_restart_an_open_one() {
        let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(5));
        assert_eq!(with_evaluation_seed(samples), with_seed(5, samples));
        assert_eq!(with_evaluation_seed(samples), with_evaluation_seed(samples), "reproducible");
        let nested = with_seed(3, || (samples(), with_evaluation_seed(samples)));
        assert_eq!(nested, with_seed(3, || (samples(), samples())), "a nested unit continues the stream of --seed 3");
        let _random = scoped_monte_carlo_seed(MonteCarloSeed::Random);
        assert_ne!(with_evaluation_seed(samples), with_evaluation_seed(samples));
        assert_eq!(EvaluationStream::from_setting(), EvaluationStream::entropy());
        assert_eq!((EvaluationStream::entropy().seed(), EvaluationStream::entropy().draws()), (None, None));
    }

    /// A host that keeps one stream across calls (the LSP, a REPL, a WASM or Java handle) continues it: a measurement
    /// of a later call never gets the samples of an earlier one, even when the configuration is read again in between
    /// (an activation, `reset_cache`: the reviewer's LSP scenario, `x - y` exactly 0).
    #[test]
    fn a_kept_stream_continues_across_calls_and_a_new_reading_of_the_setting_does_not_restart_it() {
        let mut stream = EvaluationStream::seeded(5);
        let x = stream.run(samples);
        assert_eq!(stream.draws(), Some(1));
        crate::frozen::reset_cache();
        let _again = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(5));
        let y = stream.run(samples);
        assert_ne!(x, y);
        assert_eq!((x, y), with_seed(5, || (samples(), samples())), "one unit, two calls");
        assert_eq!((stream.seed(), stream.draws()), (Some(5), Some(2)));
        // and the thread stream likewise: a re-read setting does not restart it
        std::thread::spawn(|| {
            let first = {
                let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(9));
                samples()
            };
            crate::frozen::reset_cache();
            let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(9));
            assert_ne!(first, samples());
        })
        .join()
        .unwrap();
    }

    /// A unit's position is kept when its closure unwinds, and the enclosing unit is put back.
    #[test]
    fn a_unit_is_closed_and_its_position_kept_on_unwind() {
        let mut stream = EvaluationStream::seeded(2);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            stream.run(|| {
                samples();
                panic!("inside a unit")
            })
        }));
        assert!(outcome.is_err());
        assert_eq!(stream.draws(), Some(1));
        assert!(!in_evaluation_unit());
    }

    #[test]
    fn the_seed_setting_reads_a_number_or_random() {
        assert_eq!("0".parse::<MonteCarloSeed>(), Ok(MonteCarloSeed::DEFAULT));
        assert_eq!("random".parse::<MonteCarloSeed>(), Ok(MonteCarloSeed::Random));
        assert!("-1".parse::<MonteCarloSeed>().unwrap_err().contains("'-1'"));
        let baseline = monte_carlo_seed();
        {
            let _outer = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(1));
            {
                let _inner = scoped_monte_carlo_seed(MonteCarloSeed::Random);
                assert_eq!(monte_carlo_seed(), MonteCarloSeed::Random);
            }
            assert_eq!(monte_carlo_seed(), MonteCarloSeed::Fixed(1));
        }
        assert_eq!(monte_carlo_seed(), baseline);
    }

    #[test]
    fn set_monte_carlo_seed_returns_the_previous_and_reset_restores_the_default() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let original = set_monte_carlo_seed(MonteCarloSeed::Fixed(77));
        assert_eq!(monte_carlo_seed(), MonteCarloSeed::Fixed(77));
        assert_eq!(set_monte_carlo_seed(MonteCarloSeed::Random), MonteCarloSeed::Fixed(77));
        crate::settings::reset_to_defaults();
        assert_eq!(monte_carlo_seed(), MonteCarloSeed::DEFAULT);
        set_monte_carlo_seed(original);
    }
}
