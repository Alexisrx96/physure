//! The report layer of `phs conf` (conf freeze Phase 2): `inspect`, the origin of each entry, `compare` and the
//! history rows, on real files in temporary directories.
//!
//! No test here changes the current directory, the environment or the configuration in force: `freeze` is given its
//! directory, and every report function takes paths.

use physure_core::frozen::origin::{entries_with_origin, section_and_key, select, Origin};
use physure_core::frozen::report::{compare, inspect, unfrozen_entries, EngineState, Side};
use physure_core::frozen::{
    engine_pairs, freeze, write_atomic, EngineIdentity, EnginePolicy, Entry, EntryInput, FreezeOptions, FrozenFile, FrozenParts,
    Source,
};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
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

// ---- Task 2: entries with their origin, and section.key names --------------------------------------------------

const OVERRIDES: &str = "[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
[Units]
zz = 2.5, L
";

/// A sound file made in memory from these sources, pinned to the running engine under `release`.
fn file_of(sources: &[(&str, &str)]) -> FrozenFile {
    FrozenFile::new(
        T0_TEXT,
        FrozenParts {
            engine: engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap(),
            sources: sources.iter().map(|(origin, text)| Source::new(origin, text).unwrap()).collect(),
            ..FrozenParts::default()
        },
        EntryInput { time: T0_TEXT, action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
}

fn origin_of<'a>(entries: &'a [(Entry, Origin)], kind: &str, name: &str) -> &'a Origin {
    &entries.iter().find(|(e, _)| e.kind == kind && e.name == name).unwrap_or_else(|| panic!("no {kind} {name}")).1
}

#[test]
fn each_entry_has_the_origin_of_the_last_source_that_set_it() {
    let dir = Dir::new();
    let path = freeze_in(&dir, Some(OVERRIDES));
    let entries = entries_with_origin(&read(&path)).unwrap();
    assert_eq!(origin_of(&entries, "constant", "molar_gas_constant").to_string(), "overrides");
    assert_eq!(origin_of(&entries, "constant", "avogadro_constant").to_string(), "embedded");
    assert_eq!(origin_of(&entries, "base_unit", "kg").to_string(), "embedded");
    assert_eq!(origin_of(&entries, "unit", "zz").to_string(), "overrides");
    // a third source that redefines zz
    let third = file_of(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", OVERRIDES), ("extra", "[Units]
zz = 3.5, L
")]);
    let entries = entries_with_origin(&third).unwrap();
    assert_eq!(*origin_of(&entries, "unit", "zz"), Origin { source: "extra".into(), restated_by: None });
    assert_eq!(origin_of(&entries, "constant", "molar_gas_constant").to_string(), "overrides");
    // the entries are those of the catalog, in its order
    let catalog = physure_core::frozen::load_file(&path, physure_core::frozen::LoadOptions { check_engine: false }).unwrap();
    let plain: Vec<Entry> = entries_with_origin(&read(&path)).unwrap().into_iter().map(|(e, _)| e).collect();
    assert_eq!(plain, catalog.entries());
}

#[test]
fn a_value_restated_by_a_later_source_keeps_its_first_origin_and_says_so() {
    let file = file_of(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "[Constants]
speed_of_light_in_vacuum = 299792458.0 m*s^-1
")]);
    let entries = entries_with_origin(&file).unwrap();
    let origin = origin_of(&entries, "constant", "speed_of_light_in_vacuum");
    assert_eq!(*origin, Origin { source: "embedded".into(), restated_by: Some("overrides".into()) });
    assert_eq!(origin.to_string(), "embedded (restated by overrides)");
    assert_eq!(origin_of(&entries, "constant", "avogadro_constant").restated_by, None, "not mentioned by the overrides");
}

#[test]
fn a_source_that_does_not_replay_is_an_error_naming_it() {
    let file = file_of(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "")]);
    let mut text = file.render().unwrap();
    text = text.replace("[AtomicWeights:standard-2024]", "[AtomicWeights:standard-2024]
Zz = abc, 0.1");
    // made by hand, so it no longer verifies; entries_with_origin only replays, it does not judge the seal
    let edited = FrozenFile::parse(&text).unwrap();
    let (source, problems) = entries_with_origin(&edited).unwrap_err();
    assert_eq!(source, "embedded");
    assert!(problems.iter().any(|p| p.contains("Zz = abc")), "{problems:?}");
}

#[test]
fn sections_and_keys_name_what_show_selects() {
    let entry = |kind: &'static str, name: &str| Entry { kind, name: name.into(), value: String::new() };
    let named = |kind, name| {
        let (section, key) = section_and_key(&entry(kind, name));
        format!("{section}.{key}")
    };
    assert_eq!(named("constant", "molar_gas_constant"), "constants.molar_gas_constant");
    assert_eq!(named("unit", "m"), "units.m");
    assert_eq!(named("base_unit", "kg"), "units.kg");
    assert_eq!(named("derived_unit", "N"), "units.N");
    assert_eq!(named("atomic_weight_table", "standard-2024/H"), "atomic_weights.standard-2024/H");
    assert_eq!(named("atomic_weight_selection", "atomic_weights"), "atomic_weights.atomic_weights");
    assert_eq!(named("setting", "angle_mode"), "settings.angle_mode");
    assert_eq!(named("setting_ignored", "locale"), "settings.locale");
    assert_eq!(named("category", "length"), "categories.length");
    assert_eq!(named("prefix", "k"), "prefix.k");
    assert_eq!(named("constant_meta", "c"), "constant_meta.c");
}

#[test]
fn select_takes_a_section_a_key_or_a_key_prefix() {
    let dir = Dir::new();
    let path = freeze_in(&dir, Some(OVERRIDES));
    let entries = entries_with_origin(&read(&path)).unwrap();
    assert_eq!(select(&entries, "").len(), entries.len());
    let constants = select(&entries, "constants");
    assert!(constants.len() > 100, "{}", constants.len());
    assert!(constants.iter().all(|(e, _)| e.kind == "constant"));
    assert_eq!(constants.len(), entries.iter().filter(|(e, _)| e.kind == "constant").count(), "every constant");
    let one = select(&entries, "constants.molar_gas_constant");
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].0.value, "8.0 J*mol^-1*K^-1");
    assert!(select(&entries, "nonsense").is_empty());
    assert!(select(&entries, "constants.molar").is_empty(), "a key is matched whole, not by its first letters");
    let table = select(&entries, "atomic_weights.standard-2024");
    assert!(table.len() > 50 && table.iter().all(|(e, _)| e.name.starts_with("standard-2024")), "{}", table.len());
}

// ---- Task 3: compare ---------------------------------------------------------------------------------------------

fn unfrozen(dir: &Dir) -> Side {
    Side::Unfrozen { cwd: dir.0.clone() }
}

#[test]
fn a_file_and_the_unfrozen_directory_it_was_frozen_from_are_identical() {
    let dir = Dir::new();
    let path = freeze_in(&dir, Some(OVERRIDES));
    let comparison = compare(&Side::File(path.clone()), &unfrozen(&dir), None).unwrap();
    assert!(comparison.is_identical(), "{:?} {:?} {:?}", &comparison.data[..comparison.data.len().min(3)], comparison.engine, comparison.environment);
    assert_eq!(comparison.a, path.display().to_string());
    assert_eq!(comparison.b, "the installed build, unfrozen");
    assert!(comparison.warnings.is_empty(), "{:?}", comparison.warnings);
    // and the other way round, and the unfrozen side against itself
    assert!(compare(&unfrozen(&dir), &Side::File(path), None).unwrap().is_identical());
    assert!(compare(&unfrozen(&dir), &unfrozen(&dir), None).unwrap().is_identical());
}

#[test]
fn an_edited_physure_conf_is_a_data_difference_naming_the_unit() {
    let dir = Dir::new();
    let path = freeze_in(&dir, Some(OVERRIDES));
    dir.write("physure.conf", &format!("{OVERRIDES}zz_new = 4.0, L
"));
    let comparison = compare(&Side::File(path), &unfrozen(&dir), None).unwrap();
    assert!(!comparison.is_identical());
    assert!(comparison.data.iter().any(|c| matches!(c, physure_core::frozen::Change::Added(e) if e.name == "zz_new")), "{:?}", comparison.data);
    assert!(comparison.engine.is_empty() && comparison.environment.is_empty());
}

#[test]
fn a_file_frozen_under_another_embedded_text_differs_in_that_constant_only_in_data() {
    let dir = Dir::new();
    let r = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";
    assert!(DEFAULT_PHYSURE_CONF.contains(r), "the test's anchor is gone");
    let doctored = file_of(&[("embedded", &DEFAULT_PHYSURE_CONF.replacen(r, "molar_gas_constant = 8.0 J*mol^-1*K^-1", 1)), ("overrides", "")]);
    let path = dir.write("old.frozen.conf", &doctored.render().unwrap());
    let empty = Dir::new();
    let comparison = compare(&Side::File(path), &unfrozen(&empty), None).unwrap();
    assert!(comparison.engine.is_empty(), "the same build, policy and hash left out: {:?}", comparison.engine);
    let changed = comparison.data.iter().find(|c| c.kind() == "constant" && c.name() == "molar_gas_constant").expect("the constant");
    assert!(matches!(changed, physure_core::frozen::Change::Changed { old, new, .. } if old.starts_with("8.0 ") && new.starts_with("8.314")), "{changed:?}");
    assert!(comparison.data.iter().all(|c| c.name() == "molar_gas_constant"), "{:?}", comparison.data);
}

#[test]
fn two_files_with_different_engines_list_the_keys_that_differ() {
    let dir = Dir::new();
    let a = freeze_in(&dir, None);
    let b = dir.path("b.frozen.conf");
    fs::copy(&a, &b).unwrap();
    repin(&b, &other_identity(), EnginePolicy::Record, "relaxed");
    let comparison = compare(&Side::File(a), &Side::File(b), None).unwrap();
    let keys: Vec<&str> = comparison.engine.iter().map(|c| c.key.as_str()).collect();
    assert!(keys.contains(&"version") && keys.contains(&"policy"), "{keys:?}");
    let version = comparison.engine.iter().find(|c| c.key == "version").unwrap();
    assert_eq!((version.old.as_str(), version.new.as_str()), (EngineIdentity::current().version.as_str(), "0.0.1"));
    assert!(comparison.data.is_empty());
    assert!(!comparison.is_identical());
}

#[test]
fn a_file_side_that_does_not_verify_is_not_replayed_and_never_identical() {
    let dir = Dir::new();
    let path = freeze_in(&dir, None);
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen(R_VALUE, "= not a number", 1)).unwrap();
    let comparison = compare(&Side::File(path.clone()), &unfrozen(&dir), None).unwrap();
    assert!(!comparison.a_problems.is_empty() && comparison.b_problems.is_empty());
    assert!(comparison.data.is_empty(), "a file that does not verify was replayed");
    assert!(!comparison.is_identical());
    let reversed = compare(&unfrozen(&dir), &Side::File(path), None).unwrap();
    assert!(!reversed.b_problems.is_empty() && !reversed.is_identical());
    // a file that cannot be read at all is an error naming it
    let e = compare(&Side::File(dir.path("none.frozen.conf")), &unfrozen(&dir), None).unwrap_err();
    assert!(e.contains("none.frozen.conf"), "{e}");
}

#[test]
fn a_physure_conf_the_runtime_reads_past_is_compared_as_the_runtime_reads_it_with_warnings() {
    let dir = Dir::new();
    let path = freeze_in(&dir, Some(OVERRIDES));
    // a line the lenient reader skips: the runtime ignores it, so the catalog is that of the overrides
    dir.write("physure.conf", &format!("{OVERRIDES}this is not a line of a conf
"));
    let comparison = compare(&Side::File(path), &unfrozen(&dir), None).unwrap();
    assert!(comparison.data.is_empty(), "{:?}", comparison.data);
    assert!(comparison.warnings.iter().any(|w| w.contains("physure.conf") && w.contains("this is not a line")), "{:?}", comparison.warnings);
    assert!(comparison.warnings.iter().any(|w| w.contains("strict")), "the strict reading refuses it: {:?}", comparison.warnings);
}

#[test]
fn unfrozen_entries_read_the_directory_as_the_runtime_does() {
    let dir = Dir::new();
    let (plain, warnings) = unfrozen_entries(&dir.0).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(plain, physure_core::frozen::embedded_entries().unwrap(), "no physure.conf: the embedded catalog");
    dir.write("physure.conf", OVERRIDES);
    let (with, warnings) = unfrozen_entries(&dir.0).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(with.iter().any(|e| e.kind == "constant" && e.name == "molar_gas_constant" && e.value.starts_with("8.0 ")));
    // a misspelt section: the runtime drops it with a message, the strict reading refuses the file
    dir.write("physure.conf", "[Constant]
zz = 1 m
");
    let (dropped, warnings) = unfrozen_entries(&dir.0).unwrap();
    assert_eq!(dropped, plain);
    assert!(warnings.iter().any(|w| w.contains("[Constant]")), "{warnings:?}");
    // a [Settings] line is collected, never applied to the process
    let before = physure_core::settings::parallel_threshold();
    dir.write("physure.conf", "[Settings]
parallel_threshold = 4242
");
    let (set, _) = unfrozen_entries(&dir.0).unwrap();
    assert!(set.iter().any(|e| e.kind == "setting" && e.name == "parallel_threshold" && e.value == "4242"));
    assert_eq!(physure_core::settings::parallel_threshold(), before, "a report applied a setting");
    // a file that is not text is an error naming it
    fs::write(dir.path("physure.conf"), [0xff, 0xfe, 0x00]).unwrap();
    let e = unfrozen_entries(&dir.0).unwrap_err();
    assert!(e.contains("physure.conf"), "{e}");
}
