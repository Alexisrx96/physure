//! The dimensional findings of `physure_script::check` as language-server diagnostics.
//!
//! The checker works on a parsed program and reports a line per finding (the AST carries no columns), so a
//! diagnostic covers its line. A buffer that does not parse has no program to check: the incremental
//! analysis reports the syntax error, and this reports nothing.

use physure_script::{check_source, Severity};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

/// The diagnostics for the dimensions in `text`, none if it does not parse or the checker panics.
pub fn dimension_diagnostics(text: &str) -> Vec<Diagnostic> {
    let Ok(Ok(findings)) = std::panic::catch_unwind(|| check_source(text)) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    findings
        .into_iter()
        .map(|finding| {
            let line = finding.line.saturating_sub(1);
            // LSP columns are UTF-16 code units
            let width = lines.get(line).map_or(1, |l| l.encode_utf16().count()).max(1);
            Diagnostic {
                range: Range {
                    start: Position { line: line as u32, character: 0 },
                    end: Position { line: line as u32, character: width as u32 },
                },
                severity: Some(match finding.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                code: Some(NumberOrString::String(finding.code.to_string())),
                source: Some("physure-check".to_string()),
                message: finding.message,
                ..Diagnostic::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dimension_error_is_an_error_diagnostic_over_its_line() {
        let found = dimension_diagnostics("a = 1 m\nb = 2 s\na + b\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(found[0].range.start, Position { line: 2, character: 0 });
        assert_eq!(found[0].range.end, Position { line: 2, character: 5 });
        assert_eq!(found[0].code, Some(NumberOrString::String("dimension-mismatch".to_string())));
        assert_eq!(found[0].source.as_deref(), Some("physure-check"));
    }

    #[test]
    fn a_warning_is_a_warning() {
        let found = dimension_diagnostics("max(1 m, 1 s)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn a_clean_buffer_and_a_buffer_that_does_not_parse_have_nothing_to_say() {
        assert!(dimension_diagnostics("a = 1 m\nb = 2 m\na + b\n").is_empty());
        assert!(dimension_diagnostics("1 +").is_empty());
        assert!(dimension_diagnostics("").is_empty());
    }

    #[test]
    fn the_range_is_in_utf16_code_units() {
        // the clef is outside the basic plane: two UTF-16 units, one char
        let found = dimension_diagnostics("a = 1 m\nb = \"\u{1D11E}\"\na + 1 s\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].range.start.line, 2);
        assert_eq!(found[0].range.end.character, 7);
    }

    #[test]
    fn a_keystroke_is_answered_in_well_under_a_frame() {
        let primer = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/tutorials/phs_primer.phs")).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..20 {
            dimension_diagnostics(&primer);
        }
        let each = started.elapsed() / 20;
        println!("one check of the primer ({} bytes): {each:?}", primer.len());
        assert!(each < std::time::Duration::from_millis(100), "{each:?} per check of the primer ({} bytes)", primer.len());
    }
}
