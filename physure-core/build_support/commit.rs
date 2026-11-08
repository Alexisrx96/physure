//! The rule that decides which commit an engine build may claim. It is shared by `build.rs` (through
//! `#[path]`) and unit-tested from the library, so the rule that matters most for an audit has tests.
//!
//! A false commit is worse than none: every doubt gives `unknown`.

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = "unknown";

/// A commit identifier: exactly 40 or 64 lowercase hexadecimal characters (a SHA-1 or SHA-256 object name).
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether the output of `git ls-files -v` shows a tracked file whose changes `git status` would not report:
/// a lowercase tag marks `assume-unchanged` (`h`) and `S` marks `skip-worktree`. A build that uses such a file
/// cannot say that its tree is clean, so it must not claim a commit.
pub fn hides_changes(ls_files_v: &str) -> bool {
    ls_files_v.lines().any(|line| line.chars().next().is_some_and(|tag| tag.is_ascii_lowercase() || tag == 'S'))
}

/// Environment variables that can point git at another repository, another index, another configuration, other
/// objects (replace refs) or other attributes, or stop it from finding the repository at all. The build script
/// removes them from every git command it runs (a hook, a wrapper or a CI step may set them), and the test of the
/// running identity does the same. `GIT_REPLACE_REF_BASE` names a namespace of replace refs: with it, `HEAD` can
/// name one commit while every command reads another's tree. `GIT_ATTR_SOURCE` (git 2.40) reads `.gitattributes`
/// from any tree, so a filter can apply that the working tree does not show; `GIT_ATTR_NOSYSTEM` drops the system
/// attributes. Removing a variable an older git does not know is harmless.
pub const GIT_ENV_CLEARED: [&str; 16] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_CEILING_DIRECTORIES",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_NAMESPACE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_REPLACE_REF_BASE",
    "GIT_ATTR_SOURCE",
    "GIT_ATTR_NOSYSTEM",
];

/// The paths of `git ls-tree -r -z` output, in order, to be written one per line to `git hash-object
/// --stdin-paths`. `None` when a path would not reach git intact: that command splits on newlines, strips a
/// trailing carriage return and C-unquotes a line that starts with `"`, so such a path could make git hash another
/// file than the one listed. `None` is a doubt (dirty); so is an entry without a path.
pub fn tree_paths(ls_tree_z: &str) -> Option<Vec<&str>> {
    ls_tree_z
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.split_once('\t').map(|(_, path)| path))
        .map(|path| path.filter(|p| !p.is_empty() && !p.contains(['\n', '\r']) && !p.starts_with('"')))
        .collect()
}

/// The attributes that make the bytes git hashes differ from the bytes on disk in a way the compiler sees: a
/// `filter` (clean/smudge, e.g. git-lfs), `ident` (`$Id$` expansion) and `working-tree-encoding`. `git status` and
/// `git hash-object` apply them alike, so a filter can hide an edit from both (measured: a clean filter mapping
/// `v = 9` back to `v = 2`). Only end-of-line conversion is left, which does not change what Rust compiles.
pub const CONTENT_ATTRIBUTES: [&str; 3] = ["filter", "ident", "working-tree-encoding"];

/// Whether the output of `git check-attr -z <CONTENT_ATTRIBUTES> --stdin` for `files` paths gives any of those
/// attributes a value other than `unspecified` or `unset` (then the byte comparison cannot be trusted). An answer
/// that does not hold exactly one `<path> NUL <attribute> NUL <value> NUL` record per file and attribute, in the
/// order asked, is a doubt too.
pub fn attributes_change_content(check_attr_z: &str, files: usize) -> bool {
    let fields: Vec<&str> = check_attr_z.split('\0').collect();
    // the output ends with a NUL, so splitting it leaves one empty field at the end
    let Some((last, records)) = fields.split_last() else { return true };
    if !last.is_empty() || records.len() != files * CONTENT_ATTRIBUTES.len() * 3 {
        return true;
    }
    records.chunks(3).enumerate().any(|(i, record)| {
        record[1] != CONTENT_ATTRIBUTES[i % CONTENT_ATTRIBUTES.len()] || !matches!(record[2], "unspecified" | "unset")
    })
}

/// Whether the tracked files a build uses may differ from `HEAD`, from the output of `git status --porcelain`
/// and of `git ls-files -v` over those files. `None` is a git command that **failed** (a corrupt index, a lock,
/// an unreadable object), `Some("")` one that succeeded with nothing to report: a failure is dirty, because a
/// build that cannot see its tree must not claim it is clean.
pub fn is_dirty(status: Option<&str>, ls_files_v: Option<&str>) -> bool {
    status.is_none_or(|out| !out.trim().is_empty()) || ls_files_v.is_none_or(hides_changes)
}

/// Whether the files this build uses differ **by content** from `HEAD`. `ls_tree_z` is the output of
/// `git ls-tree -r -z HEAD -- <paths>` (`<mode> <type> <object>\t<path>`, NUL-terminated) and `hashes` the output of
/// `git hash-object --stdin-paths` for the same paths in the same order (one object name per line, with the same
/// clean filters and line-ending conversion git applies when committing). `git status` trusts the size and
/// modification time it cached, so an edit that keeps both is invisible to it (measured); this reads every byte.
/// Anything that does not line up (nothing tracked, a count that differs, an entry that is not a file) is a
/// difference.
pub fn contents_differ(ls_tree_z: &str, hashes: &str) -> bool {
    let mut recorded = Vec::new();
    for entry in ls_tree_z.split('\0').filter(|e| !e.is_empty()) {
        match entry.split_once('\t').map(|(meta, _)| meta.split(' ').collect::<Vec<_>>()).as_deref() {
            Some([_, "blob", object]) => recorded.push(*object),
            _ => return true,
        }
    }
    let working: Vec<&str> = hashes.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    recorded.is_empty() || recorded != working
}

/// Everything the decision depends on.
pub struct Inputs<'a> {
    /// `PHYSURE_BUILD_COMMIT` as set in the environment of the build, if it is set.
    pub env: Option<&'a str>,
    /// `git rev-parse HEAD`, when git answered and this crate sits directly at the root of that repository.
    pub git_head: Option<&'a str>,
    /// Whether tracked files this build uses have uncommitted changes (meaningful only with `git_head`).
    pub dirty: bool,
    /// Whether the crate or one of its ancestors has a `.git` (a directory, or the file of a worktree): the crate
    /// is inside some repository, whether git can read it or not.
    pub in_repository: bool,
}

pub struct Decision {
    pub commit: String,
    pub warning: Option<String>,
}

fn unknown(why: &str) -> Decision {
    Decision { commit: UNKNOWN.to_string(), warning: Some(why.to_string()) }
}

pub fn decide(inputs: &Inputs<'_>) -> Decision {
    let env = match inputs.env {
        Some(value) if !is_commit(value) => {
            return unknown("PHYSURE_BUILD_COMMIT is not a full lowercase commit hash (40 or 64 hexadecimal characters); the build reports no commit")
        }
        other => other,
    };
    match inputs.git_head.filter(|head| is_commit(head)) {
        Some(head) => {
            if inputs.dirty {
                return unknown("the working tree has uncommitted changes to tracked files this build uses; the build reports no commit (build from a clean checkout)");
            }
            if env.is_some_and(|value| value != head) {
                return unknown("PHYSURE_BUILD_COMMIT disagrees with git HEAD; the build reports no commit");
            }
            Decision { commit: head.to_string(), warning: None }
        }
        // inside a repository, git not answering is a doubt about the tree, and the variable cannot settle it
        None if inputs.in_repository => unknown(
            "the crate is inside a git repository that git cannot read, or not at its root; the build reports no commit (PHYSURE_BUILD_COMMIT is used only outside any repository: fix git's configuration, e.g. safe.directory)",
        ),
        None => Decision { commit: env.unwrap_or(UNKNOWN).to_string(), warning: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef01234567";
    const B: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn inputs<'a>(env: Option<&'a str>, git_head: Option<&'a str>, dirty: bool) -> Inputs<'a> {
        // a git that answered implies a repository; tests of a crate outside any repository say so explicitly
        Inputs { env, git_head, dirty, in_repository: git_head.is_some() }
    }

    fn outside<'a>(env: Option<&'a str>) -> Inputs<'a> {
        Inputs { env, git_head: None, dirty: false, in_repository: false }
    }

    fn inside_but_git_cannot_answer<'a>(env: Option<&'a str>) -> Inputs<'a> {
        Inputs { env, git_head: None, dirty: false, in_repository: true }
    }

    #[test]
    fn inside_a_repository_that_git_cannot_read_the_commit_is_unknown_even_with_a_valid_variable() {
        for env in [None, Some(A)] {
            let d = decide(&inside_but_git_cannot_answer(env));
            assert_eq!(d.commit, UNKNOWN, "{env:?}");
            assert!(d.warning.as_deref().is_some_and(|w| w.contains("git")), "{env:?}: {:?}", d.warning);
        }
        // a head that is not a commit is no answer either
        let d = decide(&Inputs { env: Some(A), git_head: Some("garbage"), dirty: false, in_repository: true });
        assert_eq!(d.commit, UNKNOWN);
    }

    #[test]
    fn the_variable_is_used_only_outside_any_repository() {
        let d = decide(&outside(Some(B)));
        assert_eq!((d.commit.as_str(), d.warning), (B, None));
        let d = decide(&outside(None));
        assert_eq!((d.commit.as_str(), d.warning), (UNKNOWN, None));
    }

    #[test]
    fn the_git_variables_that_could_redirect_git_are_listed() {
        for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_CEILING_DIRECTORIES", "GIT_CONFIG_PARAMETERS",
                     "GIT_CONFIG_COUNT", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "GIT_CONFIG_NOSYSTEM", "GIT_NAMESPACE",
                     "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_REPLACE_REF_BASE", "GIT_ATTR_SOURCE",
                     "GIT_ATTR_NOSYSTEM"] {
            assert!(GIT_ENV_CLEARED.contains(&name), "{name}");
        }
    }

    #[test]
    fn content_that_differs_from_head_is_found_even_when_git_status_trusts_its_cache() {
        const ONE: &str = "1111111111111111111111111111111111111111";
        const TWO: &str = "2222222222222222222222222222222222222222";
        let tree = format!("100644 blob {ONE}\tphysure-core/src/lib.rs\0100644 blob {TWO}\tCargo.toml\0");
        assert!(!contents_differ(&tree, &format!("{ONE}\n{TWO}\n")));
        // the same object names in another order belong to other files
        assert!(contents_differ(&tree, &format!("{TWO}\n{ONE}\n")));
        // one byte edited, size and modification time kept: the blob differs
        assert!(contents_differ(&tree, &format!("{ONE}\n{}\n", "3".repeat(40))));
        // a missing answer, a missing line or an extra line is a doubt
        assert!(contents_differ(&tree, &format!("{ONE}\n")));
        assert!(contents_differ(&tree, &format!("{ONE}\n{TWO}\nx\n")));
        assert!(contents_differ(&tree, ""));
        // nothing tracked, or something that is not a file (a submodule), cannot be compared
        assert!(contents_differ("", ""));
        assert!(contents_differ(&format!("160000 commit {ONE}\tvendor\0"), &format!("{ONE}\n")));
        assert!(contents_differ("garbage\0", &format!("{ONE}\n")));
    }

    #[test]
    fn paths_that_cannot_cross_a_line_based_standard_input_intact_are_refused() {
        const ONE: &str = "1111111111111111111111111111111111111111";
        let tree = |paths: &[&str]| paths.iter().map(|p| format!("100644 blob {ONE}\t{p}\0")).collect::<String>();
        assert_eq!(tree_paths(&tree(&["crate/src/lib.rs", "Cargo.toml"])), Some(vec!["crate/src/lib.rs", "Cargo.toml"]));
        assert_eq!(tree_paths(&tree(&["a b/c d.rs"])), Some(vec!["a b/c d.rs"]));
        assert_eq!(tree_paths(""), Some(vec![]));
        // `git hash-object --stdin-paths` splits on newlines, strips a trailing CR and C-unquotes a leading quote
        for bad in ["crate/src/a\nb.rs", "crate/src/a.rs\r", "crate/\rx", "\"crate/src/lib.rs\"", "\"q"] {
            assert_eq!(tree_paths(&tree(&["Cargo.toml", bad])), None, "{bad:?}");
        }
        // an entry without a path is not a listing
        assert_eq!(tree_paths(&format!("100644 blob {ONE}\0")), None);
    }

    #[test]
    fn only_unset_or_unspecified_content_attributes_leave_the_bytes_comparable() {
        let line = |path: &str, attr: &str, value: &str| format!("{path}\0{attr}\0{value}\0");
        let all = |path: &str, values: [&str; 3]| -> String {
            CONTENT_ATTRIBUTES.iter().zip(values).map(|(attr, value)| line(path, attr, value)).collect()
        };
        let clean = all("a.rs", ["unspecified", "unspecified", "unspecified"]) + &all("b.rs", ["unset", "unspecified", "unset"]);
        assert!(!attributes_change_content(&clean, 2));
        for value in ["set", "lfs", "lossy", "UTF-16", ""] {
            for slot in 0..3 {
                let mut values = ["unspecified"; 3];
                values[slot] = value;
                let text = all("a.rs", ["unspecified"; 3]) + &all("b.rs", values);
                assert!(attributes_change_content(&text, 2), "{value:?} in slot {slot}");
            }
        }
        // an answer that does not cover every file and attribute is no answer
        assert!(attributes_change_content(&clean, 3));
        assert!(attributes_change_content(&clean, 1));
        assert!(attributes_change_content("", 1));
        assert!(attributes_change_content("garbage", 0));
        assert!(!attributes_change_content("", 0));
        // an answer about another attribute is no answer
        assert!(attributes_change_content(&line("a.rs", "text", "unset").repeat(3), 1));
    }

    #[test]
    fn a_git_failure_counts_as_dirty() {
        // `None` is a git command that failed; `Some("")` is one that succeeded with nothing to say
        assert!(!is_dirty(Some(""), Some("H src/lib.rs\n")));
        assert!(is_dirty(Some(" M src/lib.rs\n"), Some("H src/lib.rs\n")));
        assert!(is_dirty(Some(""), Some("h src/lib.rs\n")));
        assert!(is_dirty(None, Some("H src/lib.rs\n")), "git status failed (e.g. a corrupt index)");
        assert!(is_dirty(Some(""), None), "git ls-files failed");
        assert!(is_dirty(None, None));
    }

    #[test]
    fn what_is_a_commit() {
        assert!(is_commit(A));
        assert!(is_commit(&"a".repeat(64)));
        for bad in ["", UNKNOWN, "Unknown", "UNKNOWN", " unknown", "main", "HEAD", "release", "0123456", &A.to_uppercase(),
                    &"a".repeat(39), &"a".repeat(41), &"a".repeat(63), &"g".repeat(40), "bad commit"] {
            assert!(!is_commit(bad), "{bad:?}");
        }
    }

    #[test]
    fn git_clean_and_no_variable_gives_head() {
        let d = decide(&inputs(None, Some(A), false));
        assert_eq!((d.commit.as_str(), d.warning), (A, None));
    }

    #[test]
    fn git_clean_and_a_variable_that_agrees_gives_head() {
        assert_eq!(decide(&inputs(Some(A), Some(A), false)).commit, A);
    }

    #[test]
    fn a_variable_that_disagrees_with_git_gives_unknown_and_a_warning() {
        let d = decide(&inputs(Some(B), Some(A), false));
        assert_eq!(d.commit, UNKNOWN);
        assert!(d.warning.unwrap().contains("disagrees"));
    }

    #[test]
    fn uncommitted_changes_give_unknown_even_when_the_variable_agrees() {
        for env in [None, Some(A)] {
            let d = decide(&inputs(env, Some(A), true));
            assert_eq!(d.commit, UNKNOWN);
            assert!(d.warning.unwrap().contains("uncommitted"));
        }
    }

    #[test]
    fn an_invalid_variable_gives_unknown_and_a_warning_even_when_git_could_answer() {
        for bad in ["bad commit", "main", "Unknown", "", &A.to_uppercase()] {
            for head in [None, Some(A)] {
                let d = decide(&inputs(Some(bad), head, false));
                assert_eq!(d.commit, UNKNOWN, "{bad:?}");
                assert!(d.warning.is_some(), "{bad:?}");
            }
        }
    }


    #[test]
    fn without_git_a_valid_variable_is_used_and_otherwise_unknown() {
        let d = decide(&inputs(Some(B), None, false));
        assert_eq!((d.commit.as_str(), d.warning), (B, None));
        let d = decide(&inputs(None, None, false));
        assert_eq!((d.commit.as_str(), d.warning), (UNKNOWN, None));
    }

    #[test]
    fn files_whose_changes_git_status_cannot_see_are_found() {
        // `git ls-files -v`: `H` is an ordinary tracked file, `h` is assume-unchanged, `S` is skip-worktree
        assert!(!hides_changes(""));
        assert!(!hides_changes("H src/lib.rs\nH Cargo.toml\n"));
        assert!(hides_changes("H src/lib.rs\nh src/quantity.rs\n"));
        assert!(hides_changes("S src/lib.rs\n"));
        assert!(hides_changes("H a\nH b\nh c"));
        // other uppercase tags (unmerged, removed, modified) are reported by `git status`, so they are not hidden
        assert!(!hides_changes("M src/lib.rs\nR src/old.rs\nC src/x.rs\nK src/y.rs\n"));
    }

    #[test]
    fn a_head_that_is_not_a_commit_is_not_an_answer() {
        assert_eq!(decide(&inputs(None, Some("garbage"), false)).commit, UNKNOWN);
        assert_eq!(decide(&inputs(Some(B), Some("garbage"), false)).commit, UNKNOWN);
    }
}
