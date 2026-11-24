//! A sigma bound (`t +/- 2 sigma`) is a comparison tolerance of the interpreter, not an uncertainty: no target has an
//! operator for it, so every target refuses it. It used to be written as `withUncertainty(5.05, 2, "m")`, an
//! uncertainty of 2 m, and the generated code answered `false` where the interpreter answered `True`.

use physure_script::codegen::{transpile, Target};
use physure_script::parser::parse_phs;

const BOUND: &str = "a = 5.0 +/- 0.03 m\nb = a == 5.05 m +/- 2 sigma\n";

fn every_target() -> [Target; 6] {
    [
        Target::Python,
        Target::Rust,
        Target::Java,
        Target::JavaWithClass("Model".to_string()),
        Target::JavaScript,
        Target::TypeScript,
    ]
}

#[test]
fn the_interpreter_evaluates_the_sigma_bound() {
    physure_script::interpreter::eval_phs(BOUND).unwrap();
}

#[test]
fn every_target_refuses_a_sigma_bound() {
    let program = parse_phs(BOUND).unwrap();
    let mut accepted = Vec::new();
    for target in every_target() {
        match transpile(&program, target.clone()) {
            Ok(_) => accepted.push(format!("{target:?}")),
            Err(error) => assert!(error.to_string().contains("sigma"), "{target:?}: {error}"),
        }
    }
    assert!(accepted.is_empty(), "transpiled a sigma bound: {accepted:?}");
}

#[test]
fn an_uncertainty_without_sigma_is_still_transpiled() {
    let program = parse_phs("a = 5 m\nb = 5.05 +/- 0.1 m\nc = a + b\n").unwrap();
    for target in every_target() {
        transpile(&program, target.clone()).unwrap_or_else(|e| panic!("{target:?}: {e}"));
    }
}
