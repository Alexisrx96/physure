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
