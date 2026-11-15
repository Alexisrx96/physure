//! A frozen configuration in force, end to end: resolution from the real process (environment, current directory,
//! the host's setters), the cached `active()`, and the hooked functions that build the catalog for everyone.
//!
//! Everything here touches process-global state, so every test takes one lock and an [`Env`] that restores the
//! environment variables, the current directory, the host's setters and the cache when it is dropped.

use physure_core::frozen::active::{load_count, ENV_CONF, ENV_REQUIRE, FROZEN_FILE_NAME};
use physure_core::frozen::{
    active, engine_pairs, ensure_valid, load_file, refuse_unwired, set_engine_file, set_explicit_conf, ActiveError, EngineIdentity,
    EnginePolicy, EntryInput, FrozenFile, FrozenParts, LoadError, LoadOptions, Pairs, ResolveError, Source, Why,
};
use physure_core::quantity::Quantity;
use physure_core::units::conf::{
    build_registry_from_conf, build_registry_with, load_embedded_catalog, local_conf_builds, try_build_registry_from_conf,
    try_with_conf_registry, with_conf_registry, DEFAULT_PHYSURE_CONF,
};
use physure_core::units::parser::Parser;
use physure_core::UnitRegistry;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Barrier};
use std::time::Duration;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The process as a test found it, restored on drop; a unique, canonical temporary directory that is the current
/// directory while the test runs.
struct Env {
    dir: PathBuf,
    cwd: PathBuf,
    vars: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl Env {
    fn new() -> Env {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let vars = [ENV_CONF, ENV_REQUIRE].into_iter().map(|name| (name, std::env::var_os(name))).collect();
        for name in [ENV_CONF, ENV_REQUIRE] {
            std::env::remove_var(name);
        }
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("physure_active_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        set_explicit_conf(None);
        set_engine_file(None);
        reset();
        Env { dir, cwd, vars, _lock: lock }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.cwd);
        for (name, value) in &self.vars {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        set_explicit_conf(None);
        set_engine_file(None);
        reset();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn reset() {
    physure_core::frozen::active::reset_cache();
}

const R_LINE: &str = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";

/// The embedded catalog with `molar_gas_constant` set to `r` and a constant `sentinel = 1 m` added.
fn embedded_with(r: &str, sentinel: &str) -> String {
    assert!(DEFAULT_PHYSURE_CONF.contains(R_LINE), "the test's anchor is gone from the catalog");
    // no line ending in the anchor: the catalog is checked out with CRLF on Windows
    DEFAULT_PHYSURE_CONF
        .replacen(R_LINE, &format!("molar_gas_constant = {r} J*mol^-1*K^-1"), 1)
        .replacen("[Constants]", &format!("[Constants]\n{sentinel} = 1 m"), 1)
}

fn release_engine() -> Pairs {
    engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap()
}

fn frozen_with(engine: Pairs, embedded: &str, overrides: &str) -> String {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine,
            sources: vec![Source::new("embedded", embedded).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

/// A valid frozen file whose catalog has `molar_gas_constant = r` and the constant `sentinel`.
fn frozen_text(r: &str, sentinel: &str, overrides: &str) -> String {
    frozen_with(release_engine(), &embedded_with(r, sentinel), overrides)
}

fn gas_constant_in_force() -> String {
    with_conf_registry(|r| r.constants_meta["molar_gas_constant"].value.clone())
}

fn has_constant(name: &str) -> bool {
    with_conf_registry(|r| r.constants_meta.contains_key(name))
}

const LIBRARY_R: &str = "8.31446261815324 J*mol^-1*K^-1";

#[test]
fn a_discovered_file_replaces_the_library_catalog() {
    let env = Env::new();
    assert_eq!(gas_constant_in_force(), LIBRARY_R);
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    assert_eq!(gas_constant_in_force(), "8.0 J*mol^-1*K^-1", "the frozen file is not in force");
    assert!(has_constant("zz_sentinel"));
    let r = physure_core::chemistry::gas_constant().unwrap();
    assert_eq!(r.value.mean(), 8.0, "the hooked path of the chemistry module");
    // discovery upwards, from a subdirectory
    fs::create_dir_all(env.path("a/b")).unwrap();
    std::env::set_current_dir(env.path("a/b")).unwrap();
    assert_eq!(gas_constant_in_force(), "8.0 J*mol^-1*K^-1");
    // the file gone and the cache forgotten: the library's catalog again
    std::env::set_current_dir(&env.dir).unwrap();
    fs::remove_file(env.path(FROZEN_FILE_NAME)).unwrap();
    reset();
    assert_eq!(gas_constant_in_force(), LIBRARY_R);
    assert!(!has_constant("zz_sentinel"));
}


/// The message of the panic `f` raises; fails if it does not panic.
fn panic_message<R>(f: impl FnOnce() -> R) -> String {
    let payload = catch_unwind(AssertUnwindSafe(f)).err().expect("expected a panic");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn library_catalog() -> UnitRegistry {
    let (mut registry, mut constants) = (UnitRegistry::new(), HashMap::new());
    load_embedded_catalog(&mut registry, &mut constants);
    registry
}

const KNOWN_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// An identity unlike the running one, with a known commit.
fn fake_identity() -> EngineIdentity {
    EngineIdentity {
        version: "9.9.9".into(),
        commit: KNOWN_COMMIT.into(),
        profile: "release".into(),
        target: "x86_64-pc-windows-msvc".into(),
        features: String::new(),
        rustc: "rustc 1.0.0 (000000000 2015-05-15)".into(),
        flags: "opt-level=3;debug-assertions=off;panic=unwind;rustflags=00;target-features=00".into(),
    }
}

/// A directory of its own, outside the test's, with no frozen file in it.
fn elsewhere(env: &Env) -> PathBuf {
    let dir = env.dir.with_extension("elsewhere");
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn without_a_frozen_file_nothing_changes() {
    let _env = Env::new();
    assert!(active().unwrap().is_none());
    ensure_valid().unwrap();
    refuse_unwired("test").unwrap();
    let library = library_catalog();
    assert!(build_registry_with("").unwrap().0 == library, "build_registry_with(\"\") is the embedded catalog");
    assert!(build_registry_from_conf().0 == library);
    assert!(with_conf_registry(|r| *r == library));
    assert!(Quantity::new(1.0, "m").is_ok());
    assert_eq!(physure_core::chemistry::gas_constant().unwrap().value.mean(), 8.31446261815324);
}

#[test]
fn the_working_directory_conf_is_not_read_while_a_frozen_file_is_in_force() {
    let env = Env::new();
    env.write("physure.conf", "[Constants]\nzz_local = 1 m\n");
    assert!(has_constant("zz_local"), "unfrozen, the working directory's conf is read");
    assert!(build_registry_from_conf().1.contains_key("zz_local"));
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    assert!(!has_constant("zz_local"));
    assert!(has_constant("zz_sentinel"));
    let (_, constants) = build_registry_from_conf();
    assert!(!constants.contains_key("zz_local") && constants.contains_key("zz_sentinel"));
}

#[test]
fn precedence_explicit_then_variable_then_discovery() {
    let env = Env::new();
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_found", ""));
    let var = env.write("var.conf", &frozen_text("8.0", "zz_var", ""));
    let explicit = env.write("explicit.conf", &frozen_text("8.0", "zz_explicit", ""));
    std::env::set_var(ENV_CONF, "var.conf");
    set_explicit_conf(Some(explicit.clone()));
    let a = active().unwrap().unwrap();
    assert_eq!((a.path.clone(), a.why.clone()), (explicit, Why::Explicit));
    assert!(has_constant("zz_explicit") && !has_constant("zz_var"));
    set_explicit_conf(None); // forgets the cache, and with it the activation
    let a = active().unwrap().unwrap();
    assert_eq!((a.path.clone(), a.why.clone()), (var.clone(), Why::Environment));
    assert!(has_constant("zz_var"));
    // the variable goes away: the resolution is now another file, and one process uses one configuration
    std::env::remove_var(ENV_CONF);
    let e = active().unwrap_err();
    assert_eq!(e, ActiveError::ResolutionChanged { was: var, now: Some(env.path(FROZEN_FILE_NAME)) });
    assert_eq!(e.exit_code(), 2);
    reset();
    assert_eq!(active().unwrap().unwrap().why, Why::Discovered);
    assert!(has_constant("zz_found"));
}

#[test]
fn a_broken_frozen_file_stops_everything() {
    let env = Env::new();
    let valid = frozen_text("8.0", "zz_sentinel", "");
    let broken = valid.replacen("avogadro_constant = 6.02214076e+23", "avogadro_constant = 6.02214077e+23", 1);
    assert_ne!(broken, valid);
    env.write(FROZEN_FILE_NAME, &broken);
    let e = ensure_valid().unwrap_err();
    assert!(matches!(&e, ActiveError::Load(LoadError::Integrity { .. })), "{e:?}");
    assert!(e.to_string().contains(FROZEN_FILE_NAME), "{e}");
    assert_eq!(e.exit_code(), 1);
    let problems = build_registry_with("").unwrap_err();
    assert!(problems[0].contains("refusing to run") && problems[0].contains(FROZEN_FILE_NAME), "{problems:?}");
    assert!(panic_message(|| with_conf_registry(|_| ())).contains("refusing to run"));
    assert!(panic_message(build_registry_from_conf).contains("refusing to run"));
    // the fallible paths return an ordinary error, never the unfrozen catalog
    assert!(try_with_conf_registry(|_| ()).is_err());
    assert!(try_build_registry_from_conf().is_err());
    let e = Quantity::new(1.0, "m").unwrap_err();
    assert!(e.to_string().contains(FROZEN_FILE_NAME), "{e}");
    assert!(Parser::parse_expression("m").is_err());
    assert!(physure_core::chemistry::gas_constant().is_err());
    assert!(physure_core::chemistry::atomic_weight("H").is_err());
    assert!(physure_core::chemistry::parse_formula("H2O").is_err());
    assert!(refuse_unwired("test").is_err());
    // repaired (another length, so the stamp changes whatever the clock's resolution): in force without a reset
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_repaired_sentinel", ""));
    ensure_valid().unwrap();
    assert!(has_constant("zz_repaired_sentinel"));
    assert!(Quantity::new(1.0, "m").is_ok());
}

#[test]
fn requiring_a_frozen_file_without_one_stops_everything_and_is_not_cached() {
    let env = Env::new();
    std::env::set_var(ENV_REQUIRE, "1");
    let e = ensure_valid().unwrap_err();
    assert_eq!(e, ActiveError::Resolve(ResolveError::Required));
    assert_eq!(e.exit_code(), 2);
    assert!(panic_message(|| with_conf_registry(|_| ())).contains("refusing to run"));
    assert!(Quantity::new(1.0, "m").is_err());
    assert!(refuse_unwired("test").is_err());
    // a resolution error is not cached: a file made now is seen without a reset
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    ensure_valid().unwrap();
    assert!(has_constant("zz_sentinel"));
}

#[test]
fn a_typo_in_the_require_variable_stops_everything() {
    let _env = Env::new();
    std::env::set_var(ENV_REQUIRE, "true");
    assert_eq!(ensure_valid().unwrap_err(), ActiveError::Resolve(ResolveError::BadRequireValue("true".into())));
    assert!(Quantity::new(1.0, "m").is_err());
    std::env::set_var(ENV_CONF, "");
    std::env::set_var(ENV_REQUIRE, "0");
    assert_eq!(ensure_valid().unwrap_err(), ActiveError::Resolve(ResolveError::EmptyVariable(ENV_CONF)));
}

#[test]
fn a_different_engine_stops_evaluation_but_not_inspection() {
    let env = Env::new();
    let engine = engine_pairs(&fake_identity(), EnginePolicy::Build, None).unwrap();
    let path = env.write(FROZEN_FILE_NAME, &frozen_with(engine, &embedded_with("8.0", "zz_sentinel"), ""));
    let e = ensure_valid().unwrap_err();
    assert!(matches!(&e, ActiveError::Load(LoadError::Engine { .. })), "{e:?}");
    assert_eq!(e.exit_code(), 3);
    assert!(Quantity::new(1.0, "m").is_err());
    let catalog = load_file(&path, LoadOptions { check_engine: false }).unwrap();
    assert!(catalog.constants.contains_key("zz_sentinel"));
}

#[test]
fn the_binary_policy_needs_the_host_to_name_the_engine_file() {
    // a synthetic known commit and a small file standing for the engine: binary compares only the hash
    let env = Env::new();
    let engine_file = env.write("engine.bin", "the engine");
    let other = env.write("other.bin", "another engine");
    let hash = physure_core::frozen::sha256_of_file(&engine_file).unwrap();
    let engine = engine_pairs(&fake_identity(), EnginePolicy::Binary, Some(&hash)).unwrap();
    env.write(FROZEN_FILE_NAME, &frozen_with(engine, &embedded_with("8.0", "zz_sentinel"), ""));
    let e = ensure_valid().unwrap_err();
    assert!(matches!(&e, ActiveError::Load(LoadError::Engine { .. })), "{e:?}");
    assert!(e.to_string().contains("host did not say which file contains the engine"), "{e}");
    set_engine_file(Some(engine_file)); // forgets the cache
    ensure_valid().unwrap();
    assert!(has_constant("zz_sentinel"));
    set_engine_file(Some(other));
    assert!(matches!(ensure_valid().unwrap_err(), ActiveError::Load(LoadError::Engine { .. })));
}

#[test]
fn the_cache_notices_a_changed_file_and_loads_once_per_change() {
    let env = Env::new();
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_one", ""));
    let before = load_count();
    for _ in 0..50 {
        assert!(has_constant("zz_one"));
    }
    assert_eq!(load_count() - before, 1, "one load for many calls");
    let first = active().unwrap().unwrap();
    // another length, so the stamp changes whatever the clock's resolution
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_second_one", ""));
    assert!(has_constant("zz_second_one") && !has_constant("zz_one"), "the changed file is seen without a reset");
    assert_eq!(load_count() - before, 2);
    assert!(!Arc::ptr_eq(&first, &active().unwrap().unwrap()));
    // the current directory changes to a subdirectory: discovery again, the same file, no new load
    fs::create_dir_all(env.path("sub")).unwrap();
    std::env::set_current_dir(env.path("sub")).unwrap();
    assert!(has_constant("zz_second_one"));
    assert_eq!(load_count() - before, 2);
}

/// Every process-wide setting a conf can change, set to a value no catalog uses.
fn set_sentinel_settings() {
    physure_core::settings::set_parallel_threshold(777);
    physure_core::settings::set_max_loop_iterations(778);
    physure_core::settings::set_max_pipeline_steps(779);
    physure_core::settings::set_approx_rel_tol(0.125);
    physure_core::settings::set_monte_carlo_samples(780);
    physure_core::uncertainty::set_propagation_mode(physure_core::uncertainty::PropagationMode::Unscented);
    physure_core::units::angle_mode::set_angle_mode(physure_core::units::angle_mode::AngleMode::Dimension);
    physure_core::units::angle_mode::set_marker_grants(false, false);
}

type Settings = (usize, usize, usize, f64, usize, &'static str, &'static str, bool, bool);

fn settings_now() -> Settings {
    (
        physure_core::settings::parallel_threshold(),
        physure_core::settings::max_loop_iterations(),
        physure_core::settings::max_pipeline_steps(),
        physure_core::settings::approx_rel_tol(),
        physure_core::settings::monte_carlo_samples(),
        physure_core::uncertainty::propagation_mode().name(),
        physure_core::units::angle_mode::angle_mode().name(),
        physure_core::units::angle_mode::marker_converts_to_plain("rad"),
        physure_core::units::angle_mode::marker_converts_to_plain("sr"),
    )
}

const SENTINELS: Settings = (777, 778, 779, 0.125, 780, "unscented", "dimension", false, false);
/// What the embedded catalog sets (and the compiled defaults are).
const LIBRARY_SETTINGS: Settings = (10_000, 1_000_000, 1_000, 1e-3, 1_000, "correlated", "dimensionless", true, true);

/// The embedded catalog without its `max_pipeline_steps` line, so that the setting is one the file does not mention.
fn embedded_without_pipeline_steps() -> String {
    let line = "max_pipeline_steps = 1000";
    assert!(DEFAULT_PHYSURE_CONF.contains(line), "the test's anchor is gone from the catalog");
    embedded_with("8.0", "zz_sentinel").replacen(line, "", 1)
}

fn restore_library_settings() {
    physure_core::settings::set_parallel_threshold(10_000);
    physure_core::settings::set_max_loop_iterations(1_000_000);
    physure_core::settings::set_max_pipeline_steps(1_000);
    physure_core::settings::set_approx_rel_tol(1e-3);
    physure_core::settings::set_monte_carlo_samples(1_000);
    physure_core::uncertainty::set_propagation_mode(physure_core::uncertainty::PropagationMode::Correlated);
    physure_core::units::angle_mode::set_angle_mode(physure_core::units::angle_mode::AngleMode::Dimensionless);
    physure_core::units::angle_mode::set_marker_grants(true, true);
}

#[test]
fn settings_of_the_frozen_file_are_applied_at_activation_and_a_failed_load_applies_none() {
    let env = Env::new();
    assert!(active().unwrap().is_none());
    set_sentinel_settings();
    let overrides = "[Settings]\nparallel_threshold = 4242\nangle_mode = dimension\napprox_rel_tol = 1e-6\nmonte_carlo_samples = 4000\n[Compatibility]\nsteradian, plain = convert\n";
    env.write(FROZEN_FILE_NAME, &frozen_with(release_engine(), &embedded_without_pipeline_steps(), overrides));
    reset();
    ensure_valid().unwrap();
    // the file's values, and the compiled default for the one it does not mention (never the sentinel)
    assert_eq!(settings_now(), (4242, 1_000_000, 1_000, 1e-6, 4000, "correlated", "dimension", false, true));
    // a failed load applies nothing
    reset();
    set_sentinel_settings();
    let failing = format!("{overrides}[AtomicWeights]\nFe = abc, 0.1\n");
    env.write(FROZEN_FILE_NAME, &frozen_with(release_engine(), &embedded_with("8.0", "zz_sentinel"), &failing));
    assert!(ensure_valid().is_err());
    assert_eq!(settings_now(), SENTINELS);
    fs::remove_file(env.path(FROZEN_FILE_NAME)).unwrap();
    reset();
    restore_library_settings();
    assert_eq!(settings_now(), LIBRARY_SETTINGS);
}

#[test]
fn the_resolution_is_sticky_once_a_frozen_file_was_activated() {
    let env = Env::new();
    let path = env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", "[Settings]\nparallel_threshold = 4242\n"));
    set_sentinel_settings();
    ensure_valid().unwrap();
    assert_eq!(settings_now().0, 4242);
    // the current directory moves where no frozen file is: refused, never the unfrozen catalog with frozen settings
    let other_dir = elsewhere(&env);
    std::env::set_current_dir(&other_dir).unwrap();
    let e = active().unwrap_err();
    assert_eq!(e, ActiveError::ResolutionChanged { was: path.clone(), now: None });
    assert!(panic_message(|| with_conf_registry(|_| ())).contains("refusing to run"));
    assert!(Quantity::new(1.0, "m").is_err());
    assert!(refuse_unwired("test").is_err(), "a binding is refused while the activation stands");
    // an explicit reset: the unfrozen catalog with the library's settings, not the file's
    set_sentinel_settings();
    reset();
    assert!(active().unwrap().is_none());
    assert_eq!(settings_now(), LIBRARY_SETTINGS);
    assert_eq!(gas_constant_in_force(), LIBRARY_R);
    // the other direction: unfrozen first, with sentinel settings, then a frozen file comes into force
    set_sentinel_settings();
    std::env::set_current_dir(&env.dir).unwrap();
    ensure_valid().unwrap();
    assert_eq!(settings_now(), (4242, 1_000_000, 1_000, 1e-3, 1_000, "correlated", "dimensionless", true, true));
    // and to another frozen file: refused too
    let other = env.write("other.conf", &frozen_text("8.0", "zz_other", ""));
    std::env::set_var(ENV_CONF, &other);
    assert_eq!(active().unwrap_err(), ActiveError::ResolutionChanged { was: path.clone(), now: Some(other) });
    std::env::remove_var(ENV_CONF);
    // and the file in force goes away: refused, the file's settings are not left on the unfrozen catalog
    assert!(active().is_ok());
    fs::remove_file(&path).unwrap();
    assert_eq!(active().unwrap_err(), ActiveError::ResolutionChanged { was: path.clone(), now: None });
    assert!(Quantity::new(1.0, "m").is_err());
    let _ = fs::remove_dir_all(other_dir);
}

#[test]
fn extra_definitions_are_refused_while_a_frozen_file_is_in_force() {
    let env = Env::new();
    let extra = "[Units]\nzz_extra = 2.0, L, [zz_extra]\n";
    assert!(build_registry_with(extra).unwrap().0.knows("zz_extra"), "unfrozen, extra text is read");
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    let problems = build_registry_with(extra).unwrap_err();
    assert!(problems[0].contains("overrides block of the frozen file"), "{problems:?}");
    let (_, constants) = build_registry_with("").unwrap();
    assert!(constants.contains_key("zz_sentinel"), "an empty extra gets the frozen catalog");
}

#[test]
fn unwired_bindings_are_refused_while_a_frozen_file_is_in_force() {
    let env = Env::new();
    refuse_unwired("Java").unwrap();
    // refused as soon as a file resolves, valid or not
    let path = env.write(FROZEN_FILE_NAME, "not even a frozen file");
    // discovery runs once per set of inputs: a file made later in the same directory is seen after a reset, by the
    // bindings exactly when it is seen by the core (one cache), so the two never disagree
    assert!(refuse_unwired("Java").is_ok());
    reset();
    let message = refuse_unwired("Java").unwrap_err();
    assert!(message.contains(&path.display().to_string()) && message.contains("Java") && message.contains("Phase 5"), "{message}");
}

#[test]
fn units_of_the_frozen_file_are_read_by_the_parser() {
    let env = Env::new();
    assert!(Quantity::new(1.0, "zz_unit").is_err());
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", "[Units]\nzz_unit = 2.0, L, [zz_unit]\n"));
    reset();
    let q = Quantity::new(1.0, "zz_unit").unwrap();
    assert_eq!(q.to("m").unwrap().value.mean(), 2.0);
}

/// Runs `f` on a thread of its own and fails, instead of hanging, if it does not return within a minute.
fn within_a_minute<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(f());
    });
    receive.recv_timeout(Duration::from_secs(60)).expect("deadlock: the catalog did not return")
}

#[test]
fn the_catalog_may_be_read_again_from_inside_the_hook() {
    let env = Env::new();
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    let nested = within_a_minute(|| with_conf_registry(|_| Parser::parse_expression("m").is_ok()));
    assert!(nested);
    let r = within_a_minute(|| physure_core::chemistry::gas_constant().map(|r| r.value.mean()));
    assert_eq!(r.unwrap(), 8.0);
    let twice = within_a_minute(|| try_with_conf_registry(|_| try_with_conf_registry(|r| r.knows("m"))));
    assert!(twice.unwrap().unwrap());
}

#[test]
fn concurrent_first_use_loads_once() {
    let env = Env::new();
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    let before = load_count();
    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                active().unwrap().unwrap()
            })
        })
        .collect();
    let all: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(load_count() - before, 1, "exactly one load");
    assert!(all.iter().all(|a| Arc::ptr_eq(a, &all[0])), "every thread got the same catalog");
}

/// The unfrozen catalog merged with the working directory's `physure.conf` is built once per version of the file,
/// not on every call; a change, a new file, a removed one and another directory are all seen on the next call.
#[test]
fn the_working_directory_conf_is_read_once_per_change() {
    let env = Env::new();
    let knows = |name: &str| with_conf_registry(|r| r.knows(name));
    env.write("physure.conf", "[Units]\nzz_cached = 2.0, L, [zz_cached]\n");
    let before = local_conf_builds();
    for _ in 0..50 {
        assert!(knows("zz_cached"));
        assert!(Quantity::new(1.0, "zz_cached").is_ok());
    }
    assert!(build_registry_from_conf().0.knows("zz_cached"));
    assert_eq!(local_conf_builds() - before, 1, "one build for many calls with an unchanged file");
    // the same length, written at once: equal length and possibly an equal modification time, still seen
    env.write("physure.conf", "[Units]\nzz_cachee = 2.0, L, [zz_cachee]\n");
    assert!(knows("zz_cachee") && !knows("zz_cached"), "a same-length edit made right away is seen");
    // another length
    env.write("physure.conf", "[Units]\nzz_longer_name = 2.0, L, [zz_longer_name]\n");
    assert!(knows("zz_longer_name") && !knows("zz_cachee"));
    assert_eq!(local_conf_builds() - before, 3);
    // another directory with its own file, then one without
    let other = elsewhere(&env);
    fs::write(other.join("physure.conf"), "[Units]\nzz_other_dir = 2.0, L, [zz_other_dir]\n").unwrap();
    std::env::set_current_dir(&other).unwrap();
    assert!(knows("zz_other_dir") && !knows("zz_longer_name"));
    fs::remove_file(other.join("physure.conf")).unwrap();
    assert!(!knows("zz_other_dir") && knows("m"), "a removed file is seen at once");
    std::env::set_current_dir(&env.dir).unwrap();
    assert!(knows("zz_longer_name"));
    let _ = fs::remove_dir_all(other);
}

/// A file whose modification time is well in the past is trusted on its stamp alone; an edit that changes the stamp
/// is seen, and the text is compared again only while the modification time is recent.
#[test]
fn an_old_working_directory_conf_is_trusted_on_its_stamp() {
    let env = Env::new();
    let old = std::time::SystemTime::now() - Duration::from_secs(3600);
    let write_old = |text: &str, when: std::time::SystemTime| {
        let path = env.write("physure.conf", text);
        fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
    };
    write_old("[Units]\nzz_old_one = 2.0, L, [zz_old_one]\n", old);
    let before = local_conf_builds();
    for _ in 0..20 {
        assert!(with_conf_registry(|r| r.knows("zz_old_one")));
    }
    assert_eq!(local_conf_builds() - before, 1);
    // the same length and another old time: the stamp differs, so it is seen
    write_old("[Units]\nzz_old_two = 2.0, L, [zz_old_two]\n", old - Duration::from_secs(60));
    assert!(with_conf_registry(|r| r.knows("zz_old_two")));
    assert_eq!(local_conf_builds() - before, 2);
    // the limit, stated: an edit with the length and an old modification time restored by hand is not seen
    write_old("[Units]\nzz_old_six = 2.0, L, [zz_old_six]\n", old - Duration::from_secs(60));
    assert!(with_conf_registry(|r| r.knows("zz_old_two")), "the stamp alone is trusted for an old file");
    physure_core::frozen::reset_cache();
    assert!(with_conf_registry(|r| r.knows("zz_old_six")), "reset_cache forgets it");
    // a recent modification time is not trusted alone: the very same stamp with another text is seen
    let recent = std::time::SystemTime::now();
    write_old("[Units]\nzz_new_one = 2.0, L, [zz_new_one]\n", recent);
    assert!(with_conf_registry(|r| r.knows("zz_new_one")));
    write_old("[Units]\nzz_new_two = 2.0, L, [zz_new_two]\n", recent);
    assert!(with_conf_registry(|r| r.knows("zz_new_two")), "an equal stamp within the window is compared by text");
}

/// The cost of one call of `with_conf_registry`, unfrozen and frozen. Run with
/// `cargo test -p physure --release --test frozen_active -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_the_cost_per_call() {
    let per_call = || {
        with_conf_registry(|r| r.knows("m"));
        let n = 200_000;
        let start = std::time::Instant::now();
        let mut known = 0usize;
        for _ in 0..n {
            known += usize::from(with_conf_registry(|r| r.knows("m")));
        }
        assert_eq!(known, n);
        start.elapsed().as_secs_f64() * 1e6 / n as f64
    };
    // in the crate's directory first (where the cost before this change was measured), then in a temporary one
    let crate_dir = {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        per_call()
    };
    let env = Env::new();
    let unfrozen = per_call();
    env.write(FROZEN_FILE_NAME, &frozen_text("8.0", "zz_sentinel", ""));
    reset();
    let frozen = per_call();
    println!("MEASURE with_conf_registry per call: unfrozen {crate_dir:.3} us (crate directory), {unfrozen:.3} us (temporary directory), frozen {frozen:.3} us");
}
