//! `phs upgrade` -- self-updates the `phs`/`physure-lsp` binaries. Two modes: the default
//! downloads the latest published `core-v*` GitHub release binary for the current platform;
//! `--nightly` rebuilds from `main`'s current tip via `cargo install`, but only when a commit
//! that actually changed one of the crates that compile into these binaries has landed since
//! the running build.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const GITHUB_API: &str = "https://api.github.com/repos/Alexisrx96/physure";

/// What an upgrade works on: where GitHub is, which `phs` is replaced and which version it is, where a
/// `physure-lsp` is looked for besides next to it, and the scratch directory the downloads and builds use.
/// `Upgrade::current()` is the real thing; a test builds its own around a local server and a temporary directory.
struct Upgrade {
    api: String,
    phs_installed: PathBuf,
    home: Option<PathBuf>,
    running: semver::Version,
    os: &'static str,
    arch: &'static str,
    scratch: PathBuf,
}

impl Upgrade {
    fn current() -> Result<Self, String> {
        let phs_installed = std::env::current_exe()
            .map_err(|e| format!("Could not determine the running phs's own path: {e}"))?;
        Ok(Upgrade {
            api: GITHUB_API.to_string(),
            phs_installed,
            home: dirs::home_dir(),
            running: semver::Version::parse(env!("CARGO_PKG_VERSION"))
                .expect("CARGO_PKG_VERSION is always valid semver"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            scratch: std::env::temp_dir().join(format!("phs-upgrade-{}", std::process::id())),
        })
    }
}

/// Builds `krate`'s `bin` from `main` into the given root and says where the binary ended up.
type Installer<'a> = &'a dyn Fn(&str, &str, &Path) -> Result<PathBuf, String>;

pub fn run_upgrade(nightly: bool) {
    let outcome = Upgrade::current().and_then(|upgrade| {
        if nightly {
            nightly_upgrade(
                &upgrade,
                which_cargo_found(),
                &cargo_install_from_main,
                option_env!("PHS_BUILD_SHA"),
            )
        } else {
            stable_upgrade(&upgrade)
        }
    });
    if let Err(e) = outcome {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn exe_suffix() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

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

/// The `tar` that reads both release archive types. On Windows that is the bsdtar that ships in
/// `System32` (Windows 10 1803+), named by its full path: a `tar` found on `PATH` may be the GNU
/// one that comes with Git for Windows, and that one takes the `C:` of `C:\dir\file` for a remote
/// host ("Cannot connect to C: resolve failed").
fn tar_program() -> PathBuf {
    if cfg!(windows) {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let system_tar = Path::new(&root).join("System32").join("tar.exe");
            if system_tar.exists() {
                return system_tar;
            }
        }
    }
    PathBuf::from("tar")
}

/// Extracts `archive` (`.zip` or `.tar.gz`) into `dest_dir` by shelling out to the system
/// `tar` -- bsdtar, which every target platform ships by default (Windows 10 1803+, macOS,
/// Linux), reads both archive types through the same `-xf`, so this needs no archive-handling
/// crate.
fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("creating {}: {e}", dest_dir.display()))?;
    let status = Command::new(tar_program())
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

/// The release the default upgrade installs: its version, the asset built for this platform and where to fetch it.
struct Release {
    version: semver::Version,
    asset_name: &'static str,
    download_url: String,
}

/// What `plan_stable_upgrade` makes of GitHub's list of releases: nothing to do (and the words to tell the user),
/// or a release to install.
enum StablePlan {
    Nothing(String),
    Install(Release),
}

/// Reads GitHub's list of releases (newest first) the way the default upgrade needs it: the latest `core-v*`
/// release, whether it is newer than `running`, and -- if it is -- the asset for `os`/`arch` with its URL.
fn plan_stable_upgrade(
    releases: &serde_json::Value,
    running: &semver::Version,
    os: &str,
    arch: &str,
) -> Result<StablePlan, String> {
    let releases = releases
        .as_array()
        .ok_or("Unexpected response from GitHub releases API.")?;
    let release = releases
        .iter()
        .find(|r| r["tag_name"].as_str().is_some_and(|t| t.starts_with("core-v")))
        .ok_or("No core-v* release found on GitHub.")?;
    let tag = release["tag_name"].as_str().unwrap_or("");
    let latest = parse_core_tag_version(tag)
        .ok_or_else(|| format!("Could not parse a version from release tag '{tag}'."))?;

    match latest.cmp(running) {
        std::cmp::Ordering::Equal => {
            return Ok(StablePlan::Nothing(format!(
                "phs is already up to date ({running})."
            )))
        }
        std::cmp::Ordering::Less => {
            return Ok(StablePlan::Nothing(format!(
                "Running {running} is newer than the latest published release ({latest}) -- probably a --nightly build. Nothing to do."
            )))
        }
        std::cmp::Ordering::Greater => {}
    }

    let asset_name = asset_name_for(os, arch).ok_or_else(|| {
        format!("No prebuilt binary for {os}/{arch}. Try `phs upgrade --nightly` (requires cargo) or install manually.")
    })?;
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a["name"].as_str() == Some(asset_name))
        })
        .ok_or_else(|| format!("Release {tag} has no asset named '{asset_name}'."))?;
    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| format!("Release asset '{asset_name}' has no download URL."))?;
    Ok(StablePlan::Install(Release {
        version: latest,
        asset_name,
        download_url: download_url.to_string(),
    }))
}

/// The default (non-`--nightly`) upgrade flow: checks GitHub for the latest `core-v*` release,
/// and if it's newer than the running `phs`, downloads and extracts the platform-appropriate
/// asset, then replaces the running `phs` binary and (if present alongside it or in
/// `~/.cargo/bin`) `physure-lsp`. Prints progress and fails on anything that prevents `phs`
/// itself from being updated; a `physure-lsp` replace failure alone is reported but non-fatal.
fn stable_upgrade(upgrade: &Upgrade) -> Result<(), String> {
    println!("Checking for the latest release...");
    let releases = github_get(&format!("{}/releases", upgrade.api))
        .map_err(|e| format!("Failed to check for updates: {e}"))?;
    let release = match plan_stable_upgrade(
        &releases,
        &upgrade.running,
        upgrade.os,
        upgrade.arch,
    )? {
        StablePlan::Nothing(note) => {
            println!("{note}");
            return Ok(());
        }
        StablePlan::Install(release) => release,
    };

    fs::create_dir_all(&upgrade.scratch).map_err(|e| {
        format!(
            "Failed to create temp directory {}: {e}",
            upgrade.scratch.display()
        )
    })?;
    let result = install_release(upgrade, &release);
    let _ = fs::remove_dir_all(&upgrade.scratch);
    result
}

/// Downloads and unpacks `release` and puts it in place of the installed `phs` and, if there is one, `physure-lsp`.
fn install_release(upgrade: &Upgrade, release: &Release) -> Result<(), String> {
    let archive_path = upgrade.scratch.join(release.asset_name);
    println!("Downloading {}...", release.asset_name);
    download_file(&release.download_url, &archive_path)
        .map_err(|e| format!("Download failed: {e}"))?;

    let extract_dir = upgrade.scratch.join("extracted");
    extract_archive(&archive_path, &extract_dir).map_err(|e| format!("Extraction failed: {e}"))?;

    let phs_extracted = extract_dir.join(format!("phs{}", exe_suffix()));
    replace_binary(&upgrade.phs_installed, &phs_extracted)
        .map_err(|e| format!("Failed to replace phs: {e}"))?;
    println!("phs: {} -> {}", upgrade.running, release.version);

    replace_lsp(upgrade, &extract_dir, &release.version);
    Ok(())
}

/// Puts the `physure-lsp` of an extracted release in place of the installed one, when the release has one.
/// A failure here is told to the user but is not the upgrade's: `phs` itself is already updated.
fn replace_lsp(upgrade: &Upgrade, extract_dir: &Path, version: &semver::Version) {
    let lsp_name = format!("physure-lsp{}", exe_suffix());
    let lsp_extracted = extract_dir.join(&lsp_name);
    if !lsp_extracted.exists() {
        return;
    }
    let target = installed_lsp_path(&upgrade.phs_installed, &lsp_name, upgrade.home.as_deref());
    match target {
        Some(target) => match replace_binary(&target, &lsp_extracted) {
            Ok(()) => println!("physure-lsp: updated to {version}"),
            Err(e) => eprintln!(
                "phs updated, but failed to replace physure-lsp: {e} (try again after closing any editor using it)"
            ),
        },
        None => println!("physure-lsp not found alongside phs or in ~/.cargo/bin; skipped."),
    }
}

fn which_cargo_found() -> bool {
    let lookup = if cfg!(windows) { "where" } else { "which" };
    Command::new(lookup)
        .arg("cargo")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The `cargo install` that builds `krate` from the tip of `main` into `install_root`.
fn cargo_install_command(krate: &str, bin: &str, install_root: &Path) -> Command {
    let mut cmd = Command::new("cargo");
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
    cmd
}

fn cargo_install_from_main(krate: &str, bin: &str, install_root: &Path) -> Result<PathBuf, String> {
    run_install(cargo_install_command(krate, bin, install_root), krate, bin, install_root)
}

/// Runs an install command and says where it left `bin`: under `install_root/bin`.
fn run_install(
    mut cmd: Command,
    krate: &str,
    bin: &str,
    install_root: &Path,
) -> Result<PathBuf, String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(install_root
            .join("bin")
            .join(format!("{bin}{}", exe_suffix()))),
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

/// Why a nightly upgrade is not needed, when it is not: phs already is on the latest commit of main, or main
/// moved only by files nothing is built from. A failed check here is never a reason to refuse an upgrade the
/// user explicitly asked for -- the answer is then `None`, and the upgrade goes on.
fn nightly_up_to_date_note(api: &str, running_sha: Option<&str>) -> Option<String> {
    let running_sha = running_sha?;
    let commit = github_get(&format!("{api}/commits/main")).ok()?;
    let latest_sha = commit["sha"].as_str()?;
    let latest_short = &latest_sha[..latest_sha.len().min(7)];
    if latest_short == running_sha {
        return Some(format!("phs is already on the latest commit ({running_sha})."));
    }
    let compare = github_get(&format!("{api}/compare/{running_sha}...{latest_sha}")).ok()?;
    let files = complete_compare_files(&compare)?;
    if is_relevant_change(&files) {
        return None;
    }
    Some(format!(
        "main moved ({running_sha} -> {latest_short}) but no relevant crate or workspace build input changed -- phs is already effectively up to date."
    ))
}

/// The installed `physure-lsp` to replace: next to `phs`, or else in `home`'s `.cargo/bin`.
fn installed_lsp_path(phs_installed: &Path, lsp_name: &str, home: Option<&Path>) -> Option<PathBuf> {
    phs_installed
        .parent()
        .map(|dir| dir.join(lsp_name))
        .filter(|p| p.exists())
        .or_else(|| {
            home.map(|h| h.join(".cargo").join("bin").join(lsp_name))
                .filter(|p| p.exists())
        })
}

/// The `--nightly` upgrade flow: unless `phs` is already as new as `main` (or `main` changed nothing it is built
/// from), builds `phs` -- and `physure-lsp`, if there is one to replace -- from `main` with `install` and puts them
/// in place of the installed ones.
fn nightly_upgrade(
    upgrade: &Upgrade,
    cargo_found: bool,
    install: Installer,
    running_sha: Option<&str>,
) -> Result<(), String> {
    if !cargo_found {
        return Err("cargo not found on PATH. Install Rust from https://rustup.rs then re-run `phs upgrade --nightly`.".to_string());
    }
    if let Some(note) = nightly_up_to_date_note(&upgrade.api, running_sha) {
        println!("{note}");
        return Ok(());
    }

    let _ = fs::remove_dir_all(&upgrade.scratch);
    let result = nightly_install(upgrade, install);
    let _ = fs::remove_dir_all(&upgrade.scratch);
    result
}

fn nightly_install(upgrade: &Upgrade, install: Installer) -> Result<(), String> {
    println!("Building phs from main...");
    let phs_built = install("physure-cli", "phs", &upgrade.scratch)
        .map_err(|e| format!("Failed to build phs: {e}"))?;
    replace_binary(&upgrade.phs_installed, &phs_built)
        .map_err(|e| format!("Failed to replace phs: {e}"))?;

    let lsp_name = format!("physure-lsp{}", exe_suffix());
    match installed_lsp_path(&upgrade.phs_installed, &lsp_name, upgrade.home.as_deref()) {
        Some(target) => {
            println!("Building physure-lsp from main...");
            let result = install("physure-lsp", "physure-lsp", &upgrade.scratch)
                .and_then(|built| replace_binary(&target, &built));
            if let Err(e) = result {
                eprintln!(
                    "phs updated, but failed to replace physure-lsp: {e} (try again after closing any editor using it)"
                );
            }
        }
        None => println!("physure-lsp not found alongside phs or in ~/.cargo/bin; skipped."),
    }
    println!("Done. Run `phs --version` to confirm the new commit.");
    Ok(())
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
mod tests;
