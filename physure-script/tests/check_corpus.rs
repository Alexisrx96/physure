//! The checker against every program in the crate's own tests: it must not report an error for a program
//! the interpreter runs, and it should find most of what the interpreter refuses for a dimension.

use physure_script::check::{check_source, Severity};
use physure_script::interpreter::eval_phs;

/// Every `.rs` file under `dir`.
fn rust_files(dir: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

/// The string literal that starts at `rest` (just after its opening quote), unescaped. `None` when it is not
/// closed or uses an escape the corpus has no need of.
fn unescaped_literal(rest: &str) -> Option<String> {
    let mut program = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(program),
            '\\' => match chars.next() {
                Some('n') => program.push('\n'),
                Some('t') => program.push('\t'),
                Some('"') => program.push('"'),
                Some('\\') => program.push('\\'),
                Some('\n') => {
                    // a line continuation drops the newline and the indentation after it
                    while chars.clone().next().is_some_and(|c| c == ' ' || c == '\t') {
                        chars.next();
                    }
                }
                _ => return None,
            },
            other => program.push(other),
        }
    }
    None
}

/// The string literals passed to `parse_phs(`, `eval_phs(` and the like (any `phs("`) in the crate's sources and
/// tests, unescaped.
fn programs_in_the_crates_tests() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    rust_files(&root.join("src"), &mut sources);
    rust_files(&root.join("tests"), &mut sources);
    let mut programs = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("phs(\"") {
            rest = &rest[at + "phs(\"".len()..];
            if let Some(program) = unescaped_literal(rest).filter(|p| !p.is_empty()) {
                programs.push(program);
            }
        }
    }
    programs
}

#[test]
fn no_program_the_interpreter_runs_gets_an_error() {
    let programs = programs_in_the_crates_tests();
    assert!(programs.len() > 60, "the corpus is {} programs", programs.len());
    let mut run = 0;
    let mut false_positives = Vec::new();
    for program in &programs {
        // only programs that parse and that the interpreter accepts count: a test that expects a refusal is not a witness
        if eval_phs(program).is_err() {
            continue;
        }
        let Ok(findings) = check_source(program) else { continue };
        run += 1;
        for finding in findings.iter().filter(|d| d.severity == Severity::Error) {
            false_positives.push(format!("{program:?} -> line {} [{}] {}", finding.line, finding.code, finding.message));
        }
    }
    println!("{run} programs the interpreter runs, of {}", programs.len());
    assert!(run > 40, "only {run} programs ran");
    assert!(false_positives.is_empty(), "{} false positives in {run} programs:\n{}", false_positives.len(), false_positives.join("\n"));
}

#[test]
fn the_checker_finds_most_of_what_the_interpreter_refuses_for_a_dimension() {
    // not a promise (a refusal for another reason, or at a value, is not the checker's): a measure of what it catches
    let mut refused_for_dimension = 0;
    let mut caught = 0;
    for program in programs_in_the_crates_tests() {
        let Err(error) = eval_phs(&program) else { continue };
        let text = error.to_string();
        if !(text.contains("mismatch") || text.contains("ncompatible") || text.contains("dimension")) {
            continue;
        }
        refused_for_dimension += 1;
        if check_source(&program).is_ok_and(|findings| !findings.is_empty()) {
            caught += 1;
        } else {
            println!("not reported: {program:?} ({text})");
        }
    }
    println!("the interpreter refuses {refused_for_dimension} programs of the corpus for a dimension; the checker reports {caught}");
    assert!(refused_for_dimension >= 1, "the corpus has no program refused for a dimension");
    assert!(caught * 2 >= refused_for_dimension, "{caught} of {refused_for_dimension}");
}
