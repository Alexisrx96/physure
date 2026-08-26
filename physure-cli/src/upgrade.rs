//! `phs upgrade` -- self-updates the `phs`/`physure-lsp` binaries. Two modes: the default
//! downloads the latest published `core-v*` GitHub release binary for the current platform;
//! `--nightly` rebuilds from `main`'s current tip via `cargo install`, but only when a commit
//! that actually changed one of the crates that compile into these binaries has landed since
//! the running build.

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

/// The four crate directories that actually compile into `phs`/`physure-lsp`. Both binaries
/// depend directly on `physure-script`, which implements the language itself (parser,
/// interpreter, grammar), so it belongs in this set alongside the more obvious
/// `physure-core`, `physure-cli`, and `physure-lsp` -- leaving it out would miss most of
/// what a typical commit to this repo changes.
const RELEVANT_PATH_PREFIXES: &[&str] =
    &["physure-core/", "physure-script/", "physure-cli/", "physure-lsp/"];

/// Parses the version out of a `core-vX.Y.Z` release tag. `None` for any other tag shape --
/// this repo also publishes plain `vX.Y.Z` (Python package) and `py-vX.Y.Z` releases, which
/// `phs upgrade` must ignore.
fn parse_core_tag_version(tag: &str) -> Option<semver::Version> {
    tag.strip_prefix("core-v").and_then(|v| semver::Version::parse(v).ok())
}

/// The prebuilt-binary asset name `core-release.yml` publishes for a given `(os, arch)` pair,
/// as reported by `std::env::consts::OS`/`ARCH` -- `None` for any platform that pipeline
/// doesn't build for.
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

/// `asset_name_for` for the platform this binary is actually running on.
fn platform_asset_name() -> Option<&'static str> {
    asset_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// True if any changed file falls under one of the crates that actually compile into
/// `phs`/`physure-lsp` -- see `RELEVANT_PATH_PREFIXES`'s doc comment.
fn is_relevant_change(files: &[String]) -> bool {
    files.iter().any(|f| RELEVANT_PATH_PREFIXES.iter().any(|p| f.starts_with(p)))
}

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
}
