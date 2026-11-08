# `phs conf` Phase 1c: resolution, sealed loading and the freeze / update / seal operations — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A frozen file in force changes what `physure-core` builds its catalog from, for every consumer at once, and cannot be bypassed silently: `physure_core::units::conf::{build_registry_from_conf, with_conf_registry, build_registry_with}` consult `frozen::active()` first; when a frozen file is in force the catalog is built **only** from its verified sources (the library's own embedded catalog and the working directory's `physure.conf` are not read), and any problem with the file is an error that stops the program (fail closed), never a silent fall back to the unfrozen catalog. Library operations `freeze`, `plan_update` and `plan_seal` produce the files the CLI of Phase 2 will write.

**Architecture:** `frozen/active.rs` (resolution: explicit path, `PHYSURE_CONF`, discovery upward, `PHYSURE_REQUIRE_FROZEN`; the cached `active()`), `frozen/load.rs` (read, parse, verify, engine check, strict replay of the sources into a `UnitRegistry`; `LoadError`), `frozen/ops.rs` (freeze, update and seal as pure transformations of a `FrozenFile` plus an atomic writer), a strict-with-settings `replay_source` in `units/conf.rs`, and a history-time check in `seal.rs`. Three functions of `conf.rs` are hooked.

**Tech Stack:** Rust 2021, standard library only.

**Follows** Phases 1a and 1b (merged; `main` at `8a8600e`). **Not here:** the registry entry listing and the semantic `diff` (1d), the CLI (Phase 2), witnesses (Phase 3), the bindings that read the embedded catalog directly (`load_embedded_catalog` in WebAssembly and Java, the `configparser` read of Python; Phase 5).

> **REVISION NOTICE (2026-11-08, after an adversarial review of this plan, which found four critical flaws, measured or read in the code).** **Task 9 at the end of this file overrides the tasks above wherever they disagree.** Read Task 9 before implementing anything. In particular, wherever Tasks 1–8 or the decisions below say that Python's `UnitSystem` goes through `build_registry_with`, that the infallible hooks panic, that `current_dir` failure is an error, that `replay_source` applies settings, or that `seal` allows "SourceHash 2, ContentHash, Seal", Task 9 replaces it.

## Decisions fixed here (do not re-litigate)

- **Resolution order** (spec §5.1): (1) an explicit path set by the host with `set_explicit_conf` (the CLI's `--conf`); (2) the variable `PHYSURE_CONF`; (3) discovery: the nearest `physure.frozen.conf` found walking up from the current directory to the root of the file system; (4) none: the unfrozen behaviour of today.
- **`PHYSURE_REQUIRE_FROZEN`:** unset or `0` means not required; `1` means required: finding nothing is `ResolveError::Required`. **Any other value, including the empty string, is an error** (`BadRequireValue`): a typo must not silently switch the audit mode off.
- **An explicit path or `PHYSURE_CONF` that is empty, does not exist or is not a file is an error and never falls back** to discovery or to the unfrozen catalog. A relative `PHYSURE_CONF` is relative to the current directory.
- **Sealed loading reads nothing of the library's catalog.** The sources of the file are replayed in order into an empty `UnitRegistry`: source 1 (`embedded`) with the trusted reader (it alone may define the shipped atomic-weight editions), every other source untrusted; all strictly (a bad line is an error); `[Settings]` and `[Compatibility]` of the sources are applied (they are process-wide) **only if the whole file replays without a problem**. A working-directory `physure.conf` is not read while a frozen file is in force.
- **Fail closed.** `build_registry_with` returns `Err(vec![message])`; `build_registry_from_conf` and `with_conf_registry` cannot return an error and **panic** with `physure: refusing to run: <error> (see `phs conf status`)` when a frozen file is required or in force and is not valid. Programs call `frozen::ensure_valid()` early to turn that into an ordinary error message and an exit code (Phase 2: 1 for integrity, 2 for resolution, 3 for the engine).
- **Order of checks when loading:** read (size at most 64 MiB) → parse → `verify` (integrity) → `read_engine` → engine check under the file's policy → replay. The engine check comes before the replay, so a different engine is reported as such. `LoadOptions { check_engine }` lets Phase 2's `status`/`verify`/`diff` skip the engine check (`--allow-engine-mismatch`); **evaluation never skips it**.
- **Which file is hashed for the policy `binary`** is told by the host with `set_engine_file(path)` (the CLI: `current_exe()`; Python: the extension module; Java: the library). If it was not set and the policy is `binary`, the check fails with the explanation "the host did not say which file contains the engine". The core has no default (decided in Phase 1b).
- **Caches (per process):** the outcome of `active()` is cached with a key made of the resolution inputs (explicit path, `PHYSURE_CONF`, `PHYSURE_REQUIRE_FROZEN`, current directory) and the stamp (length, modification time) of the file in force; any change re-resolves and re-loads. The SHA-256 of the engine file is cached per (path, length, modification time), as `Result<String, String>`. Discovery is part of the cached resolution, so a frozen file created later in the same process is seen when the key changes or after `reset_cache()` (used by `freeze` and by tests).
- **The history is chronological** (residual risk 2 of spec §13): `verify` reports `Problem::HistoryTime` when an entry's time is earlier than the previous entry's (or than `created`); times are fixed-width RFC 3339 UTC, so text comparison is exact.
- **Residual risk 1 of spec §13 is closed by a test:** two source texts with the same canonical form build the same registry (`describe()` below).
- **`freeze`** refuses when a frozen file is already in force (that is `update`), when the working directory's `physure.conf` does not replay strictly (it would be written into source 2 and the file would never load: the problems are listed), when `freezable` refuses the policy for this build, and when the output exists. **`update`** adopts the library's embedded catalog and the running engine into a verified file, keeping source 2 and the policy; when nothing differs it returns `None`. **`seal`** accepts a hand edit of the overrides block **only**: the only problems allowed are the stale hash of source 2, the content hash and the seal; any other problem (history, engine, source 1, witnesses) is refused. All three validate with `FrozenFile::verify`, replay strictly before returning, and never write: a separate `write_atomic` does, so the CLI can show the diff and ask first.
- **Actor and time are inputs** of the operations; helpers read them (`actor_from_env`, `now_unix`) for the CLI. The acting engine written to the history is `EngineIdentity::current().short()`, derived by the operation, never typed by the caller.
- **Test isolation:** everything process-global (environment variables, current directory, the explicit path, the engine file, the caches, the process-wide settings) is touched only by tests in `physure-core/tests/frozen_active.rs`, which serialise on one mutex and restore what they change. Pure parts (`resolve`, the key, `actor_from_env`, `tree` helpers) are unit-tested without globals.

## File structure

| File | Responsibility |
|---|---|
| Create `physure-core/src/frozen/active.rs` | `Resolution`, `Why`, `ResolveError`, `resolve` (pure), host setters, `active()`, `ensure_valid()`, `reset_cache()`, the cache |
| Create `physure-core/src/frozen/load.rs` | `LoadOptions`, `FrozenCatalog`, `LoadError`, `load_file`, the engine-file hash cache |
| Create `physure-core/src/frozen/ops.rs` | `FreezeOptions`, `freeze`, `UpdatePlan`/`plan_update`, `SealPlan`/`plan_seal`, `write_atomic`, `actor_from_env`, `now_unix` |
| Modify `physure-core/src/units/conf.rs` | `replay_source`; the three hooks |
| Modify `physure-core/src/frozen/{seal,mod}.rs` | `Problem::HistoryTime`; module list and re-exports |
| Create `physure-core/tests/frozen_active.rs` | the end-to-end tests that touch global state |
| Modify | `CHANGELOG.md`, `structure.md`, `ROADMAP.md`, `docs/ROADMAP.md`, the spec (§5.1, §5.2, §6, §13) |

All commands run from the worktree root. Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. New code must be clippy-clean (older errors elsewhere do not count). Tests first; show each failing before its implementation.

---

### Task 1: Resolution (pure) and the host setters

**Files:** Create `physure-core/src/frozen/active.rs` (resolution part only); `mod.rs` gets `pub mod active;`.

- [ ] **Step 1: Tests first** (unit tests in `active.rs`; they use real temporary directories under `std::env::temp_dir()` made unique with the process id and a counter, removed at the end; no global state):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Dir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("physure_resolve_{}_{n}", std::process::id()));
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
    fn a_relative_variable_is_relative_to_the_current_directory() {
        let d = Dir::new();
        let file = d.file("sub/x.conf");
        let r = resolve(&inputs(None, Some("sub/x.conf"), None, &d.0)).unwrap();
        assert_eq!(r, Resolution::Frozen { path: file, why: Why::Environment });
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
        assert_eq!(resolve(&inputs(None, Some(""), None, &d.0)).unwrap_err(), ResolveError::EmptyVariable(ENV_CONF));
    }

    #[test]
    fn requiring_a_frozen_file_fails_closed_and_typos_are_errors() {
        let d = Dir::new();
        assert_eq!(resolve(&inputs(None, None, Some("1"), &d.0)).unwrap_err(), ResolveError::Required);
        d.file(FROZEN_FILE_NAME);
        assert!(resolve(&inputs(None, None, Some("1"), &d.0)).is_ok());
        for bad in ["", "true", "yes", "01", "1 ", "on", "2"] {
            assert_eq!(resolve(&inputs(None, None, Some(bad), &d.0)).unwrap_err(), ResolveError::BadRequireValue(bad.to_string()), "{bad:?}");
        }
    }

    #[test]
    fn the_errors_say_what_to_do() {
        for e in [
            ResolveError::Required,
            ResolveError::BadRequireValue("true".into()),
            ResolveError::EmptyVariable(ENV_CONF),
            ResolveError::Missing { path: PathBuf::from("x.conf"), why: Why::Environment },
        ] {
            let text = e.to_string();
            assert!(text.contains("PHYSURE") || text.contains("x.conf"), "{text}");
        }
        assert!(ResolveError::Required.to_string().contains(FROZEN_FILE_NAME));
    }
}
```

- [ ] **Step 2: Implement** (above the tests):

```rust
//! Which configuration is in force (spec §5.1): resolution of a frozen file, and, in Task 5, the cached outcome of
//! loading it. Resolution is pure: the environment and the current directory are inputs.

use std::fmt;
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
    /// An explicit path or `PHYSURE_CONF` that is not a file.
    Missing { path: PathBuf, why: Why },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Required => write!(
                f,
                "{ENV_REQUIRE}=1 but no frozen configuration was found (looked for {FROZEN_FILE_NAME} from the current directory upwards, in {ENV_CONF} and in the explicit path); make one with `phs conf freeze`"
            ),
            ResolveError::BadRequireValue(v) => write!(f, "{ENV_REQUIRE} must be unset, 0 or 1, not {v:?}"),
            ResolveError::EmptyVariable(name) => write!(f, "{name} is set but empty; unset it or name a frozen configuration"),
            ResolveError::Missing { path, why } => {
                let from = match why {
                    Why::Explicit => "the explicit path".to_string(),
                    Why::Environment => ENV_CONF.to_string(),
                    Why::Discovered => "discovery".to_string(),
                };
                write!(f, "{from} names {} but it is not a file; there is no fallback to the unfrozen catalog", path.display())
            }
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
    let required = match inputs.require_var {
        None | Some("0") => false,
        Some("1") => true,
        Some(other) => return Err(ResolveError::BadRequireValue(other.to_string())),
    };
    if let Some(path) = inputs.explicit {
        let path = inputs.cwd.join(path);
        return if path.is_file() {
            Ok(Resolution::Frozen { path, why: Why::Explicit })
        } else {
            Err(ResolveError::Missing { path, why: Why::Explicit })
        };
    }
    if let Some(value) = inputs.conf_var {
        if value.is_empty() {
            return Err(ResolveError::EmptyVariable(ENV_CONF));
        }
        let path = inputs.cwd.join(value);
        return if path.is_file() {
            Ok(Resolution::Frozen { path, why: Why::Environment })
        } else {
            Err(ResolveError::Missing { path, why: Why::Environment })
        };
    }
    for dir in inputs.cwd.ancestors() {
        let candidate = dir.join(FROZEN_FILE_NAME);
        if candidate.is_file() {
            return Ok(Resolution::Frozen { path: candidate, why: Why::Discovered });
        }
    }
    if required {
        Err(ResolveError::Required)
    } else {
        Ok(Resolution::Unfrozen)
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

pub(crate) fn explicit_conf() -> Option<PathBuf> {
    EXPLICIT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn engine_file() -> Option<PathBuf> {
    ENGINE_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
```

- [ ] **Step 3:** `cargo test -p physure --lib frozen::active` → 8 pass. Clippy. Commit `feat(core): which frozen configuration is in force — resolution, pure and strict`.

---

### Task 2: Strict replay of a source, with settings applied only when the whole text is clean

**Files:** Modify `physure-core/src/units/conf.rs`.

`extend_registry` is strict but does not apply `[Settings]`; the lenient reader applies them but ignores problems; and applying settings while reading leaves process-wide state half changed when a later line fails. A frozen source needs strict + settings + nothing applied on failure.

- [ ] **Step 1: Tests first** (in `conf.rs`'s existing test module or `tests/conf_replay.rs`; settings are process-wide, so the test that applies them serialises on a mutex local to that test file and restores the setting it changes: `parallel_threshold` is a convenient one, restore its previous value):
  1. `a_clean_text_is_replayed_and_its_settings_are_applied`: text `"[Settings]\nparallel_threshold = 12345\n[Constants]\nzz_const = 1 m\n"` → `Ok`, the registry has `zz_const`, `parallel_threshold()` is 12345 (restore afterwards).
  2. `a_text_with_a_problem_changes_nothing_not_even_the_settings`: `"[Settings]\nparallel_threshold = 54321\n[AtomicWeights]\nFe = abc, 0.1\n"` → `Err` with one problem naming the line; the registry is as before; `parallel_threshold()` is unchanged.
  3. `trusted_replay_may_define_a_shipped_edition_and_untrusted_may_not`: `"[AtomicWeights:standard-2024]\nH = 1.0, 0\n"` with `trusted = true` → Ok; with `false` → Err mentioning that the edition is frozen.
  4. `two_texts_with_the_same_canonical_form_build_the_same_registry` (closes residual risk 1): take `DEFAULT_PHYSURE_CONF`, make a second text that differs only by comments, blank lines, leading/trailing whitespace on every line, CRLF and extra `#`/`;` lines, replay both (`trusted = true`) into empty registries and assert `describe(&a) == describe(&b)` using the `describe` helper of Task 7 (put `describe` in `physure-core/src/units/conf.rs` under `#[cfg(test)] pub(crate)` or in a `tests/common/mod.rs`; Task 7 reuses it).
- [ ] **Step 2: Implement** `pub(crate) fn replay_source(registry: &mut UnitRegistry, constants: &mut HashMap<String, String>, text: &str, trusted: bool) -> Result<(), Vec<String>>`:
  - first a **dry run** into a clone with `Reading { apply_settings: false, trusted }` and the strict problem list; if it has problems return `Err(problems)` and touch nothing;
  - then the real run into `registry`/`constants` with `Reading { apply_settings: true, trusted }` (deterministic, it cannot fail now); `debug_assert!` that it found no problems.
  Measure the cost on `DEFAULT_PHYSURE_CONF` (report milliseconds; the frozen catalog is replayed once per process).
- [ ] **Step 3:** green, clippy, commit `feat(core): strict replay of a frozen source, settings applied only when the whole text is clean`.

---

### Task 3: Loading a frozen file

**Files:** Create `physure-core/src/frozen/load.rs`; `mod.rs` gets `pub mod load;`.

```rust
pub struct LoadOptions { pub check_engine: bool }          // Default: true
pub struct FrozenCatalog {
    pub registry: UnitRegistry,
    pub constants: HashMap<String, String>,
    pub file: FrozenFile,
    pub engine: EngineBlock,
}
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    Io { path: PathBuf, message: String },
    TooLarge { path: PathBuf, bytes: u64 },          // more than 64 MiB
    NotFrozen { path: PathBuf, message: String },    // FrozenFile::parse failed (wrong magic, malformed)
    Integrity { path: PathBuf, problems: Vec<String> },   // verify() or read_engine() failed
    Engine { path: PathBuf, mismatch: EngineMismatch },
    Replay { path: PathBuf, source: String, problems: Vec<String> },
}
impl Display, std::error::Error   // every message names the file and says what was expected and found
pub fn load_file(path: &Path, options: LoadOptions) -> Result<FrozenCatalog, LoadError>;
```

- [ ] **Step 1: Tests first** (unit tests in `load.rs` build real files in a temp dir with `FrozenFile::new` from the real `DEFAULT_PHYSURE_CONF` source and a `Release`-policy block via `engine_pairs`; engine checks use a `Release`/`Record` policy so they pass on any build; the `binary` cases use `set_engine_file`, which is global: put those in `tests/frozen_active.rs` in Task 5 instead):
  1. `a_valid_file_loads_and_its_registry_knows_the_catalog`: `load_file` → `Ok`; `registry.knows("m")`; `constants["molar_gas_constant"]` exists; the returned `file.verify()` is empty.
  2. `a_missing_file_is_an_io_error_that_names_it`; `a_file_that_is_not_frozen_says_so` (a plain conf: `NotFrozen`); `a_file_over_the_size_limit_is_refused_without_reading_it` (create a sparse file of 64 MiB + 1 with `set_len`).
  3. `any_edit_is_an_integrity_error_listing_every_problem`: edit one digit of a constant in the rendered text → `Integrity` whose problems mention the source and the seal.
  4. `an_engine_block_that_cannot_be_read_is_integrity_not_engine` (verify catches it first).
  5. `a_different_engine_is_an_engine_error_and_check_engine_false_skips_it`: a `build`-policy file made with a fake identity (version `9.9.9`, commit `0123…`) → `Engine { mismatch }` listing `version`; with `check_engine: false` it loads. `Release` policy with the same version as the running build loads; `Record` loads always.
  6. `a_source_that_does_not_replay_is_a_replay_error_naming_the_source`: build a sealed file whose source 2 (overrides) contains a bad line `Fe = abc, 0.1` under `[AtomicWeights]` → `Replay { source: "overrides", problems }`; **nothing global changed** (settings in that text are not applied: use a `parallel_threshold` line in source 1 of a second failing file).
  7. `the_catalog_comes_only_from_the_sources`: the embedded source of the file is `DEFAULT_PHYSURE_CONF` with `molar_gas_constant = 8.0 J*mol^-1*K^-1` and an extra constant `zz_sentinel = 1 m`; the loaded registry has `zz_sentinel` and the modified constant; the library's own catalog (`build_registry_with("")` is hooked in Task 5, so use `embedded_catalog_for_tests()` or compare with `DEFAULT_PHYSURE_CONF` parsed directly) has neither. This is the test that shows an upgrade of the library cannot change a value.
  8. `later_sources_are_untrusted`: a third source `systems/x.conf` containing `[AtomicWeights:standard-2024]\nH = 1.0, 0\n` → `Replay` refusal (frozen edition), while the same text as source 1 would be accepted.
- [ ] **Step 2: Implement** exactly the pipeline of "Decisions fixed here": read (check `metadata().len()` first) → `FrozenFile::parse` → `verify` (problems rendered with their `Display`) → `read_engine` → if `options.check_engine`: `block.check(&EngineIdentity::current(), || engine_file_hash())` where `engine_file_hash` reads `active::engine_file()`, hashes it with `engine::sha256_of_file` through a **per-process cache keyed by (path, length, modification time)** storing `Result<String, String>`, and returns `io::Error::other("the host did not say which file contains the engine")` when none was set → replay: `UnitRegistry::new()`, empty constants, for each source in order `replay_source(.., trusted = index == 0 && origin == "embedded")`, mapping an `Err(problems)` to `LoadError::Replay` with the source's origin.
- [ ] **Step 3:** green, clippy, commit `feat(core): loading a frozen file — verify, engine check, strict replay, nothing read from the library's catalog`.

---

### Task 4: The history is chronological

**Files:** Modify `physure-core/src/frozen/seal.rs`.

- [ ] **Step 1: Tests first:** `an_entry_earlier_than_the_previous_one_is_a_problem` (hand-built via the crate-private fields: swap the times of entries 1 and 2 and re-chain the hashes so only the time rule can catch it) → `Problem::HistoryTime { n, previous, found }`; `an_entry_earlier_than_created_is_a_problem`; equal times are allowed; the exhaustive one-byte-edit test and the independent seal vector still pass.
- [ ] **Step 2: Implement** the check in `verify` (text comparison of the fixed-width RFC 3339 strings: previous entry's time, starting from `created`) and the `Display` of the variant.
- [ ] **Step 3:** green, clippy, commit `fix(core): a frozen file's history must be chronological`.

---

### Task 5: The active catalog, its cache and the three hooks

**Files:** Modify `active.rs` (the cached part), `conf.rs` (hooks), `mod.rs`; Create `physure-core/tests/frozen_active.rs`.

Add to `active.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveError { Resolve(ResolveError), Load(LoadError) }   // Display + Error; Display of Load already names the file

pub struct Active { pub path: PathBuf, pub why: Why, pub catalog: FrozenCatalog }

/// The frozen configuration in force, if any. `Ok(None)` is the unfrozen behaviour. Cached (see "Caches").
pub fn active() -> Result<Option<Arc<Active>>, ActiveError>;
/// For programs: turns a refusal into an ordinary error before anything runs.
pub fn ensure_valid() -> Result<(), ActiveError> { active().map(|_| ()) }
/// Forgets the cache (used by `freeze` and by tests).
pub fn reset_cache();
```

`active()` builds `Inputs` from the real process (explicit path from `explicit_conf()`, `std::env::var(ENV_CONF)` and `ENV_REQUIRE` with non-Unicode values treated as an error of the same kind as an empty value, `std::env::current_dir()` — if the current directory cannot be read that is an error, not "unfrozen"), computes the key `(explicit, conf_var, require_var, cwd, stamp of the resolved file)`, returns the cached outcome when the key is equal, otherwise resolves, loads with `LoadOptions { check_engine: true }` and caches the `Result` (errors are cached too: they are not retried until the key changes).

Hooks in `conf.rs` (the only three places that build the catalog for everyone; Python's `UnitSystem` creation goes through `build_registry_with`):

```rust
pub fn build_registry_with(extra: &str) -> Result<(UnitRegistry, HashMap<String, String>), Vec<String>> {
    let (mut reg, mut constants) = match frozen::active::active() {
        Err(e) => return Err(vec![e.to_string()]),
        Ok(Some(a)) => (a.catalog.registry.clone(), a.catalog.constants.clone()),
        Ok(None) => embedded_catalog().clone(),
    };
    extend_registry(&mut reg, &mut constants, extra)?;
    Ok((reg, constants))
}
// build_registry_from_conf: Err(e) => panic!("physure: refusing to run: {e} (see `phs conf status`)");
//   Ok(Some(a)) => (clone of the frozen registry and constants, and the working directory's physure.conf is NOT read);
//   Ok(None) => exactly today's code.
// with_conf_registry: the same three arms; the frozen arm calls f(&a.catalog.registry) by reference.
```

- [ ] **Step 1: Tests first** in `tests/frozen_active.rs` (one mutex for the whole file; a guard struct that restores the environment variables, the current directory, the explicit path, the engine file and calls `reset_cache()` on drop; each test works in its own temp directory and `set_current_dir`s into it):
  1. `without_a_frozen_file_nothing_changes`: `active()` is `Ok(None)`; `build_registry_with("")` equals the embedded catalog by `describe`.
  2. `a_discovered_file_replaces_the_library_catalog` — **the promise of the whole feature**: write a frozen file in the temp dir whose embedded source is `DEFAULT_PHYSURE_CONF` with `molar_gas_constant = 8.0 J*mol^-1*K^-1` plus `zz_sentinel = 1 m`; then `with_conf_registry(|r| r.constants_meta.get("molar_gas_constant"))` is 8.0 and has `zz_sentinel`; `crate::chemistry::gas_constant()` (through the real hooked path) reads 8.0; and from a subdirectory (discovery upwards) the same; after `reset_cache()` and removing the file, back to 8.31446261815324.
  3. `the_working_directory_conf_is_not_read_while_a_frozen_file_is_in_force`: a plain `physure.conf` next to the frozen file defining `zz_local = 1 m` → not in the registry; unfrozen → it is.
  4. `precedence_explicit_then_variable_then_discovery` end to end with three different files with different sentinels.
  5. `a_broken_frozen_file_stops_everything`: edit one digit → `ensure_valid()` is `Err` with the file named; `build_registry_with("")` is `Err`; `with_conf_registry` and `build_registry_from_conf` **panic** (`std::panic::catch_unwind`, message contains "refusing to run"); it does **not** fall back to the embedded catalog; fixing the file (rewriting it) and `reset_cache()` (or just the changed stamp) makes it work again.
  6. `requiring_a_frozen_file_without_one_stops_everything` (`PHYSURE_REQUIRE_FROZEN=1`), and `a_typo_in_the_require_variable_stops_everything` (`true`).
  7. `a_different_engine_stops_evaluation_but_not_inspection`: a `build`-policy file with a fake identity: `ensure_valid()` is `Err(Load(Engine))`; `load_file(path, LoadOptions { check_engine: false })` works.
  8. `the_binary_policy_needs_the_host_to_name_the_engine_file`: a `binary` file whose recorded hash is that of the test executable (`std::env::current_exe()`): without `set_engine_file` → engine error mentioning the host; with `set_engine_file(Some(current_exe))` → loads; with another file → mismatch. (The `binary` block needs a known commit: if `EngineIdentity::current().commit == "unknown"` build the file with a synthetic identity equal to `current()` but a fixed known commit and policy `binary`, and give `check` a matching running identity — `binary` does not compare identities, only the hash; state in a comment which branch ran and assert that one of the two ran.)
  9. `the_cache_notices_a_changed_file`: load, rewrite the file with a different sentinel, the next `active()` sees it without `reset_cache()` (stamp changed; if the file system's timestamp granularity makes the stamp equal, also change the length so the test is deterministic).
  10. `settings_of_the_frozen_file_are_applied_and_a_failed_load_applies_none` (restore `parallel_threshold` afterwards).
- [ ] **Step 2: Implement** the cache, `active()`, the hooks, re-exports (`frozen::{active, ensure_valid, reset_cache, set_explicit_conf, set_engine_file, Resolution, ResolveError, Why, ActiveError, Active}`).
- [ ] **Step 3:** green; `cargo test -p physure --no-fail-fast` all green (the pre-existing tests that call `build_registry_with("")` must be unaffected: no frozen file exists above the crate directory; if one exists on the developer's machine the tests must fail loudly with a message saying so — add a guard test `no_frozen_file_is_in_force_for_the_unit_tests` in the lib tests that asserts `resolve` finds none from the crate directory, with a message explaining that a `physure.frozen.conf` above the repository changes every test); measure the cost of `with_conf_registry` per call in the unfrozen and frozen cases (report microseconds; a hot path: it must stay within the same order as today's single `stat`); commit `feat(core): a frozen configuration in force replaces the catalog for every consumer, and cannot be bypassed`.

---

### Task 6: Freeze, update and seal as library operations

**Files:** Create `physure-core/src/frozen/ops.rs`; `mod.rs` re-exports.

```rust
pub struct FreezeOptions<'a> {
    pub cwd: &'a Path,             // where physure.conf is looked for and the default output is made
    pub out: Option<PathBuf>,      // default: cwd/physure.frozen.conf
    pub reason: &'a str,
    pub policy: EnginePolicy,
    pub engine_file: Option<&'a Path>,   // required for Binary: the file that contains the engine; hashed here
    pub time_unix: u64,
    pub actor: &'a str,
}
#[derive(Debug)] pub enum OpsError { AlreadyFrozen(PathBuf), WouldOverwrite(PathBuf), Conf { problems: Vec<String> }, Engine(String), Frozen(FrozenError), Load(LoadError), Refused(String), Io{path: PathBuf, message: String} }
pub fn freeze(options: &FreezeOptions<'_>) -> Result<(FrozenFile, PathBuf), OpsError>;       // does not write

pub struct UpdatePlan { pub file: FrozenFile, pub summary: String }
pub fn plan_update(path: &Path, reason: &str, policy: Option<EnginePolicy>, engine_file: Option<&Path>, time_unix: u64, actor: &str) -> Result<Option<UpdatePlan>, OpsError>;  // None = nothing to update

pub struct SealPlan { pub file: FrozenFile, pub summary: String }
pub fn plan_seal(path: &Path, reason: &str, time_unix: u64, actor: &str) -> Result<Option<SealPlan>, OpsError>;  // None = no hand edit found

pub fn write_atomic(path: &Path, text: &str, replace: bool) -> Result<(), OpsError>;
pub fn actor_from_env(get: impl Fn(&str) -> Option<String>) -> String;
pub fn now_unix() -> u64;
```

Rules (exactly the "Decisions fixed here" paragraph on the three operations). Details:
- `freeze`: `active::active()` must be `Ok(None)` (a refusal or a frozen file in force → `AlreadyFrozen`/the error); sources are `Source::new("embedded", DEFAULT_PHYSURE_CONF)` and `Source::new("overrides", <cwd/physure.conf text, or the default comment block>)`; **before building the file replay both strictly into an empty registry** and return `Conf { problems }` if the overrides do not replay; engine block `engine_pairs(&EngineIdentity::current(), policy, hash)` where the hash is `sha256_of_file(engine_file)` (cached) for `Binary`; environment `environment_pairs()`; first entry: action `freeze`, `engine` = `EngineIdentity::current().short()`, `time` = `rfc3339_from_unix(time_unix)` (`None` past year 9999 is an error), `diff` = `"embedded <N> lines; overrides <M> lines"`; `created` = the same time.
- `plan_update`: `load_file(path, LoadOptions{check_engine:false})`-style checks are NOT used; read + parse + `verify` must be clean (otherwise `Refused("the file was modified: seal it or restore it first")`); new source 1 = `DEFAULT_PHYSURE_CONF`, new engine block of the running build with the policy given or the file's own; if the canonical text of source 1 and the engine pairs are unchanged → `Ok(None)`; else `set_source_text(0, …)`, `set_engine(…)`, strict replay of all sources (else `Conf`), `reseal` with action `update`, `diff` = `"embedded: +<a> -<r> canonical lines; engine: <changed|unchanged>; policy: <old> -> <new>"` computed from canonical-line multiset differences, and returns the plan. `summary` is the same text for the CLI.
- `plan_seal`: read + parse (not verified); `verify()`; if empty → `Ok(None)`; if any problem other than `SourceHash` with `index == 2`, `ContentHash`, `Seal` → `Refused` listing them (history, engine, source 1, witnesses are not accepted by `seal`); strict replay of the edited sources (else `Conf`); `reseal` with action `seal`, `diff` = `"overrides edited by hand: <old hash prefix> -> <new hash prefix>"`.
- `write_atomic`: temp file in the same directory (`.<name>.tmp.<pid>`), write, `sync_all`, then rename; with `replace == false` refuse if the target exists (`WouldOverwrite`) — the check and the rename are not one atomic step; document it. Never leave the temp file behind on error.
- `actor_from_env`: `USER` or `USERNAME`, `@`, `HOSTNAME` or `COMPUTERNAME`, each `unknown` if absent; control characters and whitespace replaced by `_`; at most 100 characters.

- [ ] **Step 1: Tests first** (unit tests with temp dirs; the ones that need `active()` to be `Ok(None)` and the real working directory go to `tests/frozen_active.rs`):
  1. freeze→load round trip: `freeze` with a `Release` policy in a temp dir → `write_atomic(.., false)` → `load_file` OK → **the differential test of Task 7**.
  2. `freeze_refuses_what_would_never_load`: a `physure.conf` with `Fe = abc, 0.1` under `[AtomicWeights]` → `Conf` naming the line; nothing written.
  3. `freeze_refuses_when_already_frozen_and_when_the_output_exists`.
  4. `freeze_with_binary_needs_the_engine_file_and_a_known_commit` (error text).
  5. `update_adopts_a_new_catalog_and_engine_and_records_it`: take a file frozen with a fake older embedded text and fake identity (built with `FrozenFile::new`) → `plan_update` → `Some`; its history has a second entry with action `update`, `engine` = `short()` of the running build, `diff` naming added/removed counts; overrides unchanged; **a file frozen from the current build returns `None`**; a tampered file is `Refused`.
  6. `seal_accepts_only_a_hand_edit_of_the_overrides`: edit overrides → `Some` with history `seal`; edit source 1 → `Refused`; edit the engine block → `Refused`; edit the history → `Refused`; a clean file → `None`; an overrides edit that does not replay → `Conf`.
  7. `write_atomic`: replace works, refuse-overwrite works, no temp file is left (also after a forced error: write into a directory that does not exist).
  8. `actor_from_env` sanitising and defaults; `now_unix` is after 2025-01-01.
  9. After `seal`/`update`/`freeze` the file **verifies** and every earlier entry is untouched (same `entry_sha256`).
- [ ] **Step 2: Implement**; keep functions small; no I/O in `plan_*` other than reading the one file.
- [ ] **Step 3:** green, clippy, commit `feat(core): freeze, update and seal as library operations that never write by themselves`.

---

### Task 7: Differential and independence tests

**Files:** Create `physure-core/tests/frozen_equivalence.rs` (serialised, global state like Task 5).

`describe(registry, constants) -> Vec<String>` (a function in a `tests/common/mod.rs`, also used by Task 2): the sorted lines `name => Debug of registry.facts(name)` for **every** name in `registry.names()` (where `facts` errors, the error text), plus sorted `constants` entries, sorted `constants_meta` entries (`value`, `description`, `latex_symbol`), the atomic-weight tables (sorted per edition), the selection, the definitions layer, categories (sorted), and the process-wide settings that can be read (`parallel_threshold()`, `max_loop_iterations()`, `max_pipeline_steps()`, `propagation_mode().name()`, `angle_mode().name()`).

- [ ] **Step 1: Tests:**
  1. `a_sealed_catalog_equals_the_unfrozen_one`: `freeze` in a temp dir with no `physure.conf` → write → `describe(frozen)` equals `describe(unfrozen)`; with a `physure.conf` of overrides (a new unit, a changed constant, an atomic-weight definition, `[Chemistry] atomic_weights = abridged`): frozen equals the unfrozen merge of embedded + that conf.
  2. `reformatting_the_overrides_changes_nothing`: the overrides with comments, indentation and CRLF added → `plan_seal` returns `None` (same canonical form) and the registry is equal.
  3. `the_library_can_change_and_the_file_does_not`: simulate an upgraded library by freezing, then loading the file while `DEFAULT_PHYSURE_CONF` is not consulted (Task 3 test 7 shows it; here assert through `with_conf_registry` that the value of a constant edited in the file wins over the library's).
  4. `update_after_a_simulated_upgrade_changes_exactly_what_the_diff_says`: file frozen with an older embedded text (the current text minus one constant plus one changed constant) → `plan_update` → load → the registry differs from the old one in exactly those two constants (compare `describe` outputs line by line: the set difference is those lines and nothing else).
- [ ] **Step 2:** all pass; no flakiness with `--test-threads` unspecified (the file serialises); commit `test(core): a sealed catalog equals the unfrozen one, reformatting changes nothing, an upgrade changes exactly what the update says`.

---

### Task 8: Gates, documentation, spec, roadmap

- [ ] **Step 1: Gates.** `cargo test -p physure --no-fail-fast` all green (totals); clippy nothing from `src/frozen/`, `src/units/conf.rs` additions, `tests/frozen_active.rs`, `tests/frozen_equivalence.rs`; `cargo llvm-cov -p physure --summary-only` → `frozen/{active,load,ops}.rs` ≥ 90 %; the first-use measurement: `import`-equivalent cost of the first `with_conf_registry` call unfrozen and frozen (report ms; the budget is ~0.5 s for first use); `cargo build -p physure` twice: no rebuild; `cargo check` of `physure-script`, `physure-cli`, `physure-lsp`, `physure-python`, `physure-wasm`, `physure-java` (nothing else may break).
- [ ] **Step 2: Spec** (CRLF; byte-check): §5.1 and §5.2 get the final rules of this plan (setters, cache, `ensure_valid`, fail closed, order of checks, `binary` file by the host, a relative `PHYSURE_CONF`, `PHYSURE_REQUIRE_FROZEN` values); §6 says which functions are hooked and that WebAssembly, Java and Python's own `configparser` read the embedded catalog directly and do not honour a frozen file until Phase 5 (Python's `UnitSystem` does, through `build_registry_with`); §7 says `binary` needs the host to name the file; §13: mark residual risks 1 and 2 closed with the tests that close them, add new ones: a stray `physure.frozen.conf` above a project changes everything below it (by design; `phs conf status` shows it; the unit tests have a guard); the cache stamp relies on file length and modification time (a same-size edit with a preserved timestamp is caught by the seal at load only when the file is re-read: the process-lifetime cache means a file edited **after** load in a running process is not re-verified until its stamp changes: owner Phase 2, document); the `freeze` check-then-rename race; process-wide settings and the unfrozen embedded catalog may already have been applied before a frozen file loads (defaults for keys the file does not set).
- [ ] **Step 3: CHANGELOG** one entry (CRLF; only what is true): resolution, sealed loading, fail closed, the three hooked functions, the library operations, the new history-chronology check; commands are Phase 2.
- [ ] **Step 4: structure.md** via the `update-structure` skill. **Step 5: Roadmaps** (`ROADMAP.md` and `docs/ROADMAP.md` together): Subsystem 6 Phase 1: 1a, 1b, 1c done; 1d (registry entry listing) remains; row 9 15 % → 25 %; the "next" sentence and timeline follow; strict mkdocs build with output outside the repo, delete `.cache/`.
- [ ] **Step 6: Commit** `docs: loading a frozen configuration — changelog, structure, roadmap, spec rules and residual risks`.

---

### Task 9: Corrections from the adversarial review of this plan (overrides Tasks 1–8 where they differ)

The review (opus, 2026-11-08; measured in a throwaway crate where marked **[M]**) found the flaws below. Each correction names the Task it amends. The principle behind all of them: *a frozen configuration in force must govern everything that can govern, must refuse loudly where it cannot yet, and must never leave a mixed state that looks governed.*

**A. Bindings and bypasses (amends Task 5 and the spec text of Task 8)**
1. **The claim "Python's `UnitSystem` creation goes through `build_registry_with`" is false and is removed.** Python builds with `configparser` over `default_conf()` (the embedded text) and then `extend_conf` on an empty registry (`startup.py`, `system.py`); only `physure.units` (`_CoreUnitRegistry.from_conf()`) and Rust-side parsing go through a hooked function. Java builds its own registry with `load_embedded_catalog`. WebAssembly the same. With a frozen file in force they would read the library's catalog: a silent mixed state.
2. **Unwired entry points refuse while a frozen file is in force** (Phase 5 wires them). Add to the core `frozen::refuse_unwired(binding: &str) -> Result<(), String>`: `Ok(())` when nothing is in force; otherwise an error "a frozen configuration is in force (<path>); the <binding> binding honours it from Phase 5: unset PHYSURE_CONF / PHYSURE_REQUIRE_FROZEN and move or remove the file, or use the CLI". Call it from: Python `default_conf()` and the module-init path that builds the first `UnitSystem` (raise an ordinary exception, not a panic); Java `default_catalog` / `new_handle`; `from_conf_text` of any binding with a non-empty `extra` (see 3). Verify by `cargo check` of `physure-python`, `physure-java`, `physure-wasm`, `physure-cli`, `physure-lsp`, `physure-script`, a unit test of `refuse_unwired` in the core, and a Python test file `physure-python/tests/test_frozen_refused.py` that is written **but reported as not run** unless you can build the extension without touching a shared virtualenv (do not run maturin or uv). State in the report exactly what was and was not executed.
3. **`build_registry_with(extra)` in the frozen arm refuses a non-empty `extra`**: `Err(vec!["a frozen configuration is in force: extra definitions belong in the overrides block of the frozen file (phs conf seal)"])`. `extra == ""` returns the frozen catalog.
4. **`physure-script/src/parser/quantities.rs` caches `build_registry_from_conf().0` in a `OnceLock` for the whole process** (read it): replace it with a per-use `with_conf_registry` / the fallible variant of A5, and add a `physure-script` test (in its own test file, serialised, global state restored) where a number followed by the sentinel unit `zz_unit` of a frozen file parses as a quantity and without the file it does not.
5. **Host-owned registries are outside the resolution by design** and the spec §6 says so exactly: `with_catalog` scopes (a host that scopes an unfrozen registry — Java's PHS evaluation does — is not governed), `UnitRegistry::build_default_si/imperial`, and the public `extend_registry`, `parse_physure_conf`, `load_embedded_catalog`, `DEFAULT_PHYSURE_CONF`. §6 lists what IS governed: `build_registry_from_conf`, `with_conf_registry`, `build_registry_with("")`, everything in the core, PHS and the CLI that goes through them, the `physure.units` registry of Python.

**B. Fail closed without panicking where it is dangerous (amends Task 5)**
6. Add fallible variants in `conf.rs`: `try_with_conf_registry<R>(f) -> Result<R, ActiveError>` and `try_build_registry_from_conf() -> Result<(UnitRegistry, HashMap<String,String>), ActiveError>`. **`Parser::parse_expression` (and every function of the core that already returns `PhysureResult` and obtains the catalog) uses the fallible variants** and maps `ActiveError` to `PhysureError::Generic(message)`, so `Quantity::new` returns an ordinary error. The chemistry module's `with_catalog_in_scope` becomes a fallible `try_with_catalog_in_scope` returning `PhysureResult<R>`; `atomic_weight` and `atomic_weight_edition` return `PhysureResult<…>` (they are not consumed by any binding yet). The two legacy infallible functions keep the panic **only as a backstop** with the same message. Panicking is wrong for the bindings (Java: a panic in an `extern "system"` function aborts the JVM; the language server: a handler panic exits and the client restarts it in a loop; Python: a `PanicException`, a `BaseException`).
7. `ensure_valid()` is needed at the entry points, and **this plan implements the calls that are in this repository's Rust crates**: `phs` main (`physure-cli/src/main.rs`, before dispatch: on error print the message to stderr and exit with code 1 for `Load(Integrity|Replay|NotFrozen|Io|TooLarge)`, 2 for `Resolve`, 3 for `Load(Engine)`), the language server `initialize` (publish one error diagnostic or `window/showMessage` and do not crash; read `physure-lsp/src/main.rs` first and mirror its existing error handling), and the Python module init through `refuse_unwired` (A2). Java init through `refuse_unwired`. WebAssembly needs none (C8).
8. Tests: with a broken frozen file `Quantity::new("m")` returns `Err` (not a panic) from the fallible path; the legacy functions still panic with the message (`catch_unwind`); `phs --version`-style invocations in `physure-cli/tests` run the real binary in a temp directory with a broken frozen file and assert the exit codes 1, 2 (`PHYSURE_REQUIRE_FROZEN=1` without a file) and 3 (engine), and the error on stderr.

**C. Environments without a usable current directory (amends Task 1 and Task 5)**
9. **WebAssembly:** `std::env::current_dir()` is unsupported on `wasm32-unknown-unknown`. On `cfg(all(target_family = "wasm", target_os = "unknown"))` resolution is `Unfrozen` without touching the file system or the environment; documented in §6. **Unreadable current directory elsewhere** (a deleted cwd on Unix): if there is no explicit path, no `PHYSURE_CONF` and no `PHYSURE_REQUIRE_FROZEN=1`, the result is `Unfrozen` (today's behaviour: `Path::new("physure.conf").is_file()` is simply false); it is an error only when something requires a frozen file. Tests for the unreadable-cwd path (a unit test of a `resolve_with_cwd(Result<PathBuf, io::Error>, …)` variant, since the real cwd cannot be broken portably).

**D. Cache, concurrency and cost (amends Task 5)**
10. **Never call `f` while holding the cache lock.** `gas_constant()` → `with_catalog_in_scope` → `with_conf_registry(f)` → inside `f`, `Quantity::new` → `parse_expression` → `with_conf_registry` again: holding the guard deadlocks the thread on itself (and the test would hang). Clone the `Arc<Active>` out, release the lock, call `f`. Test: `with_conf_registry(|_| Parser::parse_expression("m"))` under a frozen file returns, and `chemistry::gas_constant()` returns under a frozen file (with a timeout guard in the test so a deadlock fails instead of hanging: run it in a thread and `recv_timeout`).
11. **Key and cost.** The cache key is the resolution *inputs* (explicit path, `PHYSURE_CONF`, `PHYSURE_REQUIRE_FROZEN`, current directory). **Discovery runs once per key** (measured: an ancestor walk at depth 11 costs 123 µs per call, a ten-fold regression of the ~11.5 µs `with_conf_registry` of today, which is almost all one `stat`); per call only the stamp of the resolved file is checked (one `stat`, the one that today checks `physure.conf`). Measured budget: the added cost per call (mutex, two environment reads, `current_dir`) is about 0.66 µs; report your measurement of unfrozen and frozen per-call costs before and after, and do not exceed today's order of magnitude.
12. **Errors and setters.** `set_explicit_conf` and `set_engine_file` call `reset_cache()`. **Resolution errors are not cached** (cheap to retry; a file created later is seen); load errors are cached under the key + stamp of the file that was read. **The stamp comes from the opened handle** (`File::open` then `file.metadata()`), and the file is read with `Read::take(MAX + 1)` and refused if it hit the limit, instead of `metadata().len()` followed by a separate read (TOCTOU).
13. **Concurrency test:** N threads call `active()` for the first time at once; exactly one load happens (count loads with a test hook or by observing the cache), all get the same `Arc`.
14. **A same-size, same-mtime edit** lets nothing unverified through (the cache holds the verified content): it is staleness of an already loaded process, not a bypass. §13 words it so.

**E. Settings: replay never applies them (amends Tasks 2, 3, 5, 6)**
15. `replay_source` **never applies `[Settings]` or `[Compatibility]`**. Refactor `apply_setting` into `parse_setting(line) -> Result<Setting, String>` (an enum with the typed value, or `Ignored(key)` for the keys the core does not use) and `apply(&Setting)`; `Reading.apply_settings: bool` becomes a three-way `enum SettingsMode { Skip, Apply, Collect }` (lenient readers keep `Apply`, `extend_registry` keeps `Skip`, frozen replay uses `Collect`). `FrozenCatalog` carries `settings: Vec<Setting>` and `compatibility: Option<Vec<String>>` (the rows, already parsed and validated). Only `active()`, on **activation** (the moment it installs a loaded catalog in the cache), applies them, under the cache lock, in one step; the operations (`freeze`, `plan_update`, `plan_seal`) and `load_file` itself (so Phase 2's `status`/`verify`/`diff` too) never change process state. This also removes the race with the lib tests that serialise on `crate::settings::TEST_LOCK` (use that lock in every lib test that touches settings, not a private mutex).
16. **Frozen replay validates what the lenient reader ignores** (measured: all of these returned `Ok` from `extend_registry`: `propagation_mode = corelated`, `angle_mod = dimension`, `parallel_threshold = -5`, `[Compatibility] radian, plain = add`, a misspelt `[Constant]` section, a constant that is not a quantity): in frozen replay (a `Reading.strict_sections: bool`) a bad value or an unknown key in `[Settings]` is a problem (with the allowlist of keys the core does not use: `default_output`, `auto_simplify`, `default_system`, `readable_representation`, `verbose`, `mkml_recursion_limit`), a bad `[Compatibility]` row is a problem, an **unknown section name** is a problem (the near-miss rule of Phase 1b already covers case; this is for names that match nothing), a **line outside any section** is a problem (this also catches a UTF-8 BOM before the first header: `\u{FEFF}[Settings]`), and a `[Constants]` value that does not evaluate to a quantity is a problem. Tests for each.
17. **Each activation applies its complete set; resolution is sticky once a frozen file has been activated.** After a frozen catalog has been activated in a process, a later resolution that differs (another file, or none, because the current directory changed) is `ActiveError::ResolutionChanged { was, now }` until `reset_cache()` is called explicitly: one process, one configuration. While nothing has been activated, behaviour is today's. (This avoids frozen settings staying in force on an unfrozen catalog and the library's embedded settings overwriting the frozen ones when `embedded_catalog()` initialises late.) Test both directions with settings set to sentinel values before each build.
18. A setting that a later library adds and the frozen file does not mention takes its compiled default; **rule for future settings (spec §13): a new setting's compiled default must reproduce the previous behaviour.** Thread-local overrides (`settings::scoped`, `ModeGuard`, `with_seed`) and a host's own propagation mode are host choices, outside the freeze (§13). `ensure_valid()` at start-up is what activates (applies) the settings early.

**F. `seal` proves positively that only the overrides changed (amends Task 6; code in `seal.rs`)**
19. The allowed-problem rule is **unsound** (measured): editing source 1 with its marker hash recomputed gives `[ContentHash, Seal]`; appending a third source with a correct marker gives `[ContentHash, Seal]`; editing a `[Witness]` value gives `[Seal]`; the first two are inside "SourceHash 2, ContentHash, Seal" and would be resealed as "overrides edited by hand", the first also gaining trusted-reader powers. Replace the rule by a method `FrozenFile::only_the_overrides_changed(&self) -> bool` that is true exactly when: (a) `seal_sha256(self, <stored content_sha256>, <last entry>)` equals the stored seal (this proves format, `created`, engine, witnesses, scripts, history and the stored content hash are unchanged); (b) the content hash recomputed from every source's recomputed hash **except source 2, whose stored marker hash is used**, equals the stored content hash (this proves every other source, their count, order and origins, and the marker of source 2 are unchanged); (c) the recomputed hash of source 2 differs from its marker. `plan_seal` requires it; otherwise `Refused` listing the problems. Tests: edit source 1 with the marker recomputed → refused; append a source → refused; edit a `[Witness]` value → refused; edit a history field → refused; edit the engine block → refused; edit only the overrides → accepted; comments-only edit of the overrides → `None` (same canonical form).

**G. `freeze` and `update` (amends Task 6)**
20. `freeze` decides "is a frozen file already in force?" with `resolve(Inputs { explicit: None, conf_var: <real PHYSURE_CONF>, require_var: None, cwd: options.cwd })`, not with `active()`: with `PHYSURE_REQUIRE_FROZEN=1` and no file `active()` is `Err(Required)` and the very first file could never be made. A file found (or a variable naming one) → `AlreadyFrozen`.
21. When `build`/`binary` cannot be recorded because the commit is unknown, the error also says "use --engine-policy release or record" (the CLI flag of Phase 2) / "policy release or record".
22. The helper to compare a catalog with the embedded one is `conf::embedded_catalog_problems`-style, not `embedded_catalog_for_tests`; use `DEFAULT_PHYSURE_CONF` parsed with `parse_physure_conf` into fresh registries when a test needs the library's own catalog.

**H. Tests (amends Tasks 2, 5, 7)**
23. Task 2 test 4 cannot be built as written (`replay_source` is `pub(crate)`, `describe` lives in `tests/common`): in Task 2 compare with `UnitRegistry`'s `PartialEq` and the constants maps; `describe` stays in Task 7 (integration tests) and Task 6's round-trip test in the lib compares with `==` too.
24. Task 7's settings comparison is vacuous when both catalogs are built in the same process: set every readable setting to a sentinel value before each build and assert the frozen build set the file's values.
25. Task 5 test 8 (`binary`): always build the block with a synthetic known commit and the SHA-256 of a **small temporary file** named as the engine file (never hash the test executable: slow in debug); remove the conditional branch.
26. The guard test also checks `PHYSURE_CONF` and `PHYSURE_REQUIRE_FROZEN` are unset and says in its message that a `physure.frozen.conf` in the home directory or above the repository changes every test (Task 1's tests walk up from `temp_dir()`, which is under the user's profile: a frozen file there breaks them; make Task 1's tests create their trees under a unique directory and canonicalise it, and accept that an ancestor above `temp_dir()` is out of their control — the guard test is where that is reported).
27. Added tests: PHS end to end under a frozen file (A4); re-entrancy (D10); concurrent first load (D13); the refusal paths (A2, A3, tests of `refuse_unwired`); no-filesystem resolution (C9, compile-checked for `wasm32-unknown-unknown` with `cargo check -p physure --target wasm32-unknown-unknown` if the target is installed, otherwise say it was not); `seal` refusals (F19); strict-replay validation (E16); sticky resolution (E17).

**Spec / documentation (amends Task 8)**
28. §6 lists exactly what is governed and not governed (A5) and says the bindings refuse until Phase 5 (A2); §13 words the cache staleness as staleness (D14), adds the new-setting default rule (E18), host-owned registries and thread-local overrides (E18), the `current_dir` and WebAssembly rules (C9), sticky resolution (E17), and the exit-code mapping of `ensure_valid()` (B7). The CHANGELOG entry says that Python, Java and WebAssembly **refuse** (or, for WebAssembly, are not affected) rather than silently mixing, and that this is temporary until Phase 5. `structure.md` and the roadmaps as in Task 8 (row 9 to 25 %, Phase 1d and the binding wiring remain).

**Commits.** Keep the commit structure of Tasks 1–8 and add `fix(...)`/`feat(...)` commits per amendment group where it makes the history clearer; every message ends with the trailer. After implementation an independent review of the code follows (as for 1a and 1b).

## Self-review

- **Spec coverage (§5.1, §5.2, §6, §13 risks 1–2):** resolution and its order (Task 1), sealed loading with strict replay and trusted/untrusted sources and ignoring the working directory conf (Tasks 2–3, 5), fail closed (Task 5), engine check before replay and no override for evaluating (Task 3), hash cache (Task 3), the three operations (Task 6), the differential promise "an upgrade of the library cannot change a value" (Tasks 3 and 5 tests), chronology (Task 4).
- **Placeholders:** none; Tasks 3, 5, 6 give signatures and exhaustive test lists where the code is glue over existing, tested pieces.
- **Types:** `Inputs`/`Resolution`/`Why`/`ResolveError` (Task 1) feed `active()` (Task 5); `LoadOptions`/`FrozenCatalog`/`LoadError`/`load_file` (Task 3) feed `Active` (Task 5) and `ops` (Task 6); `replay_source` (Task 2) is used by Tasks 3 and 6; `describe` (Task 7) is also used by Task 2's test 4 and Task 6's test 1 — define it first in `tests/common/mod.rs` when implementing Task 2 and reuse.
