//! No documented program works only because an unbound name evaluates to its own text (audit T5).

use physure_script::report::{evaluate, DEFAULT_SEED};
use physure_script::Limits;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every ```phs block of a Markdown text.
fn blocks(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim_start()) {
            (None, l) if l.starts_with("```phs") => current = Some(String::new()),
            (Some(block), l) if l.starts_with("```") => {
                out.push(std::mem::take(block));
                current = None;
            }
            (Some(block), _) => {
                block.push_str(line);
                block.push('\n');
            }
            _ => {}
        }
    }
    out
}

fn files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !["target", "node_modules", ".git", ".claude", "site"].contains(&name) {
                files(&path, extension, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// Programs shown on purpose with a name that has no value (the documentation of the error itself).
const SHOWS_THE_ERROR: &[&str] = &["has no value"];

/// Documentation fragments that already name a free word of their context (`pressure`, `enabled`, `actual`) and
/// show only the syntax of one line: they say `has no value` (or are flagged for the name) today, before audit T5,
/// and are not programs that run. (file name, blocks). A new fragment is not added here without reading it.
const FRAGMENTS_OF_CONTEXT: &[(&str, &[usize])] = &[
    ("2026-08-26-phs-boolean-logic-and-assertions.md", &[0, 1, 2]),
    ("2026-08-26-phs-boolean-logic-and-assertions-design.md", &[0, 3, 6, 7, 8, 10, 12, 13, 14]),
    ("2026-08-26-phs-gum-precision-design.md", &[0]),
    ("phs_primer.md", &[1, 4, 7, 8, 9]),
];

fn is_a_fragment_of_context(origin: &str) -> bool {
    FRAGMENTS_OF_CONTEXT.iter().any(|(file, blocks)| blocks.iter().any(|block| origin.ends_with(&format!("{file} block {block}"))))
}

#[test]
fn no_documented_program_relies_on_an_unbound_name() {
    let mut programs: Vec<(String, String)> = Vec::new();
    let mut phs = Vec::new();
    files(&root(), "phs", &mut phs);
    for path in phs {
        programs.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
    }
    let mut markdown = Vec::new();
    files(&root(), "md", &mut markdown);
    for path in markdown {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (i, block) in blocks(&text).into_iter().enumerate() {
            programs.push((format!("{} block {i}", path.display()), block));
        }
    }
    assert!(programs.len() > 50, "found only {} programs: the search is broken", programs.len());
    println!("{} documented programs", programs.len());

    let mut reliant = Vec::new();
    for (origin, source) in &programs {
        if SHOWS_THE_ERROR.iter().any(|shown| source.contains(shown)) || is_a_fragment_of_context(origin) {
            continue;
        }
        let report = evaluate(source, Limits::sandboxed(Duration::from_secs(10)), DEFAULT_SEED);
        // the runtime error, or the checker's finding of the same name (the report says one problem once, and then
        // only the checker's words remain)
        if let Some(finding) = report.diagnostics.iter().find(|d| d.message.contains("has no value") || d.code == "unknown-name") {
            reliant.push(format!("{origin}: line {}: {}", finding.line, finding.message.lines().next().unwrap_or("")));
        }
    }
    assert!(reliant.is_empty(), "programs that rely on an unbound name:
{}", reliant.join("
"));
}
