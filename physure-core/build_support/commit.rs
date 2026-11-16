//! The rule that says which commit an engine build names. It is shared by `build.rs` (through `#[path]`) and
//! unit-tested from the library.
//!
//! The commit is a **claim of whoever builds**: the release pipeline sets `PHYSURE_BUILD_COMMIT` to the commit it
//! checked out, and the build records it. Nothing here asks git: a build cannot prove that the files it compiles are
//! those of a commit (a tracked file can change between any check and the compiler reading it), so the identity does
//! not pretend to. What pins the behaviour of an engine is the `binary` policy (the hash of the artefact) and the
//! witnesses of a frozen file, not this field. A value that is not a full hash is `unknown`, which never equals
//! anything.

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = "unknown";

/// A commit identifier: exactly 40 or 64 lowercase hexadecimal characters (a SHA-1 or SHA-256 object name).
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub struct Decision {
    pub commit: String,
    pub warning: Option<String>,
}

/// The commit a build names, from `PHYSURE_BUILD_COMMIT` if it is set. A value that is not a full lowercase hash is
/// refused with a warning rather than recorded: a shortened or mistyped hash would look like a commit.
pub fn decide(env: Option<&str>) -> Decision {
    match env {
        None => Decision { commit: UNKNOWN.to_string(), warning: None },
        Some(value) if is_commit(value) => Decision { commit: value.to_string(), warning: None },
        Some(_) => Decision {
            commit: UNKNOWN.to_string(),
            warning: Some("PHYSURE_BUILD_COMMIT is not a full lowercase commit hash (40 or 64 hexadecimal characters); the build reports no commit".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn what_is_a_commit() {
        assert!(is_commit(A));
        assert!(is_commit(&"a".repeat(64)));
        for bad in ["", "unknown", "0123456", &A.to_uppercase(), &format!("{A}0"), &A.replace('0', "g")] {
            assert!(!is_commit(bad), "{bad}");
        }
    }

    #[test]
    fn a_valid_variable_is_the_commit_and_no_variable_is_unknown_without_a_warning() {
        let d = decide(Some(A));
        assert_eq!((d.commit.as_str(), d.warning), (A, None));
        let d = decide(None);
        assert_eq!((d.commit.as_str(), d.warning), (UNKNOWN, None));
    }

    #[test]
    fn an_invalid_variable_is_unknown_with_a_warning() {
        for bad in ["", "main", "HEAD", "0123456", "ABCDEF0123456789ABCDEF0123456789ABCDEF01"] {
            let d = decide(Some(bad));
            assert_eq!(d.commit, UNKNOWN, "{bad}");
            assert!(d.warning.is_some(), "{bad}");
        }
    }
}
