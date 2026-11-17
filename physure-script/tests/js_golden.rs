//! The JavaScript that `phs transpile --target js` writes, pinned and checked against the interpreter (audit T19).
//!
//! For every `tests/golden/js/NAME.phs` there are two committed files:
//! - `NAME.mjs`, the JavaScript the generator writes for it (an ES module importing `physure`);
//! - `NAME.out`, what that module prints: one `name: value` line per top-level assignment and the value of each other
//!   top-level expression, the lines the generated `console.log` calls write, with the values the interpreter gives.
//!
//! This test keeps the three consistent from Rust. `physure-wasm/tests/js/generated.test.mjs` runs each `NAME.mjs`
//! with Node against the freshly built WebAssembly package and compares its stdout with `NAME.out`, so the generated
//! code is held to the interpreter's answers.
//!
//! The version in the header line of the generated code is written `v<version>` in the golden files, so a version
//! bump does not change them. `PHYSURE_BLESS_GOLDEN=1` rewrites `NAME.mjs` and `NAME.out` from the current
//! generator and interpreter; review the diff before committing it. Line endings are compared as `\n`.

use physure_script::ast::{Expr, Statement};
use physure_script::codegen::{transpile, Target};
use physure_script::interpreter::PhsInterpreter;
use physure_script::parser::parse_phs;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/golden/js")
}

/// The generated JavaScript, with the compiler version of its header written `v<version>`.
fn generated_js(source: &str) -> String {
    let program = parse_phs(source).unwrap_or_else(|e| panic!("does not parse: {e}"));
    let code = transpile(&program, Target::JavaScript).unwrap_or_else(|e| panic!("does not transpile: {e}"));
    code.replacen(concat!(" v", env!("CARGO_PKG_VERSION"), "\n"), " v<version>\n", 1)
}

/// The lines the generated module prints, as the interpreter answers them: `name: value` per top-level assignment.
fn interpreter_lines(source: &str) -> String {
    let program = parse_phs(source).unwrap_or_else(|e| panic!("does not parse: {e}"));
    let mut interpreter = PhsInterpreter::default();
    let mut out = String::new();
    for statement in &program.statements {
        let value = interpreter.run_statement(statement).unwrap_or_else(|e| panic!("the interpreter refuses it: {e}"));
        match statement {
            Statement::Assignment(node) => {
                let value = interpreter.get_var(&node.name).expect("an assigned variable");
                out.push_str(&format!("{}: {}\n", node.name, value));
            }
            // an `assert` is a check in the generated code, and prints nothing
            Statement::Expr(Expr::FunctionCall { name, .. }) if name == "assert" || name == "exact_assert" => {}
            // any other expression is printed: `console.log(`${expr}`)`, the text of its value
            Statement::Expr(_) => out.push_str(&format!("{value}\n")),
            Statement::FunctionDef(_) | Statement::While { .. } => {}
            other => panic!("a golden program prints through its assignments and expressions only, not {other:?}"),
        }
    }
    out
}

fn read_lf(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n")
}

#[test]
fn the_generated_javascript_and_its_output_match_the_golden_files() {
    let bless = std::env::var_os("PHYSURE_BLESS_GOLDEN").is_some();
    let mut programs = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(golden_dir())
        .expect("tests/golden/js")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "phs"))
        .collect();
    entries.sort();
    for phs in entries {
        let source = read_lf(&phs);
        let (mjs, out) = (phs.with_extension("mjs"), phs.with_extension("out"));
        let (code, lines) = (generated_js(&source), interpreter_lines(&source));
        if bless {
            std::fs::write(&mjs, &code).unwrap();
            std::fs::write(&out, &lines).unwrap();
        } else {
            assert_eq!(read_lf(&mjs), code, "{}: the generator writes other code (bless to update)", mjs.display());
            assert_eq!(read_lf(&out), lines, "{}: the interpreter answers otherwise (bless to update)", out.display());
        }
        programs += 1;
    }
    assert!(programs >= 10, "tests/golden/js should hold its 10 programs, found {programs}");
}
