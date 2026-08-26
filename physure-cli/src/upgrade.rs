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
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

/// GETs `url`, the one request path every helper in this module funnels through. On a non-2xx
/// response this captures GitHub's actual response body (a rate-limit explanation, a JSON
/// `message` field, etc.) in the error, not just the status code -- `ureq::Error`'s own
/// `Display` collapses a `Status` error down to `"status code 403"`, which is exactly the least
/// useful moment to lose that detail.
fn get(url: &str) -> Result<ureq::Response, String> {
    agent()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| match e {
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
    resp.into_json()
        .map_err(|e| format!("GET {url}: invalid JSON response: {e}"))
}

/// Downloads `url`'s raw body to `dest`, for a release asset (a `.zip`/`.tar.gz`, not JSON).
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = get(url)?;
    let mut file =
        fs::File::create(dest).map_err(|e| format!("creating {}: {e}", dest.display()))?;
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
    make_way_for(installed)
        .map_err(|e| format!("preparing to replace {}: {e}", installed.display()))?;
    if let Some(parent) = installed.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    let staged = installed.with_extension("new");
    let result = fs::copy(new_content, &staged)
        .map_err(|e| {
            format!(
                "copying {} -> {}: {e}",
                new_content.display(),
                staged.display()
            )
        })
        .and_then(|_| {
            fs::rename(&staged, installed).map_err(|e| {
                format!(
                    "moving {} -> {}: {e}",
                    staged.display(),
                    installed.display()
                )
            })
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

/// The default (non-`--nightly`) upgrade flow: checks GitHub for the latest `core-v*` release,
/// and if it's newer than the running `phs`, downloads and extracts the platform-appropriate
/// asset, then replaces the running `phs` binary and (if present alongside it or in
/// `~/.cargo/bin`) `physure-lsp`. Prints progress and exits 1 on any failure that prevents `phs`
/// itself from being updated; a `physure-lsp` replace failure alone is reported but non-fatal.
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
    let Some(release) = releases_arr.iter().find(|r| {
        r["tag_name"]
            .as_str()
            .map(|t| t.starts_with("core-v"))
            .unwrap_or(false)
    }) else {
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
    let Some(asset) = release["assets"].as_array().and_then(|assets| {
        assets
            .iter()
            .find(|a| a["name"].as_str() == Some(asset_name))
    }) else {
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
                Ok(()) => println!("physure-lsp: updated to {latest}"),
                Err(e) => eprintln!(
                    "phs updated, but failed to replace physure-lsp: {e} (try again after closing any editor using it)"
                ),
            },
            None => println!("physure-lsp not found alongside phs or in ~/.cargo/bin; skipped."),
        }
    }

    let _ = fs::remove_dir_all(&tmp_dir);
}

fn which_cargo_found() -> bool {
    let lookup = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(lookup)
        .arg("cargo")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cargo_install_from_main(
    krate: &str,
    bin: &str,
    install_root: &Path,
) -> Result<std::path::PathBuf, String> {
    let mut cmd = std::process::Command::new("cargo");
    cmd.args([
        "install",
        "--git",
        "https://github.com/Alexisrx96/physure",
        "--branch",
        "main",
        krate,
    ]);
    if bin != krate {
        cmd.args(["--bin", bin]);
    }
    cmd.args(["--locked", "--force", "--root"])
        .arg(install_root);
    match cmd.status() {
        Ok(s) if s.success() => Ok(install_root
            .join("bin")
            .join(format!("{bin}{}", if cfg!(windows) { ".exe" } else { "" }))),
        Ok(s) => Err(format!("cargo install {krate} exited with {s}")),
        Err(e) => Err(format!("failed to run cargo install for {krate}: {e}")),
    }
}

/// Returns GitHub's changed filenames only when the compare response is known complete.
/// GitHub caps this array at 300 entries, so 300 entries is inconclusive rather than empty.
fn complete_compare_files(compare: &serde_json::Value) -> Option<Vec<String>> {
    let files = compare["files"].as_array()?;
    if files.len() >= 300 {
        return None;
    }
    files
        .iter()
        .map(|file| file["filename"].as_str().map(str::to_string))
        .collect()
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
        if let Ok(commit) =
            github_get("https://api.github.com/repos/Alexisrx96/physure/commits/main")
        {
            if let Some(latest_sha) = commit["sha"].as_str() {
                let latest_short = &latest_sha[..latest_sha.len().min(7)];
                if latest_short == running_sha {
                    should_rebuild = false;
                    status_note = Some(format!(
                        "phs is already on the latest commit ({running_sha})."
                    ));
                } else {
                    let compare_url = format!(
                        "https://api.github.com/repos/Alexisrx96/physure/compare/{running_sha}...{latest_sha}"
                    );
                    // A failed diagnostic check here is never a reason to refuse an upgrade
                    // the user explicitly asked for -- should_rebuild simply stays true.
                    if let Ok(compare) = github_get(&compare_url) {
                        if let Some(files) = complete_compare_files(&compare) {
                            if !is_relevant_change(&files) {
                                should_rebuild = false;
                                status_note = Some(format!(
                                "main moved ({running_sha} -> {latest_short}) but no relevant crate or workspace build input changed -- phs is already effectively up to date."
                            ));
                            }
                        }
                    }
                }
            }
        }
    }

    if !should_rebuild {
        println!(
            "{}",
            status_note.unwrap_or_else(|| "phs is already up to date.".to_string())
        );
        return;
    }

    let phs_installed = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not determine the running phs's own path: {e}");
            std::process::exit(1);
        }
    };
    let temp_root =
        std::env::temp_dir().join(format!("phs-upgrade-nightly-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_root);
    println!("Building phs from main...");
    let phs_built = match cargo_install_from_main("physure-cli", "phs", &temp_root) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to build phs: {e}");
            let _ = fs::remove_dir_all(&temp_root);
            std::process::exit(1);
        }
    };
    if let Err(e) = replace_binary(&phs_installed, &phs_built) {
        eprintln!("Failed to replace phs: {e}");
        let _ = fs::remove_dir_all(&temp_root);
        std::process::exit(1);
    }

    let exe = if cfg!(windows) { ".exe" } else { "" };
    let lsp_name = format!("physure-lsp{exe}");
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
        Some(target) => {
            println!("Building physure-lsp from main...");
            let result = cargo_install_from_main("physure-lsp", "physure-lsp", &temp_root)
                .and_then(|built| replace_binary(&target, &built));
            if let Err(e) = result {
                eprintln!(
                    "phs updated, but failed to replace physure-lsp: {e} (try again after closing any editor using it)"
                );
            }
        }
        None => println!("physure-lsp not found alongside phs or in ~/.cargo/bin; skipped."),
    }

    let _ = fs::remove_dir_all(&temp_root);
    println!("Done. Run `phs --version` to confirm the new commit.");
}

/// The four crate directories that actually compile into `phs`/`physure-lsp`. Both binaries
/// depend directly on `physure-script`, which implements the language itself (parser,
/// interpreter, grammar), so it belongs in this set alongside the more obvious
/// `physure-core`, `physure-cli`, and `physure-lsp` -- leaving it out would miss most of
/// what a typical commit to this repo changes.
const RELEVANT_PATH_PREFIXES: &[&str] = &[
    "physure-core/",
    "physure-script/",
    "physure-cli/",
    "physure-lsp/",
];

/// Parses the version out of a `core-vX.Y.Z` release tag. `None` for any other tag shape --
/// this repo also publishes plain `vX.Y.Z` (Python package) and `py-vX.Y.Z` releases, which
/// `phs upgrade` must ignore.
fn parse_core_tag_version(tag: &str) -> Option<semver::Version> {
    tag.strip_prefix("core-v")
        .and_then(|v| semver::Version::parse(v).ok())
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

/// True if any changed file falls under one of the crates that compile into
/// `phs`/`physure-lsp`, or is a root workspace manifest/lockfile that controls their build.
fn is_relevant_change(files: &[String]) -> bool {
    files.iter().any(|f| {
        f == "Cargo.toml"
            || f == "Cargo.lock"
            || RELEVANT_PATH_PREFIXES.iter().any(|p| f.starts_with(p))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_core_tag_version_reads_a_core_tag() {
        assert_eq!(
            parse_core_tag_version("core-v0.2.4"),
            Some(semver::Version::new(0, 2, 4))
        );
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
        assert_eq!(
            asset_name_for("windows", "x86_64"),
            Some("phs-windows-x86_64.zip")
        );
        assert_eq!(
            asset_name_for("linux", "x86_64"),
            Some("phs-linux-x86_64.tar.gz")
        );
        assert_eq!(
            asset_name_for("linux", "aarch64"),
            Some("phs-linux-aarch64.tar.gz")
        );
        assert_eq!(
            asset_name_for("macos", "x86_64"),
            Some("phs-macos-x86_64.tar.gz")
        );
        assert_eq!(
            asset_name_for("macos", "aarch64"),
            Some("phs-macos-aarch64.tar.gz")
        );
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
        assert!(is_relevant_change(&[
            "physure-core/src/quantity.rs".to_string()
        ]));
        assert!(is_relevant_change(&[
            "physure-script/src/interpreter/mod.rs".to_string()
        ]));
        assert!(is_relevant_change(&["physure-cli/src/main.rs".to_string()]));
        assert!(is_relevant_change(&[
            "physure-lsp/src/incremental.rs".to_string()
        ]));
    }

    #[test]
    fn is_relevant_change_false_for_unrelated_paths() {
        assert!(!is_relevant_change(&[
            "physure-python/physure/__init__.py".to_string()
        ]));
        assert!(!is_relevant_change(&[
            "docs/tutorials/phs_primer.md".to_string()
        ]));
        assert!(!is_relevant_change(&["README.md".to_string()]));
        assert!(!is_relevant_change(&[]));
    }

    #[test]
    fn is_relevant_change_true_for_workspace_build_inputs() {
        assert!(is_relevant_change(&["Cargo.toml".to_string()]));
        assert!(is_relevant_change(&["Cargo.lock".to_string()]));
    }

    #[test]
    fn complete_compare_files_rejects_incomplete_or_malformed_lists() {
        assert!(complete_compare_files(&serde_json::json!({})).is_none());
        assert!(complete_compare_files(&serde_json::json!({"files": null})).is_none());
        assert!(complete_compare_files(&serde_json::json!({"files": [{}]})).is_none());
        assert!(complete_compare_files(&serde_json::json!({
            "files": (0..300).map(|i| serde_json::json!({"filename": format!("docs/{i}")})).collect::<Vec<_>>()
        }))
        .is_none());
    }

    #[test]
    fn complete_compare_files_accepts_a_complete_filename_list() {
        assert_eq!(
            complete_compare_files(&serde_json::json!({
                "files": [{"filename": "README.md"}, {"filename": "Cargo.lock"}]
            })),
            Some(vec!["README.md".to_string(), "Cargo.lock".to_string()])
        );
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
        let overwrite_err = fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap_err();
        assert_eq!(
            overwrite_err.raw_os_error(),
            Some(32),
            "expected ERROR_SHARING_VIOLATION"
        );

        make_way_for(&target).unwrap();

        assert!(
            !target.exists(),
            "the original path should be free for a new file"
        );
        assert_eq!(
            fs::read(target.with_extension("old")).unwrap(),
            b"old content"
        );

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
        assert_eq!(
            fs::read(target.with_extension("old")).unwrap(),
            b"new content"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn make_way_for_renames_on_every_platform() {
        let dir =
            std::env::temp_dir().join(format!("phs-upgrade-test-portable-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("phs");
        fs::write(&target, b"old content").unwrap();

        make_way_for(&target).unwrap();

        assert!(
            !target.exists(),
            "the original path should be free for a new file"
        );
        assert_eq!(
            fs::read(target.with_extension("old")).unwrap(),
            b"old content"
        );

        // A second call (a second upgrade in a row) must not fail just because a stale
        // .old from the first one is still sitting there.
        fs::write(&target, b"new content").unwrap();
        make_way_for(&target).unwrap();
        assert!(!target.exists());
        assert_eq!(
            fs::read(target.with_extension("old")).unwrap(),
            b"new content"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
