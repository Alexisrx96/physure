//! The operations on a frozen file (spec §5.4): `freeze`, `update` and `seal` as pure transformations that return
//! the new file and never write it, and [`write_atomic`], which does, so the CLI (Phase 2) can show the difference
//! and ask before anything changes on disk.
//!
//! Each operation validates what it returns exactly as loading would: the result verifies, and every source of it
//! replays strictly (the same replay as [`load_file`](super::load::load_file)), so a file that would never load is
//! refused here instead. None of them changes process-wide state: settings are collected by the replay, never
//! applied, and the cache of the configuration in force is not touched (a host that writes a file and goes on
//! evaluating calls [`reset_cache`](super::active::reset_cache) itself). The acting engine written to the history
//! is always the running one (`EngineIdentity::current().short()`), never given by the caller; actor and time are
//! inputs ([`actor_from_env`] and [`now_unix`] read them for the CLI).
//!
//! Each operation is two steps (module `draft`, Phase 3): `prepare_*` reads the file once and returns a draft whose
//! probe the host evaluates witnesses under, and `finish` seals once with the witnesses. [`freeze`], [`plan_update`]
//! and [`plan_seal`] are both steps with the recorded witnesses kept.

use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::active::{resolve, Inputs, Resolution, ResolveError, ENV_CONF, FROZEN_FILE_NAME};
use super::canonical::{canonical_text, source_sha256};
use super::engine::{engine_pairs, environment_pairs, freezable, read_engine, EngineIdentity, EnginePolicy};
use super::load::{engine_file_hash, read_bounded, replay_file, retry_transient, FileStamp, LoadError};
use super::model::{FrozenError, FrozenFile, Pairs, Source};
use super::seal::{EntryInput, FrozenParts};
use super::time::rfc3339_from_unix;
use crate::units::conf::DEFAULT_PHYSURE_CONF;

mod draft;
pub use draft::{
    bytes_unchanged_since, prepare_freeze, prepare_seal, prepare_update, rerecord, Draft, FreezeDraft, Plan, Rerecorded, WitnessDrift,
    DRIFT_NAMES,
};

/// The text of a new overrides block when the working directory has no `physure.conf`.
pub const EMPTY_OVERRIDES: &str =
    "# Your overrides: same grammar as physure.conf. After editing, run `phs conf seal`.\n";

/// What `freeze` needs. Actor and time are inputs: the operation reads no clock and no environment variable but
/// `PHYSURE_CONF` (to know whether a frozen file is already in force).
pub struct FreezeOptions<'a> {
    /// Where `physure.conf` is looked for and where the default output is made.
    pub cwd: &'a Path,
    /// Default: `cwd/physure.frozen.conf`; a relative path is relative to `cwd`.
    pub out: Option<PathBuf>,
    pub reason: &'a str,
    pub policy: EnginePolicy,
    /// For `binary`: the file that contains the engine, hashed here (the CLI: its own executable).
    pub engine_file: Option<&'a Path>,
    pub time_unix: u64,
    pub actor: &'a str,
}

/// Why an operation refused. Every message says what was expected and what was found.
#[derive(Debug, Clone, PartialEq)]
pub enum OpsError {
    /// A frozen file is already in force for that directory: that is `update` (or `seal`).
    AlreadyFrozen(PathBuf),
    /// The output exists and the caller did not ask to replace it.
    WouldOverwrite(PathBuf),
    /// Whether a frozen file is in force could not be decided (`PHYSURE_CONF` names nothing, ...).
    Resolve(ResolveError),
    /// A source of the result does not replay strictly: the file would never load. `source` is its origin.
    Conf { source: String, problems: Vec<String> },
    /// The engine block cannot be made for this build and policy.
    Engine(String),
    /// The model refused (a bad reason, a time earlier than the history, a bare carriage return, ...).
    Frozen(FrozenError),
    /// The file to update or seal does not read as a frozen file.
    Load(LoadError),
    /// The operation does not accept the state of the file (`update` of a modified file, `seal` of anything but a
    /// hand edit of the overrides).
    Refused(String),
    Io { path: PathBuf, message: String },
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpsError::AlreadyFrozen(path) => write!(
                f,
                "a frozen configuration is already in force here ({}): expected none for a freeze; use `phs conf update` or `phs conf seal`",
                path.display()
            ),
            OpsError::WouldOverwrite(path) => {
                write!(f, "{} exists: expected no file there; it is not overwritten", path.display())
            }
            OpsError::Resolve(e) => e.fmt(f),
            OpsError::Conf { source, problems } => write!(
                f,
                "source {source} does not replay strictly, so the frozen file would never load (expected every line to read): {}",
                problems.join("; ")
            ),
            OpsError::Engine(message) => write!(f, "the engine cannot be recorded: {message}"),
            OpsError::Frozen(e) => write!(f, "the frozen file cannot be made: {e}"),
            OpsError::Load(e) => e.fmt(f),
            OpsError::Refused(message) => f.write_str(message),
            OpsError::Io { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for OpsError {}

impl From<FrozenError> for OpsError {
    fn from(e: FrozenError) -> Self {
        OpsError::Frozen(e)
    }
}

fn io_error(path: &Path) -> impl Fn(io::Error) -> OpsError + '_ {
    move |e| OpsError::Io { path: path.to_path_buf(), message: e.to_string() }
}

/// The `[Engine]` block of the running build under `policy`; for `binary` the file that contains the engine is
/// hashed (through the per-process cache of `load`).
fn running_engine(policy: EnginePolicy, engine_file: Option<&Path>) -> Result<Pairs, OpsError> {
    let identity = EngineIdentity::current();
    freezable(&identity, policy)
        .map_err(|e| OpsError::Engine(format!("{e} (with the CLI: --engine-policy binary, release or record)")))?;
    let hash = match policy {
        EnginePolicy::Binary => {
            let path = engine_file.ok_or_else(|| {
                OpsError::Engine("the policy binary needs the file that contains the engine, and none was given".into())
            })?;
            Some(engine_file_hash(Some(path)).map_err(|e| OpsError::Engine(format!("{}: {e}", path.display())))?)
        }
        _ => None,
    };
    engine_pairs(&identity, policy, hash.as_deref()).map_err(|e| OpsError::Engine(e.0))
}

fn time_text(time_unix: u64) -> Result<String, OpsError> {
    rfc3339_from_unix(time_unix)
        .ok_or_else(|| OpsError::Refused(format!("the time {time_unix} is after the year 9999: expected a time a history can record")))
}

/// Proves that `file` is what a reader will get back: it renders, parses to the same file, verifies, and every
/// source replays strictly (all that loading does but the engine check, which depends on the build that loads).
fn validate(file: &FrozenFile) -> Result<(), OpsError> {
    validate_text(file)?;
    replay_file(file).map(|_| ()).map_err(|(source, problems)| OpsError::Conf { source, problems })
}

/// The part of [`validate`] that does not replay: it renders, parses to the same file and verifies. Enough for a file
/// whose sources are those of a file [`validate`] already proved (only `[Witness]` and the history differ).
fn validate_text(file: &FrozenFile) -> Result<(), OpsError> {
    let text = file.render()?;
    let parsed = FrozenFile::parse(&text)?;
    if parsed != *file {
        return Err(OpsError::Frozen(FrozenError("the rendered file does not parse back to the same file".into())));
    }
    let problems = parsed.verify();
    if !problems.is_empty() {
        return Err(OpsError::Frozen(FrozenError(problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))));
    }
    Ok(())
}

fn canonical_line_count(text: &str) -> usize {
    canonical_text(text).lines().count()
}

/// A new frozen file made from what is in force unfrozen in `options.cwd`: source 1 is the library's embedded
/// catalog, source 2 the text of `cwd/physure.conf` (or [`EMPTY_OVERRIDES`]); the engine is the running build under
/// `options.policy`. Returns the file and the path it is meant for; **writes nothing**.
///
/// Refused when a frozen file is already in force for `cwd` (found by discovery from `cwd` or named by
/// `PHYSURE_CONF`; `PHYSURE_REQUIRE_FROZEN` is not consulted, since the first file must be makeable under it), when
/// the output exists, when the policy cannot be recorded by this build, and when `physure.conf` does not replay
/// strictly (it would never load: every problem is listed).
pub fn freeze(options: &FreezeOptions<'_>) -> Result<(FrozenFile, PathBuf), OpsError> {
    prepare_freeze(options)?.finish(&[], Vec::new())
}

/// A planned update: see [`Plan`].
pub type UpdatePlan = Plan;

/// A planned seal of a hand edit of the overrides: see [`Plan`] (its `old` is the file as read, with the edit, so it
/// does not verify).
pub type SealPlan = Plan;

/// Reads and parses a frozen file (bounded, as loading reads it), with the stamp of the handle it was read through
/// and the SHA-256 (lowercase hex) of the bytes read.
fn read_frozen(path: &Path) -> Result<(FrozenFile, FileStamp, String), OpsError> {
    let (text, stamp) = read_bounded(path).map_err(OpsError::Load)?;
    let file = FrozenFile::parse(&text)
        .map_err(|e| OpsError::Load(LoadError::NotFrozen { path: path.to_path_buf(), message: e.0 }))?;
    Ok((file, stamp, super::sha256::sha256_hex(text.as_bytes())))
}

fn listed(problems: &[super::seal::Problem]) -> String {
    problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

/// How many canonical lines were added and removed going from `old` to `new`, as multisets.
fn line_changes(old: &str, new: &str) -> (usize, usize) {
    let mut counts: HashMap<&str, i64> = HashMap::new();
    let (old, new) = (canonical_text(old), canonical_text(new));
    for line in old.lines() {
        *counts.entry(line).or_default() -= 1;
    }
    for line in new.lines() {
        *counts.entry(line).or_default() += 1;
    }
    let added = counts.values().filter(|n| **n > 0).map(|n| *n as usize).sum();
    let removed = counts.values().filter(|n| **n < 0).map(|n| n.unsigned_abs() as usize).sum();
    (added, removed)
}

/// Adopts what the running build brings into the verified frozen file at `path`: source 1 becomes the library's
/// embedded catalog and the engine block the running build, under `policy` or, when `None`, the file's own policy.
/// The overrides and every further source are kept. `Ok(None)` when nothing differs (same canonical catalog, same
/// engine block): this one step keeps the recorded witnesses, so a draft that would only record the battery
/// ([`Draft::only_witnesses`]) is nothing to do here. **Writes nothing.**
///
/// Refused when the file does not verify (a modified file must be sealed or restored first: an update must never
/// carry an unreviewed edit into a new seal), when the policy cannot be recorded, and when the result does not
/// replay strictly.
pub fn plan_update(
    path: &Path,
    reason: &str,
    policy: Option<EnginePolicy>,
    engine_file: Option<&Path>,
    time_unix: u64,
    actor: &str,
) -> Result<Option<UpdatePlan>, OpsError> {
    prepare_update(path, reason, policy, engine_file, time_unix, actor)?
        .filter(|draft| !draft.only_witnesses)
        .map(|draft| draft.finish(None))
        .transpose()
}

/// Accepts a hand edit of the overrides block (source 2) of the frozen file at `path`, and nothing else. `Ok(None)`
/// when the file verifies (no edit, or one that changed only comments and whitespace). **Writes nothing.**
///
/// The file must be exactly as last sealed except for the text of source 2 ([`FrozenFile::only_the_overrides_changed`],
/// which proves it positively: an edit of source 1 with its marker recomputed, an appended source, an edit of the
/// engine block, the witnesses or the history are all refused, with what `verify` found). The edited overrides must
/// replay strictly.
pub fn plan_seal(path: &Path, reason: &str, time_unix: u64, actor: &str) -> Result<Option<SealPlan>, OpsError> {
    prepare_seal(path, reason, time_unix, actor)?.map(|draft| draft.finish(None)).transpose()
}

static TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Removes the temporary file when dropped, unless it was renamed into place.
struct Temporary {
    path: PathBuf,
    kept: bool,
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Writes `text` to `path` through a temporary file in the same directory (`.<name>.tmp.<pid>.<n>`, created new),
/// flushed to disk, then moved into place, so a reader sees the old file or the new one, never a part. With
/// `replace == false` an existing `path` is never overwritten: the temporary file is hard-linked to `path`, which
/// fails if `path` exists, in one step; where hard links are not supported (FAT, some network shares) the
/// existence check and the rename are two steps, and a file created between them is replaced (stated, not hidden).
/// The temporary file is removed on every error. The directory itself is not flushed.
pub fn write_atomic(path: &Path, text: &str, replace: bool) -> Result<(), OpsError> {
    let name = path.file_name().ok_or_else(|| OpsError::Io { path: path.to_path_buf(), message: "not a file name".into() })?;
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if !replace && path.exists() {
        return Err(OpsError::WouldOverwrite(path.to_path_buf()));
    }
    let n = TEMPORARY.fetch_add(1, Ordering::Relaxed);
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(name);
    temporary_name.push(format!(".tmp.{}.{n}", std::process::id()));
    let temporary_path = directory.join(temporary_name);
    let mut file = File::options().write(true).create_new(true).open(&temporary_path).map_err(io_error(&temporary_path))?;
    let mut temporary = Temporary { path: temporary_path, kept: false };
    let written = file.write_all(text.as_bytes()).and_then(|()| file.sync_all());
    drop(file);
    written.map_err(io_error(&temporary.path))?;
    if replace {
        rename_with_retry(&temporary.path, path).map_err(io_error(path))?;
        temporary.kept = true;
        return Ok(());
    }
    match fs::hard_link(&temporary.path, path) {
        Ok(()) => Ok(()), // the temporary name is removed on drop; `path` keeps the content
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(OpsError::WouldOverwrite(path.to_path_buf())),
        Err(_) => {
            if path.exists() {
                return Err(OpsError::WouldOverwrite(path.to_path_buf()));
            }
            fs::rename(&temporary.path, path).map_err(io_error(path))?;
            temporary.kept = true;
            Ok(())
        }
    }
}

/// `fs::rename` that waits out a transient refusal: the target held open without delete sharing for a moment
/// ([`retry_transient`]).
fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    retry_transient(|| fs::rename(from, to))
}

/// `user@host` for the history, from `USER` or `USERNAME` and `HOSTNAME` or `COMPUTERNAME` (`unknown` for a part
/// that is absent or empty). Whitespace and control characters become `_`; at most 100 characters. Informational,
/// not authenticated. `get` reads a variable (the CLI passes `std::env::var(..).ok()`).
pub fn actor_from_env(get: impl Fn(&str) -> Option<String>) -> String {
    let first = |names: [&str; 2]| {
        names.iter().find_map(|name| get(name).filter(|value| !value.trim().is_empty())).unwrap_or_else(|| "unknown".into())
    };
    let actor = format!("{}@{}", first(["USER", "USERNAME"]), first(["HOSTNAME", "COMPUTERNAME"]));
    actor.chars().map(|c| if c.is_whitespace() || c.is_control() { '_' } else { c }).take(100).collect()
}

/// Seconds since the Unix epoch, now (0 if the clock is before 1970).
pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::engine::sha256_of_file;
    use crate::frozen::load::{load_file, LoadOptions};
    use crate::units::conf::{load_embedded_catalog, parse_physure_conf};
    use crate::units::UnitRegistry;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(super) struct Dir(pub(super) PathBuf);
    impl Dir {
        pub(super) fn new() -> Dir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("physure_ops_{}_{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path.canonicalize().unwrap())
        }
        pub(super) fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
        pub(super) fn names(&self) -> Vec<String> {
            let mut names: Vec<String> =
                fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 2026-11-08T10:00:00Z
    pub(super) const T0: u64 = 1_794_132_000;

    pub(super) fn options(dir: &Dir, policy: EnginePolicy) -> FreezeOptions<'_> {
        FreezeOptions { cwd: &dir.0, out: None, reason: "first freeze", policy, engine_file: None, time_unix: T0, actor: "t@h" }
    }

    /// Freezes `dir` under `release` and writes the file; its path.
    pub(super) fn frozen_in(dir: &Dir) -> PathBuf {
        let (file, out) = freeze(&options(dir, EnginePolicy::Release)).unwrap();
        write_atomic(&out, &file.render().unwrap(), false).unwrap();
        out
    }

    /// The unfrozen catalog as the lenient readers build it: the embedded catalog plus `local`. The lenient readers
    /// apply settings, so the caller holds `settings::TEST_LOCK`.
    fn unfrozen(local: Option<&str>) -> (UnitRegistry, HashMap<String, String>) {
        let (mut registry, mut constants) = (UnitRegistry::new(), HashMap::new());
        load_embedded_catalog(&mut registry, &mut constants);
        if let Some(local) = local {
            parse_physure_conf(local, &mut registry, &mut constants);
        }
        (registry, constants)
    }

    #[test]
    fn freeze_writes_a_file_that_loads_into_the_unfrozen_catalog() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let dir = Dir::new();
        let path = frozen_in(&dir);
        assert_eq!(path, dir.0.join(FROZEN_FILE_NAME));
        let catalog = load_file(&path, LoadOptions::default()).unwrap();
        let (registry, constants) = unfrozen(None);
        assert!(catalog.registry == registry, "the frozen registry is not the unfrozen one");
        assert_eq!(catalog.constants, constants);
        let history = catalog.file.history();
        assert_eq!(history.len(), 1);
        assert_eq!((history[0].action(), history[0].engine()), ("freeze", EngineIdentity::current().short().as_str()));
        assert_eq!(history[0].time(), "2026-11-08T10:00:00Z");
        assert!(history[0].diff().starts_with("embedded ") && history[0].diff().ends_with("overrides 0 lines"), "{}", history[0].diff());
        assert_eq!(catalog.file.sources()[1].text(), EMPTY_OVERRIDES.trim_end());
        assert_eq!(catalog.file.environment(), &environment_pairs());
        // with overrides: the merge of the embedded catalog and that conf
        let other = Dir::new();
        let local = "[Units]\nzz_ops = 2.0, L, [zz_ops]\n[Constants]\nzz_c = 3 m\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n[Chemistry]\natomic_weights = abridged\n";
        other.write("physure.conf", local);
        let catalog = load_file(&frozen_in(&other), LoadOptions::default()).unwrap();
        let (registry, constants) = unfrozen(Some(local));
        assert!(catalog.registry == registry && catalog.registry.knows("zz_ops"));
        assert_eq!(catalog.constants, constants);
        assert_eq!(catalog.file.sources()[1].text(), local.trim_end());
    }

    #[test]
    fn freeze_refuses_what_would_never_load_and_writes_nothing() {
        let dir = Dir::new();
        dir.write("physure.conf", "[AtomicWeights]\nFe = abc, 0.1\n");
        let e = freeze(&options(&dir, EnginePolicy::Release)).unwrap_err();
        let OpsError::Conf { source, problems } = &e else { panic!("{e:?}") };
        assert_eq!(source, "overrides");
        assert!(problems.iter().any(|p| p.contains("Fe = abc, 0.1")), "{problems:?}");
        assert!(e.to_string().contains("would never load"), "{e}");
        // what the lenient reader drops in silence: a BOM, an unknown setting, a misspelt section
        for bad in ["\u{feff}[Units]\nzz = 1.0, L\n", "[Settings]\nangle_mod = dimension\n", "[Constant]\nzz = 1 m\n"] {
            dir.write("physure.conf", bad);
            assert!(matches!(freeze(&options(&dir, EnginePolicy::Release)), Err(OpsError::Conf { .. })), "{bad:?}");
        }
        // a bare carriage return cannot be stored
        dir.write("physure.conf", "[Units]\rzz = 1.0, L\n");
        assert!(matches!(freeze(&options(&dir, EnginePolicy::Release)), Err(OpsError::Frozen(_))));
        assert_eq!(dir.names(), vec!["physure.conf"], "nothing was written");
    }

    #[test]
    fn freeze_refuses_when_already_frozen_and_when_the_output_exists() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        assert_eq!(freeze(&options(&dir, EnginePolicy::Release)).unwrap_err(), OpsError::AlreadyFrozen(path.clone()));
        // from a subdirectory: discovery upwards finds it
        let sub = dir.0.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let options_sub = FreezeOptions { cwd: &sub, ..options(&dir, EnginePolicy::Release) };
        assert_eq!(freeze(&options_sub).unwrap_err(), OpsError::AlreadyFrozen(path));
        // another output name that exists
        let other = Dir::new();
        other.write("taken.conf", "x");
        let taken = FreezeOptions { out: Some(PathBuf::from("taken.conf")), ..options(&other, EnginePolicy::Release) };
        let e = freeze(&taken).unwrap_err();
        assert_eq!(e, OpsError::WouldOverwrite(other.0.join("taken.conf")));
        assert!(e.to_string().contains("not overwritten"));
        // a bad reason and a time past 9999 are refused
        let empty_reason = FreezeOptions { reason: "", ..options(&other, EnginePolicy::Release) };
        assert!(matches!(freeze(&empty_reason), Err(OpsError::Frozen(_))));
        let far = FreezeOptions { time_unix: u64::MAX, ..options(&other, EnginePolicy::Release) };
        assert!(matches!(freeze(&far), Err(OpsError::Refused(m)) if m.contains("9999")));
    }

    #[test]
    fn freeze_with_binary_needs_the_engine_file_but_not_a_known_commit() {
        let dir = Dir::new();
        let engine_file = dir.write("engine.bin", "the engine");
        let without = freeze(&options(&dir, EnginePolicy::Binary)).unwrap_err().to_string();
        assert!(without.contains("needs the file that contains the engine"), "{without}");
        let with_file = FreezeOptions { engine_file: Some(&engine_file), ..options(&dir, EnginePolicy::Binary) };
        let (file, _) = freeze(&with_file).unwrap();
        assert_eq!(file.engine().get("binary_sha256"), Some(sha256_of_file(&engine_file).unwrap().as_str()));
        assert_eq!(file.engine().get("policy"), Some("binary"));
        // only `build` needs the build to name its commit, and the CLI flag is named in the refusal
        let build = freeze(&options(&dir, EnginePolicy::Build));
        if EngineIdentity::current().commit == crate::frozen::engine::UNKNOWN {
            assert!(build.unwrap_err().to_string().contains("--engine-policy binary, release or record"));
        } else {
            build.unwrap();
        }
    }

    /// The embedded catalog as an older library would have shipped it: one constant missing, one changed.
    pub(super) fn older_embedded() -> String {
        let r = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";
        let h = "planck_constant_in_ev_s = 4.135667662e-15 eV*s";
        assert!(DEFAULT_PHYSURE_CONF.contains(r) && DEFAULT_PHYSURE_CONF.contains(h), "the test's anchors are gone");
        DEFAULT_PHYSURE_CONF.replacen(r, "molar_gas_constant = 8.314 J*mol^-1*K^-1", 1).replacen(h, "", 1)
    }

    pub(super) fn fake_identity() -> EngineIdentity {
        EngineIdentity { version: "0.0.1".into(), commit: "0123456789abcdef0123456789abcdef01234567".into(), ..EngineIdentity::current() }
    }

    fn old_file_with(embedded: &str, overrides: &str) -> FrozenFile {
        FrozenFile::new(
            "2026-11-01T10:00:00Z",
            FrozenParts {
                engine: engine_pairs(&fake_identity(), EnginePolicy::Release, None).unwrap(),
                sources: vec![Source::new("embedded", embedded).unwrap(), Source::new("overrides", overrides).unwrap()],
                ..FrozenParts::default()
            },
            EntryInput { time: "2026-11-01T10:00:00Z", action: "freeze", engine: "0.0.1+0123456789ab", actor: "a@b", reason: "old", diff: "d" },
        )
        .unwrap()
    }

    #[test]
    fn update_adopts_the_library_catalog_and_the_running_engine_and_records_it() {
        let dir = Dir::new();
        let old = old_file_with(&older_embedded(), "[Units]\nzz_kept = 2.0, L, [zz_kept]");
        let path = dir.write(FROZEN_FILE_NAME, &old.render().unwrap());
        let plan = plan_update(&path, "new library", None, None, T0, "t@h").unwrap().unwrap();
        assert_eq!(plan.summary, "embedded: +2 -1 canonical lines; engine: changed; policy: release -> release");
        let file = &plan.file;
        assert!(file.verify().is_empty(), "{:?}", file.verify());
        assert_eq!(source_sha256(file.sources()[0].text()), source_sha256(DEFAULT_PHYSURE_CONF));
        assert_eq!(file.sources()[1], old.sources()[1], "the overrides are kept");
        let history = file.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0], old.history()[0], "the earlier entry is untouched");
        let short = EngineIdentity::current().short();
        assert_eq!((history[1].action(), history[1].engine(), history[1].diff()), ("update", short.as_str(), plan.summary.as_str()));
        assert_eq!(history[1].pinned(), short);
        assert_eq!(read_engine(file.engine()).unwrap().identity, EngineIdentity::current());
        // nothing was written
        assert_eq!(FrozenFile::parse(&fs::read_to_string(&path).unwrap()).unwrap(), old);
        // written, it loads, and updating again finds nothing to do
        write_atomic(&path, &file.render().unwrap(), true).unwrap();
        let catalog = load_file(&path, LoadOptions::default()).unwrap();
        assert!(catalog.registry.knows("zz_kept"));
        assert_eq!(catalog.registry.constants_meta["molar_gas_constant"].value, "8.31446261815324 J*mol^-1*K^-1");
        assert_eq!(plan_update(&path, "again", None, None, T0, "t@h").unwrap(), None);
        // a policy change alone is an update, and the history says so
        let relaxed = plan_update(&path, "relax", Some(EnginePolicy::Record), None, T0, "t@h").unwrap().unwrap();
        assert_eq!(relaxed.summary, "embedded: +0 -0 canonical lines; engine: changed; policy: release -> record");
        assert_eq!(relaxed.file.history()[2].policy(), "record");
        // a time earlier than the history is refused
        assert!(matches!(plan_update(&path, "x", Some(EnginePolicy::Record), None, T0 - 1, "t@h"), Err(OpsError::Frozen(_))));
    }

    #[test]
    fn update_of_a_current_file_is_nothing_and_of_a_modified_one_is_refused() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        assert_eq!(plan_update(&path, "nothing", None, None, T0, "t@h").unwrap(), None);
        // a file whose embedded text differs only in comments is current too
        let text = fs::read_to_string(&path).unwrap().replacen("[Units]", "# a comment\n[Units]", 1);
        fs::write(&path, &text).unwrap();
        assert!(FrozenFile::parse(&text).unwrap().verify().is_empty());
        assert_eq!(plan_update(&path, "nothing", None, None, T0, "t@h").unwrap(), None);
        // an edit of the overrides is not carried into an update
        fs::write(&path, text.replace(EMPTY_OVERRIDES.trim_end(), "[Units]\nzz = 1.0, L")).unwrap();
        let e = plan_update(&path, "x", Some(EnginePolicy::Record), None, T0, "t@h").unwrap_err();
        assert!(matches!(&e, OpsError::Refused(m) if m.contains("seal it or restore it")), "{e:?}");
        // not a frozen file, and no file
        let plain = dir.write("plain.conf", "[Units]\n");
        assert!(matches!(plan_update(&plain, "x", None, None, T0, "t@h"), Err(OpsError::Load(LoadError::NotFrozen { .. }))));
        assert!(matches!(plan_update(&dir.0.join("none"), "x", None, None, T0, "t@h"), Err(OpsError::Load(LoadError::Io { .. }))));
    }

    #[test]
    fn update_refuses_a_result_that_would_not_load() {
        // overrides that only the older catalog can read: it defines `zz_old_unit`, the library's does not
        let older = format!("{}\n[Units]\nzz_old_unit = 1.0, L, [zz_old_unit]\n", older_embedded());
        let old = old_file_with(&older, "[Constants]\nzz_mine = 2 zz_old_unit");
        let dir = Dir::new();
        let path = dir.write(FROZEN_FILE_NAME, &old.render().unwrap());
        load_file(&path, LoadOptions { check_engine: false }).expect("the fixture loads on its own catalog");
        let e = plan_update(&path, "x", None, None, T0, "t@h").unwrap_err();
        assert!(matches!(&e, OpsError::Conf { source, .. } if source == "overrides"), "{e:?}");
    }

    #[test]
    fn seal_accepts_only_a_hand_edit_of_the_overrides() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        let base = fs::read_to_string(&path).unwrap();
        let sealed = FrozenFile::parse(&base).unwrap();
        assert_eq!(plan_seal(&path, "nothing", T0, "t@h").unwrap(), None, "a clean file");
        // comments and whitespace only: the same canonical form
        fs::write(&path, base.replace(EMPTY_OVERRIDES.trim_end(), "# reworded\n\n   ; and more")).unwrap();
        assert_eq!(plan_seal(&path, "nothing", T0, "t@h").unwrap(), None);
        // the overrides edited: accepted, recorded, nothing written
        let edited = base.replace(EMPTY_OVERRIDES.trim_end(), "[Units]\nzz_sealed = 3.0, L, [zz_sealed]");
        fs::write(&path, &edited).unwrap();
        let plan = plan_seal(&path, "my unit", T0, "t@h").unwrap().unwrap();
        assert!(plan.summary.starts_with("overrides edited by hand: ") && plan.summary.contains(" -> "), "{}", plan.summary);
        assert!(plan.file.verify().is_empty());
        let history = plan.file.history();
        assert_eq!(history[0], sealed.history()[0]);
        assert_eq!((history[1].action(), history[1].engine()), ("seal", EngineIdentity::current().short().as_str()));
        assert_eq!(fs::read_to_string(&path).unwrap(), edited, "nothing was written");
        write_atomic(&path, &plan.file.render().unwrap(), true).unwrap();
        assert!(load_file(&path, LoadOptions::default()).unwrap().registry.knows("zz_sealed"));
        // an edit of the overrides that does not replay
        fs::write(&path, base.replace(EMPTY_OVERRIDES.trim_end(), "[AtomicWeights]\nFe = abc, 0.1")).unwrap();
        assert!(matches!(plan_seal(&path, "x", T0, "t@h"), Err(OpsError::Conf { source, .. }) if source == "overrides"));
        // everything else is refused, with what verify found
        let s1 = sealed.sources()[0].sha256().to_string();
        // a change that replays, so that only the rule of seal can refuse it
        let add_unit =
            |text: &str| text.replacen("avogadro_constant = 6.02214076e+23", "avogadro_constant = 6.02214077e+23", 1);
        let edited_embedded = add_unit(sealed.sources()[0].text());
        let mut appended = sealed.clone();
        appended.sources.push(Source::new("systems/x.conf", "[Units]\nzz = 1.0, L").unwrap());
        let mut engine = sealed.clone();
        engine.set_engine(engine_pairs(&EngineIdentity::current(), EnginePolicy::Record, None).unwrap());
        let mut witnessed = sealed.clone();
        witnessed.set_witnesses(Pairs::new(&[("builtin.x", "3ff0000000000000:0000000000000000:")]).unwrap());
        let cases: Vec<(&str, String)> = vec![
            ("source 1 edited with its marker recomputed", add_unit(&base).replace(&s1, &source_sha256(&edited_embedded))),
            ("a source appended with a correct marker", appended.render().unwrap()),
            ("the engine block", engine.render().unwrap()),
            ("the witnesses", witnessed.render().unwrap()),
            ("a history field", base.replace("reason = first freeze", "reason = second freeze")),
            ("source 1 edited", add_unit(&base)),
        ];
        for (what, text) in cases {
            assert_ne!(text, base, "{what}: the edit did not apply");
            fs::write(&path, &text).unwrap();
            let e = plan_seal(&path, "x", T0, "t@h").unwrap_err();
            assert!(matches!(&e, OpsError::Refused(m) if m.contains("overrides block only")), "{what}: {e:?}");
        }
    }

    /// A scanner or an indexer holding the target open without delete sharing makes the rename fail for a moment
    /// (Windows error 5 or 32); the write waits it out, and gives up with an error, and no temporary file, if the
    /// hold lasts.
    #[cfg(windows)]
    #[test]
    fn write_atomic_waits_out_a_transient_hold_on_the_target_and_gives_up_on_a_lasting_one() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = Dir::new();
        let path = dir.0.join("held.conf");
        fs::write(&path, "old").unwrap();
        let hold = fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            drop(hold);
        });
        write_atomic(&path, "new", true).expect("the hold ended within the retries");
        releaser.join().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        // a hold that outlasts every retry is an error, the old content stays, and no temporary file is left
        let hold = fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
        assert!(matches!(write_atomic(&path, "newer", true), Err(OpsError::Io { .. })));
        drop(hold);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let leftovers: Vec<_> = fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("held.conf")], "{leftovers:?}");
    }

    #[test]
    fn write_atomic_replaces_or_refuses_and_leaves_no_temporary_file() {
        let dir = Dir::new();
        let path = dir.0.join("out.conf");
        write_atomic(&path, "one", false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        assert_eq!(write_atomic(&path, "two", false).unwrap_err(), OpsError::WouldOverwrite(path.clone()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "one", "not overwritten");
        write_atomic(&path, "three", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "three");
        assert_eq!(dir.names(), vec!["out.conf"], "no temporary file is left");
        // an error: the directory does not exist
        let e = write_atomic(&dir.0.join("missing/out.conf"), "x", true).unwrap_err();
        assert!(matches!(e, OpsError::Io { .. }), "{e:?}");
        // an error after the temporary file was made: the target is a directory
        fs::create_dir_all(dir.0.join("a_dir")).unwrap();
        assert!(write_atomic(&dir.0.join("a_dir"), "x", true).is_err());
        assert!(write_atomic(&dir.0.join("a_dir"), "x", false).is_err());
        assert_eq!(dir.names(), vec!["a_dir", "out.conf"], "no temporary file is left after an error");
        assert!(write_atomic(Path::new(""), "x", true).is_err());
    }

    #[test]
    fn the_actor_is_sanitised_and_has_defaults() {
        fn from(vars: &[(&str, &str)]) -> String {
            actor_from_env(|name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()))
        }
        assert_eq!(from(&[]), "unknown@unknown");
        assert_eq!(from(&[("USER", "ana"), ("USERNAME", "other"), ("HOSTNAME", "lab")]), "ana@lab");
        assert_eq!(from(&[("USER", " "), ("USERNAME", "irvin"), ("COMPUTERNAME", "PC")]), "irvin@PC");
        assert_eq!(from(&[("USER", "a b\tc\nd"), ("HOSTNAME", "h\u{7}")]), "a_b_c_d@h_");
        let long = actor_from_env(|name| (name == "USER").then(|| "x".repeat(300)));
        assert_eq!(long.chars().count(), 100);
        assert!(now_unix() > 1_735_689_600, "after 2025-01-01");
    }
}
