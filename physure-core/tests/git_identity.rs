//! `build_support/git.rs` against real git repositories: every case here is one in which a build could claim a
//! commit whose tree is not the code it compiles. Each test makes a throwaway repository in its own temporary
//! directory (`crate/` at its root, like `physure-core/` in this repository), changes it the way the case says,
//! and asks the very code `build.rs` runs. The tests hold one lock, because two of them set a `GIT_*` variable in
//! this process to show that the build ignores it.
//!
//! git is required: a test that cannot run git fails with a message, it never passes silently.

#[allow(dead_code)]
#[path = "../build_support/commit.rs"]
mod commit_rules;
#[allow(dead_code)]
#[path = "../build_support/git.rs"]
mod git_facts;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Variables the tests' own git commands never inherit (a git hook running `cargo test` sets `GIT_DIR`, and two
/// tests set a variable in this process on purpose).
const NOT_INHERITED: [&str; 4] = ["GIT_ATTR_SOURCE", "GIT_ATTR_NOSYSTEM", "GIT_REPLACE_REF_BASE", "GIT_NO_REPLACE_OBJECTS"];

/// A date long before the test runs: files carry it, so that the index never sees them as "racily clean" and
/// `git status` trusts its cached size and modification time, as it does in a real checkout.
fn old() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_600_000_000)
}

fn git_with(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args);
    for name in commit_rules::GIT_ENV_CLEARED.iter().chain(NOT_INHERITED.iter()) {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("git is required by these tests and could not be run ({e}); install git or put it on PATH"));
    assert!(out.status.success(), "git {args:?} failed in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    std::fs::File::options().write(true).open(path).unwrap().set_modified(old()).unwrap();
}

/// A throwaway repository with `crate/` at its root and one commit; removed when dropped.
struct Repo {
    root: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Repo {
    fn new(name: &str) -> Repo {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("physure-git-identity-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let repo = Repo { root };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write("Cargo.toml", "[workspace]\nmembers = [\"crate\"]\n");
        repo.write("Cargo.lock", "# lock\n");
        repo.write("crate/Cargo.toml", "[package]\nname = \"c\"\n");
        repo.write("crate/src/lib.rs", "pub const X: &str = \"AAAA\";\n");
        repo.write("crate/src/x.conf", "k = 1\nv = 2\n");
        repo.commit("one");
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        git_with(&self.root, args, &[])
    }

    fn write(&self, relative: &str, text: &str) {
        write(&self.root.join(relative), text);
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--no-verify", "-m", message]);
        self.head()
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn facts(&self) -> git_facts::Facts {
        git_facts::facts(&self.root.join("crate"))
    }

    /// The commit the build would claim.
    fn claimed(&self) -> String {
        let facts = self.facts();
        commit_rules::decide(&commit_rules::Inputs {
            env: None,
            git_head: facts.head.as_deref(),
            dirty: facts.dirty,
            in_repository: facts.in_repository,
        })
        .commit
    }

    fn assert_dirty(&self, case: &str) {
        let facts = self.facts();
        assert!(facts.dirty, "{case}: the tree was reported clean (head {:?})", facts.head);
        assert_eq!(self.claimed(), commit_rules::UNKNOWN, "{case}");
    }
}

/// Sets a variable in this process for the life of the guard. Only under `serial()`.
struct EnvVar(&'static str);

impl EnvVar {
    fn set(name: &'static str, value: &str) -> EnvVar {
        std::env::set_var(name, value);
        EnvVar(name)
    }
}

impl Drop for EnvVar {
    fn drop(&mut self) {
        std::env::remove_var(self.0);
    }
}

#[test]
fn a_clean_tree_claims_head() {
    let _serial = serial();
    let repo = Repo::new("clean");
    let facts = repo.facts();
    assert!(facts.in_repository);
    assert!(!facts.dirty);
    assert_eq!(facts.head.as_deref(), Some(repo.head().as_str()));
    assert_eq!(repo.claimed(), repo.head());
}

#[test]
fn a_crate_that_is_not_at_the_root_of_its_repository_names_no_commit() {
    let _serial = serial();
    let repo = Repo::new("nested");
    let facts = git_facts::facts(&repo.root.join("crate/src"));
    assert!(facts.in_repository);
    assert_eq!(facts.head, None);
}

#[test]
fn outside_any_repository_git_names_no_commit() {
    let _serial = serial();
    let repo = Repo::new("outside");
    std::fs::remove_dir_all(repo.root.join(".git")).unwrap();
    let facts = repo.facts();
    assert_eq!(facts.head, None);
    assert!(!facts.dirty);
}

#[test]
fn an_edit_that_keeps_size_and_modification_time_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("same-size");
    repo.write("crate/src/lib.rs", "pub const X: &str = \"BBBB\";\n");
    repo.assert_dirty("same-size edit, modification time restored");
}

#[test]
fn an_edit_of_an_assume_unchanged_file_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("assume-unchanged");
    repo.git(&["update-index", "--assume-unchanged", "crate/src/lib.rs"]);
    repo.write("crate/src/lib.rs", "pub const X: &str = \"BBBBBB\";\n");
    assert_eq!(repo.git(&["status", "--porcelain", "--untracked-files=no"]), "", "precondition: git status hides it");
    repo.assert_dirty("assume-unchanged");
}

#[test]
fn an_edit_of_a_skip_worktree_file_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("skip-worktree");
    repo.git(&["update-index", "--skip-worktree", "crate/src/lib.rs"]);
    repo.write("crate/src/lib.rs", "pub const X: &str = \"BBBBBB\";\n");
    assert_eq!(repo.git(&["status", "--porcelain", "--untracked-files=no"]), "", "precondition: git status hides it");
    repo.assert_dirty("skip-worktree");
}

#[test]
fn a_deleted_file_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("deleted");
    std::fs::remove_file(repo.root.join("crate/src/x.conf")).unwrap();
    repo.assert_dirty("deleted file");
}

#[test]
fn a_deleted_workspace_manifest_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("deleted-lock");
    std::fs::remove_file(repo.root.join("Cargo.lock")).unwrap();
    repo.assert_dirty("deleted Cargo.lock");
}

#[test]
fn a_symlink_that_replaces_a_file_is_dirty() {
    let _serial = serial();
    let repo = Repo::new("symlink");
    let target = repo.root.join("elsewhere.rs");
    write(&target, "pub const X: &str = \"EVIL\";\n");
    let link = repo.root.join("crate/src/lib.rs");
    std::fs::remove_file(&link).unwrap();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&target, &link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&target, &link);
    if let Err(error) = made {
        // Windows creates symbolic links only with a privilege or in Developer Mode; then the case cannot be built
        #[cfg(windows)]
        {
            println!("SKIPPED a_symlink_that_replaces_a_file_is_dirty: creating a symbolic link needs a privilege here ({error})");
            eprintln!("SKIPPED a_symlink_that_replaces_a_file_is_dirty: creating a symbolic link needs a privilege here ({error})");
            return;
        }
        #[cfg(not(windows))]
        panic!("could not create a symbolic link: {error}");
    }
    repo.assert_dirty("symlink replacing a tracked file");
}

#[test]
fn replace_objects_do_not_change_the_tree_that_is_compared() {
    let _serial = serial();
    let repo = Repo::new("replace");
    let c1 = repo.head();
    // another size: `git add` must see the change (the file keeps the old modification time)
    repo.write("crate/src/lib.rs", "pub const X: &str = \"EVIL, LONGER\";\n");
    let c2 = repo.commit("two");
    // HEAD back on C1, the index and the files stay those of C2
    repo.git(&["reset", "-q", "--soft", &c1]);
    repo.git(&["replace", &c1, &c2]);
    assert_eq!(repo.head(), c1, "precondition: rev-parse names C1");
    assert_eq!(repo.git(&["status", "--porcelain", "--untracked-files=no"]), "", "precondition: git status reads C2's tree");
    repo.assert_dirty("git replace C1 C2");

    // the same through a ref namespace named by GIT_REPLACE_REF_BASE
    repo.git(&["replace", "-d", &c1]);
    repo.git(&["update-ref", &format!("refs/elsewhere/{c1}"), &c2]);
    let status = git_with(&repo.root, &["status", "--porcelain", "--untracked-files=no"], &[("GIT_REPLACE_REF_BASE", "refs/elsewhere/")]);
    assert_eq!(status, "", "precondition: GIT_REPLACE_REF_BASE replaces C1");
    let _variable = EnvVar::set("GIT_REPLACE_REF_BASE", "refs/elsewhere/");
    repo.assert_dirty("GIT_REPLACE_REF_BASE");
}

/// A clean filter that turns the edited `v = 9` back into the committed `v = 2`: git status and git hash-object
/// both apply it, so only refusing filters can show that the compiled file is not the committed one.
fn lossy_filter(repo: &Repo) {
    repo.git(&["config", "filter.lossy.clean", "sed 's/v = 9/v = 2/'"]);
    repo.git(&["config", "filter.lossy.smudge", "cat"]);
}

#[test]
fn an_attribute_source_named_in_the_environment_does_not_hide_an_edit() {
    let _serial = serial();
    let repo = Repo::new("attr-source");
    let main = repo.head();
    // a commit whose .gitattributes puts every .conf under the lossy filter, not checked out
    repo.write(".gitattributes", "*.conf filter=lossy\n");
    let attributes = repo.commit("attributes");
    repo.git(&["reset", "-q", "--hard", &main]);
    lossy_filter(&repo);
    repo.write("crate/src/x.conf", "k = 1\nv = 9\n");
    let status = git_with(&repo.root, &["status", "--porcelain", "--untracked-files=no"], &[("GIT_ATTR_SOURCE", &attributes)]);
    assert_eq!(status, "", "precondition (git 2.40 or later): GIT_ATTR_SOURCE hides the edit from git status");
    let _variable = EnvVar::set("GIT_ATTR_SOURCE", &attributes);
    repo.assert_dirty("GIT_ATTR_SOURCE");
}

#[test]
fn a_filter_set_in_the_repository_attributes_does_not_hide_an_edit() {
    let _serial = serial();
    let repo = Repo::new("info-attributes");
    lossy_filter(&repo);
    repo.write(".git/info/attributes", "*.conf filter=lossy\n");
    repo.write("crate/src/x.conf", "k = 1\nv = 9\n");
    assert_eq!(repo.git(&["status", "--porcelain", "--untracked-files=no"]), "", "precondition: the filter hides the edit");
    repo.assert_dirty(".git/info/attributes filter");
}

#[test]
fn a_filter_is_a_doubt_even_without_an_edit() {
    let _serial = serial();
    let repo = Repo::new("filter-no-edit");
    lossy_filter(&repo);
    repo.write(".git/info/attributes", "*.conf filter=lossy\n");
    repo.assert_dirty("filter configured, no edit");
}

#[test]
fn a_head_that_holds_a_tag_claims_the_commit_it_names() {
    let _serial = serial();
    let repo = Repo::new("tag-head");
    let commit = repo.head();
    repo.git(&["tag", "-a", "v1", "-m", "v1"]);
    let tag = repo.git(&["rev-parse", "v1"]);
    assert_ne!(tag, commit);
    // `git update-ref` refuses a tag here, a direct write does not
    std::fs::write(repo.root.join(".git/HEAD"), format!("{tag}\n")).unwrap();
    assert_eq!(repo.head(), tag, "precondition: rev-parse HEAD names the tag object");
    // The build compares the files with the tree of the commit the tag names, so that commit is what it can prove;
    // the tag's own id is not a commit and must never be claimed.
    let facts = repo.facts();
    assert_eq!(facts.head.as_deref(), Some(commit.as_str()));
    assert!(!facts.dirty);
    assert_eq!(repo.claimed(), commit);
}

#[test]
fn an_untracked_workspace_manifest_is_dirty() {
    for manifest in ["Cargo.lock", "Cargo.toml"] {
        let _serial = serial();
        let repo = Repo::new("untracked-manifest");
        repo.git(&["rm", "-q", "--cached", manifest]);
        repo.git(&["commit", "-q", "--no-verify", "-m", "untrack"]);
        assert!(repo.root.join(manifest).exists());
        repo.assert_dirty(&format!("untracked {manifest}"));
    }
}
