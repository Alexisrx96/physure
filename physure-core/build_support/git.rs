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
    // replace refs (`git replace`, or the namespace `GIT_REPLACE_REF_BASE` names) let `HEAD` name one commit while
    // every other command reads another commit's tree: they are ignored, by the option and by the variable
    command.arg("--no-replace-objects").arg("-C").arg(dir).args(args).stdout(Stdio::piped()).stderr(Stdio::null());
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    command.env("GIT_NO_REPLACE_OBJECTS", "1");
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
    // `HEAD^{commit}`: a `HEAD` written by hand can hold a tag's id, which is not a commit. Peeled, it names the
    // commit whose tree is compared below, which is exactly what the build can prove; the tag's id never is.
    let head = answer(manifest, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
    let dirty = head.as_deref().is_some_and(|commit| dirty(manifest, &top, commit));
    Facts { head, dirty, in_repository }
}

/// The root of the repository, only when this crate sits directly at it: a copy vendored inside somebody else's
/// repository must not report theirs.
fn root(manifest: &Path) -> Option<PathBuf> {
    let top = PathBuf::from(answer(manifest, &["rev-parse", "--show-toplevel"])?).canonicalize().ok()?;
    (top == manifest.parent()?.canonicalize().ok()?).then_some(top)
}

/// The workspace manifests cargo reads next to the crate.
const WORKSPACE_FILES: [&str; 2] = ["Cargo.toml", "Cargo.lock"];

/// Whether the files this build uses (the crate and the workspace manifests) may differ from `commit`, the peeled
/// `HEAD`: `git status`, the files whose changes it hides (`ls-files -v`), a workspace manifest that exists but is
/// not in the commit, an attribute that lets git hash other bytes than the compiler reads (a filter), and a
/// comparison of every byte with the commit (`git status` trusts the size and modification time it cached). When
/// git cannot say, the answer is dirty.
fn dirty(manifest: &Path, top: &Path, commit: &str) -> bool {
    let crate_dir = manifest.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    // the manifests are named even when they do not exist: a tracked one that was deleted shows in `git status`
    let paths: Vec<&str> = [crate_dir.as_str()].into_iter().chain(WORKSPACE_FILES).collect();
    let run = |command: &[&str]| -> Option<String> {
        let args: Vec<&str> = command.iter().copied().chain(paths.iter().copied()).collect();
        output(top, &args, None)
    };
    let status = run(&["status", "--porcelain", "--untracked-files=no", "--"]);
    let listing = run(&["ls-files", "-v", "--"]);
    if commit_rules::is_dirty(status.as_deref(), listing.as_deref()) {
        return true;
    }
    let Some(tree) = run(&["ls-tree", "-r", "-z", commit, "--"]) else {
        return true;
    };
    let Some(names) = commit_rules::tree_paths(&tree) else {
        return true;
    };
    // cargo reads a workspace manifest that exists whether git tracks it or not
    if WORKSPACE_FILES.iter().any(|file| top.join(file).exists() && !names.contains(file)) {
        return true;
    }
    let listed: String = names.iter().map(|name| format!("{name}\0")).collect();
    let mut check_attr = vec!["check-attr", "-z", "--stdin"];
    check_attr.extend(commit_rules::CONTENT_ATTRIBUTES);
    match output(top, &check_attr, Some(&listed)) {
        Some(attributes) if !commit_rules::attributes_change_content(&attributes, names.len()) => {}
        _ => return true,
    }
    let lines: String = names.iter().map(|name| format!("{name}\n")).collect();
    match output(top, &["hash-object", "--stdin-paths"], Some(&lines)) {
        Some(hashes) => commit_rules::contents_differ(&tree, &hashes),
        None => true,
    }
}
