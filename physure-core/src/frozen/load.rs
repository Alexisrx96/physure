//! Loading a frozen file (spec §5.2): read (at most [`MAX_FILE_BYTES`]) → parse → verify (integrity) → read the
//! engine block → check the engine under the file's policy → replay every source strictly into an empty registry.
//! The engine is checked before the replay, so a different engine is reported as such.
//!
//! Nothing of the library's own catalog is read, and the working directory's `physure.conf` is not read: an upgrade
//! of the library cannot change a value. **Loading changes no process-wide state**: the `[Settings]` and
//! `[Compatibility]` of the sources are collected in the [`FrozenCatalog`], and only the activation of the catalog
//! (in `frozen::active`) applies them.

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use super::engine::{read_engine, sha256_of_reader, EngineBlock, EngineIdentity, EngineMismatch};
use super::model::FrozenFile;
use crate::units::conf::{replay_source, ProcessSettings, Setting};
use crate::units::UnitRegistry;

/// The largest frozen file that is read (64 MiB): the embedded catalog is about 60 KiB.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// How to load. Evaluation always checks the engine; `check_engine: false` is for inspecting a file from another
/// build (`phs conf status`/`verify`/`diff`), never for evaluating with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOptions {
    pub check_engine: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions { check_engine: true }
    }
}

/// The length and modification time of a file, as its open handle reported them before it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    /// `None` where the platform does not report it.
    pub modified: Option<SystemTime>,
}

impl FileStamp {
    pub fn of(metadata: &Metadata) -> FileStamp {
        FileStamp { len: metadata.len(), modified: metadata.modified().ok() }
    }
}

/// A verified frozen file and the catalog its sources build.
#[derive(Debug, Clone)]
pub struct FrozenCatalog {
    pub registry: UnitRegistry,
    pub constants: HashMap<String, String>,
    /// The `[Settings]` lines of every source, in order (a later one wins when applied). Not applied by loading.
    pub settings: Vec<Setting>,
    /// The rows of the last `[Compatibility]` section that replaces the grants (`None`: no source's section did).
    /// Validated; not applied by loading.
    pub compatibility: Option<Vec<String>>,
    pub file: FrozenFile,
    pub engine: EngineBlock,
    /// The stamp of the file as it was read (from the handle that read it).
    pub stamp: FileStamp,
}

impl FrozenCatalog {
    /// The process-wide part of the catalog, as one value.
    pub fn process_settings(&self) -> ProcessSettings {
        ProcessSettings { settings: self.settings.clone(), compatibility: self.compatibility.clone() }
    }

    /// Makes the settings and compatibility rows of the sources the process's alone, over the compiled defaults: what
    /// activating the catalog does, and nothing else may (Task 9, E15).
    #[cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(dead_code))] // no file system there: never loaded
    pub(crate) fn apply_process_settings(&self) {
        self.process_settings().activate();
    }
}

/// Why a frozen file could not be loaded. Every message names the file and says what was expected and found.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    Io { path: PathBuf, message: String },
    /// More than [`MAX_FILE_BYTES`].
    TooLarge { path: PathBuf, bytes: u64 },
    /// Not a frozen file (wrong first line, malformed framing, not UTF-8): `FrozenFile::parse` refused it.
    NotFrozen { path: PathBuf, message: String },
    /// `FrozenFile::verify` found problems, or the engine block cannot be read.
    Integrity { path: PathBuf, problems: Vec<String> },
    /// The running engine is not the one the file pins.
    Engine { path: PathBuf, mismatch: EngineMismatch },
    /// A source does not replay strictly; `source` is its origin.
    Replay { path: PathBuf, source: String, problems: Vec<String> },
}

impl LoadError {
    /// The file the error is about.
    pub fn path(&self) -> &Path {
        match self {
            LoadError::Io { path, .. }
            | LoadError::TooLarge { path, .. }
            | LoadError::NotFrozen { path, .. }
            | LoadError::Integrity { path, .. }
            | LoadError::Engine { path, .. }
            | LoadError::Replay { path, .. } => path,
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown = self.path().display();
        match self {
            LoadError::Io { message, .. } => {
                write!(f, "{shown}: the frozen configuration cannot be read: expected a readable file, found: {message}")
            }
            LoadError::TooLarge { bytes, .. } => write!(
                f,
                "{shown}: expected a frozen configuration of at most 64 MiB ({MAX_FILE_BYTES} bytes), found {bytes} bytes"
            ),
            LoadError::NotFrozen { message, .. } => write!(
                f,
                "{shown}: expected a frozen configuration (made by `phs conf freeze`, first line `#physure-frozen 1`), found: {message}"
            ),
            LoadError::Integrity { problems, .. } => write!(
                f,
                "{shown}: the frozen configuration was modified or is damaged (expected it to verify; restore it, or accept a hand edit of the overrides with `phs conf seal`): {}",
                problems.join("; ")
            ),
            LoadError::Engine { mismatch, .. } => write!(
                f,
                "{shown}: {mismatch} expected the engine the file pins; to adopt this engine, `phs conf update` records it in the history"
            ),
            LoadError::Replay { source, problems, .. } => write!(
                f,
                "{shown}: source {source} does not replay (expected every line of it to read): {}",
                problems.join("; ")
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Loads a frozen file for evaluation or inspection (see the module documentation). The policy `binary` hashes the
/// file the host named with [`set_engine_file`](super::active::set_engine_file).
pub fn load_file(path: &Path, options: LoadOptions) -> Result<FrozenCatalog, LoadError> {
    let engine_file = super::active::engine_file();
    load_with_engine_file(path, options, engine_file.as_deref())
}

/// [`load_file`] with the engine file given instead of read from the host's setting.
pub(crate) fn load_with_engine_file(
    path: &Path,
    options: LoadOptions,
    engine_file: Option<&Path>,
) -> Result<FrozenCatalog, LoadError> {
    let (text, stamp) = read_bounded(path)?;
    let file = FrozenFile::parse(&text).map_err(|e| LoadError::NotFrozen { path: path.to_path_buf(), message: e.0 })?;
    let problems = file.verify();
    if !problems.is_empty() {
        return Err(LoadError::Integrity { path: path.to_path_buf(), problems: problems.iter().map(ToString::to_string).collect() });
    }
    // verify already read the block; this cannot fail on a verified file, but it is not assumed
    let engine = read_engine(file.engine())
        .map_err(|e| LoadError::Integrity { path: path.to_path_buf(), problems: vec![e.to_string()] })?;
    if options.check_engine {
        engine
            .check(&EngineIdentity::current(), || engine_file_hash(engine_file))
            .map_err(|mismatch| LoadError::Engine { path: path.to_path_buf(), mismatch })?;
    }
    let (registry, constants, collected) = replay_file(&file)
        .map_err(|(source, problems)| LoadError::Replay { path: path.to_path_buf(), source, problems })?;
    let ProcessSettings { settings, compatibility } = collected;
    Ok(FrozenCatalog { registry, constants, settings, compatibility, file, engine, stamp })
}

/// What a replay of the sources produced.
pub(crate) type Replayed = (UnitRegistry, HashMap<String, String>, ProcessSettings);

/// Replays every source of `file` in order, strictly, into an empty registry: source 1 (`embedded`) with the trusted
/// reader, every other source untrusted. On a problem, the origin of the source (or "the sources together") and its
/// problems. Changes no process-wide state. Loading and the operations of `ops` use this one replay.
pub(crate) fn replay_file(file: &FrozenFile) -> Result<Replayed, (String, Vec<String>)> {
    replay_file_with(file, |_, _, _, _| {})
}

/// [`replay_file`], calling `after_each(index, registry, constants, collected)` once each source has replayed, with the
/// catalog the sources `0..=index` build (`phs conf show --origin` tells from it which source set each entry). The
/// trust rule and the check of the selected edition are those of [`replay_file`]: there is one replay. On an error
/// the callback has seen only the sources before the failing one, and its results are to be discarded.
pub(crate) fn replay_file_with(
    file: &FrozenFile,
    mut after_each: impl FnMut(usize, &UnitRegistry, &HashMap<String, String>, &ProcessSettings),
) -> Result<Replayed, (String, Vec<String>)> {
    let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
    for (index, source) in file.sources().iter().enumerate() {
        // `verify` made source 1 the embedded one; the check is repeated here because trust is at stake
        let trusted = index == 0 && source.origin() == "embedded";
        replay_source(&mut registry, &mut constants, &mut collected, source.text(), trusted)
            .map_err(|problems| (source.origin().to_string(), problems))?;
        after_each(index, &registry, &constants, &collected);
    }
    // with no fallback from one table to another, a selected edition that no source defines would fail every use
    // of the weights later, one element at a time: the file is refused now instead
    if let Some(edition) = registry.selected_atomic_weight_table_missing() {
        return Err((
            "the sources together".to_string(),
            vec![format!(
                "[Chemistry] atomic_weights selects the edition '{edition}' but no [AtomicWeights:{edition}] section of any source defines it"
            )],
        ));
    }
    Ok((registry, constants, collected))
}

/// The text of the file and its stamp, both from one open handle; never more than [`MAX_FILE_BYTES`] are read.
pub(crate) fn read_bounded(path: &Path) -> Result<(String, FileStamp), LoadError> {
    let io_error = |e: io::Error| LoadError::Io { path: path.to_path_buf(), message: e.to_string() };
    let file = File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(LoadError::Io { path: path.to_path_buf(), message: "not a regular file".into() });
    }
    let too_large = |bytes: u64| LoadError::TooLarge { path: path.to_path_buf(), bytes };
    if metadata.len() > MAX_FILE_BYTES {
        return Err(too_large(metadata.len()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        // it grew after it was opened
        return Err(too_large(bytes.len() as u64));
    }
    let text = String::from_utf8(bytes).map_err(|e| LoadError::NotFrozen {
        path: path.to_path_buf(),
        message: format!("the file is not UTF-8 text ({e})"),
    })?;
    Ok((text, FileStamp::of(&metadata)))
}

const NO_ENGINE_FILE: &str =
    "the host did not say which file contains the engine (set_engine_file), so the policy binary cannot be checked";

type HashKey = (PathBuf, u64, SystemTime);

/// The SHA-256 of the engine file, cached per process by (path, length, modification time): hashing a 30-100 MB
/// executable on every load would break the first-use budget. The stamp comes from the handle that is hashed. A file
/// whose platform reports no modification time is hashed every time. Only the policy `binary` calls this.
pub(crate) fn engine_file_hash(path: Option<&Path>) -> io::Result<String> {
    static CACHE: OnceLock<Mutex<HashMap<HashKey, Result<String, String>>>> = OnceLock::new();
    let path = path.ok_or_else(|| io::Error::other(NO_ENGINE_FILE))?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let Ok(modified) = metadata.modified() else {
        return sha256_of_reader(file);
    };
    let key = (path.to_path_buf(), metadata.len(), modified);
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(known) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return known.clone().map_err(io::Error::other);
    }
    // hashed without holding the lock: another thread may hash the same file at the same time, harmlessly
    let result = sha256_of_reader(file).map_err(|e| e.to_string());
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, result.clone());
    result.map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::engine::{engine_pairs, EngineIdentity, EnginePolicy};
    use crate::frozen::model::{FrozenFile, Pairs, Source};
    use crate::frozen::seal::{EntryInput, FrozenParts};
    use crate::units::conf::{Setting, DEFAULT_PHYSURE_CONF};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Dir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("physure_load_{}_{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, text).unwrap();
            path
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
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

    /// The running engine under `release`: passes on any build.
    fn release_engine() -> Pairs {
        engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap()
    }

    fn frozen(sources: &[(&str, &str)], engine: Pairs) -> FrozenFile {
        FrozenFile::new(
            "2026-11-08T10:00:00Z",
            FrozenParts {
                engine,
                sources: sources.iter().map(|(origin, text)| Source::new(origin, text).unwrap()).collect(),
                ..FrozenParts::default()
            },
            EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
        )
        .unwrap()
    }

    fn standard(overrides: &str) -> FrozenFile {
        frozen(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", overrides)], release_engine())
    }

    fn write_file(dir: &Dir, file: &FrozenFile) -> PathBuf {
        dir.write("physure.frozen.conf", &file.render().unwrap())
    }

    fn load(path: &Path) -> Result<FrozenCatalog, LoadError> {
        load_file(path, LoadOptions::default())
    }

    #[test]
    fn a_valid_file_loads_and_its_registry_knows_the_catalog() {
        let dir = Dir::new();
        let path = write_file(&dir, &standard("# none"));
        let catalog = load(&path).unwrap();
        assert!(catalog.registry.knows("m") && catalog.registry.knows("kHz"));
        assert!(catalog.constants.contains_key("molar_gas_constant"));
        assert!(catalog.registry.constants_meta.contains_key("molar_gas_constant"));
        assert!(catalog.file.verify().is_empty());
        assert_eq!(catalog.engine.policy, EnginePolicy::Release);
        assert!(catalog.settings.contains(&Setting::ParallelThreshold(10_000)));
        assert!(catalog.compatibility.is_some());
        assert_eq!(catalog.stamp.len, fs::metadata(&path).unwrap().len());
        assert!(LoadOptions::default().check_engine, "evaluation checks the engine by default");
    }

    #[test]
    fn a_missing_file_is_an_io_error_that_names_it() {
        let dir = Dir::new();
        let path = dir.0.join("nope.frozen.conf");
        let e = load(&path).unwrap_err();
        assert!(matches!(&e, LoadError::Io { path: p, .. } if *p == path), "{e:?}");
        assert!(e.to_string().contains("nope.frozen.conf"), "{e}");
        let e = load(&dir.0).unwrap_err();
        assert!(matches!(e, LoadError::Io { .. }), "a directory: {e:?}");
    }

    #[test]
    fn a_file_that_is_not_frozen_says_so() {
        let dir = Dir::new();
        let e = load(&dir.write("plain.conf", "[Units]\nm = 1.0, L\n")).unwrap_err();
        assert!(matches!(e, LoadError::NotFrozen { .. }), "{e:?}");
        assert!(e.to_string().contains("plain.conf") && e.to_string().contains("physure-frozen"), "{e}");
        let path = dir.0.join("binary.conf");
        fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).unwrap();
        assert!(matches!(load(&path).unwrap_err(), LoadError::NotFrozen { .. }));
    }

    #[test]
    fn a_file_over_the_size_limit_is_refused_without_reading_it() {
        let dir = Dir::new();
        let path = dir.0.join("huge.frozen.conf");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        drop(file);
        let e = load(&path).unwrap_err();
        assert_eq!(e, LoadError::TooLarge { path: path.clone(), bytes: MAX_FILE_BYTES + 1 });
        assert!(e.to_string().contains("huge.frozen.conf") && e.to_string().contains("64 MiB"), "{e}");
    }

    #[test]
    fn any_edit_is_an_integrity_error_listing_every_problem() {
        let dir = Dir::new();
        let text = standard("# none").render().unwrap();
        let edited = text.replacen("avogadro_constant = 6.02214076e+23", "avogadro_constant = 6.02214077e+23", 1);
        assert_ne!(edited, text);
        let e = load(&dir.write("physure.frozen.conf", &edited)).unwrap_err();
        let LoadError::Integrity { problems, .. } = &e else { panic!("{e:?}") };
        assert!(problems.iter().any(|p| p.contains("source 1") && p.contains("embedded")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("seal")), "{problems:?}");
        assert!(e.to_string().contains("physure.frozen.conf"), "{e}");
    }

    #[test]
    fn an_engine_block_that_cannot_be_read_is_integrity_not_engine() {
        let dir = Dir::new();
        let text = standard("# none").render().unwrap();
        let edited = text.replacen("policy = release\n", "policy = release\nenforce = never\n", 1);
        let e = load(&dir.write("physure.frozen.conf", &edited)).unwrap_err();
        let LoadError::Integrity { problems, .. } = &e else { panic!("{e:?}") };
        assert!(problems.iter().any(|p| p.contains("enforce")), "{problems:?}");
        // even when the engine is not checked
        let e = load_file(&dir.0.join("physure.frozen.conf"), LoadOptions { check_engine: false }).unwrap_err();
        assert!(matches!(e, LoadError::Integrity { .. }), "{e:?}");
    }

    #[test]
    fn a_different_engine_is_an_engine_error_and_check_engine_false_skips_it() {
        let dir = Dir::new();
        let build = frozen(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "")], engine_pairs(&fake_identity(), EnginePolicy::Build, None).unwrap());
        let path = write_file(&dir, &build);
        let e = load(&path).unwrap_err();
        let LoadError::Engine { mismatch, .. } = &e else { panic!("{e:?}") };
        assert!(mismatch.differences.iter().any(|d| d.field == "version" && d.recorded == "9.9.9"), "{mismatch:?}");
        assert!(e.to_string().contains("physure.frozen.conf") && e.to_string().contains("9.9.9"), "{e}");
        let catalog = load_file(&path, LoadOptions { check_engine: false }).unwrap();
        assert!(catalog.registry.knows("m"));
        // release with another version: refused; record: always loads
        let release = frozen(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "")], engine_pairs(&fake_identity(), EnginePolicy::Release, None).unwrap());
        assert!(matches!(load(&write_file(&dir, &release)).unwrap_err(), LoadError::Engine { .. }));
        let record = frozen(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "")], engine_pairs(&fake_identity(), EnginePolicy::Record, None).unwrap());
        load(&write_file(&dir, &record)).unwrap();
    }

    #[test]
    fn the_engine_is_checked_before_the_sources_are_replayed() {
        let dir = Dir::new();
        let file = frozen(
            &[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "[AtomicWeights]\nFe = abc, 0.1")],
            engine_pairs(&fake_identity(), EnginePolicy::Release, None).unwrap(),
        );
        assert!(matches!(load(&write_file(&dir, &file)).unwrap_err(), LoadError::Engine { .. }));
    }

    #[test]
    fn a_source_that_does_not_replay_is_a_replay_error_naming_the_source() {
        let dir = Dir::new();
        let e = load(&write_file(&dir, &standard("[AtomicWeights]\nFe = abc, 0.1"))).unwrap_err();
        let LoadError::Replay { source, problems, .. } = &e else { panic!("{e:?}") };
        assert_eq!(source, "overrides");
        assert!(problems.iter().any(|p| p.contains("Fe = abc, 0.1")), "{problems:?}");
        assert!(e.to_string().contains("overrides") && e.to_string().contains("physure.frozen.conf"), "{e}");
        // the strict rules of a frozen replay apply to the embedded source too
        let bad_embedded = frozen(&[("embedded", "[Settings]\nparallel_threshold = -1"), ("overrides", "")], release_engine());
        let e = load(&write_file(&dir, &bad_embedded)).unwrap_err();
        assert!(matches!(&e, LoadError::Replay { source, .. } if source == "embedded"), "{e:?}");
    }

    #[test]
    fn loading_changes_no_process_wide_state() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = crate::units::conf::build_registry_from_conf();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let now = || {
            (
                crate::settings::parallel_threshold(),
                crate::settings::max_loop_iterations(),
                crate::settings::max_pipeline_steps(),
                crate::uncertainty::mode::propagation_mode(),
                crate::units::angle_mode::angle_mode(),
                crate::units::angle_mode::marker_converts_to_plain("rad"),
                crate::units::angle_mode::marker_converts_to_plain("sr"),
            )
        };
        let before = now();
        let dir = Dir::new();
        let settings = "[Settings]\nparallel_threshold = 12345\nangle_mode = dimension\npropagation_mode = unscented\nmax_loop_iterations = 3\nmax_pipeline_steps = 4\n[Compatibility]\n";
        let catalog = load(&write_file(&dir, &standard(settings))).unwrap();
        assert_eq!(now(), before, "a successful load applied something");
        assert!(catalog.settings.ends_with(&[
            Setting::ParallelThreshold(12345),
            Setting::AngleMode(crate::units::angle_mode::AngleMode::Dimension),
            Setting::PropagationMode(crate::uncertainty::PropagationMode::Unscented),
            Setting::MaxLoopIterations(3),
            Setting::MaxPipelineSteps(4),
        ]));
        assert_eq!(catalog.compatibility, Some(vec![]), "the overrides' empty section replaces the rows");
        let failing = standard(&format!("{settings}[AtomicWeights]\nFe = abc, 0.1"));
        assert!(load(&write_file(&dir, &failing)).is_err());
        assert_eq!(now(), before, "a failed load applied something");
    }

    #[test]
    fn the_catalog_comes_only_from_the_sources() {
        let original = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";
        assert!(DEFAULT_PHYSURE_CONF.contains(original), "the test's anchor is gone from the catalog");
        let embedded = DEFAULT_PHYSURE_CONF.replacen(original, "molar_gas_constant = 8.0 J*mol^-1*K^-1", 1).replacen(
            // no line ending in the anchor: the catalog is checked out with CRLF on Windows
            "[Constants]",
            "[Constants]\nzz_sentinel = 1 m",
            1,
        );
        let dir = Dir::new();
        let file = frozen(&[("embedded", &embedded), ("overrides", "")], release_engine());
        let catalog = load(&write_file(&dir, &file)).unwrap();
        assert_eq!(catalog.registry.constants_meta["molar_gas_constant"].value, "8.0 J*mol^-1*K^-1");
        assert_eq!(catalog.constants["zz_sentinel"], "1 m");
        // the library's own catalog has neither
        let (mut registry, mut constants) = (crate::units::UnitRegistry::new(), std::collections::HashMap::new());
        let mut collected = crate::units::conf::ProcessSettings::default();
        crate::units::conf::replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true).unwrap();
        assert_eq!(registry.constants_meta["molar_gas_constant"].value, "8.31446261815324 J*mol^-1*K^-1");
        assert!(!constants.contains_key("zz_sentinel"));
    }

    #[test]
    fn a_selected_edition_that_no_source_defines_is_refused() {
        let dir = Dir::new();
        let e = load(&write_file(&dir, &standard("[Chemistry]\natomic_weights = acme-2026-03"))).unwrap_err();
        let LoadError::Replay { problems, .. } = &e else { panic!("{e:?}") };
        assert!(problems.iter().any(|p| p.contains("acme-2026-03")), "{problems:?}");
        // defined by a later source than the one that selects it: fine; an alias of a shipped edition: fine
        load(&write_file(&dir, &standard("[Chemistry]\natomic_weights = acme-2026-03\n[AtomicWeights:acme-2026-03]\nH = 1.0, 0"))).unwrap();
        load(&write_file(&dir, &standard("[Chemistry]\natomic_weights = abridged"))).unwrap();
    }

    #[test]
    fn later_sources_are_untrusted() {
        let edit = "[AtomicWeights:standard-2024]\nH = 1.0, 0";
        let dir = Dir::new();
        let third = frozen(&[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", ""), ("systems/x.conf", edit)], release_engine());
        let e = load(&write_file(&dir, &third)).unwrap_err();
        let LoadError::Replay { source, problems, .. } = &e else { panic!("{e:?}") };
        assert_eq!(source, "systems/x.conf");
        assert!(problems.iter().any(|p| p.contains("frozen")), "{problems:?}");
        let in_overrides = standard(edit);
        assert!(matches!(load(&write_file(&dir, &in_overrides)).unwrap_err(), LoadError::Replay { .. }));
        let in_embedded = frozen(&[("embedded", &format!("{DEFAULT_PHYSURE_CONF}\n{edit}")), ("overrides", "")], release_engine());
        let catalog = load(&write_file(&dir, &in_embedded)).unwrap();
        assert_eq!(catalog.registry.atomic_weight_tables["standard-2024"]["H"], (1.0, 0.0));
    }

    #[test]
    fn the_binary_policy_hashes_the_file_the_host_names() {
        // a synthetic known commit and a small file standing for the engine: binary compares only the hash
        let dir = Dir::new();
        let engine_file = dir.write("engine.bin", "the engine");
        let other = dir.write("other.bin", "another engine");
        let hash = crate::frozen::sha256_of_file(&engine_file).unwrap();
        let file = frozen(
            &[("embedded", DEFAULT_PHYSURE_CONF), ("overrides", "")],
            engine_pairs(&fake_identity(), EnginePolicy::Binary, Some(&hash)).unwrap(),
        );
        let path = write_file(&dir, &file);
        load_with_engine_file(&path, LoadOptions::default(), Some(&engine_file)).unwrap();
        let e = load_with_engine_file(&path, LoadOptions::default(), None).unwrap_err();
        let LoadError::Engine { mismatch, .. } = &e else { panic!("{e:?}") };
        assert!(e.to_string().contains("host did not say which file contains the engine"), "{e}");
        assert_eq!(mismatch.differences[0].field, "binary_sha256");
        let e = load_with_engine_file(&path, LoadOptions::default(), Some(&other)).unwrap_err();
        assert!(matches!(&e, LoadError::Engine { mismatch, .. } if mismatch.differences[0].running != mismatch.differences[0].recorded), "{e:?}");
        load_with_engine_file(&path, LoadOptions { check_engine: false }, None).unwrap();
    }

    #[test]
    fn the_engine_file_hash_is_cached_by_path_length_and_modification_time() {
        let dir = Dir::new();
        let path = dir.write("engine.bin", "one");
        let first = engine_file_hash(Some(&path)).unwrap();
        assert_eq!(first, crate::frozen::sha256_of_file(&path).unwrap());
        assert_eq!(engine_file_hash(Some(&path)).unwrap(), first, "cached");
        // a different length is a different key: never a stale hash
        fs::write(&path, "a longer engine").unwrap();
        assert_eq!(engine_file_hash(Some(&path)).unwrap(), crate::frozen::sha256_of_file(&path).unwrap());
        assert!(engine_file_hash(Some(&dir.0.join("missing.bin"))).is_err());
        assert!(engine_file_hash(None).unwrap_err().to_string().contains("host did not say"));
    }

    #[test]
    fn every_error_names_the_file() {
        let path = PathBuf::from("dir/x.frozen.conf");
        for e in [
            LoadError::Io { path: path.clone(), message: "m".into() },
            LoadError::TooLarge { path: path.clone(), bytes: 1 },
            LoadError::NotFrozen { path: path.clone(), message: "m".into() },
            LoadError::Integrity { path: path.clone(), problems: vec!["p".into()] },
            LoadError::Replay { path: path.clone(), source: "overrides".into(), problems: vec!["p".into()] },
        ] {
            assert!(e.to_string().contains("x.frozen.conf"), "{e}");
            assert!(e.to_string().contains("expected"), "{e}");
        }
    }
}
