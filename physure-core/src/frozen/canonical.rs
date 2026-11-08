//! The canonical form of a source and the hashes built on it (spec §4.1).

use super::sha256::{sha256_hex, Sha256, to_hex};

/// The canonical text of a source: lines trimmed at both ends, blank lines and lines starting with `#` or `;`
/// dropped, joined with `\n`. Reformatting, comments and line endings do not change it; one digit does. An
/// inline comment (`m = 1 # note`) is content: the conf grammar has none.
pub fn canonical_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `source_sha256`: the SHA-256 of the canonical text, as lowercase hexadecimal.
pub fn source_sha256(text: &str) -> String {
    sha256_hex(canonical_text(text).as_bytes())
}

/// `exact_sha256`: the SHA-256 of a witness script, byte-exact after CRLF becomes LF and trailing newlines are
/// removed, with no other change. Never the conf canonical form: in PHS `;` ends a statement, indentation is
/// significant and a string may span lines.
pub fn exact_sha256(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    sha256_hex(text.trim_end_matches('\n').as_bytes())
}

/// `content_sha256`: the SHA-256 over, for each `(origin, source_sha256)` in order, `origin\n` + `source_sha256\n`.
pub fn content_sha256(sources: &[(String, String)]) -> String {
    let mut hasher = Sha256::new();
    for (origin, hash) in sources {
        hasher.update(origin.as_bytes());
        hasher.update(b"\n");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    to_hex(&hasher.finish())
}

/// The canonical text of a list of `key = value` pairs: `key=value` lines sorted, joined with `\n`.
pub fn canonical_pairs(pairs: &[(String, String)]) -> String {
    let mut lines: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    lines.sort();
    lines.join("\n")
}

/// Whether `text` is 64 lowercase hexadecimal characters.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_blank_lines_and_whitespace_do_not_change_the_canonical_text() {
        let a = "# a comment\n[Units]\n  m = 1  \n\n; another\nkg = 2\n";
        let b = "[Units]\r\nm = 1\r\nkg = 2";
        assert_eq!(canonical_text(a), "[Units]\nm = 1\nkg = 2");
        assert_eq!(canonical_text(a), canonical_text(b));
        assert_eq!(source_sha256(a), source_sha256(b));
    }

    #[test]
    fn one_digit_changes_the_hash() {
        assert_ne!(source_sha256("m = 1.0"), source_sha256("m = 1.1"));
    }

    #[test]
    fn an_inline_comment_is_content_not_a_comment() {
        assert_ne!(source_sha256("m = 1"), source_sha256("m = 1 # note"));
    }

    #[test]
    fn the_content_hash_depends_on_origins_order_and_source_hashes() {
        let a = vec![("embedded".to_string(), "a".repeat(64)), ("overrides".to_string(), "b".repeat(64))];
        let swapped = vec![a[1].clone(), a[0].clone()];
        let renamed = vec![("embedded".to_string(), "a".repeat(64)), ("mine".to_string(), "b".repeat(64))];
        assert_ne!(content_sha256(&a), content_sha256(&swapped));
        assert_ne!(content_sha256(&a), content_sha256(&renamed));
        assert_eq!(content_sha256(&a), content_sha256(&a.clone()));
        assert!(is_sha256_hex(&content_sha256(&a)));
    }

    #[test]
    fn canonical_pairs_ignore_order_and_are_unambiguous() {
        let a = vec![("b".to_string(), "2".to_string()), ("a".to_string(), "1".to_string())];
        let b = vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())];
        assert_eq!(canonical_pairs(&a), "a=1\nb=2");
        assert_eq!(canonical_pairs(&a), canonical_pairs(&b));
        assert_eq!(canonical_pairs(&[]), "");
    }

    #[test]
    fn the_exact_hash_keeps_everything_but_line_endings_and_trailing_newlines() {
        assert_eq!(exact_sha256(""), sha256_hex(b""));
        assert_eq!(exact_sha256("a\r\nb\n\n"), sha256_hex(b"a\nb"));
        assert_ne!(exact_sha256(" a"), exact_sha256("a"));
        assert_ne!(exact_sha256("# c\na"), exact_sha256("a"));
    }

    #[test]
    fn the_hex_check() {
        assert!(is_sha256_hex(&"0".repeat(64)));
        assert!(!is_sha256_hex(&"0".repeat(63)));
        assert!(!is_sha256_hex(&"G".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)), "uppercase is not canonical");
    }
}
