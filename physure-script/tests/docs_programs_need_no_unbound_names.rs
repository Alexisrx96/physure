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
    // blocks 1, 5, 9 (`actual == expected`, `pressure > 0 Pa`, `flag != False`) became `has no value` with audit T5:
    // one-line fragments of the design that name words of their context, as the others of this file
    ("2026-08-26-phs-boolean-logic-and-assertions-design.md", &[0, 1, 3, 5, 6, 7, 8, 9, 10, 12, 13, 14]),
    // a sketch of the chemistry API before it existed: `mass_to_moles` is planned, not implemented, so the line that
    // would bind `n_co2` fails first and `n_co2` has no value where it is read (it was text before audit T5)
    ("chemistry_roadmap.md", &[0]),
    ("2026-08-26-phs-gum-precision-design.md", &[0]),
    // the primer is one program read in order (see `CUMULATIVE`); blocks 7, 8 and 9 show the syntax of conditions
    // and assertions with the names of an imagined pump (`count`, `sensor_disconnected`, `limit`, `enabled`,
    // `override`, `power`, `minimum_pressure`, `actual`, `expected`) that the primer never defines
    ("phs_primer.md", &[7, 8, 9]),
];

/// Documents whose blocks build one program in order: each block runs after every earlier block of the file that
/// is not a fragment, as a reader who typed them one after another (the primer's block 6 prints what blocks 0, 2
/// and 4 computed).
const CUMULATIVE: &[&str] = &["phs_primer.md"];

fn is_a_fragment_of_context(origin: &str) -> bool {
    FRAGMENTS_OF_CONTEXT.iter().any(|(file, blocks)| blocks.iter().any(|block| origin.ends_with(&format!("{file} block {block}"))))
}

/// The programs of the `phs` blocks of the Markdown file `path` (whose text is `text`), each with where it comes from.
fn markdown_programs(path: &Path, text: &str, programs: &mut Vec<(String, String)>) {
    let cumulative = CUMULATIVE.iter().any(|name| path.ends_with(name));
    let mut earlier = String::new();
    for (i, block) in blocks(text).into_iter().enumerate() {
        let origin = format!("{} block {i}", path.display());
        if !cumulative {
            programs.push((origin, block));
            continue;
        }
        programs.push((origin.clone(), format!("{earlier}{block}")));
        if !is_a_fragment_of_context(&origin) {
            earlier.push_str(&block);
        }
    }
}

/// Every documented program: the `.phs` files and the `phs` blocks of the Markdown files of the repository.
fn documented_programs() -> Vec<(String, String)> {
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
        markdown_programs(&path, &text, &mut programs);
    }
    programs
}

/// The finding of an unbound name in the program `source`, `None` when it has none or is not checked.
fn unbound_name_finding(origin: &str, source: &str) -> Option<String> {
    if SHOWS_THE_ERROR.iter().any(|shown| source.contains(shown)) || is_a_fragment_of_context(origin) {
        return None;
    }
    let report = evaluate(source, Limits::sandboxed(Duration::from_secs(10)), DEFAULT_SEED);
    // the runtime error, or the checker's finding of the same name (the report says one problem once, and then
    // only the checker's words remain)
    let finding = report.diagnostics.iter().find(|d| d.message.contains("has no value") || d.code == "unknown-name")?;
    Some(format!("{origin}: line {}: {}", finding.line, finding.message.lines().next().unwrap_or("")))
}

#[test]
fn no_documented_program_relies_on_an_unbound_name() {
    let programs = documented_programs();
    assert!(programs.len() > 50, "found only {} programs: the search is broken", programs.len());
    println!("{} documented programs", programs.len());

    let reliant: Vec<String> =
        programs.iter().filter_map(|(origin, source)| unbound_name_finding(origin, source)).collect();
    assert!(reliant.is_empty(), "programs that rely on an unbound name:
{}", reliant.join("
"));
}
