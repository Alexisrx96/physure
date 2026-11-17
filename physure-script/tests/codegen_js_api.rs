//! The JavaScript and TypeScript targets call only what the WebAssembly `Quantity` of the `physure` package has
//! (`physure-wasm/src/quantity.rs`): its constructor `new Quantity(value, unit)` (the unit is required, `""` for a
//! plain number), `Quantity.withUncertainty(value, uncertainty, unit)`, the `value` getter, and the arithmetic,
//! conversion and comparison methods. A builtin with no method there is refused when transpiling (audit T19).
//!
//! Running the generated code against the package is `js_golden.rs` and `physure-wasm/tests/js`.

use physure_script::codegen::{transpile, Target};
use physure_script::parser::parse_phs;

fn emit(source: &str, target: Target) -> Result<String, String> {
    let program = parse_phs(source).unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"));
    transpile(&program, target).map_err(|e| e.to_string())
}

const JS_TARGETS: [Target; 2] = [Target::JavaScript, Target::TypeScript];

#[test]
fn a_literal_is_built_with_the_constructor_and_always_a_unit() {
    for target in JS_TARGETS {
        let code = emit("x = 2 m\nn = 2\n", target.clone()).unwrap();
        assert!(code.contains("new Quantity(2, \"m\")"), "{target:?}:\n{code}");
        assert!(code.contains("new Quantity(2, \"\")"), "{target:?}: a plain number has the empty unit:\n{code}");
        assert!(!code.contains("Quantity.of("), "{target:?}: `of` is not a method of the WASM Quantity:\n{code}");
    }
}

#[test]
fn a_literal_with_uncertainty_passes_its_unit() {
    for target in JS_TARGETS {
        let code = emit("x = 1 +/- 0.1 m\nk = 3 +/- 0.5\n", target.clone()).unwrap();
        assert!(code.contains("Quantity.withUncertainty(1, 0.1, \"m\")"), "{target:?}:\n{code}");
        assert!(code.contains("Quantity.withUncertainty(3, 0.5, \"\")"), "{target:?}:\n{code}");
    }
}

#[test]
fn a_variable_exponent_is_read_with_the_value_getter() {
    let source = "x = 2 m\nn = 2\ny = x ^ n\n";
    for target in JS_TARGETS {
        let code = emit(source, target.clone()).unwrap();
        assert!(code.contains("x.pow(n.value)"), "{target:?}:\n{code}");
        assert!(!code.contains("getValue()"), "{target:?}: `getValue` is not a method of the WASM Quantity:\n{code}");
    }
    // the Java Quantity has `getValue()`, and keeps it
    let java = emit(source, Target::Java).unwrap();
    assert!(java.contains("x.pow(n.getValue())"), "{java}");
}

#[test]
fn unary_minus_multiplies_by_a_plain_minus_one() {
    for target in JS_TARGETS {
        let code = emit("x = 2 m\ny = -x\n", target.clone()).unwrap();
        assert!(code.contains("new Quantity(-1, \"\").multiply(x)"), "{target:?}:\n{code}");
    }
}

#[test]
fn a_ternary_is_a_conditional_expression() {
    for target in JS_TARGETS {
        let code = emit("x = 2 m\nv = x > 1 m ? 1 m : 2 m\n", target.clone()).unwrap();
        assert!(
            code.contains("(x.phsCompare(new Quantity(1, \"m\"), \">\") ? new Quantity(1, \"m\") : new Quantity(2, \"m\"))"),
            "{target:?}:\n{code}"
        );
        assert!(!code.contains("ternary("), "{target:?}: no `ternary` function exists in JavaScript:\n{code}");
    }
}

#[test]
fn a_builtin_the_wasm_quantity_has_no_method_for_is_refused() {
    for (source, name) in [
        ("x = 1\ny = sin(x)\n", "sin"),
        ("x = 4 m^2\ny = sqrt(x)\n", "sqrt"),
        ("x = 2 m\ny = abs(x)\n", "abs"),
        ("y = max(1 m, 2 m)\n", "max"),
    ] {
        for target in JS_TARGETS {
            let error = emit(source, target.clone()).expect_err(&format!("{target:?} transpiled a call to {name}"));
            assert!(error.contains(&format!("'{name}'")), "{target:?}: the error does not name {name}: {error}");
            assert!(error.contains("WebAssembly"), "{target:?}: {error}");
            assert!(!error.contains("   "), "{target:?}: the message has a run of spaces: {error}");
        }
        for target in [Target::Python, Target::Rust, Target::Java] {
            emit(source, target.clone()).unwrap_or_else(|e| panic!("{target:?} refused {name}: {e}"));
        }
    }
}

#[test]
fn a_user_function_is_still_called() {
    for target in JS_TARGETS {
        let code = emit("fn area(w, h) = w * h\na = area(2 m, 3 m)\n", target.clone()).unwrap();
        assert!(code.contains("area(new Quantity(2, \"m\"), new Quantity(3, \"m\"))"), "{target:?}:\n{code}");
    }
}

#[test]
fn a_top_level_reassignment_declares_once_with_let() {
    for target in JS_TARGETS {
        let code = emit("q = 2 m\nq = 5 m\nr = 1 m\n", target.clone()).unwrap();
        let first = if target == Target::TypeScript {
            "let q: Quantity = new Quantity(2, \"m\");"
        } else {
            "let q = new Quantity(2, \"m\");"
        };
        assert!(code.contains(first), "{target:?}:\n{code}");
        assert!(code.contains("\nq = new Quantity(5, \"m\");"), "{target:?}: the second is a plain assignment:\n{code}");
        assert!(!code.contains("const q"), "{target:?}:\n{code}");
        // a name assigned once stays `const`
        assert!(code.contains("const r"), "{target:?}:\n{code}");
    }
}

#[test]
fn an_expression_statement_prints_the_text_of_its_value() {
    for target in JS_TARGETS {
        let code = emit("x = 2 m\nx * 3\n", target.clone()).unwrap();
        assert!(code.contains("console.log(`${x.multiply(new Quantity(3, \"\"))}`);"), "{target:?}:\n{code}");
    }
}

