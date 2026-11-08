//! What git says about the tree a build of physure-core comes from: the inputs of the commit rule (`commit.rs`).
//! Shared by `build.rs` and by the test of the running identity (both through `#[path]`), so that the test asks git
//! exactly what the build asked. Every git command runs without the `GIT_*` variables that could point it
//! elsewhere, and a command that fails is a doubt, never a clean answer.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::commit_rules;

/// The output of a git command run in `dir` that **succeeded**, even when it is empty, with `input` written to its
/// standard input; `None` when git could not be run or failed. The two must not be confused: an empty
/// `git status` is a clean tree, a failed one is a doubt.
pub fn output(dir: &Path, args: &[&str], input: Option<&str>) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args).stdout(Stdio::piped()).stderr(Stdio::null());
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    // a hook, a wrapper or a CI step may point git somewhere else
    for name in commit_rules::GIT_ENV_CLEARED {
        command.env_remove(name);
    }
    let mut child = command.spawn().ok()?;
    // written from another thread: git may fill its output pipe before it has read all of its input
    let writer = match (input, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => {
            let text = text.to_string();
            Some(std::thread::spawn(move || stdin.write_all(text.as_bytes()).is_ok()))
        }
        _ => None,
    };
    let result = child.wait_with_output().ok()?;
    let written = writer.is_none_or(|w| w.join().unwrap_or(false));
    if !written || !result.status.success() {
        return None;
    }
    String::from_utf8(result.stdout).ok()
}

/// A git answer that must say something (a path, a hash): empty output is no answer.
pub fn answer(dir: &Path, args: &[&str]) -> Option<String> {
    output(dir, args, None).map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
}

/// The inputs of the commit rule that come from git and the file system.
pub struct Facts {
    /// `HEAD`, only when git can read the repository and the crate sits directly at its root.
    pub head: Option<String>,
    /// Whether the files this build uses may differ from `HEAD` (only computed with `head`).
    pub dirty: bool,
    /// Whether the crate or one of its ancestors has a `.git` (a directory, or a worktree's file).
    pub in_repository: bool,
}

pub fn facts(manifest: &Path) -> Facts {
    let in_repository = manifest.ancestors().any(|dir| dir.join(".git").exists());
    let Some(top) = root(manifest) else {
        return Facts { head: None, dirty: false, in_repository };
    };
    let head = answer(manifest, &["rev-parse", "HEAD"]);
    let dirty = head.is_some() && dirty(manifest, &top);
    Facts { head, dirty, in_repository }
}

/// The root of the repository, only when this crate sits directly at it: a copy vendored inside somebody else's
/// repository must not report theirs.
fn root(manifest: &Path) -> Option<PathBuf> {
    let top = PathBuf::from(answer(manifest, &["rev-parse", "--show-toplevel"])?).canonicalize().ok()?;
    (top == manifest.parent()?.canonicalize().ok()?).then_some(top)
}

/// Whether the tracked files this build uses (the crate and the workspace manifests) may differ from `HEAD`:
/// `git status`, the files whose changes it hides (`ls-files -v`), and a comparison of every byte with `HEAD`
/// (`git status` trusts the size and modification time it cached). When git cannot say, the answer is dirty.
fn dirty(manifest: &Path, top: &Path) -> bool {
    let mut paths = vec![manifest.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned())];
    for extra in ["Cargo.toml", "Cargo.lock"] {
        if top.join(extra).exists() {
            paths.push(extra.to_string());
        }
    }
    let with = |command: &[&'static str]| -> Vec<String> {
        command.iter().map(|word| word.to_string()).chain(paths.iter().cloned()).collect()
    };
    let run = |args: Vec<String>| output(top, &args.iter().map(String::as_str).collect::<Vec<_>>(), None);
    let status = run(with(&["status", "--porcelain", "--untracked-files=no", "--"]));
    let listing = run(with(&["ls-files", "-v", "--"]));
    if commit_rules::is_dirty(status.as_deref(), listing.as_deref()) {
        return true;
    }
    let Some(tree) = run(with(&["ls-tree", "-r", "-z", "HEAD", "--"])) else {
        return true;
    };
    let names: String = tree.split('\0').filter_map(|entry| entry.split_once('\t')).map(|(_, path)| format!("{path}\n")).collect();
    match output(top, &["hash-object", "--stdin-paths"], Some(&names)) {
        Some(hashes) => commit_rules::contents_differ(&tree, &hashes),
        None => true,
    }
}
