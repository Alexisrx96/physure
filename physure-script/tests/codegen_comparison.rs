//! A comparison transpiled to Rust, Python, Java, JavaScript or TypeScript answers what the interpreter answers: every
//! target calls the core's comparison (`Quantity::phs_compare`, through the WASM `phsCompare`, the JNI
//! `phsCompare` and Python's `physure.ext.helpers.phs_compare`), which refuses different dimensions, a zero written
//! without a unit and a level against a plain number. Two Bools compare with the target's own operator.
//!
//! The output text is tested here; running it is `transpile_parity_tests.rs`'s (Rust with cargo, Python with uv,
//! Java with javac and the JNI library, JavaScript with `node --check`, each where the tool is installed).

use physure_script::codegen::{transpile, Target};
use physure_script::parser::parse_phs;

fn emit(source: &str, target: Target) -> Result<String, String> {
    let program = parse_phs(source).unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"));
    transpile(&program, target).map_err(|e| e.to_string())
}

const PROGRAM: &str = "x = 2 m\nbigger = x > 0 m\n";

#[test]
fn rust_calls_the_core_comparison() {
    let code = emit(PROGRAM, Target::Rust).unwrap();
    assert!(
        code.contains("(&(x)).phs_compare(&(Quantity::new(0.0, \"m\").unwrap()), \">\").unwrap_or_else(|e| panic!(\"{e}\"))"),
        "{code}"
    );
    assert!(!code.contains("canonical_magnitude"), "magnitudes compared with no dimension check:\n{code}");
}

#[test]
fn python_calls_the_core_comparison() {
    let code = emit(PROGRAM, Target::Python).unwrap();
    assert!(code.contains("phs_compare(x, \">\", Q_(0.0, \"m\"))"), "{code}");
    assert!(code.contains("from physure.ext.helpers import dof, coverage_factor, expanded, from_samples, phs_compare"), "{code}");
}

#[test]
fn java_calls_the_core_comparison() {
    let code = emit(PROGRAM, Target::Java).unwrap();
    assert!(code.contains("x.phsCompare(Quantity.of(0.0, \"m\"), \">\")"), "{code}");
    for old in ["greaterThan", "lessThan", ".equals("] {
        assert!(!code.contains(old), "{old} converts or compares exactly, not as PHS does:\n{code}");
    }
}

#[test]
fn javascript_and_typescript_call_the_core_comparison() {
    for target in [Target::JavaScript, Target::TypeScript] {
        let code = emit(PROGRAM, target).unwrap();
        assert!(code.contains("x.phsCompare(Quantity.of(0, \"m\"), \">\")"), "{code}");
        assert!(!code.contains("getValue()"), "`getValue` is not a method of the WASM Quantity:\n{code}");
    }
}

#[test]
fn every_operator_reaches_the_core_with_its_own_symbol() {
    for op in ["<", ">", "<=", ">=", "==", "!="] {
        let source = format!("x = 2 m\nb = x {op} 3 m\n");
        assert!(emit(&source, Target::Rust).unwrap().contains(&format!("\"{op}\").unwrap_or_else")), "{op}");
        assert!(emit(&source, Target::Python).unwrap().contains(&format!("phs_compare(x, \"{op}\", ")), "{op}");
        assert!(emit(&source, Target::Java).unwrap().contains(&format!("\"{op}\")")), "{op}");
        assert!(emit(&source, Target::JavaScript).unwrap().contains(&format!(".phsCompare(Quantity.of(3, \"m\"), \"{op}\")")), "{op}");
    }
}

#[test]
fn two_bools_compare_with_the_target_s_own_operator() {
    let source = "a = 1 m > 0 m\nb = (1 m > 0 m) == True\n";
    let rust = emit(source, Target::Rust).unwrap();
    assert!(rust.contains(" == true)"), "{rust}");
    let python = emit(source, Target::Python).unwrap();
    assert!(python.contains(" == True)"), "{python}");
    let js = emit(source, Target::JavaScript).unwrap();
    assert!(js.contains(" === true)"), "{js}");
    let java = emit(source, Target::Java).unwrap();
    assert!(java.contains(" == true)"), "{java}");
}

#[test]
fn a_program_the_interpreter_refuses_is_not_transpiled() {
    // `transpile` runs the program first: a zero without a unit and a level against a plain number stop it there
    for target in [Target::Rust, Target::Python, Target::Java, Target::JavaScript, Target::TypeScript] {
        let zero = emit("x = 2 m\nbigger = x > 0\n", target.clone()).unwrap_err();
        assert!(zero.contains("a zero needs a unit"), "{zero}");
        let level = emit("same = 20 dB == 100\n", target).unwrap_err();
        assert!(level.contains("write the level with its unit"), "{level}");
    }
}
