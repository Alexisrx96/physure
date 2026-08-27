use std::process::Command;

/// Captures the short commit SHA this binary was built from, exposed to the crate as the
/// `PHS_BUILD_SHA` env var (read back via `option_env!("PHS_BUILD_SHA")`). `phs --version`
/// shows it, and `phs upgrade --nightly` uses it as the baseline for "did anything relevant
/// change since this build." Silently produces no env var (not a build failure) when `git`
/// isn't available or this isn't a git checkout at all -- the crates.io sdist case.
fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(sha) = sha {
        println!("cargo:rustc-env=PHS_BUILD_SHA={sha}");
    }
    // Re-run on anything that can change what HEAD resolves to. `.git/HEAD` itself only
    // changes on a branch switch or a detached-HEAD commit -- an ordinary `git commit` on
    // the checked-out branch instead updates `.git/refs/heads/<branch>`, which the reflog
    // (`.git/logs/HEAD`) captures on every commit/checkout/merge/amend. Watch both: HEAD
    // for the branch-switch case, logs/HEAD so same-branch commits aren't cached stale.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/logs/HEAD");
}
