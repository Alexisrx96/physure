//! The promise of a frozen file, checked differentially: the catalog sealed by `freeze` is the catalog in force
//! unfrozen, line for line (`common::describe`, settings included); reformatting the overrides changes nothing; an
//! upgrade of the library changes nothing until `update`, and `update` changes exactly what its summary says.
//!
//! Every test touches process-global state (the current directory, the cache of the configuration in force, the
//! settings), so they take one lock and an [`Env`] that restores it.

mod common;

use common::{describe, set_compiled_defaults, set_sentinel_settings, settings_lines};
use physure_core::frozen::active::{ENV_CONF, ENV_REQUIRE, FROZEN_FILE_NAME};
use physure_core::frozen::{
    active, engine_pairs, ensure_valid, freeze, load_file, plan_seal, plan_update, reset_cache, set_engine_file, set_explicit_conf,
    write_atomic, EngineIdentity, EnginePolicy, EntryInput, FreezeOptions, FrozenFile, FrozenParts, LoadOptions, Source,
};
use physure_core::units::conf::{build_registry_from_conf, load_embedded_catalog, parse_physure_conf, with_conf_registry, DEFAULT_PHYSURE_CONF};
use physure_core::UnitRegistry;
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The process as the test found it, restored on drop; a unique, canonical temporary directory that is the current
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
        let dir = std::env::temp_dir().join(format!("physure_equivalence_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        set_explicit_conf(None);
        set_engine_file(None);
        reset_cache();
        Env { dir, cwd, vars, _lock: lock }
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.dir.join(relative);
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
        reset_cache();
        set_compiled_defaults();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// 2026-11-08T10:00:00Z
const T0: u64 = 1_794_132_000;

/// What a fresh process would have unfrozen in this directory: the compiled settings, then the embedded catalog and
/// the working directory's conf read as the unfrozen catalog reads them (applying their settings), described.
fn unfrozen_fresh(local: Option<&str>) -> Vec<String> {
    set_compiled_defaults();
    let (mut registry, mut constants) = (UnitRegistry::new(), HashMap::new());
    load_embedded_catalog(&mut registry, &mut constants);
    if let Some(local) = local {
        parse_physure_conf(local, &mut registry, &mut constants);
    }
    describe(&registry, &constants)
}

/// Freezes the current directory under `release`, writes the file, and activates it with every setting at a
/// sentinel value first; the description of the catalog in force afterwards.
fn freeze_and_activate(env: &Env) -> Vec<String> {
    let options = FreezeOptions {
        cwd: &env.dir,
        out: None,
        reason: "differential test",
        policy: EnginePolicy::Release,
        engine_file: None,
        time_unix: T0,
        actor: "t@h",
    };
    let (file, out) = freeze(&options).unwrap();
    write_atomic(&out, &file.render().unwrap(), false).unwrap();
    activate()
}

/// Sentinel settings, a fresh resolution, the catalog in force described (with the settings the activation set).
fn activate() -> Vec<String> {
    reset_cache();
    set_sentinel_settings();
    ensure_valid().unwrap();
    let active = active().unwrap().expect("a frozen file is in force");
    describe(&active.catalog.registry, &active.catalog.constants)
}

/// The lines of `a` that are not in `b`.
fn only_in(a: &[String], b: &[String]) -> Vec<String> {
    let b: BTreeSet<&String> = b.iter().collect();
    a.iter().filter(|line| !b.contains(line)).cloned().collect()
}

fn assert_same(frozen: &[String], unfrozen: &[String]) {
    let (extra, missing) = (only_in(frozen, unfrozen), only_in(unfrozen, frozen));
    assert!(extra.is_empty() && missing.is_empty(), "only frozen: {extra:#?}\nonly unfrozen: {missing:#?}");
}

const OVERRIDES: &str = "[Settings]\nparallel_threshold = 4321\nangle_mode = dimension\n[Compatibility]\nsteradian, plain = convert\n[Units]\nzz_eq = 2.5, L, [zz_eq, zz_equivalence]\n[Constants]\nzz_eq_const = 3 zz_eq\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n[AtomicWeights]\nFe = 55.8, 0.002\n[Chemistry]\natomic_weights = abridged\n";

#[test]
fn a_sealed_catalog_equals_the_unfrozen_one() {
    let env = Env::new();
    // no physure.conf: the embedded catalog alone
    let unfrozen = unfrozen_fresh(None);
    let frozen = freeze_and_activate(&env);
    assert_same(&frozen, &unfrozen);
    assert!(!settings_lines().contains(&"setting parallel_threshold = 777".to_string()), "the activation set no setting");
    // with overrides: units, constants (new and changed), atomic weights, the selection, settings, compatibility
    let env2 = {
        drop(env);
        Env::new()
    };
    env2.write("physure.conf", OVERRIDES);
    let unfrozen = unfrozen_fresh(Some(OVERRIDES));
    for line in ["setting parallel_threshold = 4321", "setting angle_mode = dimension", "setting radian plain = false"] {
        assert!(unfrozen.contains(&line.to_string()), "the fixture does not set {line}");
    }
    // the hooked unfrozen path agrees with the fresh reading on the catalog
    let (registry, constants) = build_registry_from_conf();
    assert_same(&describe(&registry, &constants), &unfrozen);
    let frozen = freeze_and_activate(&env2);
    assert_same(&frozen, &unfrozen);
    assert!(frozen.iter().any(|line| line.starts_with("unit zz_equivalence => Some")));
    assert!(frozen.contains(&"atomic weight selection = abridged".to_string()));
}

#[test]
fn reformatting_the_overrides_changes_nothing() {
    let env = Env::new();
    env.write("physure.conf", OVERRIDES);
    let before = freeze_and_activate(&env);
    let path = env.dir.join(FROZEN_FILE_NAME);
    let text = fs::read_to_string(&path).unwrap();
    let source = OVERRIDES.trim_end();
    assert!(text.contains(source), "the overrides are not in the file as written");
    let reformatted: String = source
        .lines()
        .map(|line| format!("   {line}  \r\n# a comment\r\n\r\n; another\r\n"))
        .collect::<String>()
        .trim_end()
        .to_string();
    fs::write(&path, text.replace(source, &reformatted)).unwrap();
    assert_eq!(plan_seal(&path, "nothing", T0, "t@h").unwrap(), None, "the canonical form did not change");
    let after = activate();
    assert_same(&after, &before);
}

/// The embedded catalog with `molar_gas_constant` set to `r`.
fn embedded_with_r(r: &str) -> String {
    let line = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";
    assert!(DEFAULT_PHYSURE_CONF.contains(line), "the test's anchor is gone from the catalog");
    DEFAULT_PHYSURE_CONF.replacen(line, &format!("molar_gas_constant = {r} J*mol^-1*K^-1"), 1)
}

fn file_with_embedded(embedded: &str, policy: EnginePolicy) -> FrozenFile {
    let identity = EngineIdentity { version: "0.0.1".into(), commit: "0123456789abcdef0123456789abcdef01234567".into(), ..EngineIdentity::current() };
    FrozenFile::new(
        "2026-11-01T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&identity, policy, None).unwrap(),
            sources: vec![Source::new("embedded", embedded).unwrap(), Source::new("overrides", "").unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-01T10:00:00Z", action: "freeze", engine: "0.0.1+0123456789ab", actor: "a@b", reason: "old", diff: "d" },
    )
    .unwrap()
}

#[test]
fn the_library_can_change_and_the_file_does_not() {
    // a file made by a library whose catalog said R = 8.3: the running library says otherwise, and the file wins
    let env = Env::new();
    env.write(FROZEN_FILE_NAME, &file_with_embedded(&embedded_with_r("8.3"), EnginePolicy::Record).render().unwrap());
    reset_cache();
    assert_eq!(with_conf_registry(|r| r.constants_meta["molar_gas_constant"].value.clone()), "8.3 J*mol^-1*K^-1");
    assert_eq!(physure_core::chemistry::gas_constant().unwrap().value.mean(), 8.3);
    assert!(DEFAULT_PHYSURE_CONF.contains("molar_gas_constant = 8.31446261815324"), "the library's own value");
}

#[test]
fn update_after_a_simulated_upgrade_changes_exactly_what_the_summary_says() {
    let env = Env::new();
    let removed = "planck_constant_in_ev_s = 4.135667662e-15 eV*s";
    assert!(DEFAULT_PHYSURE_CONF.contains(removed), "the test's anchor is gone from the catalog");
    let older = embedded_with_r("8.3").replacen(removed, "", 1);
    let path = env.write(FROZEN_FILE_NAME, &file_with_embedded(&older, EnginePolicy::Release).render().unwrap());
    let old = load_file(&path, LoadOptions { check_engine: false }).unwrap();
    let before = describe(&old.registry, &old.constants);
    let plan = plan_update(&path, "the new library", None, None, T0, "t@h").unwrap().unwrap();
    assert_eq!(plan.summary, "embedded: +2 -1 canonical lines; engine: changed; policy: release -> release");
    write_atomic(&path, &plan.file.render().unwrap(), true).unwrap();
    let new = load_file(&path, LoadOptions::default()).unwrap();
    let after = describe(&new.registry, &new.constants);
    let changed: Vec<String> = only_in(&after, &before).into_iter().chain(only_in(&before, &after)).collect();
    assert!(!changed.is_empty());
    for line in &changed {
        assert!(line.contains("planck_constant_in_ev_s") || line.contains("molar_gas_constant"), "an unexpected difference: {line}");
    }
    for name in ["planck_constant_in_ev_s", "molar_gas_constant"] {
        assert!(changed.iter().any(|line| line.contains(name)), "{name} did not change");
    }
}
