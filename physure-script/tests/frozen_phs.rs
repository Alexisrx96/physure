//! PHS under a frozen configuration: the parser judges a word after a number a unit by the catalog in force, read
//! on every use (it was cached once per process, so a frozen file was never seen by it), and a frozen file that
//! cannot be used is an ordinary error, not a panic.
//!
//! These tests change the current directory and the cache of the configuration in force: they serialise on one
//! lock and put everything back when they end.

use physure_core::frozen::active::{reset_cache, ENV_CONF, ENV_REQUIRE, FROZEN_FILE_NAME};
use physure_core::frozen::{engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Source};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// A temporary current directory; the previous one and the cache are restored on drop.
struct Dir {
    path: PathBuf,
    previous: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Dir {
    fn new(name: &str) -> Dir {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for variable in [ENV_CONF, ENV_REQUIRE] {
            assert!(std::env::var_os(variable).is_none(), "{variable} is set in the environment of the tests: unset it");
        }
        let path = std::env::temp_dir().join(format!("physure_phs_frozen_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();
        reset_cache();
        Dir { path, previous, _lock: lock }
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
        reset_cache();
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A valid frozen file: the embedded catalog, and `overrides` as source 2.
fn frozen(overrides: &str) -> String {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap(),
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

fn metres(source: &str) -> Option<f64> {
    match eval_phs(source).ok()?.into_iter().last() {
        Some(PhsValue::Quantity(q)) => Some(q.to("m").ok()?.value.mean()),
        _ => None,
    }
}

#[test]
fn a_unit_of_the_frozen_file_is_a_unit_for_phs_and_without_it_is_not() {
    let dir = Dir::new("sentinel");
    // first without the file, as a long-running process would have read the catalog before
    assert_eq!(metres("3 m"), Some(3.0));
    assert_eq!(metres("3 zz_unit"), None, "zz_unit is not a unit of the library");
    assert_eq!(metres("zz_unit"), None, "a free name that is not a unit");
    fs::write(dir.path.join(FROZEN_FILE_NAME), frozen("[Units]\nzz_unit = 2.0, L, [zz_unit]\n")).unwrap();
    reset_cache();
    assert_eq!(metres("3 zz_unit"), Some(6.0), "the frozen file's unit");
    assert_eq!(metres("3 zz_unit + 1 m"), Some(7.0));
    // the parser's own judgement of what is a unit (a unit chain, a free name) reads the catalog in force too
    assert_eq!(metres("(3 zz_unit / s) * 1 s"), Some(6.0), "the unit chain `zz_unit / s`");
    assert_eq!(metres("zz_unit"), Some(2.0), "a free name that is a unit of the frozen file");
    fs::remove_file(dir.path.join(FROZEN_FILE_NAME)).unwrap();
    reset_cache();
    assert_eq!(metres("3 zz_unit"), None);
    assert_eq!(metres("zz_unit"), None);
}

#[test]
fn a_broken_frozen_file_is_an_error_of_phs_not_a_panic() {
    let dir = Dir::new("broken");
    let text = frozen("").replacen("avogadro_constant = 6.02214076e+23", "avogadro_constant = 6.02214077e+23", 1);
    fs::write(dir.path.join(FROZEN_FILE_NAME), text).unwrap();
    reset_cache();
    let outcome = std::panic::catch_unwind(|| eval_phs("3 m + 1 m"));
    let error = outcome.expect("evaluating panicked").unwrap_err();
    assert!(error.to_string().contains(FROZEN_FILE_NAME), "{error}");
    let checked = std::panic::catch_unwind(|| physure_script::check_source("x = 3 m"));
    assert!(checked.expect("checking panicked").is_err());
}

fn holds(source: &str) -> bool {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source}: {e}")).into_iter().last() {
        Some(PhsValue::Bool(b)) => b,
        other => panic!("{source}: {other:?}"),
    }
}

#[test]
fn approx_uses_the_tolerance_of_the_frozen_file_and_the_library_one_without_it() {
    let dir = Dir::new("approx");
    assert!(holds("1 km ≈ 1.0009 km"), "the library's approx_rel_tol is 1e-3");
    fs::write(dir.path.join(FROZEN_FILE_NAME), frozen("[Settings]\napprox_rel_tol = 1e-6\n")).unwrap();
    reset_cache();
    assert!(!holds("1 km ≈ 1.0009 km"), "the frozen file's approx_rel_tol is 1e-6");
    assert!(holds("1 km ≈ 1.0000009 km"));
    assert!(holds("approx(1 km, 1.0009 km, 1e-3)"), "a tolerance given in the call wins");
    assert!(!holds("approx(1 km, 1.0009 km)"));
    fs::remove_file(dir.path.join(FROZEN_FILE_NAME)).unwrap();
    reset_cache();
    assert!(holds("1 km ≈ 1.0009 km"), "the library's tolerance is back");
    assert_eq!(physure_core::settings::approx_rel_tol(), 1e-3);
}

/// `y = (10 +/- 1 m)^2` in Monte Carlo mode under seed 0, as `(mean, standard uncertainty, sample count)`.
fn monte_carlo_square() -> (f64, f64, usize) {
    let values = physure_core::uncertainty::with_seed(0, || eval_phs("y = (10 +/- 1 m)^2")).unwrap();
    match values.into_iter().last() {
        Some(PhsValue::Quantity(q)) => match &q.value {
            physure_core::uncertainty::UncertaintyValue::MonteCarlo(mc) => (q.value.mean(), q.value.std_dev(), mc.samples.len()),
            other => panic!("not Monte Carlo: {}", other.get_model_name()),
        },
        other => panic!("{other:?}"),
    }
}

/// `[Settings] monte_carlo_samples` of a frozen file is the sample count of a Monte Carlo literal; a file that does
/// not set it gives the bits of `phs eval --seed 0` before the setting existed (`100.77165846524241 ±
/// 20.343001315331314`, 1000 samples).
#[test]
fn a_monte_carlo_literal_draws_the_sample_count_of_the_frozen_file() {
    let dir = Dir::new("samples");
    fs::write(dir.path.join(FROZEN_FILE_NAME), frozen("[Settings]\npropagation_mode = monte_carlo\n")).unwrap();
    reset_cache();
    assert_eq!(monte_carlo_square(), (100.77165846524241, 20.343001315331314, 1000), "today's bits");
    fs::write(dir.path.join(FROZEN_FILE_NAME), frozen("[Settings]\npropagation_mode = monte_carlo\nmonte_carlo_samples = 50\n")).unwrap();
    reset_cache();
    let (mean, u, count) = monte_carlo_square();
    assert_eq!(count, 50);
    assert_ne!((mean, u), (100.77165846524241, 20.343001315331314));
    fs::remove_file(dir.path.join(FROZEN_FILE_NAME)).unwrap();
    reset_cache();
    assert_eq!(physure_core::settings::monte_carlo_samples(), 1000, "the library's count is back");
}
