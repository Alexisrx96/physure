//! A literal that declares its degrees of freedom is written out by the targets that can carry them and
//! refused by the ones that cannot yet: a quantity with nine degrees of freedom must never come out as one
//! with infinitely many.

use physure_script::codegen::{transpile, Target};
use physure_script::parser::parse_phs;

fn emit(source: &str, target: Target) -> Result<String, String> {
    let program = parse_phs(source).unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"));
    transpile(&program, target).map_err(|e| e.to_string())
}

const NINE: &str = "x = 10.003 +/- 0.002 dof 9 V\n";

#[test]
fn python_writes_the_degrees_of_freedom_as_a_keyword() {
    let code = emit(NINE, Target::Python).unwrap();
    assert!(code.contains("Q_(10.003, \"V\", uncertainty=0.002, dof=9.0)"), "{code}");
    let infinite = emit("x = 10.003 +/- 0.002 dof inf V\n", Target::Python).unwrap();
    assert!(infinite.contains("dof=float(\"inf\")"), "{infinite}");
    assert!(!emit("x = 10.003 +/- 0.002 V\n", Target::Python).unwrap().contains("dof="));
}

#[test]
fn rust_declares_them_on_the_quantity_it_builds() {
    let code = emit(NINE, Target::Rust).unwrap();
    assert!(code.contains("Quantity::with_uncertainty(10.003, 0.002, \"V\").unwrap().with_dof(9.0).unwrap()"), "{code}");
    let infinite = emit("x = 10.003 +/- 0.002 dof inf V\n", Target::Rust).unwrap();
    assert!(infinite.contains(".with_dof(f64::INFINITY).unwrap()"), "{infinite}");
    assert!(!emit("x = 10.003 +/- 0.002 V\n", Target::Rust).unwrap().contains("with_dof"));
}

#[test]
fn the_targets_that_cannot_carry_them_yet_refuse() {
    for (name, target) in [
        ("Java", Target::Java),
        ("Java", Target::JavaWithClass("Model".to_string())),
        ("JavaScript", Target::JavaScript),
        ("JavaScript", Target::TypeScript),
    ] {
        let error = emit(NINE, target).expect_err(name);
        assert!(error.contains("degrees of freedom") && error.contains("cannot be carried") && error.contains(name), "{name}: {error}");
    }
}
