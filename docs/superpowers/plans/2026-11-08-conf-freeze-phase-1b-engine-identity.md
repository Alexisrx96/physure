# `phs conf` Phase 1b: the engine identity and its pinning policies — Implementation Plan (revised after adversarial review)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `physure_core::frozen::engine`: a truthful identity of the running engine (version, commit, profile, target, features, compiler, build flags), the four pinning policies of spec §7 (`binary`, `build`, `release`, `record`) with a precise mismatch report, the `[Engine]` block as a strictly validated `Pairs`, the `[Environment]` record, and a history that records which engine block each entry sealed. No registry, no loading (1c).

**Architecture:** A `build.rs` in `physure-core` computes the build facts and **always** emits all of them under internal names (`PHYSURE_ENGINE_*`) that the code reads with `env!` — never `option_env!` on a name a user can set. The commit decision is a pure function in `build_support/commit.rs`, shared by `build.rs` (through `#[path]`) and unit-tested from the library. `engine.rs` turns the facts into an `EngineIdentity`, validates every field, serialises it into the `[Engine]` pairs and compares a recorded identity with the running one under a policy. Everything that can be doubtful fails **toward refusing**.

**Tech Stack:** Rust 2021, standard library only (`build.rs` calls `git` and `rustc` through `std::process::Command`).

**Follows** Phase 1a (merged). **Not here:** resolution, sealed loading, `freeze`/`update`/`seal` over the real catalog, the per-process cache of the executable's hash (1c); the registry entry listing (1d); the CLI (Phase 2).

## Why this plan was rewritten

An independent adversarial review of the first version of this plan (2026-11-08) found, among others, two **measured** critical flaws: a build whose `PHYSURE_BUILD_COMMIT` was rejected by `build.rs` still carried the raw rejected value into the binary (`option_env!` reads the environment rustc runs in), and the commit was not validated, so `main`, `Unknown`, `""` were "known" commits and two `""` compared equal under `build`. It also measured that, in a git worktree (the only way this repository is worked on), `rerun-if-changed` on `../.git/...` paths that do not exist makes cargo rebuild the crate and all its dependents on every invocation. Every decision below answers one of those findings.

## Decisions fixed here (do not re-litigate)

- **A commit is exactly 40 or 64 lowercase hexadecimal characters, or the literal `unknown`.** Nothing else, in `build.rs` and in `read_engine`. The full hash is recorded; `short()` (first 12 characters) is only for display and the history.
- **The commit rules** (pure function `commit_rules::decide`, tested): the environment variable `PHYSURE_BUILD_COMMIT` is an *input* of the build, never read by the library. (1) Set but not a full lowercase hash → `unknown` plus a `cargo:warning`. (2) Git can answer (the crate sits directly at the root of the repository, `git rev-parse HEAD` succeeds) and tracked files used by this build have uncommitted changes → `unknown` plus a warning ("build from a clean checkout"). (3) Git can answer, clean: the commit is `HEAD`; if the variable is also set and differs → `unknown` plus a warning. (4) Git cannot answer (crates.io tarball, vendored copy inside another repository, no git): a valid variable is used, otherwise `unknown`.
- **Trust boundary, stated in the docs:** whoever controls the build environment (the variable, `PATH`, `RUSTFLAGS`, the toolchain) controls every field of the identity. `build` protects against accidents, not against a malicious builder; `binary` is the only policy whose evidence is the artefact itself.
- **The build script always emits** `PHYSURE_ENGINE_COMMIT`, `PHYSURE_ENGINE_PROFILE`, `PHYSURE_ENGINE_TARGET`, `PHYSURE_ENGINE_RUSTC`, `PHYSURE_ENGINE_FLAGS`, and the library reads them with `env!`.
- **`features` is computed in code, not from `CARGO_FEATURE_*`,** as the sorted list of the *named* features of `physure-core` that are enabled, **without `default`** (`default` is a bundle, and the same code built with `default-features = false, features = ["html-viewer"]` must give the same identity as with the default bundle). A test keeps the list in step with `[features]` of `Cargo.toml`.
- **`rustc` and `flags` are part of the identity.** `rustc` is the one-line output of `$RUSTC -V`. `flags` is `opt-level=<n>;debug-assertions=<on|off>;panic=<unwind|abort>;rustflags=<sha256 of CARGO_ENCODED_RUSTFLAGS>;target-features=<sha256 of CARGO_CFG_TARGET_FEATURE>`. Compared as text under `build`. **Not visible to a build script, therefore not pinned, and documented as such:** `overflow-checks`, `lto`, `codegen-units`, the dependency versions when `physure` is built inside someone else's workspace (their `Cargo.lock` decides), dynamically linked libraries (libm, the C runtime, the interpreter, numpy, torch).
- **`unknown` never equals anything,** not even another `unknown`. `freezable` refuses to *record* `build` and `binary` for an identity whose commit is unknown (a binary hash alone does not tell an auditor which source it came from); `release` and `record` are allowed.
- **`binary` needs a recorded hash,** and `check` does not hash the executable when nothing is recorded. **`engine_pairs` enforces this at construction:** it calls `freezable` and refuses `Binary` without a hash.
- **The core has no `binary_sha256()` convenience.** `current_exe()` is the interpreter in Python and Java; hashing it would pin the interpreter and let a swapped extension module pass. Only `sha256_of_file(path)` exists; the caller must hash **the file that contains this engine**, and the doc of `check` says so.
- **The `[Engine]` block is strict:** required keys `version, commit, profile, target, features, rustc, flags, policy`; optional `binary_sha256`; any other key is an error (an unknown key in the block that sets the policy is an invisible knob). Every value is validated (forms below). `read_engine` returns a struct `EngineBlock`.
- **`verify` reads the engine block** (a sealed file whose block is unreadable is a discrepancy, `Problem::Engine`), and **each history entry records `engine_sha256`**, the SHA-256 of the canonical `[Engine]` pairs at the moment of that entry, inside the entry hash. A change of the engine block, including weakening the policy, therefore shows in the log as a different `engine_sha256` between consecutive entries, mechanically, whatever free-text `diff` the caller wrote. `verify` requires the last entry's `engine_sha256` to equal the hash of the current block.
- **`binary` attests the file at the path at check time,** not the image in memory (on Windows a running executable can be renamed and replaced).
- **Field forms (`EngineIdentity::validate`):** `version` non-empty, `[0-9A-Za-z.+-]`; `commit` per above; `profile` `[a-z0-9_-]+`; `target` `[A-Za-z0-9_.-]+`; `features` empty or sorted, unique, comma-separated `[a-z0-9-]+` names; `rustc` and `flags` one line, non-empty, no leading/trailing whitespace.
- **`[Environment]`** records `os`, `arch`, `family`, `endian`, `pointer_width`, `cpus`; informational, outside the seal; no behaviour may ever depend on it.

## File structure

| File | Responsibility |
|---|---|
| Create `physure-core/build_support/commit.rs` | `is_commit`, `UNKNOWN`, `Inputs`, `Decision`, `decide` + its unit tests |
| Create `physure-core/build.rs` | gathers the inputs, calls `decide`, emits all `PHYSURE_ENGINE_*`, the `rerun-if-changed` list |
| Create `physure-core/src/frozen/engine.rs` | identity, validation, policies, `check`, `freezable`, `engine_pairs`, `read_engine`, `EngineBlock`, `environment_pairs`, `sha256_of_file` |
| Modify `physure-core/src/frozen/{model,seal,mod}.rs` | history `engine_sha256`; `verify` reads the engine block; re-exports |
| Create `physure-core/tests/frozen_engine.rs` | end to end through the public API |
| Modify | `CHANGELOG.md`, `structure.md`, `ROADMAP.md`, `docs/ROADMAP.md`, the spec (§4, §5.3, §6, §7, §9, §13) |

All commands run from the worktree root. Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. New code must be clippy-clean (older errors elsewhere do not count; `src/frozen/`, `build.rs`, `build_support/` and the new tests do).

---

### Task 1: The commit decision (pure) and the build script

**Files:** Create `physure-core/build_support/commit.rs`, `physure-core/build.rs`; the test hook is added in Task 2 (`engine.rs`). Until then the unit tests of `commit.rs` run through a temporary `#[cfg(test)] #[path = "../../build_support/commit.rs"] mod commit_rules;` line in `physure-core/src/frozen/mod.rs` that Task 2 moves into `engine.rs`.

- [ ] **Step 1: Tests first.** Create `build_support/commit.rs` containing only this module, add the temporary hook line to `frozen/mod.rs`, run `cargo test -p physure --lib commit_rules` → FAIL to compile:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef01234567";
    const B: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn inputs<'a>(env: Option<&'a str>, git_head: Option<&'a str>, dirty: bool) -> Inputs<'a> {
        Inputs { env, git_head, dirty }
    }

    #[test]
    fn what_is_a_commit() {
        assert!(is_commit(A));
        assert!(is_commit(&"a".repeat(64)));
        for bad in ["", UNKNOWN, "Unknown", "UNKNOWN", " unknown", "main", "HEAD", "release", "0123456", &A.to_uppercase(),
                    &"a".repeat(39), &"a".repeat(41), &"a".repeat(63), &"g".repeat(40), "bad commit"] {
            assert!(!is_commit(bad), "{bad:?}");
        }
    }

    #[test]
    fn git_clean_and_no_variable_gives_head() {
        let d = decide(&inputs(None, Some(A), false));
        assert_eq!((d.commit.as_str(), d.warning), (A, None));
    }

    #[test]
    fn git_clean_and_a_variable_that_agrees_gives_head() {
        assert_eq!(decide(&inputs(Some(A), Some(A), false)).commit, A);
    }

    #[test]
    fn a_variable_that_disagrees_with_git_gives_unknown_and_a_warning() {
        let d = decide(&inputs(Some(B), Some(A), false));
        assert_eq!(d.commit, UNKNOWN);
        assert!(d.warning.unwrap().contains("disagrees"));
    }

    #[test]
    fn uncommitted_changes_give_unknown_even_when_the_variable_agrees() {
        for env in [None, Some(A)] {
            let d = decide(&inputs(env, Some(A), true));
            assert_eq!(d.commit, UNKNOWN);
            assert!(d.warning.unwrap().contains("uncommitted"));
        }
    }

    #[test]
    fn an_invalid_variable_gives_unknown_and_a_warning_even_when_git_could_answer() {
        for bad in ["bad commit", "main", "Unknown", "", &A.to_uppercase()] {
            for head in [None, Some(A)] {
                let d = decide(&inputs(Some(bad), head, false));
                assert_eq!(d.commit, UNKNOWN, "{bad:?}");
                assert!(d.warning.is_some(), "{bad:?}");
            }
        }
    }

    #[test]
    fn without_git_a_valid_variable_is_used_and_otherwise_unknown() {
        let d = decide(&inputs(Some(B), None, false));
        assert_eq!((d.commit.as_str(), d.warning), (B, None));
        let d = decide(&inputs(None, None, false));
        assert_eq!((d.commit.as_str(), d.warning), (UNKNOWN, None));
    }

    #[test]
    fn a_head_that_is_not_a_commit_is_not_an_answer() {
        assert_eq!(decide(&inputs(None, Some("garbage"), false)).commit, UNKNOWN);
        assert_eq!(decide(&inputs(Some(B), Some("garbage"), false)).commit, B);
    }
}
```

- [ ] **Step 2: Implement** (above the tests, same file):

```rust
//! The rule that decides which commit an engine build may claim. It is shared by `build.rs` (through
//! `#[path]`) and unit-tested from the library, so the rule that matters most for an audit has tests.
//!
//! A false commit is worse than none: every doubt gives `unknown`.

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = "unknown";

/// A commit identifier: exactly 40 or 64 lowercase hexadecimal characters (a SHA-1 or SHA-256 object name).
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Everything the decision depends on.
pub struct Inputs<'a> {
    /// `PHYSURE_BUILD_COMMIT` as set in the environment of the build, if it is set.
    pub env: Option<&'a str>,
    /// `git rev-parse HEAD`, when git answered and this crate sits directly at the root of that repository.
    pub git_head: Option<&'a str>,
    /// Whether tracked files this build uses have uncommitted changes (meaningful only with `git_head`).
    pub dirty: bool,
}

pub struct Decision {
    pub commit: String,
    pub warning: Option<String>,
}

fn unknown(why: &str) -> Decision {
    Decision { commit: UNKNOWN.to_string(), warning: Some(why.to_string()) }
}

pub fn decide(inputs: &Inputs<'_>) -> Decision {
    let env = match inputs.env {
        Some(value) if !is_commit(value) => {
            return unknown("PHYSURE_BUILD_COMMIT is not a full lowercase commit hash (40 or 64 hexadecimal characters); the build reports no commit")
        }
        other => other,
    };
    match inputs.git_head.filter(|head| is_commit(head)) {
        Some(head) => {
            if inputs.dirty {
                return unknown("the working tree has uncommitted changes to tracked files this build uses; the build reports no commit (build from a clean checkout)");
            }
            if env.is_some_and(|value| value != head) {
                return unknown("PHYSURE_BUILD_COMMIT disagrees with git HEAD; the build reports no commit");
            }
            Decision { commit: head.to_string(), warning: None }
        }
        None => Decision { commit: env.unwrap_or(UNKNOWN).to_string(), warning: None },
    }
}
```

- [ ] **Step 3: `build.rs`.**

```rust
//! Build-time facts for `frozen::engine`. Everything is emitted under an internal name, always, so that the
//! library never reads a name a user can set (`option_env!` would carry a rejected value into the binary).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[allow(dead_code)]
#[path = "src/frozen/sha256.rs"]
mod sha256;
#[allow(dead_code)]
#[path = "build_support/commit.rs"]
mod commit_rules;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    watch(&manifest);

    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=PHYSURE_ENGINE_PROFILE={profile}");
    println!("cargo:rustc-env=PHYSURE_ENGINE_TARGET={target}");
    println!("cargo:rustc-env=PHYSURE_ENGINE_RUSTC={}", rustc_version());
    println!("cargo:rustc-env=PHYSURE_ENGINE_FLAGS={}", flags());

    let env_commit = env::var("PHYSURE_BUILD_COMMIT").ok();
    let head = git_head(&manifest);
    let dirty = head.is_some() && git_dirty(&manifest);
    let decision = commit_rules::decide(&commit_rules::Inputs { env: env_commit.as_deref(), git_head: head.as_deref(), dirty });
    if let Some(warning) = &decision.warning {
        println!("cargo:warning={warning}");
    }
    println!("cargo:rustc-env=PHYSURE_ENGINE_COMMIT={}", decision.commit);
}

/// What can change the answer: only paths that exist are watched (a missing path makes cargo rebuild this
/// crate and everything that depends on it on every invocation, which is what happens in a git worktree if
/// `../.git/HEAD` is named).
fn watch(manifest: &Path) {
    println!("cargo:rerun-if-env-changed=PHYSURE_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    for local in ["build.rs", "build_support", "src", "Cargo.toml", "../Cargo.toml", "../Cargo.lock"] {
        if manifest.join(local).exists() {
            println!("cargo:rerun-if-changed={local}");
        }
    }
    let mut git_paths = vec!["HEAD".to_string(), "logs/HEAD".to_string(), "packed-refs".to_string()];
    if let Some(reference) = git(manifest, &["rev-parse", "--symbolic-full-name", "HEAD"]).filter(|r| r.starts_with("refs/")) {
        git_paths.push(reference);
    }
    for name in git_paths {
        if let Some(path) = git(manifest, &["rev-parse", "--git-path", &name]) {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() { path } else { manifest.join(path) };
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}

fn git(manifest: &Path, args: &[&str]) -> Option<String> {
    Command::new("git")
        .arg("-C")
        .arg(manifest)
        .args(args)
        // a hook or a wrapper may point git somewhere else
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// `HEAD`, only when this crate sits directly at the root of the repository it is built from: a copy vendored
/// inside somebody else's repository must not report theirs.
fn git_head(manifest: &Path) -> Option<String> {
    let top = PathBuf::from(git(manifest, &["rev-parse", "--show-toplevel"])?).canonicalize().ok()?;
    if top != manifest.parent()?.canonicalize().ok()? {
        return None;
    }
    git(manifest, &["rev-parse", "HEAD"])
}

/// Uncommitted changes to tracked files this crate is built from (the crate and the workspace manifests).
fn git_dirty(manifest: &Path) -> bool {
    let mut args = vec!["status", "--porcelain", "--untracked-files=no", "--", "."];
    for extra in ["../Cargo.toml", "../Cargo.lock"] {
        if manifest.join(extra).exists() {
            args.push(extra);
        }
    }
    // when git cannot say, assume the worst
    git(manifest, &args).is_some() || git(manifest, &["rev-parse", "--git-dir"]).is_none()
}

fn rustc_version() -> String {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.lines().next().unwrap_or("").trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn flags() -> String {
    let opt = env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".into());
    let assertions = if env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() { "on" } else { "off" };
    let panic = env::var("CARGO_CFG_PANIC").unwrap_or_else(|_| "unknown".into());
    let rustflags = sha256::sha256_hex(env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default().as_bytes());
    let features = sha256::sha256_hex(env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default().as_bytes());
    format!("opt-level={opt};debug-assertions={assertions};panic={panic};rustflags={rustflags};target-features={features}")
}
```

   Notes: `git_dirty` returns true when `git status --porcelain` printed anything, or when git cannot even name the git dir (assume the worst); `git()` filters empty output, so a clean tree gives `None` from the status call. `src/frozen/sha256.rs` has `#[cfg(test)]` tests that are not compiled in the build script; the `#[allow(dead_code)]` silences the unused functions. If `mod sha256` pulls in anything from the crate (it must not), fix `sha256.rs`, not the build script.

- [ ] **Step 4: Run.** `cargo test -p physure --lib commit_rules` → 8 pass. `cargo build -p physure` succeeds. **Measure the worktree claim:** run `cargo build -p physure` twice in a row and show that the second reports no recompilation of `physure` (`Finished` with no `Compiling physure`), and then `touch physure-core/src/lib.rs` is the only thing that triggers a rebuild. Show `cargo build -p physure -vv 2>&1 | grep -c "rerun-if-changed"`-style evidence of the emitted paths, and confirm none of them is a nonexistent path. Clippy: nothing from `build.rs` or `build_support/`.
- [ ] **Step 5: Commit** `build(core): the engine build facts, with a tested rule that never claims a commit it cannot prove`.

---

### Task 2: `EngineIdentity`, its validation and `EnginePolicy`

**Files:** Create `physure-core/src/frozen/engine.rs`; `mod.rs` gets `pub mod engine;` (move the test hook for `commit_rules` from `mod.rs` into `engine.rs` as `#[cfg(test)] #[path = "../../build_support/commit.rs"] mod commit_rules;`).

- [ ] **Step 1: Tests first** (`engine.rs` contains only this + the hook line):

```rust
#[cfg(test)]
#[path = "../../build_support/commit.rs"]
mod commit_rules;

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    fn identity(commit: &str) -> EngineIdentity {
        EngineIdentity {
            version: "0.3.1".into(),
            commit: commit.into(),
            profile: "release".into(),
            target: "x86_64-pc-windows-msvc".into(),
            features: "html-viewer".into(),
            rustc: "rustc 1.97.1 (8bab26f4f 2026-07-14)".into(),
            flags: "opt-level=3;debug-assertions=off;panic=unwind;rustflags=00;target-features=00".into(),
        }
    }

    #[test]
    fn the_unknown_constant_is_the_one_of_the_build_rule() {
        assert_eq!(UNKNOWN, commit_rules::UNKNOWN);
    }

    #[test]
    fn the_running_identity_is_valid_and_its_commit_is_head_when_the_tree_is_clean() {
        let now = EngineIdentity::current();
        now.validate().unwrap();
        assert_eq!(now.version, env!("CARGO_PKG_VERSION"));
        assert!(["debug", "release"].contains(&now.profile.as_str()), "{}", now.profile);
        assert!(now.target.contains('-'), "a target triple: {}", now.target);
        assert!(now.rustc.starts_with("rustc "), "{}", now.rustc);
        assert!(now.flags.starts_with("opt-level="), "{}", now.flags);
        // the commit must be what git says at test time, or `unknown` if the tree is dirty or git cannot say
        let git = |args: &[&str]| {
            std::process::Command::new("git").arg("-C").arg(env!("CARGO_MANIFEST_DIR")).args(args).output().ok()
                .filter(|o| o.status.success()).and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_string())
        };
        let top = git(&["rev-parse", "--show-toplevel"]).and_then(|t| std::path::PathBuf::from(t).canonicalize().ok());
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(|p| p.canonicalize().ok());
        if top.is_some() && top == parent && std::env::var_os("PHYSURE_BUILD_COMMIT").is_none() {
            let dirty = git(&["status", "--porcelain", "--untracked-files=no", "--", ".", "../Cargo.toml", "../Cargo.lock"]).is_some();
            let head = git(&["rev-parse", "HEAD"]).unwrap();
            assert_eq!(now.commit, if dirty { UNKNOWN.to_string() } else { head }, "stale or false commit");
        } else {
            assert!(now.commit == UNKNOWN || commit_rules::is_commit(&now.commit), "{}", now.commit);
        }
    }

    #[test]
    fn features_are_the_named_ones_without_default_and_match_cargo_toml() {
        let manifest = include_str!("../../Cargo.toml");
        let section = manifest.split("\n[features]").nth(1).expect("[features]").split("\n[").next().unwrap();
        let mut declared: Vec<&str> = section
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
            .filter(|k| !k.is_empty() && !k.starts_with('#') && *k != "default")
            .collect();
        declared.sort_unstable();
        let mut known: Vec<&str> = FEATURES.iter().map(|(name, _)| *name).collect();
        known.sort_unstable();
        assert_eq!(known, declared, "a feature was added to Cargo.toml: add it to FEATURES in engine.rs");
        assert!(!EngineIdentity::current().features.split(',').any(|f| f == "default"));
    }

    #[test]
    fn the_short_form_uses_twelve_characters_of_the_commit() {
        assert_eq!(identity(HEAD).short(), "0.3.1+0123456789ab");
        assert_eq!(identity(UNKNOWN).short(), "0.3.1+unknown");
    }

    #[test]
    fn validation_refuses_what_could_make_two_builds_look_alike() {
        identity(HEAD).validate().unwrap();
        identity(UNKNOWN).validate().unwrap();
        let bad_commits = ["", "Unknown", "UNKNOWN", " unknown", "main", "HEAD", "release", "0123456", "ABCDEF0123456789ABCDEF0123456789ABCDEF01"];
        for commit in bad_commits {
            assert!(identity(commit).validate().is_err(), "commit {commit:?}");
        }
        let edits: [(&str, fn(&mut EngineIdentity)); 12] = [
            ("empty version", |i| i.version = String::new()),
            ("version with space", |i| i.version = "0.3 1".into()),
            ("profile upper case", |i| i.profile = "Release".into()),
            ("empty target", |i| i.target = String::new()),
            ("target with space", |i| i.target = "a b".into()),
            ("features unsorted", |i| i.features = "b,a".into()),
            ("features duplicated", |i| i.features = "a,a".into()),
            ("features with empty name", |i| i.features = "a,,b".into()),
            ("features upper case", |i| i.features = "Html".into()),
            ("two lines in rustc", |i| i.rustc = "a\nb".into()),
            ("padded flags", |i| i.flags = " x".into()),
            ("empty rustc", |i| i.rustc = String::new()),
        ];
        for (what, edit) in edits {
            let mut i = identity(HEAD);
            edit(&mut i);
            assert!(i.validate().is_err(), "{what}");
        }
        let mut empty_features = identity(HEAD);
        empty_features.features = String::new();
        empty_features.validate().unwrap();
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

- [ ] **Step 2: Implement** (above the tests):

```rust
//! The identity of the running engine and the policies that pin it (spec §6, §7).
//!
//! An identity can be wrong in only one direction here: it can fail to prove that two builds are the same. It
//! never claims they are when it cannot tell, so every doubt is a mismatch. **Trust boundary:** whoever controls
//! the build environment (the variable `PHYSURE_BUILD_COMMIT`, `PATH`, `RUSTFLAGS`, the toolchain) controls every
//! field; `build` guards against accidents, not against a malicious builder. Not visible to a build script and
//! therefore not pinned: `overflow-checks`, `lto`, `codegen-units`, the versions of dependencies when `physure`
//! is built inside someone else's workspace, and every dynamically linked library.

use std::str::FromStr;

use super::model::{FrozenError, FrozenResult};

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = "unknown";

/// The named features of this crate and whether each is enabled. `default` is a bundle and is not listed. Keep in
/// step with `[features]` of `Cargo.toml` (a test checks it).
const FEATURES: [(&str, bool); 1] = [("html-viewer", cfg!(feature = "html-viewer"))];

/// Which build of the engine this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    pub version: String,
    /// 40 or 64 lowercase hexadecimal characters, or `unknown`.
    pub commit: String,
    pub profile: String,
    pub target: String,
    /// Sorted, comma-separated named features, without `default`.
    pub features: String,
    /// The first line of `rustc -V` of the compiler that built it.
    pub rustc: String,
    /// `opt-level=…;debug-assertions=…;panic=…;rustflags=<sha256>;target-features=<sha256>`.
    pub flags: String,
}

fn is_word(text: &str, extra: &[char]) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

fn one_line(what: &str, text: &str) -> FrozenResult<()> {
    if text.is_empty() || text != text.trim() || text.contains('\n') || text.contains('\r') {
        return Err(FrozenError(format!("the engine's {what} must be one non-empty line without padding: {text:?}")));
    }
    Ok(())
}

impl EngineIdentity {
    /// The identity of the engine that is running. All values come from the build script, which always sets them.
    pub fn current() -> EngineIdentity {
        let features: Vec<&str> = FEATURES.iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
        EngineIdentity {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: env!("PHYSURE_ENGINE_COMMIT").to_string(),
            profile: env!("PHYSURE_ENGINE_PROFILE").to_string(),
            target: env!("PHYSURE_ENGINE_TARGET").to_string(),
            features: features.join(","),
            rustc: env!("PHYSURE_ENGINE_RUSTC").to_string(),
            flags: env!("PHYSURE_ENGINE_FLAGS").to_string(),
        }
    }

    /// Refuses a value that could make two different builds look alike or that cannot be stored on one line.
    pub fn validate(&self) -> FrozenResult<()> {
        let bad = |what: &str, value: &str| -> FrozenResult<()> { Err(FrozenError(format!("invalid engine {what}: {value:?}"))) };
        if !is_word(&self.version, &['.', '+', '-']) {
            return bad("version", &self.version);
        }
        if self.commit != UNKNOWN && !(matches!(self.commit.len(), 40 | 64) && self.commit.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))) {
            return bad("commit (40 or 64 lowercase hexadecimal characters, or unknown)", &self.commit);
        }
        if !self.profile.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') || self.profile.is_empty() {
            return bad("profile", &self.profile);
        }
        if !is_word(&self.target, &['_', '.', '-']) {
            return bad("target", &self.target);
        }
        if !self.features.is_empty() {
            let names: Vec<&str> = self.features.split(',').collect();
            let well_formed = names.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            let sorted_unique = names.windows(2).all(|w| w[0] < w[1]);
            if !well_formed || !sorted_unique {
                return bad("features (sorted, unique, lowercase names)", &self.features);
            }
        }
        one_line("rustc", &self.rustc)?;
        one_line("flags", &self.flags)
    }

    /// `version+commit` (twelve characters of the commit, or `unknown`), the form the history records.
    pub fn short(&self) -> String {
        let commit = if self.commit == UNKNOWN { UNKNOWN } else { &self.commit[..12.min(self.commit.len())] };
        format!("{}+{}", self.version, commit)
    }
}

/// How strictly a frozen file pins the engine (spec §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnginePolicy {
    /// The same executable file (SHA-256) — the only policy whose evidence is the artefact itself.
    Binary,
    /// The same version, commit, profile, target, features, compiler and build flags.
    #[default]
    Build,
    /// The same version string. (Development builds between releases keep the last release's number, so this
    /// accepts unreleased code.)
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

- [ ] **Step 3:** `mod.rs`: `pub mod engine;` and `pub use engine::{EngineIdentity, EnginePolicy};`. `cargo test -p physure --lib frozen::engine` and `commit_rules` → green. Clippy. Commit `feat(core): the engine identity with validated fields, its features from the code and the four policies`.

---

### Task 3: Comparing under a policy; refusing to record what cannot be enforced

**Files:** Modify `engine.rs`.

- [ ] **Step 1: Tests** (add to `tests`; they replace the first plan's matrix and add the review's cases):

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
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.version = "9.9.9".into();
        assert!(run_check(&a, None, EnginePolicy::Record, &b, Err("unreadable")).is_ok());
    }

    #[test]
    fn release_compares_only_the_version() {
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.profile = "debug".into();
        b.target = "aarch64-apple-darwin".into();
        b.rustc = "rustc 1.0.0".into();
        assert!(run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).is_ok());
        b.version = "0.3.2".into();
        let m = run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).unwrap_err();
        assert_eq!(fields(&m), ["version"]);
        assert_eq!((m.differences[0].recorded.as_str(), m.differences[0].running.as_str()), ("0.3.1", "0.3.2"));
    }

    #[test]
    fn build_compares_every_field_one_at_a_time() {
        let a = identity(HEAD);
        assert!(run_check(&a, None, EnginePolicy::Build, &a.clone(), Ok("x")).is_ok());
        let cases: [(&str, fn(&mut EngineIdentity)); 7] = [
            ("version", |i| i.version = "0.3.2".into()),
            ("commit", |i| i.commit = "89abcdef0123456789abcdef0123456789abcdef".into()),
            ("profile", |i| i.profile = "debug".into()),
            ("target", |i| i.target = "aarch64-apple-darwin".into()),
            ("features", |i| i.features = String::new()),
            ("rustc", |i| i.rustc = "rustc 1.98.0".into()),
            ("flags", |i| i.flags = "opt-level=0;debug-assertions=on;panic=unwind;rustflags=00;target-features=00".into()),
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
        let known = identity(HEAD);
        let unknown = identity(UNKNOWN);
        for (recorded, running) in [(&known, &unknown), (&unknown, &known), (&unknown, &unknown)] {
            let m = run_check(recorded, None, EnginePolicy::Build, running, Ok("x")).unwrap_err();
            assert_eq!(fields(&m), ["commit"]);
            assert!(m.to_string().contains("unknown"), "{m}");
        }
    }

    #[test]
    fn a_hash_recorded_under_build_is_ignored() {
        let a = identity(HEAD);
        let mut b = a.clone();
        b.profile = "debug".into();
        let m = run_check(&a, Some("ab12"), EnginePolicy::Build, &b, Ok("ab12")).unwrap_err();
        assert_eq!(fields(&m), ["profile"]);
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Build, &a, Ok("cd34")).is_ok(), "informational under build");
    }

    #[test]
    fn binary_compares_the_hash_and_needs_one_to_have_been_recorded() {
        let a = identity(HEAD);
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("ab12")).is_ok());
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("cd34")).unwrap_err();
        assert_eq!(fields(&m), ["binary_sha256"]);
        let m = run_check(&a, None, EnginePolicy::Binary, &a, Ok("ab12")).unwrap_err();
        assert!(m.to_string().contains("not recorded"), "{m}");
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Err("no such file")).unwrap_err();
        assert!(m.to_string().contains("no such file"), "{m}");
    }

    #[test]
    fn binary_does_not_look_at_the_other_fields_and_hashes_only_when_something_is_recorded() {
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.version = "0.3.2".into();
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &b, Ok("ab12")).is_ok());
        let result = check(&a, None, EnginePolicy::Binary, &a, || panic!("hashed although nothing was recorded"));
        assert!(result.is_err());
    }

    #[test]
    fn the_hash_is_only_read_under_binary() {
        let a = identity(HEAD);
        for policy in [EnginePolicy::Record, EnginePolicy::Release, EnginePolicy::Build] {
            assert!(check(&a, Some("ab12"), policy, &a, || panic!("the executable was hashed under {policy:?}")).is_ok());
        }
    }

    #[test]
    fn the_mismatch_says_what_was_recorded_and_what_is_running() {
        let a = identity(HEAD);
        let mut b = a.clone();
        b.commit = "89abcdef0123456789abcdef0123456789abcdef".into();
        let text = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err().to_string();
        assert!(text.contains("build") && text.contains("commit") && text.contains(HEAD) && text.contains("89abcdef"), "{text}");
    }

    #[test]
    fn freezing_needs_a_known_commit_for_build_and_binary_only() {
        let unknown = identity(UNKNOWN);
        assert!(freezable(&unknown, EnginePolicy::Build).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Binary).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Release).is_ok());
        assert!(freezable(&unknown, EnginePolicy::Record).is_ok());
        for p in [EnginePolicy::Binary, EnginePolicy::Build, EnginePolicy::Release, EnginePolicy::Record] {
            assert!(freezable(&identity(HEAD), p).is_ok());
        }
    }
```

- [ ] **Step 2: Implement** (below `FromStr`, above the tests). Write `same` as a **free function** from the start (a closure borrowing `differences` mutably fails the borrow checker, measured):

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

fn same(differences: &mut Vec<Difference>, field: &'static str, recorded: &str, running: &str) {
    if recorded != running {
        differences.push(Difference { field, recorded: recorded.to_string(), running: running.to_string() });
    }
}

/// Compares the engine a frozen file pins with the one that is running, under `policy`.
///
/// `recorded_binary` is the `binary_sha256` of the file, if it has one. `running_binary` must compute the hash of
/// **the file that contains this engine** (for a Python extension module that is the module, not the
/// interpreter) and is called **only** under the policy `binary` and only when a hash was recorded. Every doubt is
/// a difference: an unknown commit equals nothing, a missing recorded hash under `binary` is a difference, an
/// unreadable file is a difference. Under `build` a recorded hash is informational and ignored. `binary` attests
/// the file at its path at check time, not the image in memory.
pub fn check(
    recorded: &EngineIdentity,
    recorded_binary: Option<&str>,
    policy: EnginePolicy,
    running: &EngineIdentity,
    running_binary: impl FnOnce() -> std::io::Result<String>,
) -> Result<(), EngineMismatch> {
    let mut differences = Vec::new();
    match policy {
        EnginePolicy::Record => {}
        EnginePolicy::Release => same(&mut differences, "version", &recorded.version, &running.version),
        EnginePolicy::Build => {
            same(&mut differences, "version", &recorded.version, &running.version);
            if recorded.commit == UNKNOWN || running.commit == UNKNOWN || recorded.commit != running.commit {
                let shown = |c: &str| if c == UNKNOWN { "unknown (this build cannot be told apart from another)".to_string() } else { c.to_string() };
                differences.push(Difference { field: "commit", recorded: shown(&recorded.commit), running: shown(&running.commit) });
            }
            same(&mut differences, "profile", &recorded.profile, &running.profile);
            same(&mut differences, "target", &recorded.target, &running.target);
            same(&mut differences, "features", &recorded.features, &running.features);
            same(&mut differences, "rustc", &recorded.rustc, &running.rustc);
            same(&mut differences, "flags", &recorded.flags, &running.flags);
        }
        EnginePolicy::Binary => match recorded_binary {
            None => differences.push(Difference {
                field: "binary_sha256",
                recorded: "(not recorded)".to_string(),
                running: "(the policy binary requires it)".to_string(),
            }),
            Some(recorded_hash) => match running_binary() {
                Ok(running_hash) => same(&mut differences, "binary_sha256", recorded_hash, &running_hash),
                Err(e) => differences.push(Difference {
                    field: "binary_sha256",
                    recorded: recorded_hash.to_string(),
                    running: format!("(the file could not be read: {e})"),
                }),
            },
        },
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(EngineMismatch { policy, differences })
    }
}

/// Whether a frozen file may be made with this identity under `policy`: `build` and `binary` name a source
/// revision (a binary hash alone does not tell an auditor which source it came from), so they need a known
/// commit. `release` and `record` do not.
pub fn freezable(identity: &EngineIdentity, policy: EnginePolicy) -> Result<(), String> {
    if matches!(policy, EnginePolicy::Build | EnginePolicy::Binary) && identity.commit == UNKNOWN {
        return Err(format!(
            "this build does not know its commit (build it from a clean git checkout, or set PHYSURE_BUILD_COMMIT to the full hash when git is not available), so the policy {} cannot be recorded; use release or record",
            policy.as_str()
        ));
    }
    Ok(())
}
```

- [ ] **Step 3:** `mod.rs` re-exports `Difference, EngineMismatch, check, freezable`. `cargo test -p physure --lib frozen::engine` green. Clippy. Commit `feat(core): comparing the engine under a policy, every doubt a difference`.

---

### Task 4: The `[Engine]` and `[Environment]` blocks, and hashing a file

**Files:** Modify `engine.rs`.

- [ ] **Step 1: Tests** (add; note `Pairs::iter()` yields `(&str, &str)` by value, so rebuild lists through `as_slice()`):

```rust
    use crate::frozen::model::Pairs;

    #[test]
    fn the_engine_block_round_trips() {
        let a = identity(HEAD);
        let hash = "a".repeat(64);
        let pairs = engine_pairs(&a, EnginePolicy::Binary, Some(&hash)).unwrap();
        assert_eq!((pairs.get("policy"), pairs.get("commit")), (Some("binary"), Some(HEAD)));
        let block = read_engine(&pairs).unwrap();
        assert_eq!((block.identity, block.policy, block.binary_sha256), (a.clone(), EnginePolicy::Binary, Some(hash)));
        let without = engine_pairs(&a, EnginePolicy::Build, None).unwrap();
        assert_eq!(without.get("binary_sha256"), None);
        assert_eq!(read_engine(&without).unwrap().binary_sha256, None);
    }

    #[test]
    fn building_a_block_that_could_never_pass_is_refused() {
        let a = identity(HEAD);
        assert!(engine_pairs(&a, EnginePolicy::Binary, None).unwrap_err().to_string().contains("binary_sha256"));
        assert!(engine_pairs(&a, EnginePolicy::Binary, Some("not a hash")).is_err());
        assert!(engine_pairs(&identity(UNKNOWN), EnginePolicy::Build, None).unwrap_err().to_string().contains("commit"));
        assert!(engine_pairs(&identity(UNKNOWN), EnginePolicy::Release, None).is_ok());
        let mut bad = identity(HEAD);
        bad.target = "two\nlines".into();
        assert!(engine_pairs(&bad, EnginePolicy::Build, None).is_err());
        bad.target = "x86_64-pc-windows-msvc".into();
        bad.commit = "Unknown".into();
        assert!(engine_pairs(&bad, EnginePolicy::Release, None).is_err());
    }

    fn rebuilt(pairs: &Pairs, edit: impl Fn(Vec<(String, String)>) -> Vec<(String, String)>) -> Pairs {
        Pairs::from_vec(edit(pairs.as_slice().to_vec())).unwrap()
    }

    #[test]
    fn the_engine_block_is_read_strictly() {
        let good = engine_pairs(&identity(HEAD), EnginePolicy::Build, None).unwrap();
        for key in ["version", "commit", "profile", "target", "features", "rustc", "flags", "policy"] {
            let missing = rebuilt(&good, |v| v.into_iter().filter(|(k, _)| k != key).collect());
            let err = read_engine(&missing).unwrap_err().to_string();
            assert!(err.contains(key), "{key}: {err}");
        }
        // an unknown key is an invisible knob
        let extra = rebuilt(&good, |mut v| { v.push(("enforce".into(), "never".into())); v });
        assert!(read_engine(&extra).unwrap_err().to_string().contains("enforce"));
        // a policy that does not exist
        let bad_policy = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "policy" { (k, "exact".into()) } else { (k, x) }).collect());
        assert!(read_engine(&bad_policy).unwrap_err().to_string().contains("exact"));
        // a hash that is not a hash
        let bad_hash = rebuilt(&good, |mut v| { v.push(("binary_sha256".into(), "xyz".into())); v });
        assert!(read_engine(&bad_hash).unwrap_err().to_string().contains("binary_sha256"));
        // every spelling of an unknown commit that is not exactly `unknown` is refused
        for spelling in ["", "Unknown", "UNKNOWN", "main"] {
            let block = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "commit" { (k, spelling.into()) } else { (k, x) }).collect());
            assert!(read_engine(&block).is_err(), "commit {spelling:?}");
        }
        // unsorted or duplicated features are refused
        for features in ["b,a", "a,a"] {
            let block = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "features" { (k, features.into()) } else { (k, x) }).collect());
            assert!(read_engine(&block).is_err(), "features {features:?}");
        }
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
    fn hashing_a_file_agrees_with_hashing_its_bytes() {
        let path = std::env::current_exe().unwrap();
        let direct = crate::frozen::sha256::sha256_hex(&std::fs::read(&path).unwrap());
        assert_eq!(sha256_of_file(&path).unwrap(), direct);
        assert!(crate::frozen::canonical::is_sha256_hex(&direct));
    }

    #[test]
    fn hashing_a_missing_file_is_an_error() {
        assert!(sha256_of_file(std::path::Path::new("definitely/not/here.bin")).is_err());
    }
```

- [ ] **Step 2: Implement** (add to `engine.rs`; the `use` lines go with the others at the top):

```rust
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::canonical::is_sha256_hex;
use super::model::Pairs;
use super::sha256::{to_hex, Sha256};

/// A decoded `[Engine]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineBlock {
    pub identity: EngineIdentity,
    pub policy: EnginePolicy,
    /// The SHA-256 of the file that contains the engine, if recorded. Required by `binary`, informational under
    /// `build`.
    pub binary_sha256: Option<String>,
}

/// The `[Engine]` block for `identity` and `policy`, with the hash of the engine's file if given. Refuses an
/// invalid identity, a policy that cannot be recorded for it (`freezable`) and `binary` without a hash.
pub fn engine_pairs(identity: &EngineIdentity, policy: EnginePolicy, binary_sha256: Option<&str>) -> FrozenResult<Pairs> {
    identity.validate()?;
    freezable(identity, policy).map_err(FrozenError)?;
    match (policy, binary_sha256) {
        (EnginePolicy::Binary, None) => return Err(FrozenError("the policy binary needs the binary_sha256 of the engine's file".into())),
        (_, Some(hash)) if !is_sha256_hex(hash) => {
            return Err(FrozenError(format!("binary_sha256 is not a SHA-256 in lowercase hexadecimal: {hash:?}")))
        }
        _ => {}
    }
    let mut items = vec![
        ("version".to_string(), identity.version.clone()),
        ("commit".to_string(), identity.commit.clone()),
        ("profile".to_string(), identity.profile.clone()),
        ("target".to_string(), identity.target.clone()),
        ("features".to_string(), identity.features.clone()),
        ("rustc".to_string(), identity.rustc.clone()),
        ("flags".to_string(), identity.flags.clone()),
        ("policy".to_string(), policy.as_str().to_string()),
    ];
    if let Some(hash) = binary_sha256 {
        items.push(("binary_sha256".to_string(), hash.to_string()));
    }
    Pairs::from_vec(items)
}

/// Reads an `[Engine]` block back. Strict: the eight keys are required, `binary_sha256` is optional and must be a
/// SHA-256, every value is validated, and **any other key is an error**: an unknown key in the block that sets the
/// policy would be a knob nobody can see.
pub fn read_engine(pairs: &Pairs) -> FrozenResult<EngineBlock> {
    const KNOWN: [&str; 9] = ["version", "commit", "profile", "target", "features", "rustc", "flags", "policy", "binary_sha256"];
    if let Some((key, _)) = pairs.iter().find(|(k, _)| !KNOWN.contains(k)) {
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
        rustc: need("rustc")?,
        flags: need("flags")?,
    };
    identity.validate().map_err(|e| FrozenError(format!("[Engine] {e}")))?;
    let policy: EnginePolicy = need("policy")?.parse().map_err(|e: String| FrozenError(format!("[Engine] {e}")))?;
    let binary_sha256 = match pairs.get("binary_sha256") {
        None => None,
        Some(hash) if is_sha256_hex(hash) => Some(hash.to_string()),
        Some(hash) => return Err(FrozenError(format!("[Engine] binary_sha256 is not a SHA-256 in lowercase hexadecimal: {hash:?}"))),
    };
    Ok(EngineBlock { identity, policy, binary_sha256 })
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

/// The SHA-256 of a file, read in blocks. Hash **the file that contains the engine**: for the CLI that is
/// `std::env::current_exe()`, for a Python extension module it is the module (not the interpreter), for the Java
/// library the library. The core deliberately has no "hash my own executable" convenience, because in a binding
/// that would pin the interpreter and let a swapped module pass.
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
```

   If `Pairs::iter()`'s item type differs from `(&str, &str)` in the real Phase 1a code, adapt the closure patterns; do not change `Pairs`' public API.

- [ ] **Step 3:** `mod.rs` re-exports `EngineBlock, engine_pairs, read_engine, environment_pairs, sha256_of_file`. `cargo test -p physure --lib frozen::engine` green. Clippy. Commit `feat(core): the [Engine] block, strict and validated, the environment record and hashing a file`.

---

### Task 5: The history records the engine block; `verify` reads it

**Files:** Modify `physure-core/src/frozen/model.rs`, `seal.rs`, `mod.rs`; fixtures of the Phase 1a tests.

This closes the review's I8: before, weakening the policy through `set_engine` + `reseal` left no trace in the log, and a sealed file with an unreadable engine block verified. Phase 1a's format is unreleased, so the entry gains a field.

- [ ] **Step 1: Tests first** (in `seal.rs` tests; they fail on the current code):
  1. `a_new_file_records_the_engine_block_in_its_first_entry`: `history()[0].engine_sha256() == sha256_hex(canonical_pairs(engine).as_bytes())`.
  2. `changing_the_engine_and_resealing_shows_in_the_log`: build with a `build` block, `set_engine` with the same identity and policy `record` (use `engine_pairs`), `reseal(seal, reason "weaken")` with a free-text `diff` of `"n/a"`; the two entries have **different** `engine_sha256`; `verify()` is empty.
  3. `resealing_without_changing_the_engine_keeps_the_hash`: edit only the `overrides` source; consecutive entries have equal `engine_sha256`.
  4. `the_last_entry_must_record_the_current_engine_block`: take a file, replace its engine pairs through a crate-private write **without** resealing; `verify()` contains `Problem::HistoryEngine` (and the seal problem). Forge the other way: change the last entry's `engine_sha256` text; `verify()` reports the entry hash.
  5. `an_unreadable_engine_block_is_a_problem`: a file built with the pairs `[("version","0.3.1"),("policy","build")]` (as the Phase 1a fixtures use) → `verify()` contains `Problem::Engine` whose message names the missing key.
  6. `the_render_parse_round_trip_keeps_engine_sha256_and_the_independent_seal_check_is_updated`: the fixed expected values of `the_seal_matches_an_independent_computation` change because the entry hash now includes the new field: **re-derive them with the Python script** (`C:/Users/Irvin/AppData/Local/Temp/claude/94e2112c21e15c2d/5b76dbee-ec58-421d-bf30-ff88a0b413eb/scratchpad/seal_check/seal.py`, update it for the new field, in the scratchpad, not in the repo) and put the new fixed values in the test.
- [ ] **Step 2: Implement.** `HistoryEntry` gets `engine_sha256` (after `engine` in `HISTORY_FIELDS`, in the rendered `[History.N]`, in the parser's required fields with the `is_sha256_hex` check, in `entry_sha256`'s text as `engine_sha256=…` right after `engine=…`); `append_entry` fills it from `canonical_pairs(&self.engine)`; `verify` gains `Problem::HistoryEngine { expected, found }` (the last entry's `engine_sha256` against the current block) and `Problem::Engine { message }` (when `read_engine(self.engine())` fails). The Phase 1a fixtures that used toy engine blocks switch to a helper that builds a valid block with `engine_pairs(&EngineIdentity{…fixed fields…}, EnginePolicy::Release, None)`; keep every other assertion of those tests.
- [ ] **Step 3:** whole suite green (`cargo test -p physure --no-fail-fast`), the exhaustive one-byte-edit test of Phase 1a still passes. Clippy. Commit `fix(core): each history entry records the engine block it sealed, and verify reads the block`.

---

### Task 6: End to end through the public API

**Files:** Create `physure-core/tests/frozen_engine.rs`.

- [ ] **Step 1: Write it** (no conditional skips: every branch runs everywhere):

```rust
//! The engine block of a frozen file, through the public API only.

use physure_core::frozen::engine::{check, engine_pairs, environment_pairs, read_engine, EngineIdentity, EnginePolicy, UNKNOWN};
use physure_core::frozen::model::{FrozenFile, Pairs, Source};
use physure_core::frozen::seal::{EntryInput, FrozenParts};

const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

fn known() -> EngineIdentity {
    let mut i = EngineIdentity::current();
    i.commit = HEAD.into();
    i
}

fn file(identity: &EngineIdentity, policy: EnginePolicy) -> FrozenFile {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(identity, policy, None).unwrap(),
            environment: environment_pairs(),
            witnesses: Pairs::default(),
            sources: vec![Source::new("embedded", "[Constants]\nx = 1 m").unwrap(), Source::new("overrides", "").unwrap()],
            witness_scripts: vec![],
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: &identity.short(), actor: "ci@host", reason: "test", diff: "n/a" },
    )
    .unwrap()
}

#[test]
fn a_file_frozen_with_a_known_engine_checks_against_it_and_against_nothing_else() {
    let id = known();
    let parsed = FrozenFile::parse(&file(&id, EnginePolicy::Build).render().unwrap()).unwrap();
    assert!(parsed.verify().is_empty());
    let block = read_engine(parsed.engine()).unwrap();
    assert_eq!(block.identity, id);
    assert!(check(&block.identity, None, block.policy, &id, || unreachable!()).is_ok());
    let mut other = id.clone();
    other.rustc = "rustc 0.0.0".into();
    assert!(check(&block.identity, None, block.policy, &other, || unreachable!()).is_err());
}

#[test]
fn the_running_engine_can_always_be_recorded_under_release_and_record() {
    let now = EngineIdentity::current();
    for policy in [EnginePolicy::Release, EnginePolicy::Record] {
        let parsed = FrozenFile::parse(&file(&now, policy).render().unwrap()).unwrap();
        assert!(parsed.verify().is_empty());
    }
    // build and binary are recorded only if the running build knows its commit
    let build = engine_pairs(&now, EnginePolicy::Build, None);
    assert_eq!(build.is_ok(), now.commit != UNKNOWN);
}

#[test]
fn weakening_the_policy_in_the_text_breaks_the_seal() {
    let text = file(&known(), EnginePolicy::Build).render().unwrap();
    let weakened = text.replace("policy = build", "policy = record");
    assert_ne!(weakened, text);
    assert!(!FrozenFile::parse(&weakened).unwrap().verify().is_empty());
}

#[test]
fn an_unknown_key_slipped_into_the_engine_block_is_refused_by_the_seal_and_by_the_reader() {
    let text = file(&known(), EnginePolicy::Build).render().unwrap();
    let with_knob = text.replace("[Engine]\n", "[Engine]\nenforce = never\n");
    let parsed = FrozenFile::parse(&with_knob).unwrap();
    assert!(!parsed.verify().is_empty());
    assert!(read_engine(parsed.engine()).is_err());
}

#[test]
fn the_policy_is_part_of_what_the_seal_protects_and_of_what_the_log_records() {
    let a = file(&known(), EnginePolicy::Build);
    let b = file(&known(), EnginePolicy::Release);
    assert_ne!(a.seal_sha256(), b.seal_sha256());
    assert_ne!(a.history()[0].engine_sha256(), b.history()[0].engine_sha256());
}
```

   Use the real accessor names of Phase 1a/Task 5 (`engine()`, `seal_sha256()`, `history()`, `engine_sha256()`); do not add API to make the test compile.

- [ ] **Step 2:** `cargo test -p physure --test frozen_engine` → 5 pass. Commit `test(core): a frozen file carries its engine, through the public API`.

---

### Task 7: Gates, documentation, spec, roadmap

- [ ] **Step 1: Gates.** `cargo test -p physure --no-fail-fast` all green (totals); clippy nothing from `src/frozen/`, `build.rs`, `build_support/`, `tests/frozen_engine.rs`; `cargo llvm-cov -p physure --summary-only` → `frozen/*.rs` ≥ 90 %. **Measured demonstrations, with output in the report:** (a) `PHYSURE_BUILD_COMMIT="bad commit" cargo build -p physure` prints the cargo warning and a following `cargo test -p physure --lib the_running_identity` reports commit `unknown` (use a throwaway `CARGO_TARGET_DIR` outside the repo so the normal target is not disturbed); (b) a release build reports `profile = release` (`cargo test -p physure --release --lib frozen::engine::tests::the_running_identity_is_valid_and_its_commit_is_head_when_the_tree_is_clean`); (c) two consecutive `cargo build -p physure` in the worktree do not recompile `physure` the second time; (d) a clean checkout reports the full 40-character `HEAD`, and after `touch`-ing nothing but editing a tracked file (then reverting it) the commit is `unknown` and back.
- [ ] **Step 2: Spec** (`docs/superpowers/specs/2026-11-07-conf-freeze-design.md`, keep CRLF): §4's example block (full 40-character commit, `features = html-viewer`, `rustc`, `flags`, no `rayon_threads`, `cpus`; the history entry gains `engine_sha256`); §5.3's entry list gains `engine_sha256`; §6 "Build metadata" gets the commit rules of this plan and the clean-checkout rule; §7 gets: the table's `build` row now includes compiler and build flags, the trust boundary, what `build`/`binary`/`release` do **not** prove (overflow-checks, lto, codegen-units, consumer lockfile, dynamic libraries, a dirty tree only detected for tracked files of this crate, development builds sharing a version number), `binary` attests the file at the path at check time, `binary_sha256` under `build` is informational, `binary` and `build` need a known commit to be recorded **by design choice** (a hash alone does not name a source); §9 adds the same limits for the audit documentation; §13 gets the residual risks of the table in the next step.
- [ ] **Step 3: §13 residual risks, each with an owner** (add these bullets):
  | Risk | Owner |
  |---|---|
  | Hashing a 30–100 MB executable or module with the in-house SHA-256 on every load would break the ~0.5 s first-use budget: cache per process | Phase 1c |
  | Which file to hash per binding (CLI `current_exe`, Python module, Java library) and that the core has no convenience for it | Phase 5 |
  | A LSP build without `html-viewer` has a different `features` string and will refuse a file frozen with `build` by another binding: document or choose `release` | Phase 2 / 5 |
  | Unpinned build inputs listed in §7 | documentation, Phase 2 |
  | `release` accepts development builds that share a release's version number | Phase 2 documentation, or a development-version rule |
  | `physure-cli/build.rs` names `../.git/...` paths that do not exist in a worktree, so the CLI and everything depending on it recompile on every invocation | separate small fix (use `git rev-parse --git-path`, as `physure-core/build.rs` does now) |
  | A copy of the crate placed at `<their repo>/physure-core` reports their commit (it does identify that tree, but an auditor will not find it in physure's history) | documentation |
- [ ] **Step 4: CHANGELOG** one entry at the top of `[Unreleased]` → `### Added` (keep CRLF; only what is true): the engine identity (`physure_core::frozen::engine`) and its `build.rs`, the commit rule with its four cases, the validated fields, the four policies, the strict `[Engine]` block, the environment record, hashing a file, the history recording `engine_sha256` and `verify` reading the engine block, the limits (what is not pinned). Say that commands and loading are not here yet.
- [ ] **Step 5: structure.md** via the `update-structure` skill (`physure-core/build.rs`, `build_support/commit.rs`, `frozen/engine.rs`).
- [ ] **Step 6: Roadmaps** (`ROADMAP.md` and `docs/ROADMAP.md`, together, each keeping its link style and CRLF): Subsystem 6 Phase 1 line: 1a and 1b done; 1c (resolution, sealed loading, freeze/update/seal operations, hash cache) and 1d (registry entry listing) remain; dashboard row 9: 10 % → 15 %; the "next" sentence and the timeline entry follow. Then `D:/Projects/physure/physure-python/.venv/Scripts/python.exe -m mkdocs build --strict -f physure-python/mkdocs.yml -d <a directory outside the repo>` and delete any `.cache/`.
- [ ] **Step 7: Commit** `docs: the engine identity — changelog, structure, roadmap, spec rules and residual risks`.

---

## Self-review

- **Spec coverage (§6, §7):** build metadata and the unknown-commit rule (Tasks 1–2), the four policies (Task 3), the policy inside the seal and a trace of its changes in the log (Task 5), no override for evaluating (nothing here provides one), environment record (Task 4), file hashing (Task 4).
- **Review findings:** C1 (Task 1: always emit, internal names, `env!`; demonstration (a)), C2 (`is_commit`, `validate`, `read_engine`; spelling tests), I1 (existing paths only via `git rev-parse --git-path`; demonstration (c)), I2 (dirty → `unknown`), I3 (env must agree with git), I4 (`rustc` + `flags`; unpinned inputs documented), I5/I6 (features from code, consumer lockfile documented), I7 (`engine_pairs` validates, `freezable`, `Binary` needs a hash), I8 (Task 5), I9/I10 (pure rule with tests; the identity test compares with git; no conditional skips), compile fixes (free function `same`, `KNOWN.contains(k)`, `as_slice()`), `binary_sha256()` dropped, M1 (match on `recorded_binary` first), M4 (`EngineBlock`), spec/docs wording.
- **Placeholders:** none.
- **Types:** `EngineIdentity` (7 fields), `EnginePolicy`, `Difference`, `EngineMismatch`, `check`, `freezable`, `EngineBlock`, `engine_pairs`, `read_engine`, `environment_pairs`, `sha256_of_file` are defined in Tasks 2–4 and used with those signatures in Tasks 5–6; `Pairs`, `FrozenParts`, `EntryInput`, `FrozenFile` are Phase 1a's.
