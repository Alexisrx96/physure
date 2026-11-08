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

/// Everything the decision depends on.
pub struct Inputs<'a> {
    /// `PHYSURE_BUILD_COMMIT` as set in the environment of the build, if it is set.
    pub env: Option<&'a str>,
    /// `git rev-parse HEAD`, when git answered and this crate sits directly at the root of that repository.
    pub git_head: Option<&'a str>,
    /// Whether tracked files this build uses have uncommitted changes (meaningful only with `git_head`).
    pub dirty: bool,
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
        None => Decision { commit: env.unwrap_or(UNKNOWN).to_string(), warning: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef01234567";
    const B: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn inputs<'a>(env: Option<&'a str>, git_head: Option<&'a str>, dirty: bool) -> Inputs<'a> {
        Inputs { env, git_head, dirty }
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
    fn a_head_that_is_not_a_commit_is_not_an_answer() {
        assert_eq!(decide(&inputs(None, Some("garbage"), false)).commit, UNKNOWN);
        assert_eq!(decide(&inputs(Some(B), Some("garbage"), false)).commit, B);
    }
}
