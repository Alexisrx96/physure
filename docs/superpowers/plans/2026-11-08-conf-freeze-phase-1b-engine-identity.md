# `phs conf` Phase 1b: the engine identity and its pinning policies — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `physure_core::frozen::engine`: a truthful identity of the running engine (version, commit, profile, target, features, and on request the SHA-256 of the executable), the four pinning policies of spec §7 (`binary`, `build`, `release`, `record`) with a precise mismatch report, the `[Engine]` block as a validated `Pairs`, and the `[Environment]` record. No registry, no file format change, no I/O except hashing an executable on request.

**Architecture:** A `build.rs` in `physure-core` records the profile, the target triple, the enabled cargo features and — only when it can say so truthfully — the commit. `engine.rs` turns these into an `EngineIdentity`, serialises it into the `[Engine]` pairs of a frozen file, reads it back strictly, and compares a recorded identity with the running one under a policy. Everything that can be wrong about an identity fails **toward refusing**: an unknown commit never equals anything, not even another unknown.

**Tech Stack:** Rust 2021, standard library only (`build.rs` calls `git` through `std::process::Command`, as `physure-cli/build.rs` already does).

**Follows** Phase 1a (`docs/superpowers/plans/2026-11-07-conf-freeze-phase-1a-file-model.md`, merged). **Not here:** resolution, sealed loading, the `freeze`/`update` operations over the real catalog (1c), the registry entry listing (1d), the CLI (Phase 2).

## Decisions fixed here

- **The commit is reported only when it is true of this very source tree.** Sources, in order: (1) the environment variable `PHYSURE_BUILD_COMMIT` at build time (what a release workflow sets; 1–64 ASCII alphanumerics, otherwise the build prints a `cargo:warning` and reports no commit); (2) `git rev-parse --short=12 HEAD`, **but only when** `git rev-parse --show-toplevel` is exactly the parent directory of this crate (`CARGO_MANIFEST_DIR/..`), so that a copy vendored inside somebody else's repository never reports *their* commit; (3) otherwise no commit, and the identity says `unknown`.
- **Uncommitted changes are not detected.** A build from a dirty working tree reports the commit of `HEAD`. This is stated in the documentation (Task 6), and it is why the policy `binary` exists: the hash of the executable is the only identity that does not depend on a claim about a source tree.
- **`unknown` never equals anything**, including another `unknown`. Under `build` a recorded or running unknown commit is a mismatch with the explanation "the commit is unknown, so this build cannot be told apart from another". `freezable` refuses to *record* the policies `build` and `binary` for an identity whose commit is unknown (spec §6); `release` and `record` are allowed.
- **`binary` needs a recorded `binary_sha256`.** A file with policy `binary` and no recorded hash is itself a mismatch (never silently "ok").
- **`features` is the sorted, comma-separated list of the cargo features enabled for `physure-core`**, from `CARGO_FEATURE_*`, lower-cased with `_` → `-` (it includes `default` when the default features are on). It is compared as text.
- **The `[Engine]` block is strict:** the keys `version`, `commit`, `profile`, `target`, `features`, `policy` are required; `binary_sha256` is optional; **any other key is an error** (an unknown key in the block that sets the policy would be an invisible knob).
- **`current_exe` is the executable of the process.** For the Python extension module the file to hash is the module, not `python.exe`; that is Phase 5, so the hashing function takes a path (`sha256_of_file`) and `binary_sha256()` is the convenience for `current_exe`.
- **`[Environment]` is informational and outside the seal** (Phase 1a decision); `environment_pairs()` records `os`, `arch`, `family`, `endian`, `pointer_width` and `cpus`. No behaviour may ever depend on it.

## File structure

| File | Responsibility |
|---|---|
| Create `physure-core/build.rs` | `PHYSURE_BUILD_PROFILE`, `PHYSURE_BUILD_TARGET`, `PHYSURE_BUILD_FEATURES`, and `PHYSURE_BUILD_COMMIT` when truthful |
| Create `physure-core/src/frozen/engine.rs` | `EngineIdentity`, `EnginePolicy`, `Difference`, `EngineMismatch`, `check`, `freezable`, `engine_pairs`, `read_engine`, `environment_pairs`, `sha256_of_file`, `binary_sha256` |
| Modify `physure-core/src/frozen/mod.rs` | `pub mod engine;` and re-exports |
| Modify | `CHANGELOG.md`, `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` (§6, §7), `ROADMAP.md`, `docs/ROADMAP.md`, `structure.md` (Task 6) |

All commands run from the worktree root. Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. New code must be clippy-clean (`cargo clippy -p physure --lib --tests -- -D warnings` has older errors elsewhere; only `src/frozen/` and `build.rs` count).

---

### Task 1: `build.rs`

**Files:** Create `physure-core/build.rs`.

- [ ] **Step 1: Write it** (there is nothing to unit-test in a build script; Task 2's tests read what it emits):

```rust
//! Build-time facts for `frozen::engine`: the profile, the target, the enabled features and, only when it can
//! be said truthfully, the commit this build was made from.

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PHYSURE_BUILD_COMMIT");
    // `git commit` on the checked-out branch changes `.git/logs/HEAD` but not `.git/HEAD`; watch both, as
    // `physure-cli/build.rs` does. (In a git worktree `.git` is a file and these paths do not exist, so cargo
    // reruns the script every time: slower, never stale.)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/logs/HEAD");

    println!("cargo:rustc-env=PHYSURE_BUILD_PROFILE={}", env::var("PROFILE").unwrap_or_else(|_| "unknown".into()));
    println!("cargo:rustc-env=PHYSURE_BUILD_TARGET={}", env::var("TARGET").unwrap_or_else(|_| "unknown".into()));

    let mut features: Vec<String> = env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(|f| f.to_lowercase().replace('_', "-")))
        .collect();
    features.sort();
    println!("cargo:rustc-env=PHYSURE_BUILD_FEATURES={}", features.join(","));

    if let Some(commit) = commit() {
        println!("cargo:rustc-env=PHYSURE_BUILD_COMMIT={commit}");
    }
}

/// The commit of this source tree, or `None` when that cannot be told truthfully.
fn commit() -> Option<String> {
    if let Ok(value) = env::var("PHYSURE_BUILD_COMMIT") {
        let value = value.trim().to_string();
        if !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Some(value);
        }
        println!("cargo:warning=PHYSURE_BUILD_COMMIT is not 1-64 ASCII alphanumerics; the build reports no commit");
        return None;
    }
    let manifest = env::var("CARGO_MANIFEST_DIR").ok()?;
    let top = git(&["rev-parse", "--show-toplevel"])?;
    // Only a crate that sits directly in the root of the repository it is built from may report that
    // repository's commit: a copy vendored inside somebody else's repository must not report theirs.
    let expected = Path::new(&manifest).parent()?.canonicalize().ok()?;
    if Path::new(&top).canonicalize().ok()? != expected {
        return None;
    }
    git(&["rev-parse", "--short=12", "HEAD"])
}

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}
```

- [ ] **Step 2: Build.** `cargo build -p physure` → success. `cargo clippy -p physure --lib -- -D warnings`: nothing from `build.rs`.
- [ ] **Step 3: Commit** `build(core): record the profile, target, features and (only when truthful) the commit of the engine`.

---

### Task 2: `EngineIdentity` and `EnginePolicy`

**Files:** Create `physure-core/src/frozen/engine.rs`; `mod.rs` gets `pub mod engine;`.

- [ ] **Step 1: Tests first** (`engine.rs` contains only this):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn identity(commit: &str) -> EngineIdentity {
        EngineIdentity {
            version: "0.3.1".into(),
            commit: commit.into(),
            profile: "release".into(),
            target: "x86_64-pc-windows-msvc".into(),
            features: "default,html-viewer".into(),
        }
    }

    #[test]
    fn the_running_identity_is_filled_in() {
        let now = EngineIdentity::current();
        assert_eq!(now.version, env!("CARGO_PKG_VERSION"));
        assert!(["debug", "release"].contains(&now.profile.as_str()), "{}", now.profile);
        assert!(now.target.contains('-'), "a target triple: {}", now.target);
        assert!(now.features.split(',').any(|f| f == "default"), "{}", now.features);
        assert!(!now.commit.is_empty());
        assert!(now.short().starts_with(&format!("{}+", now.version)));
    }

    #[test]
    fn the_short_form() {
        assert_eq!(identity("9f2c1ab4e0d1").short(), "0.3.1+9f2c1ab4e0d1");
        assert_eq!(identity(UNKNOWN).short(), "0.3.1+unknown");
    }

    #[test]
    fn policies_round_trip_and_refuse_anything_else() {
        for p in [EnginePolicy::Binary, EnginePolicy::Build, EnginePolicy::Release, EnginePolicy::Record] {
            assert_eq!(p.as_str().parse::<EnginePolicy>().unwrap(), p);
        }
        for bad in ["", "Build", "exact", "build ", "none"] {
            assert!(bad.parse::<EnginePolicy>().is_err(), "{bad:?}");
        }
        assert_eq!(EnginePolicy::default(), EnginePolicy::Build);
    }
}
```

   Run → FAIL to compile.

- [ ] **Step 2: Implement** (above the tests):

```rust
//! The identity of the running engine and the policies that pin it (spec §6, §7).
//!
//! An identity can be wrong in only one direction here: it can fail to prove that two builds are the same. It
//! never claims they are when it cannot tell, so every doubt is a mismatch.

use std::str::FromStr;

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = "unknown";

/// Which build of the engine this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    pub version: String,
    pub commit: String,
    pub profile: String,
    pub target: String,
    pub features: String,
}

impl EngineIdentity {
    /// The identity of the engine that is running.
    pub fn current() -> EngineIdentity {
        EngineIdentity {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: option_env!("PHYSURE_BUILD_COMMIT").unwrap_or(UNKNOWN).to_string(),
            profile: env!("PHYSURE_BUILD_PROFILE").to_string(),
            target: env!("PHYSURE_BUILD_TARGET").to_string(),
            features: env!("PHYSURE_BUILD_FEATURES").to_string(),
        }
    }

    /// `version+commit`, the form the history of a frozen file records.
    pub fn short(&self) -> String {
        format!("{}+{}", self.version, self.commit)
    }
}

/// How strictly a frozen file pins the engine (spec §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnginePolicy {
    /// The same executable (SHA-256).
    Binary,
    /// The same version, commit, profile, target and features.
    #[default]
    Build,
    /// The same version.
    Release,
    /// Nothing is enforced; the identity is only recorded.
    Record,
}

impl EnginePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            EnginePolicy::Binary => "binary",
            EnginePolicy::Build => "build",
            EnginePolicy::Release => "release",
            EnginePolicy::Record => "record",
        }
    }
}

impl FromStr for EnginePolicy {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "binary" => Ok(EnginePolicy::Binary),
            "build" => Ok(EnginePolicy::Build),
            "release" => Ok(EnginePolicy::Release),
            "record" => Ok(EnginePolicy::Record),
            other => Err(format!("unknown engine policy {other:?}: use binary, build, release or record")),
        }
    }
}
```

- [ ] **Step 3:** `mod.rs`: `pub mod engine;` and `pub use engine::{EngineIdentity, EnginePolicy};`. `cargo test -p physure --lib frozen::engine` → 3 pass. Clippy. Commit `feat(core): the identity of the running engine and the four pinning policies`.

---

### Task 3: Comparing an identity under a policy, and refusing to record what cannot be enforced

**Files:** Modify `physure-core/src/frozen/engine.rs`.

- [ ] **Step 1: Add the tests** to the `tests` module:

```rust
    fn run_check(
        recorded: &EngineIdentity,
        recorded_binary: Option<&str>,
        policy: EnginePolicy,
        running: &EngineIdentity,
        running_binary: Result<&str, &str>,
    ) -> Result<(), EngineMismatch> {
        check(recorded, recorded_binary, policy, running, || {
            running_binary.map(str::to_string).map_err(|e| std::io::Error::other(e.to_string()))
        })
    }

    fn fields(mismatch: &EngineMismatch) -> Vec<&str> {
        mismatch.differences.iter().map(|d| d.field).collect()
    }

    #[test]
    fn record_never_refuses() {
        let a = identity("aaa");
        let mut b = identity("bbb");
        b.version = "9.9.9".into();
        assert!(run_check(&a, None, EnginePolicy::Record, &b, Err("unreadable")).is_ok());
    }

    #[test]
    fn release_compares_only_the_version() {
        let a = identity("aaa");
        let mut b = identity("bbb");
        b.profile = "debug".into();
        b.target = "aarch64-apple-darwin".into();
        assert!(run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).is_ok());
        b.version = "0.3.2".into();
        let m = run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).unwrap_err();
        assert_eq!(fields(&m), ["version"]);
        assert_eq!(m.differences[0].recorded, "0.3.1");
        assert_eq!(m.differences[0].running, "0.3.2");
    }

    #[test]
    fn build_compares_version_commit_profile_target_and_features() {
        let a = identity("aaa111");
        assert!(run_check(&a, None, EnginePolicy::Build, &a.clone(), Ok("x")).is_ok());
        let cases: [(&str, fn(&mut EngineIdentity)); 5] = [
            ("version", |i| i.version = "0.3.2".into()),
            ("commit", |i| i.commit = "bbb222".into()),
            ("profile", |i| i.profile = "debug".into()),
            ("target", |i| i.target = "aarch64-apple-darwin".into()),
            ("features", |i| i.features = "default".into()),
        ];
        for (field, change) in cases {
            let mut b = a.clone();
            change(&mut b);
            let m = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err();
            assert_eq!(fields(&m), [field], "{field}");
        }
        let mut b = a.clone();
        b.version = "0.3.2".into();
        b.profile = "debug".into();
        assert_eq!(fields(&run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err()), ["version", "profile"]);
    }

    #[test]
    fn an_unknown_commit_never_equals_anything_not_even_itself() {
        let known = identity("aaa111");
        let unknown = identity(UNKNOWN);
        for (recorded, running) in [(&known, &unknown), (&unknown, &known), (&unknown, &unknown)] {
            let m = run_check(recorded, None, EnginePolicy::Build, running, Ok("x")).unwrap_err();
            assert_eq!(fields(&m), ["commit"]);
            assert!(m.to_string().contains("unknown"), "{m}");
        }
    }

    #[test]
    fn binary_compares_the_hash_and_needs_one_to_have_been_recorded() {
        let a = identity("aaa111");
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("ab12")).is_ok());
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("cd34")).unwrap_err();
        assert_eq!(fields(&m), ["binary_sha256"]);
        let m = run_check(&a, None, EnginePolicy::Binary, &a, Ok("ab12")).unwrap_err();
        assert!(m.to_string().contains("not recorded"), "{m}");
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Err("no such file")).unwrap_err();
        assert!(m.to_string().contains("no such file"), "{m}");
    }

    #[test]
    fn binary_does_not_look_at_the_other_fields_the_hash_decides() {
        let a = identity("aaa111");
        let mut b = identity("bbb222");
        b.version = "0.3.2".into();
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &b, Ok("ab12")).is_ok());
    }

    #[test]
    fn the_hash_is_only_read_when_the_policy_needs_it() {
        let a = identity("aaa111");
        for policy in [EnginePolicy::Record, EnginePolicy::Release, EnginePolicy::Build] {
            let result = check(&a, None, policy, &a, || panic!("the executable was hashed under {policy:?}"));
            assert!(result.is_ok());
        }
    }

    #[test]
    fn the_mismatch_says_what_was_recorded_and_what_is_running() {
        let a = identity("aaa111");
        let mut b = a.clone();
        b.commit = "bbb222".into();
        let text = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err().to_string();
        assert!(text.contains("build") && text.contains("commit") && text.contains("aaa111") && text.contains("bbb222"), "{text}");
    }

    #[test]
    fn freezing_needs_a_known_commit_for_build_and_binary_only() {
        let unknown = identity(UNKNOWN);
        assert!(freezable(&unknown, EnginePolicy::Build).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Binary).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Release).is_ok());
        assert!(freezable(&unknown, EnginePolicy::Record).is_ok());
        for p in [EnginePolicy::Binary, EnginePolicy::Build, EnginePolicy::Release, EnginePolicy::Record] {
            assert!(freezable(&identity("aaa111"), p).is_ok());
        }
    }
```

   Run → FAIL to compile.

- [ ] **Step 2: Implement** (below `EnginePolicy`'s `FromStr`, above the tests):

```rust
/// One field in which the recorded and the running engines differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub field: &'static str,
    pub recorded: String,
    pub running: String,
}

/// Why the running engine is not the one a frozen file pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineMismatch {
    pub policy: EnginePolicy,
    pub differences: Vec<Difference>,
}

impl std::fmt::Display for EngineMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "this is not the engine the configuration was validated with (policy {}):", self.policy.as_str())?;
        for d in &self.differences {
            write!(f, " {}: recorded {}, running {};", d.field, d.recorded, d.running)?;
        }
        Ok(())
    }
}

fn differ(field: &'static str, recorded: &str, running: &str) -> Difference {
    Difference { field, recorded: recorded.to_string(), running: running.to_string() }
}

/// Compares the engine a frozen file pins with the one that is running, under `policy`.
///
/// `recorded_binary` is the `binary_sha256` of the file, if it has one. `running_binary` computes the hash of the
/// running executable and is called **only** under the policy `binary`. Every doubt is a difference: an unknown
/// commit equals nothing, a missing recorded hash under `binary` is a difference, an unreadable executable is a
/// difference.
pub fn check(
    recorded: &EngineIdentity,
    recorded_binary: Option<&str>,
    policy: EnginePolicy,
    running: &EngineIdentity,
    running_binary: impl FnOnce() -> std::io::Result<String>,
) -> Result<(), EngineMismatch> {
    let mut differences = Vec::new();
    let mut same = |field: &'static str, a: &str, b: &str| {
        if a != b {
            differences.push(differ(field, a, b));
        }
    };
    match policy {
        EnginePolicy::Record => {}
        EnginePolicy::Release => same("version", &recorded.version, &running.version),
        EnginePolicy::Build => {
            same("version", &recorded.version, &running.version);
            if recorded.commit == UNKNOWN || running.commit == UNKNOWN || recorded.commit != running.commit {
                let note = |c: &str| if c == UNKNOWN { "unknown (this build cannot be told apart from another)".to_string() } else { c.to_string() };
                differences.push(differ("commit", &note(&recorded.commit), &note(&running.commit)));
            }
            same("profile", &recorded.profile, &running.profile);
            same("target", &recorded.target, &running.target);
            same("features", &recorded.features, &running.features);
        }
        EnginePolicy::Binary => match (recorded_binary, running_binary()) {
            (None, _) => differences.push(differ("binary_sha256", "(not recorded)", "(the policy binary requires it)")),
            (Some(a), Ok(b)) => same("binary_sha256", a, &b),
            (Some(a), Err(e)) => differences.push(differ("binary_sha256", a, &format!("(the executable could not be read: {e})"))),
        },
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(EngineMismatch { policy, differences })
    }
}

/// Whether a frozen file may be made with this identity under `policy`: `build` and `binary` name a source
/// revision, so they need a known commit. `release` and `record` do not.
pub fn freezable(identity: &EngineIdentity, policy: EnginePolicy) -> Result<(), String> {
    if matches!(policy, EnginePolicy::Build | EnginePolicy::Binary) && identity.commit == UNKNOWN {
        return Err(format!(
            "this build does not know its commit (build it from a git checkout, or set PHYSURE_BUILD_COMMIT), so the policy {} cannot be recorded; use release or record",
            policy.as_str()
        ));
    }
    Ok(())
}
```

   `same` borrows `differences` mutably while the `Build` arm also pushes to it: if the borrow checker objects, make `same` a plain function `fn same(differences: &mut Vec<Difference>, field, a, b)` and pass the vector; behaviour and tests unchanged.

- [ ] **Step 3:** `mod.rs` re-exports `Difference, EngineMismatch, check, freezable`. `cargo test -p physure --lib frozen::engine` → 12 pass. Clippy. Commit `feat(core): comparing the engine under a policy; every doubt is a difference`.

---

### Task 4: The `[Engine]` block and the `[Environment]` record

**Files:** Modify `physure-core/src/frozen/engine.rs`.

- [ ] **Step 1: Tests** (add to `tests`):

```rust
    #[test]
    fn the_engine_block_round_trips() {
        let a = identity("aaa111");
        let pairs = engine_pairs(&a, EnginePolicy::Binary, Some(&"a".repeat(64))).unwrap();
        assert_eq!(pairs.get("policy"), Some("binary"));
        assert_eq!(pairs.get("commit"), Some("aaa111"));
        let (back, policy, binary) = read_engine(&pairs).unwrap();
        assert_eq!((back, policy, binary), (a.clone(), EnginePolicy::Binary, Some("a".repeat(64))));
        let without = engine_pairs(&a, EnginePolicy::Build, None).unwrap();
        assert_eq!(without.get("binary_sha256"), None);
        assert_eq!(read_engine(&without).unwrap().2, None);
    }

    #[test]
    fn the_engine_block_is_read_strictly() {
        let a = identity("aaa111");
        let good = engine_pairs(&a, EnginePolicy::Build, None).unwrap();
        // a missing key
        for key in ["version", "commit", "profile", "target", "features", "policy"] {
            let kept: Vec<(String, String)> = good.iter().filter(|(k, _)| k != key).cloned().collect();
            let err = read_engine(&Pairs::from_vec(kept).unwrap()).unwrap_err().to_string();
            assert!(err.contains(key), "{key}: {err}");
        }
        // an unknown key is an invisible knob: refused
        let mut extra: Vec<(String, String)> = good.iter().cloned().collect();
        extra.push(("enforce".into(), "never".into()));
        assert!(read_engine(&Pairs::from_vec(extra).unwrap()).unwrap_err().to_string().contains("enforce"));
        // a policy that does not exist
        let mut bad: Vec<(String, String)> = good.iter().cloned().collect();
        bad.iter_mut().find(|(k, _)| k == "policy").unwrap().1 = "exact".into();
        assert!(read_engine(&Pairs::from_vec(bad).unwrap()).unwrap_err().to_string().contains("exact"));
        // a hash that is not a hash
        let mut hash: Vec<(String, String)> = good.iter().cloned().collect();
        hash.push(("binary_sha256".into(), "xyz".into()));
        assert!(read_engine(&Pairs::from_vec(hash).unwrap()).unwrap_err().to_string().contains("binary_sha256"));
    }

    #[test]
    fn an_identity_with_a_value_that_cannot_be_stored_is_refused() {
        let mut a = identity("aaa111");
        a.target = "two\nlines".into();
        assert!(engine_pairs(&a, EnginePolicy::Build, None).is_err());
    }

    #[test]
    fn the_environment_record_is_complete_and_valid() {
        let env = environment_pairs();
        for key in ["os", "arch", "family", "endian", "pointer_width", "cpus"] {
            assert!(env.get(key).is_some(), "{key}");
        }
        assert!(["little", "big"].contains(&env.get("endian").unwrap()));
        assert!(env.get("pointer_width").unwrap().parse::<u32>().is_ok());
        assert!(env.get("cpus").unwrap().parse::<usize>().unwrap() >= 1);
    }

    #[test]
    fn hashing_the_running_executable_agrees_with_hashing_its_bytes() {
        let path = std::env::current_exe().unwrap();
        let direct = crate::frozen::sha256::sha256_hex(&std::fs::read(&path).unwrap());
        assert_eq!(sha256_of_file(&path).unwrap(), direct);
        assert_eq!(binary_sha256().unwrap(), direct);
        assert!(crate::frozen::canonical::is_sha256_hex(&direct));
    }

    #[test]
    fn hashing_a_missing_file_is_an_error() {
        assert!(sha256_of_file(std::path::Path::new("definitely/not/here.bin")).is_err());
    }
```

- [ ] **Step 2: Implement** (add to `engine.rs`; `use` lines at the top of the file):

```rust
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::canonical::is_sha256_hex;
use super::model::{FrozenError, FrozenResult, Pairs};
use super::sha256::{to_hex, Sha256};

/// The `[Engine]` block of a frozen file for `identity` and `policy`, with the executable's hash if given.
pub fn engine_pairs(identity: &EngineIdentity, policy: EnginePolicy, binary_sha256: Option<&str>) -> FrozenResult<Pairs> {
    let mut items = vec![
        ("version".to_string(), identity.version.clone()),
        ("commit".to_string(), identity.commit.clone()),
        ("profile".to_string(), identity.profile.clone()),
        ("target".to_string(), identity.target.clone()),
        ("features".to_string(), identity.features.clone()),
        ("policy".to_string(), policy.as_str().to_string()),
    ];
    if let Some(hash) = binary_sha256 {
        items.push(("binary_sha256".to_string(), hash.to_string()));
    }
    Pairs::from_vec(items)
}

/// Reads an `[Engine]` block back. Strict: the six keys are required, `binary_sha256` is optional and must be a
/// SHA-256, and **any other key is an error**: an unknown key in the block that sets the policy would be a knob
/// nobody can see.
pub fn read_engine(pairs: &Pairs) -> FrozenResult<(EngineIdentity, EnginePolicy, Option<String>)> {
    const KNOWN: [&str; 7] = ["version", "commit", "profile", "target", "features", "policy", "binary_sha256"];
    if let Some((key, _)) = pairs.iter().find(|(k, _)| !KNOWN.contains(&k.as_str())) {
        return Err(FrozenError(format!("[Engine] has an unknown key {key}; the keys are {}", KNOWN.join(", "))));
    }
    let need = |key: &str| -> FrozenResult<String> {
        pairs.get(key).map(str::to_string).ok_or_else(|| FrozenError(format!("[Engine] has no {key}")))
    };
    let identity = EngineIdentity {
        version: need("version")?,
        commit: need("commit")?,
        profile: need("profile")?,
        target: need("target")?,
        features: need("features")?,
    };
    let policy: EnginePolicy = need("policy")?.parse().map_err(|e: String| FrozenError(format!("[Engine] {e}")))?;
    let binary = match pairs.get("binary_sha256") {
        None => None,
        Some(hash) if is_sha256_hex(hash) => Some(hash.to_string()),
        Some(hash) => return Err(FrozenError(format!("[Engine] binary_sha256 is not a SHA-256 in lowercase hexadecimal: {hash:?}"))),
    };
    Ok((identity, policy, binary))
}

/// What can be said about the machine, for the `[Environment]` block. Informational: outside the seal, and no
/// behaviour may ever depend on it.
pub fn environment_pairs() -> Pairs {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    Pairs::from_vec(vec![
        ("os".to_string(), std::env::consts::OS.to_string()),
        ("arch".to_string(), std::env::consts::ARCH.to_string()),
        ("family".to_string(), std::env::consts::FAMILY.to_string()),
        ("endian".to_string(), if cfg!(target_endian = "little") { "little" } else { "big" }.to_string()),
        ("pointer_width".to_string(), usize::BITS.to_string()),
        ("cpus".to_string(), cpus.to_string()),
    ])
    .expect("the environment record is valid by construction")
}

/// The SHA-256 of a file, read in blocks.
pub fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut block = [0u8; 65_536];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            break;
        }
        hasher.update(&block[..read]);
    }
    Ok(to_hex(&hasher.finish()))
}

/// The SHA-256 of the executable of this process. (For the Python extension module the file to hash is the
/// module, not the interpreter: use [`sha256_of_file`] there.)
pub fn binary_sha256() -> std::io::Result<String> {
    sha256_of_file(&std::env::current_exe()?)
}
```

   `Pairs` is defined in Phase 1a (`model.rs`); the tests call `good.iter()` expecting `(String, String)` tuples by reference — if `Pairs::iter` yields `&(String, String)` adapt the closures (`|(k, _)|` patterns on `&(String, String)` work with `|(k, _)|` through match ergonomics; if not, `|p| p.0 != key`). Do not change `Pairs`' public API to make a test compile; adapt the test and say so.

- [ ] **Step 3:** `mod.rs` re-exports `engine_pairs, read_engine, environment_pairs, sha256_of_file, binary_sha256`. `cargo test -p physure --lib frozen::engine` → 18 pass. Clippy. Commit `feat(core): the [Engine] and [Environment] blocks, strict on the way back, and the executable's hash`.

---

### Task 5: A frozen file carries its engine, end to end

**Files:** Create `physure-core/tests/frozen_engine.rs`.

An integration test through the public API only: build a frozen file whose `[Engine]` block comes from `engine_pairs`, render, parse, verify, read the engine back, and prove that editing the policy breaks the seal.

- [ ] **Step 1: Write the test.**

```rust
//! The engine block of a frozen file, through the public API only.

use physure_core::frozen::engine::{check, engine_pairs, environment_pairs, freezable, read_engine, EngineIdentity, EnginePolicy};
use physure_core::frozen::model::{FrozenFile, Pairs, Source};
use physure_core::frozen::seal::{EntryInput, FrozenParts};

fn file(policy: EnginePolicy) -> FrozenFile {
    let now = EngineIdentity::current();
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&now, policy, None).unwrap(),
            environment: environment_pairs(),
            witnesses: Pairs::default(),
            sources: vec![Source::new("embedded", "[Constants]\nx = 1 m").unwrap(), Source::new("overrides", "").unwrap()],
            witness_scripts: vec![],
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: &now.short(), actor: "ci@host", reason: "test", diff: "n/a" },
    )
    .unwrap()
}

#[test]
fn the_engine_of_a_file_comes_back_and_matches_the_running_one() {
    let text = file(EnginePolicy::Build).render().unwrap();
    let parsed = FrozenFile::parse(&text).unwrap();
    assert!(parsed.verify().is_empty());
    let (recorded, policy, binary) = read_engine(parsed.engine()).unwrap();
    assert_eq!(policy, EnginePolicy::Build);
    assert_eq!(recorded, EngineIdentity::current());
    // a build without a known commit cannot be pinned by `build`: the file only exists when freezable said yes
    if freezable(&recorded, policy).is_ok() {
        assert!(check(&recorded, binary.as_deref(), policy, &EngineIdentity::current(), || unreachable!()).is_ok());
    }
}

#[test]
fn weakening_the_policy_in_the_text_breaks_the_seal() {
    let text = file(EnginePolicy::Build).render().unwrap();
    let weakened = text.replace("policy = build", "policy = record");
    assert_ne!(weakened, text);
    assert!(!FrozenFile::parse(&weakened).unwrap().verify().is_empty());
}

#[test]
fn an_unknown_key_slipped_into_the_engine_block_is_refused_by_the_engine_reader() {
    let text = file(EnginePolicy::Build).render().unwrap();
    let with_knob = text.replace("[Engine]\n", "[Engine]\nenforce = never\n");
    let parsed = FrozenFile::parse(&with_knob).unwrap();
    assert!(!parsed.verify().is_empty(), "the seal covers the engine block");
    assert!(read_engine(parsed.engine()).is_err());
}

#[test]
fn the_policy_is_part_of_what_the_seal_protects_even_without_knowing_the_engine() {
    let a = file(EnginePolicy::Build);
    let b = file(EnginePolicy::Release);
    assert_ne!(a.seal_sha256(), b.seal_sha256());
}
```

   If the Phase 1a API names differ from these (`engine()`, `seal_sha256()`, `Pairs::default()`, `FrozenParts` fields), use the real ones from `physure_core::frozen` and say so; do not add API to make the test compile.

- [ ] **Step 2:** `cargo test -p physure --test frozen_engine` → 4 pass. Commit `test(core): a frozen file carries its engine, through the public API`.

---

### Task 6: Gates, documentation, spec, roadmap

- [ ] **Step 1: Gates.** `cargo test -p physure --no-fail-fast` all green (report totals); clippy nothing from `src/frozen/`, `build.rs` or `tests/frozen_engine.rs`; `cargo llvm-cov -p physure --summary-only` → `frozen/engine.rs` ≥ 90 %; also run `cargo build -p physure --release` once and `cargo test -p physure --release --lib frozen::engine::tests::the_running_identity_is_filled_in` to show `profile = release` is reported; and, from a copy of the crate outside any git repository (e.g. `git archive HEAD physure-core | tar -x -C <a temp dir outside the repo>`, then build there with `--manifest-path` and a `CARGO_TARGET_DIR` outside the repo, **and** the workspace `Cargo.toml` replaced by one that only names this crate if needed), show that the identity says `commit = unknown`; if building the copy standalone is impractical because of workspace inheritance, say so and instead run the same test with `PHYSURE_BUILD_COMMIT=` set to an invalid value (`bad commit`) and show the cargo warning and `unknown`. Report both observations.
- [ ] **Step 2: Spec.** In `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` (keep CRLF): §6 "Build metadata" bullet and §7 get the rules fixed above: the three sources of the commit in order, the vendored-copy rule, uncommitted changes not detected (so `binary` is the policy that does not depend on a claim about a source tree), `unknown` never equals anything, `build` and `binary` need a known commit to be recorded, `binary` needs a recorded hash, the `[Engine]` block is strict (unknown key = error), `[Environment]` keys. Do not change anything else in the spec.
- [ ] **Step 3: CHANGELOG.** One entry at the top of `[Unreleased]` → `### Added` (keep CRLF; state only what is true): the engine identity of a frozen configuration (`physure_core::frozen::engine`), its `build.rs`, the four policies, the strict `[Engine]` block, the environment record, the executable hash, the limit about uncommitted changes. Commands and loading are not here yet.
- [ ] **Step 4: structure.md** via the `update-structure` skill (it should add `physure-core/build.rs` and `frozen/engine.rs`).
- [ ] **Step 5: Roadmaps** (`ROADMAP.md` and `docs/ROADMAP.md`, together): Subsystem 6 Phase 1 line: 1a and 1b done; 1c (resolution, sealed loading, freeze/update operations) and 1d (registry entry listing) remain; dashboard row 9 percentage 10 % → 15 %; the "next" sentence and the timeline entry follow. Run `mkdocs build --strict -f physure-python/mkdocs.yml -d <outside the repo>` with `D:/Projects/physure/physure-python/.venv/Scripts/python.exe -m mkdocs`, then delete any `.cache/`.
- [ ] **Step 6: Commit** `docs: the engine identity — changelog, structure, roadmap and the spec rules`.

---

## Self-review

- **Spec coverage (§6, §7):** build metadata and the unknown-commit rule (Tasks 1, 3), the four policies and the exit-3 condition's logic (Task 3; the exit code itself is Phase 2), no override for evaluating (nothing here provides one), the `[Engine]` block with the policy inside the seal (Tasks 4, 5), the environment record (Task 4), the executable hash (Task 4).
- **Placeholders:** none.
- **Types:** `Pairs`, `FrozenParts`, `EntryInput`, `FrozenFile::{new, parse, render, verify, engine, seal_sha256}` are Phase 1a's; `EngineIdentity`, `EnginePolicy`, `check`, `freezable`, `engine_pairs`, `read_engine`, `environment_pairs`, `sha256_of_file`, `binary_sha256` are defined in Tasks 2–4 and used in Task 5 with the same signatures.
