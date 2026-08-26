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
