//! The report layer of `phs conf` (conf freeze Phase 2): `inspect`, the origin of each entry, `compare` and the
//! history rows, on real files in temporary directories.
//!
//! No test here changes the current directory, the environment or the configuration in force: `freeze` is given its
//! directory, and every report function takes paths.

use physure_core::frozen::report::{inspect, EngineState};
use physure_core::frozen::{
    engine_pairs, freeze, write_atomic, EngineIdentity, EnginePolicy, EntryInput, FreezeOptions, FrozenFile,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A unique, canonical temporary directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("physure_report_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path.canonicalize().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 2026-11-08T10:00:00Z
const T0: u64 = 1_794_132_000;
const T0_TEXT: &str = "2026-11-08T10:00:00Z";
const R_VALUE: &str = "= 8.31446261815324";

/// Freezes `dir` (with `overrides` as its `physure.conf` if given) under `release` and writes the file; its path.
fn freeze_in(dir: &Dir, overrides: Option<&str>) -> PathBuf {
    if let Some(text) = overrides {
        dir.write("physure.conf", text);
    }
    let options = FreezeOptions {
        cwd: &dir.0,
        out: None,
        reason: "first freeze",
        policy: EnginePolicy::Release,
        engine_file: None,
        time_unix: T0,
        actor: "t@h",
    };
    let (file, out) = freeze(&options).unwrap();
    write_atomic(&out, &file.render().unwrap(), false).unwrap();
    out
}

fn read(path: &Path) -> FrozenFile {
    FrozenFile::parse(&fs::read_to_string(path).unwrap()).unwrap()
}

/// An identity unlike the running one (another version), with a known commit.
fn other_identity() -> EngineIdentity {
    EngineIdentity { version: "0.0.1".into(), commit: "0123456789abcdef0123456789abcdef01234567".into(), ..EngineIdentity::current() }
}

/// The file at `path` with its engine block replaced by `identity` under `policy`, resealed as an update and written
/// back: a sound file that pins another engine.
fn repin(path: &Path, identity: &EngineIdentity, policy: EnginePolicy, reason: &str) {
    let mut file = read(path);
    file.set_engine(engine_pairs(identity, policy, None).unwrap());
    file.reseal(EntryInput { time: T0_TEXT, action: "update", engine: "test", actor: "t@h", reason, diff: "repinned" }).unwrap();
    fs::write(path, file.render().unwrap()).unwrap();
}

// ---- Task 1: inspect ----------------------------------------------------------------------------------------------

#[test]
fn a_fresh_file_inspects_clean() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let inspection = inspect(&path, None);
    assert_eq!(inspection.path, path);
    assert!(inspection.unreadable.is_none(), "{:?}", inspection.unreadable);
    assert!(inspection.problems.is_empty(), "{:?}", inspection.problems);
    assert!(matches!(&inspection.engine, Some(EngineState::Matches { policy: EnginePolicy::Release, pinned }) if *pinned == EngineIdentity::current().short()), "{:?}", inspection.engine);
    let catalog = inspection.catalog.as_ref().expect("replayed").as_ref().expect("the catalog replays");
    assert!(catalog.iter().any(|e| e.kind == "constant" && e.name == "molar_gas_constant"));
    assert!(inspection.atomic_weight_edition.as_deref().is_some_and(|e| e.starts_with("standard-2024")), "{:?}", inspection.atomic_weight_edition);
    assert!(inspection.environment.is_empty(), "{:?}", inspection.environment);
    assert_eq!(inspection.exit_code(), 0);
}

#[test]
fn an_edited_value_is_a_problem_and_exit_1() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(R_VALUE), "the test's anchor is gone");
    fs::write(&path, text.replacen(R_VALUE, "= 8.31446261815325", 1)).unwrap();
    let inspection = inspect(&path, None);
    assert!(!inspection.problems.is_empty());
    assert!(inspection.problems.iter().any(|p| p.contains("source 1")), "{:?}", inspection.problems);
    assert_eq!(inspection.exit_code(), 1);
}

#[test]
fn a_tampered_file_is_not_replayed() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let text = fs::read_to_string(&path).unwrap();
    // an edit that would not even replay: the report must not run the sources of a file that does not verify
    fs::write(&path, text.replacen(R_VALUE, "= not a number", 1)).unwrap();
    let inspection = inspect(&path, None);
    assert!(!inspection.problems.is_empty());
    assert!(inspection.catalog.is_none(), "a file that does not verify was replayed: {:?}", inspection.catalog.map(|c| c.is_ok()));
    assert!(inspection.atomic_weight_edition.is_none());
    assert_eq!(inspection.exit_code(), 1);
}

#[test]
fn an_engine_of_another_build_is_exit_3_only() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    repin(&path, &other_identity(), EnginePolicy::Release, "another engine");
    let inspection = inspect(&path, None);
    assert!(inspection.problems.is_empty(), "{:?}", inspection.problems);
    let Some(EngineState::Differs { policy, differences }) = &inspection.engine else { panic!("{:?}", inspection.engine) };
    assert_eq!(*policy, EnginePolicy::Release);
    assert!(differences.iter().any(|d| d.starts_with("version") && d.contains("0.0.1")), "{differences:?}");
    assert!(matches!(inspection.catalog, Some(Ok(_))), "a sound file is replayed whatever its engine");
    assert_eq!(inspection.exit_code(), 3);
}

#[test]
fn the_binary_policy_without_an_engine_file_differs_and_does_not_panic() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let engine_file = dir.write("engine.bin", "the engine");
    let hash = physure_core::frozen::sha256_of_file(&engine_file).unwrap();
    let mut file = read(&path);
    file.set_engine(engine_pairs(&other_identity(), EnginePolicy::Binary, Some(&hash)).unwrap());
    file.reseal(EntryInput { time: T0_TEXT, action: "update", engine: "test", actor: "t@h", reason: "binary", diff: "d" }).unwrap();
    fs::write(&path, file.render().unwrap()).unwrap();
    let without = inspect(&path, None);
    let Some(EngineState::Differs { differences, .. }) = &without.engine else { panic!("{:?}", without.engine) };
    assert!(differences.iter().any(|d| d.contains("host did not say")), "{differences:?}");
    assert_eq!(without.exit_code(), 3);
    let with = inspect(&path, Some(&engine_file));
    assert!(matches!(with.engine, Some(EngineState::Matches { policy: EnginePolicy::Binary, .. })), "{:?}", with.engine);
    assert_eq!(with.exit_code(), 0);
}

#[test]
fn a_missing_or_garbage_file_is_unreadable_and_exit_1() {
    let dir = Dir::new();
    let missing = inspect(&dir.path("nope.frozen.conf"), None);
    assert!(missing.unreadable.as_deref().is_some_and(|m| m.contains("nope.frozen.conf")), "{:?}", missing.unreadable);
    assert!(missing.file.is_none() && missing.engine.is_none() && missing.catalog.is_none());
    assert_eq!(missing.exit_code(), 1);
    let garbage = inspect(&dir.write("garbage.conf", "[Units]\nm = 1.0, L\n"), None);
    assert!(garbage.unreadable.as_deref().is_some_and(|m| m.contains("physure-frozen")), "{:?}", garbage.unreadable);
    assert_eq!(garbage.exit_code(), 1);
    let path = dir.path("binary.conf");
    fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).unwrap();
    assert_eq!(inspect(&path, None).exit_code(), 1);
    assert!(!missing.seal_matches(&"0".repeat(64)));
}

#[test]
fn an_unreadable_engine_block_is_exit_1() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("policy = release\n", "policy = release\nenforce = never\n", 1)).unwrap();
    let inspection = inspect(&path, None);
    assert!(matches!(&inspection.engine, Some(EngineState::Unreadable(m)) if m.contains("enforce")), "{:?}", inspection.engine);
    assert_eq!(inspection.exit_code(), 1);
}

#[test]
fn environment_differences_are_listed_but_do_not_change_the_exit_code() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let file = read(&path);
    let cpus = file.environment().get("cpus").unwrap().to_string();
    let text = fs::read_to_string(&path).unwrap();
    let edited = text.replacen(&format!("cpus = {cpus}\n"), "cpus = 99999\nzz_extra = 1\n", 1);
    assert_ne!(edited, text, "the [Environment] block was not found");
    fs::write(&path, edited).unwrap();
    let inspection = inspect(&path, None);
    assert!(inspection.problems.is_empty(), "the environment is outside the seal: {:?}", inspection.problems);
    let cpus_change = inspection.environment.iter().find(|c| c.key == "cpus").expect("cpus is listed");
    assert_eq!((cpus_change.old.as_str(), cpus_change.new.as_str()), ("99999", cpus.as_str()));
    let extra = inspection.environment.iter().find(|c| c.key == "zz_extra").expect("a key of the file only");
    assert_eq!((extra.old.as_str(), extra.new.as_str()), ("1", ""));
    assert_eq!(inspection.exit_code(), 0);
}

#[test]
fn seal_matches_is_case_insensitive_and_exact() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let seal = read(&path).seal_sha256().to_string();
    let inspection = inspect(&path, None);
    assert!(inspection.seal_matches(&seal));
    assert!(inspection.seal_matches(&seal.to_uppercase()));
    let last = seal.chars().last().unwrap();
    let off = format!("{}{}", &seal[..63], if last == '0' { '1' } else { '0' });
    assert!(!inspection.seal_matches(&off));
    assert!(!inspection.seal_matches(&seal[..63]));
    assert!(!inspection.seal_matches(""));
    assert!(!inspection.seal_matches(&format!("{seal}0")));
}

#[test]
fn seal_matches_is_false_once_a_source_was_edited() {
    // the stored seal is unchanged by a hand edit: matching it alone would vouch for a file that no longer verifies
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let seal = read(&path).seal_sha256().to_string();
    let text = fs::read_to_string(&path).unwrap();
    let edited = text.replacen(physure_core::frozen::EMPTY_OVERRIDES.trim_end(), "[Constants]\nzz_c = 3 m", 1);
    assert_ne!(edited, text);
    fs::write(&path, edited).unwrap();
    let inspection = inspect(&path, None);
    assert_eq!(read(&path).seal_sha256(), seal, "the stored seal is the original one");
    assert!(!inspection.seal_matches(&seal));
    assert_eq!(inspection.exit_code(), 1);
}
