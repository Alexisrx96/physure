//! `phs upgrade` -- self-updates the `phs`/`physure-lsp` binaries. Two modes: the default
//! downloads the latest published `core-v*` GitHub release binary for the current platform;
//! `--nightly` rebuilds from `main`'s current tip via `cargo install`, but only when a commit
//! that actually changed one of the crates that compile into these binaries has landed since
//! the running build.

use std::fs;
use std::io;
use std::path::Path;

/// Makes `target` writable even if it's currently executing (this process's own exe, or one
/// another process has open), by renaming it out of the way -- unconditionally, on every
/// platform. On Windows this is load-bearing: Windows allows *renaming* an in-use executable
/// even though it refuses to delete or overwrite it directly, so the renamed file keeps
/// running until whichever process has it open exits. On Unix it's cheap insurance rather
/// than a workaround for a lock: `replace_binary`'s subsequent `fs::copy` opens the
/// *destination* file and truncates it in place (`O_WRONLY|O_CREAT|O_TRUNC` on the same
/// inode) rather than unlinking and recreating it, and truncating a file that's still mapped
/// into a running process's address space is exactly what Unix's `ETXTBSY` ("Text file
/// busy") protection blocks. Renaming first ensures `fs::copy` always writes into a fresh
/// inode instead of ever risking a truncate-in-place on a possibly-running binary.
fn make_way_for(target: &Path) -> io::Result<()> {
    if target.exists() {
        let stale = target.with_extension("old");
        // Belt-and-suspenders, not load-bearing: `fs::rename` below already replaces an
        // existing `stale` destination on its own (Windows `MoveFileExW` is called with
        // `MOVEFILE_REPLACE_EXISTING`). This just clears the way first, best-effort, in
        // case a leftover `.old` from a previous upgrade can't be replaced in place for
        // some reason -- if it truly can't be removed, the rename below will surface that.
        let _ = fs::remove_file(&stale);
        fs::rename(target, &stale)?;
    }
    Ok(())
}

const USER_AGENT: &str = "phs-upgrade";

/// A `ureq` agent with a 30s timeout covering the whole request -- DNS, connect, and reading
/// the response body -- so a connection that connects fine but then stalls mid-response can't
/// hang `phs upgrade` forever with no feedback.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(30)).build()
}

/// GETs `url`, the one request path every helper in this module funnels through. On a non-2xx
/// response this captures GitHub's actual response body (a rate-limit explanation, a JSON
/// `message` field, etc.) in the error, not just the status code -- `ureq::Error`'s own
/// `Display` collapses a `Status` error down to `"status code 403"`, which is exactly the least
/// useful moment to lose that detail.
fn get(url: &str) -> Result<ureq::Response, String> {
    agent().get(url).set("User-Agent", USER_AGENT).call().map_err(|e| match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!("GET {url} failed: {code} {body}")
        }
        ureq::Error::Transport(t) => format!("GET {url} failed: {t}"),
    })
}

/// GETs `url` expecting a JSON body -- every GitHub API call this module makes.
fn github_get(url: &str) -> Result<serde_json::Value, String> {
    let resp = get(url)?;
    resp.into_json().map_err(|e| format!("GET {url}: invalid JSON response: {e}"))
}

/// Downloads `url`'s raw body to `dest`, for a release asset (a `.zip`/`.tar.gz`, not JSON).
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = get(url)?;
    let mut file = fs::File::create(dest).map_err(|e| format!("creating {}: {e}", dest.display()))?;
    io::copy(&mut resp.into_reader(), &mut file)
        .map_err(|e| format!("writing {}: {e}", dest.display()))?;
    Ok(())
}

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

/// Replaces `installed` with the content at `new_content`. Never leaves `installed` missing:
/// the existing file is moved aside first (`make_way_for`, needed on Windows because it
/// refuses to overwrite a currently-executing file in place, and harmless insurance on Unix
/// since `fs::copy` truncates the destination inode rather than unlinking it -- which Unix
/// blocks for a running executable with ETXTBSY), the new content is copied to a fresh
/// same-directory temp path first, and only *that* gets renamed into `installed`'s place. If
/// anything fails before the final rename, the original is renamed back from `.old` so a
/// failed upgrade leaves the previous working binary in place, not a missing one.
fn replace_binary(installed: &Path, new_content: &Path) -> Result<(), String> {
    let had_original = installed.exists();
    make_way_for(installed).map_err(|e| format!("preparing to replace {}: {e}", installed.display()))?;
    if let Some(parent) = installed.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    let staged = installed.with_extension("new");
    let result = fs::copy(new_content, &staged)
        .map_err(|e| format!("copying {} -> {}: {e}", new_content.display(), staged.display()))
        .and_then(|_| {
            fs::rename(&staged, installed)
                .map_err(|e| format!("moving {} -> {}: {e}", staged.display(), installed.display()))
        });

    if let Err(e) = &result {
        let _ = fs::remove_file(&staged);
        if had_original {
            let _ = fs::rename(installed.with_extension("old"), installed);
        }
        return Err(format!("{e} (restored the previous binary)"));
    }

    let _ = fs::remove_file(installed.with_extension("old"));
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
        use std::io::Read;
        use std::os::windows::fs::OpenOptionsExt;

        // The exact sharing the Windows loader grants a running exe's image: read + delete,
        // but *not* write. A plain `File::open` handle grants write-sharing by default and
        // would let a direct overwrite succeed, so it wouldn't catch a regression here --
        // only these flags actually reproduce the "Acceso denegado" failure `cargo install`
        // hit rebuilding a running phs.exe.
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_DELETE: u32 = 0x4;

        let dir = std::env::temp_dir().join(format!("phs-upgrade-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("phs.exe");
        fs::write(&target, b"old content").unwrap();

        // Hold `target` open the way a running phs.exe would hold itself open, across the
        // whole first `make_way_for` call below.
        let mut locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .open(&target)
            .unwrap();

        // Prove this is really the locked-executable scenario and not an idle-file rename:
        // a direct overwrite attempt fails with ERROR_SHARING_VIOLATION while `locked` is
        // open.
        let overwrite_err = fs::OpenOptions::new().write(true).open(&target).unwrap_err();
        assert_eq!(overwrite_err.raw_os_error(), Some(32), "expected ERROR_SHARING_VIOLATION");

        make_way_for(&target).unwrap();

        assert!(!target.exists(), "the original path should be free for a new file");
        assert_eq!(fs::read(target.with_extension("old")).unwrap(), b"old content");

        // The handle opened before the rename is still valid -- this is the property that
        // lets a process keep running normally after being renamed out from under itself.
        let mut still_readable = String::new();
        locked.read_to_string(&mut still_readable).unwrap();
        assert_eq!(still_readable, "old content");
        drop(locked);

        // A second call (a second upgrade in a row) must not fail just because a stale
        // .old from the first one is still sitting there.
        fs::write(&target, b"new content").unwrap();
        make_way_for(&target).unwrap();
        assert!(!target.exists());
        assert_eq!(fs::read(target.with_extension("old")).unwrap(), b"new content");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn make_way_for_renames_on_every_platform() {
        let dir = std::env::temp_dir()
            .join(format!("phs-upgrade-test-portable-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("phs");
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
}
