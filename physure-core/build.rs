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
#[path = "build_support/git.rs"]
mod git_facts;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    watch(&manifest);

    // two builds whose facts were all "unknown" would compare equal under `build`: a fact that cannot be
    // determined stops the build instead
    println!("cargo:rustc-env=PHYSURE_ENGINE_PROFILE={}", required("PROFILE"));
    println!("cargo:rustc-env=PHYSURE_ENGINE_TARGET={}", required("TARGET"));
    println!("cargo:rustc-env=PHYSURE_ENGINE_RUSTC={}", rustc_version());
    println!("cargo:rustc-env=PHYSURE_ENGINE_FLAGS={}", flags());

    let env_commit = env::var("PHYSURE_BUILD_COMMIT").ok();
    let facts = git_facts::facts(&manifest);
    let decision = commit_rules::decide(&commit_rules::Inputs {
        env: env_commit.as_deref(),
        git_head: facts.head.as_deref(),
        dirty: facts.dirty,
        in_repository: facts.in_repository,
    });
    if let Some(warning) = &decision.warning {
        println!("cargo:warning={warning}");
    }
    println!("cargo:rustc-env=PHYSURE_ENGINE_COMMIT={}", decision.commit);
}

fn required(name: &str) -> String {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!("physure build script: cargo did not set {name}; the engine identity cannot be determined"),
    }
}

/// What can change the answer: only paths that exist are watched (a missing path makes cargo rebuild this
/// crate and everything that depends on it on every invocation, which is what happens in a git worktree if
/// `../.git/HEAD` is named).
fn watch(manifest: &Path) {
    println!("cargo:rerun-if-env-changed=PHYSURE_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    for local in git_facts::CRATE_INPUTS.iter().copied().chain(["../Cargo.toml", "../Cargo.lock"]) {
        if manifest.join(local).exists() {
            println!("cargo:rerun-if-changed={local}");
        }
    }
    // `refs` and `reftable` are directories: cargo watches every file in them, so a new loose ref (a commit on a
    // branch whose ref was packed, with no reflog) is seen, and so is a commit in a repository that stores its refs
    // in the reftable format (where `HEAD` and `refs/` never change)
    let mut git_paths: Vec<String> = ["HEAD", "logs/HEAD", "packed-refs", "refs", "reftable"].map(String::from).to_vec();
    if let Some(reference) = git_facts::answer(manifest, &["rev-parse", "--symbolic-full-name", "HEAD"]).filter(|r| r.starts_with("refs/")) {
        git_paths.push(reference);
    }
    for name in git_paths {
        if let Some(path) = git_facts::answer(manifest, &["rev-parse", "--git-path", &name]) {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() { path } else { manifest.join(path) };
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}

fn rustc_version() -> String {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    Command::new(&rustc)
        .arg("-V")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.lines().next().unwrap_or("").trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| panic!("physure build script: `{rustc} -V` gave no version; the engine identity cannot be determined"))
}

fn flags() -> String {
    let opt = required("OPT_LEVEL");
    let assertions = if env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() { "on" } else { "off" };
    let panic = required("CARGO_CFG_PANIC");
    let rustflags = sha256::sha256_hex(env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default().as_bytes());
    let features = sha256::sha256_hex(env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default().as_bytes());
    format!("opt-level={opt};debug-assertions={assertions};panic={panic};rustflags={rustflags};target-features={features}")
}
