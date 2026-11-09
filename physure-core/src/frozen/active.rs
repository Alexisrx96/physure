//! Which configuration is in force (spec §5.1): resolution of a frozen file. Resolution is pure: the environment
//! and the current directory are inputs.
//!
//! The order is: (1) an explicit path set by the host with [`set_explicit_conf`] (the CLI's `--conf`); (2) the
//! variable `PHYSURE_CONF`; (3) discovery, the nearest `physure.frozen.conf` walking up from the current directory;
//! (4) nothing, the unfrozen behaviour. An explicit path or variable that does not name a file is an error and never
//! falls back. `PHYSURE_REQUIRE_FROZEN=1` turns (4) into an error; any value but unset, `0` or `1` is an error, so a
//! typo cannot switch the audit mode off.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::load::{FileStamp, FrozenCatalog, LoadError};
use crate::units::conf::LocalCatalog;

pub const FROZEN_FILE_NAME: &str = "physure.frozen.conf";
pub const ENV_CONF: &str = "PHYSURE_CONF";
pub const ENV_REQUIRE: &str = "PHYSURE_REQUIRE_FROZEN";

/// Why a frozen file is in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The host set the path (the CLI's `--conf`).
    Explicit,
    /// `PHYSURE_CONF`.
    Environment,
    /// The nearest `physure.frozen.conf` walking up from the current directory.
    Discovered,
}

impl Why {
    fn describe(&self) -> &'static str {
        match self {
            Why::Explicit => "the explicit path (--conf)",
            Why::Environment => ENV_CONF,
            Why::Discovered => "discovery",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Frozen { path: PathBuf, why: Why },
    Unfrozen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// `PHYSURE_REQUIRE_FROZEN=1` and no frozen file was found.
    Required,
    /// `PHYSURE_REQUIRE_FROZEN` has a value other than unset, `0` or `1`.
    BadRequireValue(String),
    /// A variable that must name a file is set and empty.
    EmptyVariable(&'static str),
    /// A variable is set to something that is not Unicode (the core reads it as text).
    NotUnicode(&'static str),
    /// An explicit path or `PHYSURE_CONF` that is not a file.
    Missing { path: PathBuf, why: Why },
    /// The current directory cannot be read, and something needs it: a relative path, or a required frozen file
    /// that would have to be discovered. `needed_for` says what.
    CurrentDir { message: String, needed_for: String },
    /// A candidate of discovery exists but cannot be examined (permission denied, ...): it might be a frozen file,
    /// so it is not passed over in silence.
    Unreadable { path: PathBuf, message: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Required => write!(
                f,
                "{ENV_REQUIRE}=1 but no frozen configuration was found (looked in the explicit path, in {ENV_CONF} and for {FROZEN_FILE_NAME} from the current directory upwards); make one with `phs conf freeze`"
            ),
            ResolveError::BadRequireValue(v) => {
                write!(f, "{ENV_REQUIRE} must be unset, 0 or 1, found {v:?}; a typo must not switch the audit mode off")
            }
            ResolveError::EmptyVariable(name) => write!(f, "{name} is set but empty; unset it or name a frozen configuration"),
            ResolveError::NotUnicode(name) => {
                write!(f, "{name} is set to a value that is not Unicode text; unset it or name a frozen configuration")
            }
            ResolveError::Missing { path, why } => write!(
                f,
                "{} names {} but it is not a file; there is no fallback to the unfrozen catalog",
                why.describe(),
                path.display()
            ),
            ResolveError::CurrentDir { message, needed_for } => write!(
                f,
                "the current directory cannot be read ({message}) and it is needed for {needed_for}; there is no fallback to the unfrozen catalog"
            ),
            ResolveError::Unreadable { path, message } => write!(
                f,
                "{} may be a frozen configuration but cannot be examined ({message}); there is no fallback to the unfrozen catalog",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Everything resolution depends on.
pub struct Inputs<'a> {
    pub explicit: Option<&'a Path>,
    pub conf_var: Option<&'a str>,
    pub require_var: Option<&'a str>,
    pub cwd: &'a Path,
}

/// The configuration in force: an explicit path, `PHYSURE_CONF`, the nearest `physure.frozen.conf` upwards, or
/// nothing. An explicit path or variable that is not a file is an error, never a fallback.
pub fn resolve(inputs: &Inputs<'_>) -> Result<Resolution, ResolveError> {
    resolve_with_cwd(Ok(inputs.cwd), inputs.explicit, inputs.conf_var, inputs.require_var)
}

/// [`resolve`] when the current directory may be unreadable (deleted on Unix, unsupported on WebAssembly). Without
/// it: an absolute explicit path or `PHYSURE_CONF` still resolves; a relative one, or `PHYSURE_REQUIRE_FROZEN=1`
/// with nothing named, is [`ResolveError::CurrentDir`]; otherwise the result is `Unfrozen`, which is what the
/// unfrozen behaviour does without a current directory (its `physure.conf` is simply not found).
pub fn resolve_with_cwd(
    cwd: Result<&Path, &io::Error>,
    explicit: Option<&Path>,
    conf_var: Option<&str>,
    require_var: Option<&str>,
) -> Result<Resolution, ResolveError> {
    let required = match require_var {
        None | Some("0") => false,
        Some("1") => true,
        Some(other) => return Err(ResolveError::BadRequireValue(other.to_string())),
    };
    let named = |path: &Path, why: Why| -> Result<Resolution, ResolveError> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match cwd {
                Ok(cwd) => cwd.join(path),
                Err(e) => {
                    return Err(ResolveError::CurrentDir {
                        message: e.to_string(),
                        needed_for: format!("the relative path {} of {}", path.display(), why.describe()),
                    })
                }
            }
        };
        if path.is_file() {
            Ok(Resolution::Frozen { path, why })
        } else {
            Err(ResolveError::Missing { path, why })
        }
    };
    if let Some(path) = explicit {
        return named(path, Why::Explicit);
    }
    if let Some(value) = conf_var {
        if value.is_empty() {
            return Err(ResolveError::EmptyVariable(ENV_CONF));
        }
        return named(Path::new(value), Why::Environment);
    }
    let cwd = match cwd {
        Ok(cwd) => cwd,
        Err(e) if required => {
            return Err(ResolveError::CurrentDir {
                message: e.to_string(),
                needed_for: format!("discovering {FROZEN_FILE_NAME}, since {ENV_REQUIRE}=1"),
            })
        }
        Err(_) => return Ok(Resolution::Unfrozen),
    };
    if let Some(path) = discover(cwd)? {
        return Ok(Resolution::Frozen { path, why: Why::Discovered });
    }
    if required {
        Err(ResolveError::Required)
    } else {
        Ok(Resolution::Unfrozen)
    }
}

/// The nearest `physure.frozen.conf` that is a file, from `cwd` upwards. Anything else with that name (a directory,
/// a link whose target is missing, a file that cannot be examined) is an error rather than "nothing found": it
/// might be the frozen file, and running unfrozen in its place would be silent.
fn discover(cwd: &Path) -> Result<Option<PathBuf>, ResolveError> {
    for dir in cwd.ancestors() {
        let candidate = dir.join(FROZEN_FILE_NAME);
        match std::fs::metadata(&candidate) {
            Ok(meta) if meta.is_file() => return Ok(Some(candidate)),
            Ok(_) => {
                return Err(ResolveError::Unreadable { path: candidate, message: "it is not a file".into() });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // `metadata` follows links: an entry that exists but points nowhere is not "not found"
                if std::fs::symlink_metadata(&candidate).is_ok() {
                    return Err(ResolveError::Unreadable { path: candidate, message: "it is a link to something that does not exist".into() });
                }
            }
            Err(e) => return Err(ResolveError::Unreadable { path: candidate, message: e.to_string() }),
        }
    }
    Ok(None)
}

/// The resolution of this process: the explicit path set by the host, the two variables and the current directory.
/// On WebAssembly without an operating system (`wasm32-unknown-unknown`) there is no file system, no environment and
/// no current directory: the result is `Unfrozen` without touching anything.
pub fn resolve_process() -> Result<Resolution, ResolveError> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        Ok(Resolution::Unfrozen)
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        let explicit = explicit_conf();
        let conf_var = env_text(ENV_CONF)?;
        let require_var = env_text(ENV_REQUIRE)?;
        let cwd = std::env::current_dir();
        resolve_with_cwd(cwd.as_deref(), explicit.as_deref(), conf_var.as_deref(), require_var.as_deref())
    }
}

/// A variable as text: unset is `None`, a value that is not Unicode is an error (never read as unset).
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn env_text(name: &'static str) -> Result<Option<String>, ResolveError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(ResolveError::NotUnicode(name)),
    }
}

static EXPLICIT: Mutex<Option<PathBuf>> = Mutex::new(None);
static ENGINE_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The host says which frozen file is in force (the CLI's `--conf`). `None` clears it. The cache is forgotten
/// ([`reset_cache`]): the next [`active`] resolves and loads again.
pub fn set_explicit_conf(path: Option<PathBuf>) {
    *EXPLICIT.lock().unwrap_or_else(|e| e.into_inner()) = path;
    reset_cache();
}

/// The host says which file contains the engine, for the policy `binary` (the CLI: its own executable; Python:
/// the extension module; Java: the library). Without it a file pinned with `binary` cannot be loaded. The cache is
/// forgotten ([`reset_cache`]).
pub fn set_engine_file(path: Option<PathBuf>) {
    *ENGINE_FILE.lock().unwrap_or_else(|e| e.into_inner()) = path;
    reset_cache();
}

/// The explicit path the host set, if any.
pub fn explicit_conf() -> Option<PathBuf> {
    EXPLICIT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The engine file the host named, if any.
pub fn engine_file() -> Option<PathBuf> {
    ENGINE_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ---- the configuration in force, cached (Task 5 and Task 9 D/E of the Phase 1c plan) ----

/// Why the configuration in force cannot be used. Every message says what to do.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveError {
    /// Resolution failed (a missing explicit file, `PHYSURE_REQUIRE_FROZEN=1` without a file, ...).
    Resolve(ResolveError),
    /// The frozen file in force does not load (its message names the file).
    Load(LoadError),
    /// A frozen configuration was activated in this process and the resolution now gives another file, or none:
    /// one process, one configuration, until [`reset_cache`].
    ResolutionChanged { was: PathBuf, now: Option<PathBuf> },
    /// The catalog was asked for while this thread was loading a frozen file (a defect: loading reads no catalog).
    Reentrant,
}

impl ActiveError {
    /// The exit code a program reports this error with: 1 for the integrity of the file (it does not read,
    /// verify or replay), 2 for resolution, 3 for the engine.
    pub fn exit_code(&self) -> i32 {
        match self {
            ActiveError::Resolve(_) | ActiveError::ResolutionChanged { .. } => 2,
            ActiveError::Load(LoadError::Engine { .. }) => 3,
            ActiveError::Load(_) | ActiveError::Reentrant => 1,
        }
    }
}

impl fmt::Display for ActiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveError::Resolve(e) => e.fmt(f),
            ActiveError::Load(e) => e.fmt(f),
            ActiveError::ResolutionChanged { was, now } => {
                let now = match now {
                    Some(path) => path.display().to_string(),
                    None => "no frozen configuration".to_string(),
                };
                write!(
                    f,
                    "the frozen configuration {} was activated in this process and the configuration in force is now {now} (the current directory or the environment changed); a process keeps the path it activated (the file itself is re-read when it changes): restart the program",
                    was.display()
                )
            }
            ActiveError::Reentrant => {
                write!(f, "the catalog was asked for while a frozen configuration was being loaded on the same thread")
            }
        }
    }
}

impl std::error::Error for ActiveError {}

impl From<ResolveError> for ActiveError {
    fn from(e: ResolveError) -> Self {
        ActiveError::Resolve(e)
    }
}

/// A frozen configuration in force: where it is, why it is in force, and its verified catalog.
#[derive(Debug)]
pub struct Active {
    pub path: PathBuf,
    pub why: Why,
    pub catalog: FrozenCatalog,
}

/// The inputs of resolution, as this process has them now: the cache key.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    explicit: Option<PathBuf>,
    conf_var: Option<String>,
    require_var: Option<String>,
    /// The error text when the current directory cannot be read.
    cwd: Result<PathBuf, String>,
}

/// The outcome of loading one file, kept with the stamp of what was read.
#[cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(dead_code))] // no file system there: never loaded
struct Loaded {
    path: PathBuf,
    why: Why,
    /// `None` when the file could not even be examined: never reused.
    stamp: Option<FileStamp>,
    outcome: Result<Arc<Active>, ActiveError>,
}

#[derive(Default)]
struct State {
    /// The key the resolution below was made with. Resolution errors are not kept.
    key: Option<Key>,
    resolution: Option<Resolution>,
    loaded: Option<Loaded>,
    /// The frozen file whose catalog was activated (its settings applied), if any: sticky until [`reset_cache`].
    activated: Option<PathBuf>,
    loads: u64,
    /// The unfrozen catalog merged with the working directory's `physure.conf` (built by `units::conf`).
    local: Option<Arc<LocalCatalog>>,
}

static STATE: Mutex<State> =
    Mutex::new(State { key: None, resolution: None, loaded: None, activated: None, loads: 0, local: None });

thread_local! {
    /// This thread is loading a frozen file while holding the cache: a nested `active()` would deadlock on it.
    static LOADING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Sets [`LOADING`] for its lifetime, also on unwind.
#[cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(dead_code))] // no file system there: never loaded
struct LoadingGuard;

#[cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(dead_code))] // no file system there: never loaded
impl LoadingGuard {
    fn new() -> LoadingGuard {
        LOADING.with(|loading| loading.set(true));
        LoadingGuard
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        LOADING.with(|loading| loading.set(false));
    }
}

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The stamp of `path` now, `None` if it cannot be examined.
#[cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(dead_code))] // no file system there: never loaded
fn stamp_of(path: &Path) -> Option<FileStamp> {
    std::fs::metadata(path).ok().map(|metadata| FileStamp::of(&metadata))
}

/// The frozen configuration in force, if any; `Ok(None)` is the unfrozen behaviour.
///
/// The outcome is cached per process under the inputs of resolution (the explicit path, `PHYSURE_CONF`,
/// `PHYSURE_REQUIRE_FROZEN` and the current directory): discovery runs once per set of inputs, and each call
/// checks only the stamp (length, modification time) of the file in force, so an edited file is loaded again. A
/// resolution error is not cached; a load error is, under the stamp of the file that was read.
///
/// Loading a file **activates** it: the process-wide settings are put back to their compiled defaults and the
/// file's `[Settings]` and `[Compatibility]` are applied, in one step under the cache lock. From then on the
/// resolution is sticky: if it later gives another file or none (the current directory changed), the result is
/// [`ActiveError::ResolutionChanged`] until [`reset_cache`]. Where there is no file system
/// (`wasm32-unknown-unknown`) the result is always `Ok(None)`.
///
/// The lock is never held while a caller's code runs: the hooks of `units::conf` clone the `Arc` out and release it.
pub fn active() -> Result<Option<Arc<Active>>, ActiveError> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        Ok(None)
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        if LOADING.with(std::cell::Cell::get) {
            return Err(ActiveError::Reentrant);
        }
        let key = Key {
            explicit: explicit_conf(),
            conf_var: env_text(ENV_CONF)?,
            require_var: env_text(ENV_REQUIRE)?,
            cwd: std::env::current_dir().map_err(|e| e.to_string()),
        };
        let mut state = state();
        if state.key.as_ref() == Some(&key) {
            match &state.resolution {
                Some(Resolution::Unfrozen) => return Ok(None),
                Some(Resolution::Frozen { path, .. }) => {
                    if let Some(loaded) = &state.loaded {
                        if loaded.path == *path && loaded.stamp.is_some() && loaded.stamp == stamp_of(path) {
                            return loaded.outcome.clone().map(Some);
                        }
                    }
                }
                None => {}
            }
        }
        resolve_and_load(&mut state, key)
    }
}

/// The slow path of [`active`]: resolve, apply the sticky rule, load (or reuse what was loaded) and activate.
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn resolve_and_load(state: &mut State, key: Key) -> Result<Option<Arc<Active>>, ActiveError> {
    state.key = None;
    state.resolution = None;
    let cwd_error;
    let cwd = match &key.cwd {
        Ok(cwd) => Ok(cwd.as_path()),
        Err(message) => {
            cwd_error = io::Error::other(message.clone());
            Err(&cwd_error)
        }
    };
    let resolution = resolve_with_cwd(cwd, key.explicit.as_deref(), key.conf_var.as_deref(), key.require_var.as_deref())?;
    let (path, why) = match &resolution {
        Resolution::Unfrozen => {
            if let Some(was) = &state.activated {
                return Err(ActiveError::ResolutionChanged { was: was.clone(), now: None });
            }
            state.key = Some(key);
            state.resolution = Some(resolution);
            return Ok(None);
        }
        Resolution::Frozen { path, why } => (path.clone(), why.clone()),
    };
    if let Some(was) = &state.activated {
        if *was != path {
            return Err(ActiveError::ResolutionChanged { was: was.clone(), now: Some(path) });
        }
    }
    let stamp = stamp_of(&path);
    let reusable = state
        .loaded
        .as_ref()
        .is_some_and(|loaded| loaded.path == path && loaded.why == why && loaded.stamp.is_some() && loaded.stamp == stamp);
    if !reusable {
        let _loading = LoadingGuard::new();
        state.loads += 1;
        let loaded = match super::load::load_file(&path, super::load::LoadOptions::default()) {
            Ok(catalog) => {
                let stamp = Some(catalog.stamp);
                // activation: the complete set of the file over the compiled defaults, each value stored once (a reader
                // on another thread never sees a default in between)
                catalog.apply_process_settings();
                state.activated = Some(path.clone());
                let active = Active { path: path.clone(), why: why.clone(), catalog };
                Loaded { path, why, stamp, outcome: Ok(Arc::new(active)) }
            }
            Err(e) => Loaded { path, why, stamp, outcome: Err(ActiveError::Load(e)) },
        };
        state.loaded = Some(loaded);
    }
    state.key = Some(key);
    state.resolution = Some(resolution);
    state.loaded.as_ref().map_or(Ok(None), |loaded| loaded.outcome.clone().map(Some))
}

/// For programs: resolves and loads the configuration in force now, so that a problem is an ordinary error at
/// start-up (with [`ActiveError::exit_code`]) instead of a refusal in the middle of a computation. It also
/// activates a frozen configuration early (applies its settings).
pub fn ensure_valid() -> Result<(), ActiveError> {
    active().map(|_| ())
}

/// Forgets the cache: the next [`active`] resolves and loads again, and the next use of the unfrozen catalog reads
/// the working directory's `physure.conf` again (which applies its settings again). If a frozen configuration had been activated,
/// the process-wide settings go back to what the unfrozen catalog gives them (the compiled defaults, then the
/// embedded catalog's `[Settings]` and `[Compatibility]`), so the settings of a file no longer in force do not stay.
pub fn reset_cache() {
    let mut state = state();
    let was_activated = state.activated.take().is_some();
    state.key = None;
    state.resolution = None;
    state.loaded = None;
    state.local = None;
    if was_activated {
        crate::settings::reset_to_defaults();
        crate::units::conf::apply_embedded_settings();
    }
}

/// The cached merge of the unfrozen catalog with the working directory's `physure.conf`, if any. The lock is held
/// only to clone the `Arc` out.
pub(crate) fn cached_local_catalog() -> Option<Arc<LocalCatalog>> {
    state().local.clone()
}

/// Caches a merge of the unfrozen catalog with the working directory's `physure.conf` (it replaces the one before).
pub(crate) fn store_local_catalog(local: Arc<LocalCatalog>) {
    state().local = Some(local);
}

/// How many times this process loaded a frozen file through [`active`] (for the tests of the cache).
#[doc(hidden)]
pub fn load_count() -> u64 {
    state().loads
}

/// For a binding that does not honour frozen configurations yet (Phase 5 wires them): `Ok(())` when none is in
/// force, otherwise an error that says what to do. The binding must refuse rather than read the library's own
/// catalog, which would mix two configurations in one process without a word. It goes through the cached
/// [`active`] (Java calls it on every static operation): a valid frozen file, a broken one, a required one that is
/// missing and a changed resolution after an activation are all refusals.
pub fn refuse_unwired(binding: &str) -> Result<(), String> {
    let later = format!("the {binding} binding honours frozen configurations from Phase 5");
    let what_to_do = "unset PHYSURE_CONF and PHYSURE_REQUIRE_FROZEN and move or remove the file, or use the CLI";
    let in_force = match active() {
        Ok(Some(active)) => Some(active.path.clone()),
        Ok(None) => None,
        Err(e) => return Err(format!("{e}; {later}: {what_to_do}")),
    };
    match in_force {
        None => Ok(()),
        Some(path) => Err(format!(
            "a frozen configuration is in force ({}); {later}: {what_to_do}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A unique temporary directory, canonicalised, removed on drop. An ancestor of `temp_dir()` that holds a
    /// `physure.frozen.conf` is out of these tests' control: the guard test of the library reports it.
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Dir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("physure_resolve_{}_{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path.canonicalize().unwrap())
        }
        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "x").unwrap();
            path
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn inputs<'a>(explicit: Option<&'a Path>, conf: Option<&'a str>, require: Option<&'a str>, cwd: &'a Path) -> Inputs<'a> {
        Inputs { explicit, conf_var: conf, require_var: require, cwd }
    }

    #[test]
    fn nothing_found_is_unfrozen() {
        let d = Dir::new();
        assert_eq!(resolve(&inputs(None, None, None, &d.0)).unwrap(), Resolution::Unfrozen);
        assert_eq!(resolve(&inputs(None, None, Some("0"), &d.0)).unwrap(), Resolution::Unfrozen);
    }

    #[test]
    fn the_nearest_file_found_walking_up_wins() {
        let d = Dir::new();
        let outer = d.file(FROZEN_FILE_NAME);
        let inner = d.file(&format!("a/b/{FROZEN_FILE_NAME}"));
        let deep = d.0.join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(resolve(&inputs(None, None, None, &deep)).unwrap(), Resolution::Frozen { path: inner.clone(), why: Why::Discovered });
        assert_eq!(resolve(&inputs(None, None, None, &d.0.join("a"))).unwrap(), Resolution::Frozen { path: outer, why: Why::Discovered });
        assert_eq!(resolve(&inputs(None, None, None, &d.0.join("a/b"))).unwrap(), Resolution::Frozen { path: inner, why: Why::Discovered });
    }

    #[test]
    fn a_directory_with_the_name_is_an_error_not_unfrozen() {
        let d = Dir::new();
        fs::create_dir_all(d.0.join(FROZEN_FILE_NAME)).unwrap();
        assert!(matches!(resolve(&inputs(None, None, None, &d.0)), Err(ResolveError::Unreadable { .. })));
    }

    #[test]
    fn a_dangling_link_with_the_name_is_an_error_not_unfrozen() {
        let d = Dir::new();
        let link = d.0.join(FROZEN_FILE_NAME);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(d.0.join("missing"), &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(d.0.join("missing"), &link);
        #[cfg(not(any(unix, windows)))]
        let made: io::Result<()> = Err(io::Error::other("no links"));
        if made.is_err() {
            return; // creating links needs a privilege on some systems: nothing to test there
        }
        assert!(matches!(resolve(&inputs(None, None, None, &d.0)), Err(ResolveError::Unreadable { .. })));
    }

    #[test]
    fn the_explicit_path_beats_the_variable_beats_discovery() {
        let d = Dir::new();
        let found = d.file(FROZEN_FILE_NAME);
        let var = d.file("var.conf");
        let explicit = d.file("explicit.conf");
        let r = resolve(&inputs(Some(&explicit), Some(var.to_str().unwrap()), None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: explicit, why: Why::Explicit });
        let r = resolve(&inputs(None, Some(var.to_str().unwrap()), None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: var, why: Why::Environment });
        let r = resolve(&inputs(None, None, None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: found, why: Why::Discovered });
    }

    #[test]
    fn a_relative_variable_or_explicit_path_is_relative_to_the_current_directory() {
        let d = Dir::new();
        let file = d.file("sub/x.conf");
        let r = resolve(&inputs(None, Some("sub/x.conf"), None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: file.clone(), why: Why::Environment });
        let r = resolve(&inputs(Some(Path::new("sub/x.conf")), None, None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: file, why: Why::Explicit });
    }

    #[test]
    fn an_explicit_path_or_variable_that_is_missing_is_an_error_and_never_falls_back() {
        let d = Dir::new();
        d.file(FROZEN_FILE_NAME);
        let missing = d.0.join("nope.conf");
        assert!(matches!(resolve(&inputs(Some(&missing), None, None, &d.0)), Err(ResolveError::Missing { why: Why::Explicit, .. })));
        assert!(matches!(resolve(&inputs(None, Some("nope.conf"), None, &d.0)), Err(ResolveError::Missing { why: Why::Environment, .. })));
        let dir_path = d.0.clone();
        assert!(matches!(resolve(&inputs(Some(&dir_path), None, None, &d.0)), Err(ResolveError::Missing { .. })), "a directory is not a file");
        assert!(matches!(resolve(&inputs(Some(Path::new("")), None, None, &d.0)), Err(ResolveError::Missing { .. })), "an empty explicit path");
        assert_eq!(resolve(&inputs(None, Some(""), None, &d.0)).unwrap_err(), ResolveError::EmptyVariable(ENV_CONF));
        // a missing explicit path is an error even when the variable names a good file
        let var = d.file("var.conf");
        assert!(matches!(
            resolve(&inputs(Some(&missing), Some(var.to_str().unwrap()), None, &d.0)),
            Err(ResolveError::Missing { why: Why::Explicit, .. })
        ));
    }

    #[test]
    fn requiring_a_frozen_file_fails_closed_and_typos_are_errors() {
        let d = Dir::new();
        assert_eq!(resolve(&inputs(None, None, Some("1"), &d.0)).unwrap_err(), ResolveError::Required);
        d.file(FROZEN_FILE_NAME);
        assert!(resolve(&inputs(None, None, Some("1"), &d.0)).is_ok());
        for bad in ["", "true", "yes", "01", "1 ", " 1", "on", "2", "0 "] {
            assert_eq!(resolve(&inputs(None, None, Some(bad), &d.0)).unwrap_err(), ResolveError::BadRequireValue(bad.to_string()), "{bad:?}");
        }
        // a bad require value is an error even when an explicit path names a good file
        let file = d.file("x.conf");
        assert_eq!(
            resolve(&inputs(Some(&file), None, Some("true"), &d.0)).unwrap_err(),
            ResolveError::BadRequireValue("true".into())
        );
    }

    #[test]
    fn the_errors_say_what_to_do() {
        for e in [
            ResolveError::Required,
            ResolveError::BadRequireValue("true".into()),
            ResolveError::EmptyVariable(ENV_CONF),
            ResolveError::NotUnicode(ENV_CONF),
            ResolveError::Missing { path: PathBuf::from("x.conf"), why: Why::Environment },
            ResolveError::CurrentDir { message: "gone".into(), needed_for: "x.conf".into() },
            ResolveError::Unreadable { path: PathBuf::from("x.conf"), message: "denied".into() },
        ] {
            let text = e.to_string();
            assert!(text.contains("PHYSURE") || text.contains("x.conf"), "{text}");
        }
        assert!(ResolveError::Required.to_string().contains(FROZEN_FILE_NAME));
        assert!(ResolveError::Missing { path: PathBuf::from("x.conf"), why: Why::Explicit }.to_string().contains("no fallback"));
    }

    /// Every other test of the library assumes the unfrozen catalog. A `physure.frozen.conf` in the home directory or
    /// anywhere above the repository, or `PHYSURE_CONF` / `PHYSURE_REQUIRE_FROZEN` set in the environment, changes
    /// what every test reads: this one says so instead of letting the others fail mysteriously.
    #[test]
    fn no_frozen_file_is_in_force_for_the_unit_tests() {
        for name in [ENV_CONF, ENV_REQUIRE] {
            assert!(std::env::var_os(name).is_none(), "{name} is set in the environment of the tests: unset it");
        }
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let found = resolve(&inputs(None, None, None, crate_dir)).unwrap();
        assert_eq!(found, Resolution::Unfrozen, "a {FROZEN_FILE_NAME} above the repository changes every test: move it");
        assert_eq!(resolve_process(), Ok(Resolution::Unfrozen), "the tests' own process resolves to a frozen file");
    }

    // ---- an unreadable current directory (Task 9, C9) ----

    fn gone() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::NotFound, "the current directory was deleted")
    }

    #[test]
    fn an_unreadable_current_directory_is_unfrozen_when_nothing_requires_a_frozen_file() {
        let e = gone();
        assert_eq!(resolve_with_cwd(Err(&e), None, None, None).unwrap(), Resolution::Unfrozen);
        assert_eq!(resolve_with_cwd(Err(&e), None, None, Some("0")).unwrap(), Resolution::Unfrozen);
    }

    #[test]
    fn an_unreadable_current_directory_is_an_error_when_something_requires_a_frozen_file() {
        let e = gone();
        assert!(matches!(resolve_with_cwd(Err(&e), None, None, Some("1")), Err(ResolveError::CurrentDir { .. })));
        assert!(matches!(resolve_with_cwd(Err(&e), Some(Path::new("rel.conf")), None, None), Err(ResolveError::CurrentDir { .. })));
        assert!(matches!(resolve_with_cwd(Err(&e), None, Some("rel.conf"), None), Err(ResolveError::CurrentDir { .. })));
        assert_eq!(resolve_with_cwd(Err(&e), None, Some(""), None).unwrap_err(), ResolveError::EmptyVariable(ENV_CONF));
        assert_eq!(resolve_with_cwd(Err(&e), None, None, Some("yes")).unwrap_err(), ResolveError::BadRequireValue("yes".into()));
        let message = resolve_with_cwd(Err(&e), None, None, Some("1")).unwrap_err().to_string();
        assert!(message.contains("deleted") && message.contains(ENV_REQUIRE), "{message}");
    }

    #[test]
    fn an_absolute_explicit_path_or_variable_needs_no_current_directory() {
        let d = Dir::new();
        let file = d.file("abs.conf");
        let e = gone();
        assert_eq!(
            resolve_with_cwd(Err(&e), Some(&file), None, None).unwrap(),
            Resolution::Frozen { path: file.clone(), why: Why::Explicit }
        );
        assert_eq!(
            resolve_with_cwd(Err(&e), None, Some(file.to_str().unwrap()), Some("1")).unwrap(),
            Resolution::Frozen { path: file.clone(), why: Why::Environment }
        );
        let missing = d.0.join("missing.conf");
        assert!(matches!(resolve_with_cwd(Err(&e), Some(&missing), None, None), Err(ResolveError::Missing { .. })));
        // and with a readable directory it is the same as `resolve`
        assert_eq!(resolve_with_cwd(Ok(&d.0), Some(&file), None, None).unwrap(), resolve(&inputs(Some(&file), None, None, &d.0)).unwrap());
    }
}
