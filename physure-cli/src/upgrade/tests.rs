use super::*;
use std::cell::Cell;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---- helpers: a fresh directory, a local stand-in for GitHub, a real release archive ----------------------------

fn scratch(label: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "phs_upgrade_{label}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A server on a free local port that answers each path with the bytes it is given, and anything else with a 404.
/// `routes` is handed the server's base URL, since the answers may point back at the server.
fn serve_with(routes: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
    let routes = routes(&base);
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let answer = routes.iter().find(|(path, _)| *path == request.url());
            let _ = match answer {
                Some((_, body)) => request.respond(tiny_http::Response::from_data(body.clone())),
                None => request.respond(tiny_http::Response::from_string("no such thing").with_status_code(404)),
            };
        }
    });
    base
}

fn serve(routes: Vec<(&str, Vec<u8>)>) -> String {
    serve_with(|_| routes.into_iter().map(|(path, body)| (path.to_string(), body)).collect())
}

/// The URL of a port nothing listens on.
fn nobody_home() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())
}

/// A `.tar.gz` made by the same `tar` the upgrade extracts with, holding the given files.
fn archive_of(files: &[(&str, &str)]) -> Vec<u8> {
    let source = scratch("archive_source");
    for (name, content) in files {
        fs::write(source.join(name), content).unwrap();
    }
    let archive = scratch("archive_out").join("release.tar.gz");
    let status = Command::new(tar_program())
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .args(files.iter().map(|(name, _)| name))
        .status()
        .unwrap();
    assert!(status.success());
    fs::read(archive).unwrap()
}

const ASSET: &str = "phs-linux-x86_64.tar.gz";

fn exe(name: &str) -> String {
    format!("{name}{}", exe_suffix())
}

/// The list of releases GitHub would answer: a Python one in front of `tag`, whose assets are served by `base`.
fn releases_json(base: &str, tag: &str) -> Vec<u8> {
    serde_json::json!([
        {"tag_name": "v9.9.9", "assets": []},
        {"tag_name": tag, "assets": [
            {"name": "another-asset.zip", "browser_download_url": format!("{base}/download/another-asset.zip")},
            {"name": ASSET, "browser_download_url": format!("{base}/download/{ASSET}")}
        ]}
    ])
    .to_string()
    .into_bytes()
}

/// A `phs` 0.3.0 installed in `<dir>/bin`, an upgrade of it that talks to `api`, and the directory.
fn upgrade_of(api: &str, label: &str) -> (Upgrade, PathBuf) {
    let dir = scratch(label);
    let phs = dir.join("bin").join(exe("phs"));
    fs::create_dir_all(phs.parent().unwrap()).unwrap();
    fs::write(&phs, "old phs").unwrap();
    let upgrade = Upgrade {
        api: api.to_string(),
        phs_installed: phs,
        home: Some(dir.join("home")),
        running: semver::Version::new(0, 3, 0),
        os: "linux",
        arch: "x86_64",
        scratch: dir.join("scratch"),
    };
    (upgrade, dir)
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

/// A server that has a release `tag` whose archive holds the given files.
fn server_with_release(tag: &str, files: &[(&str, &str)]) -> String {
    let archive = archive_of(files);
    let tag = tag.to_string();
    serve_with(move |base| {
        vec![
            ("/releases".to_string(), releases_json(base, &tag)),
            (format!("/download/{ASSET}"), archive),
        ]
    })
}

// ---- reading the releases ------------------------------------------------------------------------------------------

fn running() -> semver::Version {
    semver::Version::new(0, 3, 0)
}

fn plan(releases: serde_json::Value, os: &str, arch: &str) -> Result<StablePlan, String> {
    plan_stable_upgrade(&releases, &running(), os, arch)
}

fn release_list(tag: &str, assets: serde_json::Value) -> serde_json::Value {
    serde_json::json!([{"tag_name": "v1.0.0"}, {"tag_name": tag, "assets": assets}])
}

#[test]
fn a_newer_release_is_planned_with_the_asset_of_the_platform() {
    let assets = serde_json::json!([
        {"name": "phs-windows-x86_64.zip", "browser_download_url": "https://example.test/windows"},
        {"name": ASSET, "browser_download_url": "https://example.test/linux"}
    ]);
    let Ok(StablePlan::Install(release)) = plan(release_list("core-v0.4.1", assets), "linux", "x86_64") else {
        panic!("a newer release must be installed");
    };
    assert_eq!(
        (release.version, release.asset_name, release.download_url.as_str()),
        (semver::Version::new(0, 4, 1), ASSET, "https://example.test/linux")
    );
}

#[test]
fn a_release_that_is_not_newer_leaves_nothing_to_do() {
    let note = |tag: &str| match plan(release_list(tag, serde_json::json!([])), "linux", "x86_64") {
        Ok(StablePlan::Nothing(note)) => note,
        _ => panic!("{tag} must leave nothing to do"),
    };
    assert_eq!(note("core-v0.3.0"), "phs is already up to date (0.3.0).");
    assert!(note("core-v0.2.9").contains("is newer than the latest published release (0.2.9)"));
}

#[test]
fn what_cannot_be_installed_is_said_why() {
    let error = |releases: serde_json::Value, os: &str, arch: &str| plan(releases, os, arch).err().unwrap();
    let good_assets = serde_json::json!([{"name": ASSET, "browser_download_url": "https://example.test/linux"}]);
    assert_eq!(error(serde_json::json!({"message": "rate limited"}), "linux", "x86_64"), "Unexpected response from GitHub releases API.");
    assert_eq!(error(serde_json::json!([{"tag_name": "v1.0.0"}, {}]), "linux", "x86_64"), "No core-v* release found on GitHub.");
    assert!(error(release_list("core-vnotaversion", good_assets.clone()), "linux", "x86_64").contains("Could not parse a version from release tag 'core-vnotaversion'"));
    assert!(error(release_list("core-v0.4.0", good_assets.clone()), "freebsd", "x86_64").contains("No prebuilt binary for freebsd/x86_64"));
    assert_eq!(error(release_list("core-v0.4.0", serde_json::json!([])), "linux", "x86_64"), format!("Release core-v0.4.0 has no asset named '{ASSET}'."));
    assert_eq!(error(release_list("core-v0.4.0", serde_json::json!(null)), "linux", "x86_64"), format!("Release core-v0.4.0 has no asset named '{ASSET}'."));
    assert_eq!(error(release_list("core-v0.4.0", serde_json::json!([{"name": ASSET}])), "linux", "x86_64"), format!("Release asset '{ASSET}' has no download URL."));
}

// ---- the requests --------------------------------------------------------------------------------------------------

#[test]
fn a_request_returns_the_body_and_says_why_it_failed() {
    let base = serve(vec![("/json", br#"{"a": 1}"#.to_vec()), ("/words", b"not json".to_vec())]);
    assert_eq!(get(&format!("{base}/words")).unwrap().into_string().unwrap(), "not json");
    assert_eq!(github_get(&format!("{base}/json")).unwrap()["a"], 1);

    let missing = get(&format!("{base}/nowhere")).err().unwrap();
    assert!(missing.contains("failed: 404 no such thing"), "the answer of the server is part of the error: {missing}");
    let invalid = github_get(&format!("{base}/words")).err().unwrap();
    assert!(invalid.contains("invalid JSON response"), "{invalid}");
    let unreachable = get(&format!("{}/x", nobody_home())).err().unwrap();
    assert!(unreachable.contains("failed") && !unreachable.contains("404"), "{unreachable}");
}

#[test]
fn a_download_is_written_where_it_is_asked() {
    let base = serve(vec![("/asset", b"the bytes".to_vec())]);
    let dir = scratch("download");
    let dest = dir.join("asset.bin");
    download_file(&format!("{base}/asset"), &dest).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"the bytes");

    assert!(download_file(&format!("{base}/nowhere"), &dir.join("never")).unwrap_err().contains("404"));
    assert!(!dir.join("never").exists(), "nothing is written for a download that failed");
    let unwritable = download_file(&format!("{base}/asset"), &dir.join("no_such_dir").join("asset.bin")).unwrap_err();
    assert!(unwritable.starts_with("creating "), "{unwritable}");
}

// ---- the files -----------------------------------------------------------------------------------------------------

#[test]
fn tar_is_the_one_that_reads_windows_paths() {
    let tar = tar_program();
    if cfg!(windows) {
        assert!(tar.ends_with("System32/tar.exe") || tar.ends_with("System32\\tar.exe"), "{tar:?}");
        assert!(tar.exists());
    } else {
        assert_eq!(tar, PathBuf::from("tar"));
    }
}

#[test]
fn an_archive_is_extracted_and_what_is_not_one_is_refused() {
    let dir = scratch("extract");
    let archive = dir.join("release.tar.gz");
    fs::write(&archive, archive_of(&[("phs", "inside")])).unwrap();
    let out = dir.join("out").join("deeper");
    extract_archive(&archive, &out).unwrap();
    assert_eq!(text(&out.join("phs")), "inside");

    let not_an_archive = dir.join("notes.txt");
    fs::write(&not_an_archive, "just words").unwrap();
    let error = extract_archive(&not_an_archive, &dir.join("nothing")).unwrap_err();
    assert!(error.contains("tar exited with"), "{error}");
    // the destination cannot be made when a file is in its way
    assert!(extract_archive(&archive, &not_an_archive.join("inside")).unwrap_err().starts_with("creating "));
}

#[test]
fn a_binary_is_replaced_by_the_new_one_and_the_old_one_does_not_stay() {
    let dir = scratch("replace");
    let installed = dir.join(exe("phs"));
    let fresh = dir.join("fresh");
    fs::write(&installed, "old").unwrap();
    fs::write(&fresh, "new").unwrap();

    replace_binary(&installed, &fresh).unwrap();

    assert_eq!(text(&installed), "new");
    assert!(!installed.with_extension("old").exists() && !installed.with_extension("new").exists());
    assert_eq!(text(&fresh), "new", "the new content is copied, not moved");
}

#[test]
fn a_binary_is_installed_where_there_was_none() {
    let dir = scratch("install_fresh");
    let fresh = dir.join("fresh");
    fs::write(&fresh, "new").unwrap();
    let installed = dir.join("not").join("yet").join(exe("phs"));
    replace_binary(&installed, &fresh).unwrap();
    assert_eq!(text(&installed), "new");
}

#[test]
fn a_replacement_that_fails_gives_the_previous_binary_back() {
    let dir = scratch("replace_fails");
    let installed = dir.join(exe("phs"));
    fs::write(&installed, "old").unwrap();

    let error = replace_binary(&installed, &dir.join("does_not_exist")).unwrap_err();

    assert!(error.contains("copying") && error.ends_with("(restored the previous binary)"), "{error}");
    assert_eq!(text(&installed), "old");
    assert!(!installed.with_extension("old").exists() && !installed.with_extension("new").exists());
}

#[test]
fn the_lsp_to_replace_is_the_one_next_to_phs_or_else_the_one_of_cargo() {
    let dir = scratch("lsp_path");
    let name = exe("physure-lsp");
    let phs = dir.join("bin").join(exe("phs"));
    let home = dir.join("home");
    fs::create_dir_all(phs.parent().unwrap()).unwrap();
    assert_eq!(installed_lsp_path(&phs, &name, Some(&home)), None);
    assert_eq!(installed_lsp_path(&phs, &name, None), None);

    let in_cargo = home.join(".cargo").join("bin").join(&name);
    fs::create_dir_all(in_cargo.parent().unwrap()).unwrap();
    fs::write(&in_cargo, "cargo's").unwrap();
    assert_eq!(installed_lsp_path(&phs, &name, Some(&home)), Some(in_cargo));
    assert_eq!(installed_lsp_path(&phs, &name, None), None);

    let beside = phs.with_file_name(&name);
    fs::write(&beside, "beside").unwrap();
    assert_eq!(installed_lsp_path(&phs, &name, Some(&home)), Some(beside));
}

// ---- the default upgrade, end to end against a local stand-in for GitHub ------------------------------------------

#[test]
fn a_newer_release_replaces_phs_and_the_lsp_next_to_it() {
    let base = server_with_release("core-v0.4.0", &[(&exe("phs"), "new phs"), (&exe("physure-lsp"), "new lsp")]);
    let (upgrade, dir) = upgrade_of(&base, "stable_both");
    let lsp = dir.join("bin").join(exe("physure-lsp"));
    fs::write(&lsp, "old lsp").unwrap();

    stable_upgrade(&upgrade).unwrap();

    assert_eq!((text(&upgrade.phs_installed).as_str(), text(&lsp).as_str()), ("new phs", "new lsp"));
    assert!(!upgrade.phs_installed.with_extension("old").exists() && !lsp.with_extension("old").exists());
    assert!(!upgrade.scratch.exists(), "the downloads are cleaned up");
}

#[test]
fn the_lsp_of_cargo_is_replaced_when_none_is_next_to_phs() {
    let base = server_with_release("core-v0.4.0", &[(&exe("phs"), "new phs"), (&exe("physure-lsp"), "new lsp")]);
    let (upgrade, _) = upgrade_of(&base, "stable_cargo_lsp");
    let in_cargo = upgrade.home.as_ref().unwrap().join(".cargo").join("bin").join(exe("physure-lsp"));
    fs::create_dir_all(in_cargo.parent().unwrap()).unwrap();
    fs::write(&in_cargo, "old lsp").unwrap();

    stable_upgrade(&upgrade).unwrap();

    assert_eq!(text(&in_cargo), "new lsp");
}

#[test]
fn an_lsp_that_is_nowhere_is_skipped_and_a_release_without_one_leaves_it_alone() {
    let (upgrade, dir) = upgrade_of("unused", "stable_no_lsp_target");
    let with_lsp = server_with_release("core-v0.4.0", &[(&exe("phs"), "new phs"), (&exe("physure-lsp"), "new lsp")]);
    let upgrade = Upgrade { api: with_lsp, ..upgrade };
    stable_upgrade(&upgrade).unwrap();
    assert_eq!(text(&upgrade.phs_installed), "new phs");
    assert!(!dir.join("bin").join(exe("physure-lsp")).exists(), "an lsp is not installed where there was none");

    let without_lsp = server_with_release("core-v0.5.0", &[(&exe("phs"), "newer phs")]);
    let lsp = dir.join("bin").join(exe("physure-lsp"));
    fs::write(&lsp, "old lsp").unwrap();
    let upgrade = Upgrade { api: without_lsp, running: semver::Version::new(0, 4, 0), ..upgrade };
    stable_upgrade(&upgrade).unwrap();
    assert_eq!((text(&upgrade.phs_installed).as_str(), text(&lsp).as_str()), ("newer phs", "old lsp"));
}

#[test]
fn an_lsp_that_cannot_be_replaced_does_not_undo_the_upgrade_of_phs() {
    let base = server_with_release("core-v0.4.0", &[(&exe("phs"), "new phs"), (&exe("physure-lsp"), "new lsp")]);
    let (upgrade, dir) = upgrade_of(&base, "stable_lsp_fails");
    let lsp = dir.join("bin").join(exe("physure-lsp"));
    fs::write(&lsp, "old lsp").unwrap();
    // a directory where the staged copy of the lsp goes
    fs::create_dir_all(lsp.with_extension("new")).unwrap();

    stable_upgrade(&upgrade).unwrap();

    assert_eq!((text(&upgrade.phs_installed).as_str(), text(&lsp).as_str()), ("new phs", "old lsp"));
}

#[test]
fn a_release_that_is_not_newer_changes_nothing() {
    let base = server_with_release("core-v0.3.0", &[(&exe("phs"), "same phs")]);
    let (upgrade, _) = upgrade_of(&base, "stable_same");
    stable_upgrade(&upgrade).unwrap();
    assert_eq!(text(&upgrade.phs_installed), "old phs");
    assert!(!upgrade.scratch.exists());
}

#[test]
fn an_upgrade_that_cannot_finish_says_where_it_stopped_and_keeps_phs() {
    let (upgrade, dir) = upgrade_of(&nobody_home(), "stable_fails");
    assert!(stable_upgrade(&upgrade).unwrap_err().starts_with("Failed to check for updates: GET "));

    // the release is listed but its asset is not served
    let missing_asset = serve_with(|base| vec![("/releases".to_string(), releases_json(base, "core-v0.4.0"))]);
    let upgrade = Upgrade { api: missing_asset, ..upgrade };
    let error = stable_upgrade(&upgrade).unwrap_err();
    assert!(error.starts_with("Download failed: ") && error.contains("404"), "{error}");

    // an asset that is not an archive
    let garbage = serve_with(|base| {
        vec![("/releases".to_string(), releases_json(base, "core-v0.4.0")), (format!("/download/{ASSET}"), b"not an archive".to_vec())]
    });
    let upgrade = Upgrade { api: garbage, ..upgrade };
    assert!(stable_upgrade(&upgrade).unwrap_err().starts_with("Extraction failed: "));

    // an archive without a phs in it
    let no_phs = server_with_release("core-v0.4.0", &[("README", "no binary here")]);
    let upgrade = Upgrade { api: no_phs, ..upgrade };
    let error = stable_upgrade(&upgrade).unwrap_err();
    assert!(error.starts_with("Failed to replace phs: ") && error.ends_with("(restored the previous binary)"), "{error}");

    assert_eq!(text(&upgrade.phs_installed), "old phs");
    assert!(!upgrade.scratch.exists(), "the downloads are cleaned up when the upgrade fails too");

    // a scratch directory that cannot be made
    let blocker = dir.join("blocker");
    fs::write(&blocker, "a file").unwrap();
    let base = server_with_release("core-v0.4.0", &[(&exe("phs"), "new phs")]);
    let upgrade = Upgrade { api: base, scratch: blocker.join("scratch"), ..upgrade };
    assert!(stable_upgrade(&upgrade).unwrap_err().starts_with("Failed to create temp directory "));
}

// ---- the nightly upgrade -------------------------------------------------------------------------------------------

/// What `cargo install` leaves behind, made up: `<root>/bin/<bin>` holding the name of the crate it was built from.
fn fake_install(krate: &str, bin: &str, root: &Path) -> Result<PathBuf, String> {
    let built = root.join("bin").join(exe(bin));
    fs::create_dir_all(built.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&built, format!("built {krate}")).map_err(|e| e.to_string())?;
    Ok(built)
}

fn commit_and_compare(running: &str, latest: &str, files: serde_json::Value) -> String {
    serve(vec![
        ("/commits/main", serde_json::json!({"sha": latest}).to_string().into_bytes()),
        (&format!("/compare/{running}...{latest}"), serde_json::json!({"files": files}).to_string().into_bytes()),
    ])
}

const LATEST: &str = "1234567890abcdef";

#[test]
fn the_nightly_note_says_phs_is_current_when_main_has_nothing_to_build() {
    // nothing is known of the running build
    assert_eq!(nightly_up_to_date_note("unused", None), None);
    // GitHub cannot be asked
    assert_eq!(nightly_up_to_date_note(&nobody_home(), Some("abc1234")), None);
    assert_eq!(nightly_up_to_date_note(&serve(vec![("/commits/main", b"{}".to_vec())]), Some("abc1234")), None);

    let same = serve(vec![("/commits/main", serde_json::json!({"sha": LATEST}).to_string().into_bytes())]);
    assert_eq!(nightly_up_to_date_note(&same, Some("1234567")).unwrap(), "phs is already on the latest commit (1234567).");

    let only_docs = commit_and_compare("abc1234", LATEST, serde_json::json!([{"filename": "README.md"}, {"filename": "docs/a.md"}]));
    let note = nightly_up_to_date_note(&only_docs, Some("abc1234")).unwrap();
    assert!(note.starts_with("main moved (abc1234 -> 1234567) but no relevant crate"), "{note}");

    let relevant = commit_and_compare("abc1234", LATEST, serde_json::json!([{"filename": "physure-core/src/lib.rs"}]));
    assert_eq!(nightly_up_to_date_note(&relevant, Some("abc1234")), None);
    let too_many: Vec<_> = (0..300).map(|i| serde_json::json!({"filename": format!("docs/{i}.md")})).collect();
    let inconclusive = commit_and_compare("abc1234", LATEST, serde_json::Value::Array(too_many));
    assert_eq!(nightly_up_to_date_note(&inconclusive, Some("abc1234")), None);
    // the commit is there but the comparison is not
    let no_compare = serve(vec![("/commits/main", serde_json::json!({"sha": LATEST}).to_string().into_bytes())]);
    assert_eq!(nightly_up_to_date_note(&no_compare, Some("abc1234")), None);
}

#[test]
fn a_nightly_upgrade_builds_phs_and_the_lsp_beside_it_from_main() {
    let (upgrade, dir) = upgrade_of(&nobody_home(), "nightly_both");
    let lsp = dir.join("bin").join(exe("physure-lsp"));
    fs::write(&lsp, "old lsp").unwrap();

    nightly_upgrade(&upgrade, true, &fake_install, None).unwrap();

    assert_eq!((text(&upgrade.phs_installed).as_str(), text(&lsp).as_str()), ("built physure-cli", "built physure-lsp"));
    assert!(!upgrade.scratch.exists(), "the builds are cleaned up");
}

#[test]
fn a_nightly_upgrade_does_not_build_an_lsp_there_is_none_to_replace() {
    let (upgrade, _) = upgrade_of(&nobody_home(), "nightly_no_lsp");
    let builds = Cell::new(0);
    let counting = |krate: &str, bin: &str, root: &Path| {
        builds.set(builds.get() + 1);
        fake_install(krate, bin, root)
    };

    nightly_upgrade(&upgrade, true, &counting, None).unwrap();

    assert_eq!((text(&upgrade.phs_installed).as_str(), builds.get()), ("built physure-cli", 1));
}

#[test]
fn a_nightly_upgrade_stops_when_there_is_no_cargo_or_nothing_new() {
    let never = |_: &str, _: &str, _: &Path| -> Result<PathBuf, String> { panic!("nothing may be built") };
    let (upgrade, _) = upgrade_of(&nobody_home(), "nightly_nothing");
    assert!(nightly_upgrade(&upgrade, false, &never, None).unwrap_err().starts_with("cargo not found on PATH"));

    let same = serve(vec![("/commits/main", serde_json::json!({"sha": LATEST}).to_string().into_bytes())]);
    let upgrade = Upgrade { api: same, ..upgrade };
    nightly_upgrade(&upgrade, true, &never, Some("1234567")).unwrap();
    assert_eq!(text(&upgrade.phs_installed), "old phs");
}

#[test]
fn a_nightly_upgrade_that_fails_keeps_phs_and_an_lsp_that_fails_does_not_undo_it() {
    let (upgrade, dir) = upgrade_of(&nobody_home(), "nightly_fails");
    let no_build = |_: &str, _: &str, _: &Path| -> Result<PathBuf, String> { Err("no compiler".to_string()) };
    assert_eq!(nightly_upgrade(&upgrade, true, &no_build, None).unwrap_err(), "Failed to build phs: no compiler");

    let builds_nothing = |_: &str, _: &str, root: &Path| -> Result<PathBuf, String> { Ok(root.join("not_built")) };
    let error = nightly_upgrade(&upgrade, true, &builds_nothing, None).unwrap_err();
    assert!(error.starts_with("Failed to replace phs: "), "{error}");
    assert_eq!(text(&upgrade.phs_installed), "old phs");

    let lsp = dir.join("bin").join(exe("physure-lsp"));
    fs::write(&lsp, "old lsp").unwrap();
    let only_phs = |krate: &str, bin: &str, root: &Path| {
        if krate == "physure-lsp" { Err("lsp does not build".to_string()) } else { fake_install(krate, bin, root) }
    };
    nightly_upgrade(&upgrade, true, &only_phs, None).unwrap();
    assert_eq!((text(&upgrade.phs_installed).as_str(), text(&lsp).as_str()), ("built physure-cli", "old lsp"));
}

// ---- cargo, and the program that is running ------------------------------------------------------------------------

#[test]
fn cargo_is_asked_to_build_the_binary_from_main_into_the_root() {
    let args = |krate: &str, bin: &str| -> Vec<String> {
        let command = cargo_install_command(krate, bin, Path::new("root"));
        assert_eq!(command.get_program(), "cargo");
        command.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    };
    let same_name = args("physure-lsp", "physure-lsp");
    assert_eq!(&same_name[..6], ["install", "--git", "https://github.com/Alexisrx96/physure", "--branch", "main", "physure-lsp"]);
    assert_eq!(&same_name[6..], ["--locked", "--force", "--root", "root"]);
    let other_bin = args("physure-cli", "phs");
    assert_eq!(&other_bin[6..8], ["--bin", "phs"]);
}

#[test]
fn an_install_says_where_the_binary_is_or_why_there_is_none() {
    let root = Path::new("root");
    // this very test program stands for cargo: it succeeds when asked to list no tests, and fails on a flag it lacks
    let this_program = |args: &[&str]| {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(args).stdout(Stdio::null()).stderr(Stdio::null());
        command
    };
    let built = run_install(this_program(&["--list", "no_test_has_this_name"]), "physure-cli", "phs", root).unwrap();
    assert_eq!(built, root.join("bin").join(exe("phs")));
    let failed = run_install(this_program(&["--no-such-flag"]), "physure-cli", "phs", root).unwrap_err();
    assert!(failed.starts_with("cargo install physure-cli exited with "), "{failed}");
    let missing = run_install(Command::new(scratch("no_cargo").join("cargo")), "physure-cli", "phs", root).unwrap_err();
    assert!(missing.starts_with("failed to run cargo install for physure-cli: "), "{missing}");
}

#[test]
fn cargo_is_found_when_it_can_be_run() {
    let runs = Command::new("cargo").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    assert_eq!(which_cargo_found(), runs);
}

#[test]
fn the_real_upgrade_replaces_the_running_phs_of_this_platform() {
    let upgrade = Upgrade::current().unwrap();
    assert_eq!(upgrade.phs_installed, std::env::current_exe().unwrap());
    assert_eq!(upgrade.api, GITHUB_API);
    assert_eq!(upgrade.running.to_string(), env!("CARGO_PKG_VERSION"));
    // this repo's CI and dev machines are always one of the five targets `core-release.yml` builds for
    assert!(asset_name_for(upgrade.os, upgrade.arch).is_some());
}

// ---- the small decisions -------------------------------------------------------------------------------------------

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

    let dir = scratch("make_way_locked");
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
}

#[test]
fn make_way_for_renames_on_every_platform() {
    let dir = scratch("make_way_portable");
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
}
