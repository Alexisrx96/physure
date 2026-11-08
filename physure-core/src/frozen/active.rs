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
use std::sync::Mutex;

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

/// The nearest `physure.frozen.conf` that is a file, from `cwd` upwards. A directory with that name is passed over;
/// a candidate that exists but cannot be examined is an error rather than "nothing found".
fn discover(cwd: &Path) -> Result<Option<PathBuf>, ResolveError> {
    for dir in cwd.ancestors() {
        let candidate = dir.join(FROZEN_FILE_NAME);
        match std::fs::metadata(&candidate) {
            Ok(meta) if meta.is_file() => return Ok(Some(candidate)),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
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

/// The host says which frozen file is in force (the CLI's `--conf`). `None` clears it.
pub fn set_explicit_conf(path: Option<PathBuf>) {
    *EXPLICIT.lock().unwrap_or_else(|e| e.into_inner()) = path;
}

/// The host says which file contains the engine, for the policy `binary` (the CLI: its own executable; Python:
/// the extension module; Java: the library). Without it a file pinned with `binary` cannot be loaded.
pub fn set_engine_file(path: Option<PathBuf>) {
    *ENGINE_FILE.lock().unwrap_or_else(|e| e.into_inner()) = path;
}

/// The explicit path the host set, if any.
pub fn explicit_conf() -> Option<PathBuf> {
    EXPLICIT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The engine file the host named, if any.
pub fn engine_file() -> Option<PathBuf> {
    ENGINE_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
    fn a_directory_with_the_name_is_not_a_frozen_file() {
        let d = Dir::new();
        fs::create_dir_all(d.0.join(FROZEN_FILE_NAME)).unwrap();
        assert_eq!(resolve(&inputs(None, None, None, &d.0)).unwrap(), Resolution::Unfrozen);
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
