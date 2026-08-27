# `phs upgrade` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Design spec:** `docs/superpowers/specs/2026-08-26-phs-upgrade-design.md` — read it first; this plan implements it exactly.

**Goal:** Add `phs upgrade` (downloads the latest published `core-v*` release binary) and `phs upgrade --nightly` (rebuilds from `main`'s tip via `cargo install`, but only when a relevant crate actually changed since the running build), plus the `phs --version`/`-V` flag both paths need to compare against.

**Architecture:** All new logic lives in one new file, `physure-cli/src/upgrade.rs`, built up in layers: pure/testable helpers first (version parsing, asset-name selection, the relevant-path filter, the Windows rename trick), then the network/filesystem orchestration that calls them. A new `build.rs` captures the running build's commit SHA at compile time for `--version` and the nightly relevant-commit check. `main.rs` gains two small additions: an early `--version`/`-V` check and an `args[1] == "upgrade"` dispatch, matching the crate's existing manual-dispatch style (`new-plugin`, `export`, `debug`).

**Tech Stack:** Rust (`physure-cli` crate only). New dependencies: `ureq` (HTTP), `semver` (version comparison). Archive extraction shells out to the system `tar` binary — no new crate for that.

---

## Task 1: Dependencies and the commit-SHA `build.rs`

**Files:**
- Modify: `physure-cli/Cargo.toml`
- Create: `physure-cli/build.rs`

- [ ] **Step 1: Add the two new dependencies**

In `physure-cli/Cargo.toml`, in the `[dependencies]` block, add after `chrono = "0.4"`:

```toml
ureq           = "2"
semver         = "1"
```

- [ ] **Step 2: Create `build.rs`**

Create `physure-cli/build.rs`:

```rust
use std::process::Command;

/// Captures the short commit SHA this binary was built from, exposed to the crate as the
/// `PHS_BUILD_SHA` env var (read back via `option_env!("PHS_BUILD_SHA")`). `phs --version`
/// shows it, and `phs upgrade --nightly` uses it as the baseline for "did anything relevant
/// change since this build." Silently produces no env var (not a build failure) when `git`
/// isn't available or this isn't a git checkout at all -- the crates.io sdist case.
fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(sha) = sha {
        println!("cargo:rustc-env=PHS_BUILD_SHA={sha}");
    }
    // Best-effort: re-run when HEAD moves to a different commit. Doesn't cover every way
    // HEAD can change (e.g. `git commit --amend` keeps the same ref), which is fine -- a
    // stale SHA in a dev build is a display nicety, not correctness-critical.
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
```

- [ ] **Step 3: Verify it builds**

```bash
cd d:/Projects/physure
cargo build -p physure-cli 2>&1 | tail -20
```

Expected: builds clean (the new dependencies resolve and compile; `build.rs` runs with no
errors). No new code uses `ureq`/`semver`/`PHS_BUILD_SHA` yet, so nothing observable changes.

- [ ] **Step 4: Commit**

```bash
git add physure-cli/Cargo.toml physure-cli/Cargo.lock physure-cli/build.rs
git commit -m "build(cli): add ureq/semver deps and capture the build's commit SHA"
```

---

## Task 2: `phs --version` / `-V`

**Files:**
- Modify: `physure-cli/src/main.rs`

- [ ] **Step 1: Write the failing test**

Add to `physure-cli/tests/cli_tests.rs`, after `test_phs_missing_file`:

```rust
#[test]
fn test_version_flag_prints_the_crate_version() {
    let output = Command::new(get_phs_bin())
        .arg("--version")
        .output()
        .expect("Failed to execute phs binary");

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "Expected the crate version in output, got: {}", stdout);
    assert!(stdout.starts_with("phs "), "Expected output to start with 'phs ', got: {}", stdout);
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd d:/Projects/physure
cargo test -p physure-cli test_version_flag_prints_the_crate_version -- --nocapture
```

Expected: FAIL. `--version` currently falls through to being parsed as a PHS script and prints
a syntax error instead, so `output.status.success()` is false.

- [ ] **Step 3: Add the flag and its handler**

In `physure-cli/src/main.rs`, add this function right after `print_help()` (before `fn
run_repl()`):

```rust
fn print_version() {
    match option_env!("PHS_BUILD_SHA") {
        Some(sha) => println!("phs {} ({})", env!("CARGO_PKG_VERSION"), sha),
        None => println!("phs {}", env!("CARGO_PKG_VERSION")),
    }
}
```

In `fn main()`, add this check immediately after the existing `--daemon` block (before the
`--help` block):

```rust
    if args.iter().any(|a| a == "--version" || a == "-V") {
        print_version();
        return;
    }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p physure-cli test_version_flag_prints_the_crate_version -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Update the help text**

In `print_help()`, add to the `FLAGS & OPTIONS:` block, right after the `-h, --help` line:

```rust
    println!("    -V, --version            Print version and build commit");
```

- [ ] **Step 6: Run the full physure-cli suite**

```bash
cargo test -p physure-cli
```

Expected: all green, one more test than before.

- [ ] **Step 7: Commit**

```bash
git add physure-cli/src/main.rs physure-cli/tests/cli_tests.rs
git commit -m "feat(cli): add phs --version / -V"
```

---

## Task 3: `upgrade.rs` — version parsing, asset selection, relevant-path filter

**Files:**
- Create: `physure-cli/src/upgrade.rs`
- Modify: `physure-cli/src/main.rs` (module declaration only)

- [ ] **Step 1: Declare the module**

In `physure-cli/src/main.rs`, add to the `mod` block (alphabetical, after `tui;`):

```rust
mod upgrade;
```

- [ ] **Step 2: Write the failing tests**

Create `physure-cli/src/upgrade.rs` with just the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_core_tag_version_reads_a_core_tag() {
        assert_eq!(parse_core_tag_version("core-v0.2.4"), Some(semver::Version::new(0, 2, 4)));
    }

    #[test]
    fn parse_core_tag_version_rejects_other_tag_shapes() {
        assert_eq!(parse_core_tag_version("v0.2.4"), None);
        assert_eq!(parse_core_tag_version("py-v0.2.4"), None);
        assert_eq!(parse_core_tag_version("py-core-v0.2.4"), None);
        assert_eq!(parse_core_tag_version("core-vnotaversion"), None);
    }

    #[test]
    fn asset_name_for_covers_every_core_release_yml_target() {
        assert_eq!(asset_name_for("windows", "x86_64"), Some("phs-windows-x86_64.zip"));
        assert_eq!(asset_name_for("linux", "x86_64"), Some("phs-linux-x86_64.tar.gz"));
        assert_eq!(asset_name_for("linux", "aarch64"), Some("phs-linux-aarch64.tar.gz"));
        assert_eq!(asset_name_for("macos", "x86_64"), Some("phs-macos-x86_64.tar.gz"));
        assert_eq!(asset_name_for("macos", "aarch64"), Some("phs-macos-aarch64.tar.gz"));
    }

    #[test]
    fn asset_name_for_is_none_for_an_unsupported_target() {
        assert_eq!(asset_name_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn platform_asset_name_finds_a_match_on_the_platform_running_this_test() {
        // This repo's CI and dev machines are always one of the five core-release.yml
        // targets, so this should never be None in practice.
        assert!(platform_asset_name().is_some());
    }

    #[test]
    fn is_relevant_change_true_for_any_of_the_four_crate_directories() {
        assert!(is_relevant_change(&["physure-core/src/quantity.rs".to_string()]));
        assert!(is_relevant_change(&["physure-script/src/interpreter/mod.rs".to_string()]));
        assert!(is_relevant_change(&["physure-cli/src/main.rs".to_string()]));
        assert!(is_relevant_change(&["physure-lsp/src/incremental.rs".to_string()]));
    }

    #[test]
    fn is_relevant_change_false_for_unrelated_paths() {
        assert!(!is_relevant_change(&["physure-python/physure/__init__.py".to_string()]));
        assert!(!is_relevant_change(&["docs/tutorials/phs_primer.md".to_string()]));
        assert!(!is_relevant_change(&[]));
    }
}
```

- [ ] **Step 3: Run to verify it fails to compile**

```bash
cargo test -p physure-cli --lib upgrade:: 2>&1 | tail -30
```

Expected: compile errors — `parse_core_tag_version`, `asset_name_for`, `platform_asset_name`,
`is_relevant_change` don't exist yet.

- [ ] **Step 4: Implement the four functions**

Add above the `#[cfg(test)]` block in `physure-cli/src/upgrade.rs`:

```rust
/// The four crate directories that actually compile into `phs`/`physure-lsp`.
/// `physure-script` implements the language itself (parser, interpreter, grammar) and both
/// binaries depend on it directly -- leaving it out would miss most of what a typical commit
/// here changes, confirmed against this session's own history, where nearly every fix touched
/// `physure-script`, not `physure-core`.
const RELEVANT_PATH_PREFIXES: &[&str] =
    &["physure-core/", "physure-script/", "physure-cli/", "physure-lsp/"];

/// Parses the version out of a `core-vX.Y.Z` release tag. `None` for any other tag shape --
/// this repo also publishes plain `vX.Y.Z` (Python package) and `py-vX.Y.Z` releases, which
/// `phs upgrade` must ignore.
fn parse_core_tag_version(tag: &str) -> Option<semver::Version> {
    tag.strip_prefix("core-v").and_then(|v| semver::Version::parse(v).ok())
}

fn asset_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("phs-windows-x86_64.zip"),
        ("linux", "x86_64") => Some("phs-linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Some("phs-linux-aarch64.tar.gz"),
        ("macos", "x86_64") => Some("phs-macos-x86_64.tar.gz"),
        ("macos", "aarch64") => Some("phs-macos-aarch64.tar.gz"),
        _ => None,
    }
}

/// The prebuilt-binary asset name `core-release.yml` publishes for the platform this binary
/// is actually running on, or `None` for a platform that pipeline doesn't build for.
fn platform_asset_name() -> Option<&'static str> {
    asset_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// True if any changed file falls under one of the crates that actually compile into
/// `phs`/`physure-lsp` -- see `RELEVANT_PATH_PREFIXES`'s doc comment.
fn is_relevant_change(files: &[String]) -> bool {
    files.iter().any(|f| RELEVANT_PATH_PREFIXES.iter().any(|p| f.starts_with(p)))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p physure-cli --lib upgrade:: -- --nocapture
```

Expected: all 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add physure-cli/src/main.rs physure-cli/src/upgrade.rs
git commit -m "feat(cli): upgrade.rs version/asset/relevant-path helpers"
```

---

## Task 4: `make_way_for` — freeing a path that might be a running executable

**Files:**
- Modify: `physure-cli/src/upgrade.rs`

- [ ] **Step 1: Write the failing tests**

Add to `physure-cli/src/upgrade.rs`'s test module (inside `mod tests`, after
`is_relevant_change_false_for_unrelated_paths`):

```rust
    #[test]
    #[cfg(windows)]
    fn make_way_for_renames_an_existing_file_out_of_the_target_path() {
        let dir = std::env::temp_dir().join(format!("phs-upgrade-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("phs.exe");
        fs::write(&target, b"old content").unwrap();

        make_way_for(&target).unwrap();

        assert!(!target.exists(), "the original path should be free for a new file");
        assert_eq!(fs::read(target.with_extension("old")).unwrap(), b"old content");

        // A second call (a second upgrade in a row) must not fail just because a stale
        // .old from the first one is still sitting there.
        fs::write(&target, b"new content").unwrap();
        make_way_for(&target).unwrap();
        assert!(!target.exists());
        assert_eq!(fs::read(target.with_extension("old")).unwrap(), b"new content");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(not(windows))]
    fn make_way_for_is_a_no_op_outside_windows() {
        let dir = std::env::temp_dir().join(format!("phs-upgrade-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("phs");
        fs::write(&target, b"content").unwrap();

        make_way_for(&target).unwrap();

        // Nothing renamed -- overwriting a running executable's file is always fine on Unix,
        // so make_way_for has nothing to do there.
        assert!(target.exists());
        assert!(!target.with_extension("old").exists());

        let _ = fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run to verify it fails to compile**

```bash
cargo test -p physure-cli --lib upgrade:: 2>&1 | tail -20
```

Expected: `make_way_for` not found, and `fs` not imported yet in the test module's scope
(it comes from `use super::*` once the real function imports it below).

- [ ] **Step 3: Implement `make_way_for`**

Add near the top of `physure-cli/src/upgrade.rs`, before `RELEVANT_PATH_PREFIXES`:

```rust
use std::fs;
use std::io;
use std::path::Path;

/// Makes `target` writable even if it's currently executing (this process's own exe, or one
/// another process has open) -- Windows allows *renaming* an in-use executable even though it
/// refuses to delete or overwrite it directly; the renamed file keeps running until whichever
/// process has it open exits. Unix needs nothing: removing/overwriting a running executable's
/// file is always fine there, so this is a no-op except on Windows.
fn make_way_for(target: &Path) -> io::Result<()> {
    if cfg!(windows) && target.exists() {
        let stale = target.with_extension("old");
        let _ = fs::remove_file(&stale); // best-effort: a leftover from a previous upgrade
        fs::rename(target, &stale)?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p physure-cli --lib upgrade:: -- --nocapture
```

Expected: all tests pass (9 total: the 7 from Task 3 plus the one that applies on this
platform from this task).

- [ ] **Step 5: Commit**

```bash
git add physure-cli/src/upgrade.rs
git commit -m "feat(cli): make_way_for frees a path that might be a running executable"
```

---

## Task 5: GitHub API and download helpers

**Files:**
- Modify: `physure-cli/src/upgrade.rs`

Not TDD'd (per the design spec's testing strategy: these are network-dependent and verified
manually against the real API in Task 9, not asserted in CI) — implement directly.

- [ ] **Step 1: Add the helpers**

Add to `physure-cli/src/upgrade.rs`, after `make_way_for`:

```rust
const USER_AGENT: &str = "phs-upgrade";

/// GETs `url` expecting a JSON body -- every GitHub API call this module makes.
fn github_get(url: &str) -> Result<serde_json::Value, String> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("GET {url} failed: {e}"))?;
    resp.into_json().map_err(|e| format!("GET {url}: invalid JSON response: {e}"))
}

/// Downloads `url`'s raw body to `dest`, for a release asset (a `.zip`/`.tar.gz`, not JSON).
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("GET {url} failed: {e}"))?;
    let mut file = fs::File::create(dest).map_err(|e| format!("creating {}: {e}", dest.display()))?;
    io::copy(&mut resp.into_reader(), &mut file)
        .map_err(|e| format!("writing {}: {e}", dest.display()))?;
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p physure-cli 2>&1 | tail -20
```

Expected: builds clean. `github_get`/`download_file` are unused so far — a `never used`
warning is expected and will go away once Task 7 calls them; don't suppress it, it'll resolve
itself.

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/upgrade.rs
git commit -m "feat(cli): upgrade.rs GitHub API GET and file download helpers"
```

---

## Task 6: Archive extraction and binary replacement

**Files:**
- Modify: `physure-cli/src/upgrade.rs`

- [ ] **Step 1: Add the helpers**

Add to `physure-cli/src/upgrade.rs`, after `download_file`:

```rust
/// Extracts `archive` (`.zip` or `.tar.gz`) into `dest_dir` by shelling out to the system
/// `tar` -- bsdtar, which every target platform ships by default (Windows 10 1803+, macOS,
/// Linux), reads both archive types through the same `-xf`, so this needs no archive-handling
/// crate.
fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("creating {}: {e}", dest_dir.display()))?;
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dest_dir)
        .status()
        .map_err(|e| format!("running tar: {e} (is `tar` on PATH?)"))?;
    if !status.success() {
        return Err(format!("tar exited with {status}"));
    }
    Ok(())
}

/// Replaces `installed` with the content at `new_content`, freeing the path first via
/// `make_way_for` in case `installed` is a currently-running executable. Best-effort cleans
/// up the `.old` file `make_way_for` may have left behind.
fn replace_binary(installed: &Path, new_content: &Path) -> Result<(), String> {
    make_way_for(installed).map_err(|e| format!("preparing to replace {}: {e}", installed.display()))?;
    if let Some(parent) = installed.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::copy(new_content, installed)
        .map_err(|e| format!("copying {} -> {}: {e}", new_content.display(), installed.display()))?;
    let _ = fs::remove_file(installed.with_extension("old"));
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p physure-cli 2>&1 | tail -20
```

Expected: builds clean (same "never used" warnings as Task 5 until Task 7 wires everything
together).

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/upgrade.rs
git commit -m "feat(cli): upgrade.rs archive extraction and binary replacement"
```

---

## Task 7: `run_stable_upgrade`

**Files:**
- Modify: `physure-cli/src/upgrade.rs`

- [ ] **Step 1: Add `run_stable_upgrade`**

Add to `physure-cli/src/upgrade.rs`, after `replace_binary` and before the `#[cfg(test)]`
block:

```rust
fn run_stable_upgrade() {
    println!("Checking for the latest release...");
    let releases = match github_get("https://api.github.com/repos/Alexisrx96/physure/releases") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to check for updates: {e}");
            std::process::exit(1);
        }
    };
    let Some(releases_arr) = releases.as_array() else {
        eprintln!("Unexpected response from GitHub releases API.");
        std::process::exit(1);
    };
    let Some(release) = releases_arr
        .iter()
        .find(|r| r["tag_name"].as_str().map(|t| t.starts_with("core-v")).unwrap_or(false))
    else {
        eprintln!("No core-v* release found on GitHub.");
        std::process::exit(1);
    };
    let tag = release["tag_name"].as_str().unwrap_or("");
    let Some(latest) = parse_core_tag_version(tag) else {
        eprintln!("Could not parse a version from release tag '{tag}'.");
        std::process::exit(1);
    };
    let running = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION is always valid semver");

    match latest.cmp(&running) {
        std::cmp::Ordering::Equal => {
            println!("phs is already up to date ({running}).");
            return;
        }
        std::cmp::Ordering::Less => {
            println!(
                "Running {running} is newer than the latest published release ({latest}) -- probably a --nightly build. Nothing to do."
            );
            return;
        }
        std::cmp::Ordering::Greater => {}
    }

    let Some(asset_name) = platform_asset_name() else {
        eprintln!(
            "No prebuilt binary for {}/{}. Try `phs upgrade --nightly` (requires cargo) or install manually.",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        std::process::exit(1);
    };
    let Some(asset) = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"].as_str() == Some(asset_name)))
    else {
        eprintln!("Release {tag} has no asset named '{asset_name}'.");
        std::process::exit(1);
    };
    let Some(download_url) = asset["browser_download_url"].as_str() else {
        eprintln!("Release asset '{asset_name}' has no download URL.");
        std::process::exit(1);
    };

    let tmp_dir = std::env::temp_dir().join(format!("phs-upgrade-{}", std::process::id()));
    if let Err(e) = fs::create_dir_all(&tmp_dir) {
        eprintln!("Failed to create temp directory {}: {e}", tmp_dir.display());
        std::process::exit(1);
    }
    let archive_path = tmp_dir.join(asset_name);

    println!("Downloading {asset_name}...");
    if let Err(e) = download_file(download_url, &archive_path) {
        eprintln!("Download failed: {e}");
        let _ = fs::remove_dir_all(&tmp_dir);
        std::process::exit(1);
    }

    let extract_dir = tmp_dir.join("extracted");
    if let Err(e) = extract_archive(&archive_path, &extract_dir) {
        eprintln!("Extraction failed: {e}");
        let _ = fs::remove_dir_all(&tmp_dir);
        std::process::exit(1);
    }

    let exe = if cfg!(windows) { ".exe" } else { "" };
    let phs_installed = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not determine the running phs's own path: {e}");
            let _ = fs::remove_dir_all(&tmp_dir);
            std::process::exit(1);
        }
    };
    let phs_extracted = extract_dir.join(format!("phs{exe}"));
    match replace_binary(&phs_installed, &phs_extracted) {
        Ok(()) => println!("phs: {running} -> {latest}"),
        Err(e) => {
            eprintln!("Failed to replace phs: {e}");
            let _ = fs::remove_dir_all(&tmp_dir);
            std::process::exit(1);
        }
    }

    let lsp_name = format!("physure-lsp{exe}");
    let lsp_extracted = extract_dir.join(&lsp_name);
    if lsp_extracted.exists() {
        let lsp_target = phs_installed
            .parent()
            .map(|dir| dir.join(&lsp_name))
            .filter(|p| p.exists())
            .or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".cargo").join("bin").join(&lsp_name))
                    .filter(|p| p.exists())
            });
        match lsp_target {
            Some(target) => match replace_binary(&target, &lsp_extracted) {
                Ok(()) => println!("physure-lsp: -> {latest}"),
                Err(e) => eprintln!("Failed to replace physure-lsp: {e}"),
            },
            None => println!("physure-lsp not found alongside phs or in ~/.cargo/bin; skipped."),
        }
    }

    let _ = fs::remove_dir_all(&tmp_dir);
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p physure-cli 2>&1 | tail -20
```

Expected: builds clean. `run_stable_upgrade` is unused until Task 9 wires it into `main()` —
expected warning, resolves itself there.

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/upgrade.rs
git commit -m "feat(cli): upgrade.rs run_stable_upgrade"
```

---

## Task 8: `run_nightly_upgrade`

**Files:**
- Modify: `physure-cli/src/upgrade.rs`

- [ ] **Step 1: Add `run_nightly_upgrade` and its two small helpers**

Add to `physure-cli/src/upgrade.rs`, after `run_stable_upgrade` and before the `#[cfg(test)]`
block:

```rust
fn which_cargo_found() -> bool {
    let lookup = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(lookup)
        .arg("cargo")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cargo_install_from_main(krate: &str, bin: &str) -> bool {
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(["install", "--git", "https://github.com/Alexisrx96/physure", "--branch", "main", krate]);
    if bin != krate {
        cmd.args(["--bin", bin]);
    }
    cmd.args(["--locked", "--force"]);
    match cmd.status() {
        Ok(s) if s.success() => true,
        Ok(s) => {
            eprintln!("cargo install {krate} exited with {s}");
            false
        }
        Err(e) => {
            eprintln!("Failed to run cargo install for {krate}: {e}");
            false
        }
    }
}

fn run_nightly_upgrade() {
    if !which_cargo_found() {
        eprintln!("cargo not found on PATH. Install Rust from https://rustup.rs then re-run `phs upgrade --nightly`.");
        std::process::exit(1);
    }

    let running_sha = option_env!("PHS_BUILD_SHA");
    let mut should_rebuild = true;
    let mut status_note: Option<String> = None;

    if let Some(running_sha) = running_sha {
        if let Ok(commit) = github_get("https://api.github.com/repos/Alexisrx96/physure/commits/main") {
            if let Some(latest_sha) = commit["sha"].as_str() {
                let latest_short = &latest_sha[..latest_sha.len().min(7)];
                if latest_short == running_sha {
                    should_rebuild = false;
                    status_note = Some(format!("phs is already on the latest commit ({running_sha})."));
                } else {
                    let compare_url = format!(
                        "https://api.github.com/repos/Alexisrx96/physure/compare/{running_sha}...{latest_sha}"
                    );
                    // A failed diagnostic check here is never a reason to refuse an upgrade
                    // the user explicitly asked for -- should_rebuild simply stays true.
                    if let Ok(compare) = github_get(&compare_url) {
                        let files: Vec<String> = compare["files"]
                            .as_array()
                            .map(|arr| {
                                arr.iter().filter_map(|f| f["filename"].as_str().map(str::to_string)).collect()
                            })
                            .unwrap_or_default();
                        if !is_relevant_change(&files) {
                            should_rebuild = false;
                            status_note = Some(format!(
                                "main moved ({running_sha} -> {latest_short}) but nothing under physure-core/physure-script/physure-cli/physure-lsp changed -- phs is already effectively up to date."
                            ));
                        }
                    }
                }
            }
        }
    }

    if !should_rebuild {
        println!("{}", status_note.unwrap_or_else(|| "phs is already up to date.".to_string()));
        return;
    }

    let phs_installed = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not determine the running phs's own path: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = make_way_for(&phs_installed) {
        eprintln!("Failed to prepare {} for rebuild: {e}", phs_installed.display());
        std::process::exit(1);
    }
    println!("Building phs from main...");
    if !cargo_install_from_main("physure-cli", "phs") {
        std::process::exit(1);
    }

    let exe = if cfg!(windows) { ".exe" } else { "" };
    if let Some(dir) = phs_installed.parent() {
        let lsp_path = dir.join(format!("physure-lsp{exe}"));
        if lsp_path.exists() {
            match make_way_for(&lsp_path) {
                Ok(()) => {
                    println!("Building physure-lsp from main...");
                    cargo_install_from_main("physure-lsp", "physure-lsp");
                }
                Err(e) => eprintln!("Failed to prepare {} for rebuild: {e}", lsp_path.display()),
            }
        }
    }

    println!("Done. Run `phs --version` to confirm the new commit.");
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p physure-cli 2>&1 | tail -20
```

Expected: builds clean. `run_nightly_upgrade` unused until Task 9 — expected, resolves there.

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/upgrade.rs
git commit -m "feat(cli): upgrade.rs run_nightly_upgrade with the relevant-commit check"
```

---

## Task 9: Wire `upgrade` into `main.rs`, update help text

**Files:**
- Modify: `physure-cli/src/upgrade.rs`
- Modify: `physure-cli/src/main.rs`

- [ ] **Step 1: Add the public entry point**

Add to `physure-cli/src/upgrade.rs`, immediately after the three `use` lines
(`use std::fs;` / `use std::io;` / `use std::path::Path;`) and before `const
RELEVANT_PATH_PREFIXES`:

```rust
pub fn run_upgrade(nightly: bool) {
    if nightly {
        run_nightly_upgrade();
    } else {
        run_stable_upgrade();
    }
}
```

- [ ] **Step 2: Dispatch from `main()`**

In `physure-cli/src/main.rs`, add this block in `fn main()` right after the existing `if
args[1] == "debug" { ... }` block and before `if handle_transpile(&args) { ... }`:

```rust
    if args[1] == "upgrade" {
        let nightly = args.iter().any(|a| a == "--nightly");
        upgrade::run_upgrade(nightly);
        return;
    }
```

- [ ] **Step 3: Update the help text**

In `print_help()`, add to the `USAGE:` block, right after the `phs doc [--save]` line:

```rust
    println!("    phs upgrade [--nightly]  Update phs and physure-lsp to the latest release (or main, with --nightly)");
```

And add to the `EXAMPLES:` block, after the `phs debug orbit_sim.phs --break 12` line:

```rust
    println!("    phs upgrade");
    println!("    phs upgrade --nightly");
```

- [ ] **Step 4: Verify the whole crate builds with no unused-function warnings**

```bash
cargo build -p physure-cli 2>&1 | grep -i "warning: function.*never used\|warning: unused"
```

Expected: no output naming anything in `upgrade.rs` — `run_stable_upgrade`,
`run_nightly_upgrade`, `github_get`, `download_file`, `extract_archive`, `replace_binary` are
all reachable now.

- [ ] **Step 5: Run the full physure-cli suite**

```bash
cargo test -p physure-cli
```

Expected: all green, including every test from Tasks 2-4.

- [ ] **Step 6: Commit**

```bash
git add physure-cli/src/upgrade.rs physure-cli/src/main.rs
git commit -m "feat(cli): wire phs upgrade into the dispatch table and help text"
```

---

## Task 10: Final verification

**Files:** none — verification only.

- [ ] **Step 1: Full physure-cli suite from a clean build**

```bash
cd d:/Projects/physure
cargo clean -p physure-cli
cargo test -p physure-cli
```

Expected: all green.

- [ ] **Step 2: Whole-workspace build**

```bash
cargo build --workspace
```

Expected: clean (confirms the two new dependencies don't collide with anything else in the
workspace's dependency graph).

- [ ] **Step 3: Manual smoke test — `--version`**

```bash
cargo run -p physure-cli --bin phs -- --version
```

Expected: `phs 0.2.3 (` followed by a 7-character commit hash and `)` — confirms `build.rs`
actually captured a real SHA in this checkout.

- [ ] **Step 4: Manual smoke test — stable upgrade against the real GitHub API**

```bash
cargo run -p physure-cli --bin phs -- upgrade
```

Expected: either `phs is already up to date (X.Y.Z).` (if the workspace version matches the
latest `core-v*` tag) or a real download+replace happens. Read the actual output; if it
downloads, confirm afterward with `phs --version` (the freshly-built dev binary, not the one
`upgrade` just replaced — the one `cargo run` just produced under `target/debug`) that the
replaced `~/.cargo/bin/phs` (or wherever `current_exe()` pointed) actually updated. Since this
session already has `phs`/`physure-lsp` installed at commits ahead of the last release (see
Task 9 of this session's `phs upgrade` work), the "newer than latest release, nothing to do"
branch is the more likely real result here — that is itself a valid pass: it proves the
never-downgrade check works against live data, not just the unit test's synthetic case.

- [ ] **Step 5: Manual smoke test — nightly upgrade's relevant-commit check**

```bash
cargo run -p physure-cli --bin phs -- upgrade --nightly
```

Expected: since the installed `~/.cargo/bin/phs` was already built from this branch's tip
earlier in this session, this should report either "already on the latest commit" or "main
moved but nothing relevant changed" (if the PR merge commit or anything after it touched only
non-relevant paths) — or a real rebuild, if `main` has moved with a relevant change since. Any
of these is a pass as long as the reported reasoning matches what `git log` on `main` actually
shows; read the real output and cross-check it, don't assume.

- [ ] **Step 6: No commit for this task** — verification-only. If any step fails, stop and fix
      the specific task whose code caused it before proceeding.
