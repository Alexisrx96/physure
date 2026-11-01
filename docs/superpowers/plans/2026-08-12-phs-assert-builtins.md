# PHS `assert` / `exact_assert` builtins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `assert(actual, expected)` and `exact_assert(actual, expected)` as PHS builtins that work identically in the interpreter and in every transpile target (Rust, Python, Java, JS, TS), so a failed assertion is a hard runtime failure in whichever environment computes the wrong physics.

**Architecture:** Implement the comparison once in `physure-core` (`Quantity::phs_assert` / `phs_exact_assert`, backed by the existing `approx_eq` and `RationalUnit::PartialEq`), dispatch it from the interpreter's builtin table, and add one new codegen emission arm per transpile target that calls the equivalent method on that target's runtime `Quantity` type. Java and WASM need new thin wrapper methods on their runtime `Quantity` classes (JNI / wasm-bindgen); Python needs none — its existing `approx_eq`/`__eq__` on `PyQuantity` already implement the exact semantics needed, so generated Python emits native `assert` statements calling them directly.

**Tech Stack:** Rust (physure-core, physure-script, physure-java JNI glue, physure-wasm), Java 8 (physure-java runtime classes), Python (generated-code target only, no binding changes).

Full design: `docs/superpowers/specs/2026-08-12-phs-assert-builtins-design.md`.

---

## File Structure

- `physure-core/src/quantity.rs` — new `phs_assert`/`phs_exact_assert` methods (the single source of truth for the comparison).
- `physure-core/src/error.rs` — new `AssertionFailed` error variant.
- `physure-script/src/builtins.rs` — new `"assert"`/`"exact_assert"` arm in `eval_core_builtin`, the interpreter's dispatch point.
- `physure-script/src/interpreter.rs` — new tests only (no production code change; `eval_core_builtin` is already wired in).
- `physure-script/src/codegen/mod.rs` — new `as_assert_call` shared helper, used by every codegen target.
- `physure-script/src/codegen/rust.rs`, `python.rs`, `java.rs`, `js.rs` — one new emission arm each (`Statement::Expr` interception + a guard in the generic `Expr::FunctionCall` fallback).
- `physure-java/src/main/java/com/physure/NativeEngine.java` — two new `native` method declarations.
- `physure-java/src/main/java/com/physure/Quantity.java` — two new wrapper methods (`physAssert`/`physExactAssert`).
- `physure-java/src/lib.rs` — two new JNI extern functions backing the above.
- `physure-wasm/src/quantity.rs` — two new wasm-bindgen methods (`physAssert`/`physExactAssert`).
- `physure-python/tests/measurement_tests/test_quantity.py` — one new test confirming the pre-existing `approx_eq`/`__eq__` behavior the Python codegen target relies on (no binding code changes needed there).

---

### Task 1: `physure-core` — `phs_assert`/`phs_exact_assert`

**Files:**
- Modify: `physure-core/src/quantity.rs:392` (right after `approx_eq`)
- Test: `physure-core/src/quantity.rs` (existing `#[cfg(test)] mod tests` at the bottom of the same file)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-core/src/quantity.rs` (near the other tests using `ohm_unit()`/`milliamp_unit()`, e.g. right after `format_fraction_answers_only_when_a_fraction_applies`):

```rust
    #[test]
    fn phs_assert_passes_for_equal_dimension_and_magnitude() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        assert!(a.phs_assert(&b).is_ok());
    }

    #[test]
    fn phs_assert_fails_on_dimension_mismatch() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.0, "s").unwrap();
        let err = a.phs_assert(&b).unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }

    #[test]
    fn phs_assert_fails_when_magnitude_differs_beyond_tolerance() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.1, "m").unwrap();
        assert!(a.phs_assert(&b).is_err());
    }

    #[test]
    fn phs_exact_assert_passes_for_alias_units_with_same_magnitude() {
        // "m" and "meter" share the same id/scale/offset in physure.conf; only the
        // display alias differs, which `exact_assert` deliberately ignores.
        let a = Quantity::new(5.0, "m").unwrap();
        let b = Quantity::new(5.0, "meter").unwrap();
        assert!(a.phs_exact_assert(&b).is_ok());
    }

    #[test]
    fn phs_exact_assert_fails_when_conversion_would_be_required() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        let err = a.phs_exact_assert(&b).unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "exact_assert", .. }));
    }

    #[test]
    fn phs_exact_assert_fails_on_bit_inexact_magnitude() {
        let a = Quantity::new(5.0, "m").unwrap();
        let b = Quantity::new(5.0 + 1e-12, "m").unwrap();
        assert!(a.phs_exact_assert(&b).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-core phs_assert phs_exact_assert -- --nocapture`
Expected: FAIL to compile — `no method named 'phs_assert' found for reference '&Quantity'`

- [ ] **Step 3: Implement `phs_assert`/`phs_exact_assert`**

Insert into `physure-core/src/quantity.rs`, immediately after the existing `approx_eq` method (line 401, right before `canonical_magnitude`):

```rust
    /// Backing implementation for PHS's `assert(actual, expected)` builtin: passes when the
    /// two quantities have compatible dimensions and their magnitudes agree once converted
    /// to a common scale, within a fixed tolerance chosen to catch real bugs while
    /// tolerating floating-point drift across languages.
    pub fn phs_assert(&self, other: &Quantity) -> PhysureResult<()> {
        const REL_TOL: f64 = 1e-9;
        const ABS_TOL: f64 = 1e-12;
        if self.approx_eq(other, REL_TOL, ABS_TOL) {
            return Ok(());
        }
        if !self.unit.same_dimensions(&other.unit) {
            return Err(PhysureError::AssertionFailed {
                kind: "assert",
                message: format!("{} and {} have incompatible dimensions", self, other),
            });
        }
        Err(PhysureError::AssertionFailed {
            kind: "assert",
            message: format!("{} != {}", self, other),
        })
    }

    /// Backing implementation for PHS's `exact_assert(actual, expected)` builtin: passes
    /// only when both operands carry the literal same unit — aliases like `m`/`meter` still
    /// match, since `RationalUnit`'s `PartialEq` already ignores the display alias — and the
    /// magnitudes are bit-exact. No conversion, no tolerance.
    pub fn phs_exact_assert(&self, other: &Quantity) -> PhysureResult<()> {
        if self.unit == other.unit && self.value.mean().to_bits() == other.value.mean().to_bits() {
            return Ok(());
        }
        Err(PhysureError::AssertionFailed {
            kind: "exact_assert",
            message: format!("{} != {}", self, other),
        })
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-core phs_assert phs_exact_assert -- --nocapture`
Expected: still FAIL — `AssertionFailed` doesn't exist yet on `PhysureError`. That's Task 2; come back and re-run after it.

- [ ] **Step 5: Commit**

Hold this commit until Task 2 lands (the code doesn't compile without it) — commit both together at the end of Task 2.

---

### Task 2: `physure-core` — `PhysureError::AssertionFailed`

**Files:**
- Modify: `physure-core/src/error.rs:4-20` (enum), `:22-54` (Display impl)
- Test: `physure-core/src/error.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` block in `physure-core/src/error.rs`, after `contract_violation_displays_decorator_and_message`:

```rust
    #[test]
    fn assertion_failed_displays_kind_and_message() {
        let err = PhysureError::AssertionFailed {
            kind: "exact_assert",
            message: "5 m != 5 s".to_string(),
        };
        assert_eq!(err.to_string(), "exact_assert failed: 5 m != 5 s");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p physure-core assertion_failed_displays_kind_and_message`
Expected: FAIL to compile — `no variant named 'AssertionFailed' found for enum 'PhysureError'`

- [ ] **Step 3: Add the variant and its Display arm**

In `physure-core/src/error.rs`, add the variant to the enum right after `ContractViolation`:

```rust
    /// A `@requires`/`@ensures` condition evaluated to false. `decorator` is the
    /// decorator name without the `@` (`"requires"` or `"ensures"`); `message` is the
    /// user-supplied explanation string from the decorator's second argument.
    ContractViolation { decorator: String, message: String },
    /// A PHS `assert`/`exact_assert` builtin call failed. `kind` is `"assert"` or
    /// `"exact_assert"`; `message` names both operands and why they didn't match.
    AssertionFailed { kind: &'static str, message: String },
```

Add the matching Display arm right after `ContractViolation`'s:

```rust
            PhysureError::ContractViolation { decorator, message } => {
                write!(f, "@{} violated: {}", decorator, message)
            }
            PhysureError::AssertionFailed { kind, message } => {
                write!(f, "{} failed: {}", kind, message)
            }
```

- [ ] **Step 4: Run all `physure-core` tests to verify everything passes**

Run: `cargo test -p physure-core`
Expected: PASS — includes the new `assertion_failed_displays_kind_and_message` test and the five `phs_assert`/`phs_exact_assert` tests from Task 1.

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/quantity.rs physure-core/src/error.rs
git commit -m "feat(core): add Quantity::phs_assert/phs_exact_assert and AssertionFailed error"
```

---

### Task 3: `physure-script` codegen — shared `as_assert_call` helper

**Files:**
- Modify: `physure-script/src/codegen/mod.rs:257` (right after `expr_to_unit_string`)

- [ ] **Step 1: Add the helper**

There's no separate test for this step — it's a pure pattern-matcher with no branching logic worth a dedicated unit test; it's exercised end-to-end by every codegen test in Tasks 5–8. Insert into `physure-script/src/codegen/mod.rs`, right after the `expr_to_unit_string` function (after line 283, before `node_to_expr`):

```rust
/// Recognizes `assert(actual, expected)` / `exact_assert(actual, expected)` — the only
/// two-argument, no-kwargs shape v1 supports — used as a standalone statement. Every
/// codegen target intercepts this before falling through to its generic `FunctionCall`
/// handling, since none of them can express PHS's `assert`/`exact_assert` as an ordinary
/// function call (Python's `assert` is a statement keyword, not a name; Rust/Java/JS have
/// no builtin of that name at all).
pub(crate) fn as_assert_call(expr: &Expr) -> Option<(&'static str, &Expr, &Expr)> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() || args.len() != 2 {
        return None;
    }
    match name.as_str() {
        "assert" => Some(("assert", &args[0], &args[1])),
        "exact_assert" => Some(("exact_assert", &args[0], &args[1])),
        _ => None,
    }
}
```

- [ ] **Step 2: Run the workspace build to verify it compiles**

Run: `cargo build -p physure-script`
Expected: PASS (the function is currently unused, which is fine — it's about to be consumed by Task 5 onward. `cargo build` warns on unused `pub(crate)` items only if nothing in the crate calls them across all targets, which will stop being true after Task 5's first caller lands. If you see an `unused` warning at this step, that's expected and not a failure.)

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/codegen/mod.rs
git commit -m "feat(script): add as_assert_call codegen helper"
```

---

### Task 4: `physure-script` interpreter — `"assert"`/`"exact_assert"` builtin dispatch

**Files:**
- Modify: `physure-script/src/builtins.rs:170-179` (inside `eval_core_builtin`, right after the `"format"` arm)
- Test: `physure-script/src/interpreter.rs:1051` (existing `mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-script/src/interpreter.rs`, after `range_lowered_to_requires_is_enforced_at_call_time`:

```rust
    #[test]
    fn assert_passes_when_dimensions_and_magnitude_agree() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("assert(1.0 km, 1000.0 m)").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn assert_fails_with_assertion_failed_error_on_mismatch() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(1.0 m, 1.0 s)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }

    #[test]
    fn exact_assert_passes_for_alias_units() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("exact_assert(5.0 m, 5.0 meter)").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn exact_assert_fails_when_conversion_would_be_required() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("exact_assert(1.0 km, 1000.0 m)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "exact_assert", .. }));
    }

    #[test]
    fn assert_rejects_non_quantity_arguments() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(\"a\", \"b\")").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-script assert_passes_when_dimensions exact_assert -- --nocapture`
Expected: FAIL — `assert`/`exact_assert` fall through `eval_core_builtin`'s final `_ => Ok(None)` arm, and the interpreter reports an unknown-function error instead of succeeding or raising `AssertionFailed`.

- [ ] **Step 3: Add the dispatch arm**

In `physure-script/src/builtins.rs`, inside `eval_core_builtin`'s `match name { ... }`, add this arm right after the closing `}` of the `"format"` arm (before `"op_>" | "op_gt" => compare(args, |l, r| l > r),`):

```rust
        "assert" | "exact_assert" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic(format!("{name} expects 2 arguments (actual, expected)")));
            }
            match (&args[0], &args[1]) {
                (PhsValue::Quantity(a), PhsValue::Quantity(b)) => {
                    if name == "assert" {
                        a.phs_assert(b)?;
                    } else {
                        a.phs_exact_assert(b)?;
                    }
                    Ok(Some(PhsValue::None))
                }
                _ => Err(PhysureError::Generic(format!("{name} expects two quantities"))),
            }
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-script assert_passes_when_dimensions exact_assert assert_rejects_non_quantity assert_fails_with_assertion_failed`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/builtins.rs physure-script/src/interpreter.rs
git commit -m "feat(script): wire assert/exact_assert into the interpreter's builtin dispatch"
```

---

### Task 5: Rust codegen (`rust.rs`)

**Files:**
- Modify: `physure-script/src/codegen/rust.rs:26-29` (`generate_program`'s `Statement::Expr` arm), `:56` (`generate_statement`'s `Statement::Expr` arm), `:165-177` (`generate_expr`'s generic `FunctionCall` fallback)
- Test: `physure-script/src/codegen/rust.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-script/src/codegen/rust.rs`, after `test_transpile_quantity_with_uncertainty`:

```rust
    #[test]
    fn transpiles_assert_call_to_phs_assert_with_question_mark() {
        let transpiler = RustTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".phs_assert(&"), "expected phs_assert call:\n{code}");
        assert!(code.contains('?'), "expected the call to propagate its Result:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_phs_exact_assert() {
        let transpiler = RustTranspiler;
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".phs_exact_assert(&"), "expected phs_exact_assert call:\n{code}");
    }

    #[test]
    fn rejects_assert_used_as_a_nested_expression() {
        let transpiler = RustTranspiler;
        let program = crate::parser::parse_phs("x = assert(1.0 m, 1.0 m)").unwrap();
        assert!(transpiler.generate_program(&program).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-script transpiles_assert_call transpiles_exact_assert_call rejects_assert_used_as_a_nested_expression -- --nocapture`
Expected: FAIL — today `assert(a, b)` generic-fallbacks to `println!("{}", assert(a, b))`, which contains neither `.phs_assert(&` nor a `?`, and the nested case does not currently error.

- [ ] **Step 3: Implement**

In `physure-script/src/codegen/rust.rs`, replace the `Statement::Expr` arm inside `generate_program` (lines 26-29):

```rust
                Statement::Expr(expr) => {
                    if let Some((kind, a, b)) = super::as_assert_call(expr) {
                        let a_code = self.generate_expr(a)?;
                        let b_code = self.generate_expr(b)?;
                        let method = if kind == "assert" { "phs_assert" } else { "phs_exact_assert" };
                        main_statements.push(format!("    ({}).{}(&({}))?;", a_code, method, b_code));
                    } else {
                        let expr_code = self.generate_expr(expr)?;
                        main_statements.push(format!("    println!(\"{{}}\", {});", expr_code));
                    }
                }
```

Replace the `Statement::Expr` arm inside `generate_statement` (line 56):

```rust
            Statement::Expr(expr) => {
                if let Some((kind, a, b)) = super::as_assert_call(expr) {
                    let a_code = self.generate_expr(a)?;
                    let b_code = self.generate_expr(b)?;
                    let method = if kind == "assert" { "phs_assert" } else { "phs_exact_assert" };
                    Ok(format!("({}).{}(&({}))?", a_code, method, b_code))
                } else {
                    self.generate_expr(expr)
                }
            }
```

Add a guard at the top of the `Expr::FunctionCall` arm in `generate_expr` (right after the `if !kwargs.is_empty()` check, before building `arg_codes`, around line 172):

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Rust codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
                let mut arg_codes = Vec::new();
```

(The rest of that arm — the `for arg in args { ... }` loop and the trailing `Ok(format!(...))` — is unchanged.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-script transpiles_assert_call transpiles_exact_assert_call rejects_assert_used_as_a_nested_expression`
Expected: PASS

- [ ] **Step 5: Run the full `physure-script` test suite to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS — including `where_bindings_are_inlined_instead_of_emitted_as_a_let_call` and `named_argument_equation_call_compiles_to_a_real_function` in `codegen/mod.rs`, which exercise all targets in a loop and must be unaffected.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/codegen/rust.rs
git commit -m "feat(codegen): transpile assert/exact_assert to phs_assert/phs_exact_assert in Rust"
```

---

### Task 6: Python codegen (`python.rs`)

**Files:**
- Modify: `physure-script/src/codegen/python.rs:29-32` (`generate_program`'s `Statement::Expr` arm), `:184-198` (`generate_expr`'s generic `FunctionCall` fallback)
- Test: `physure-script/src/codegen/python.rs` (existing `#[cfg(test)] mod tests`)

Python needs no new binding code: `physure-python`'s existing `PyQuantity.approx_eq(other, rel_tol, abs_tol)` and `__eq__` already implement exactly the semantics `assert`/`exact_assert` need (`approx_eq` delegates straight to `physure_core::Quantity::approx_eq`; `__eq__` compares `self.0.unit == q.0.unit` — alias-tolerant via `RationalUnit::PartialEq` — and `value.mean() == value.mean()`, which is bit-exact). Generated code uses Python's native `assert` statement calling these directly.

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-script/src/codegen/python.rs`, after `test_transpile_function_def_and_import`:

```rust
    #[test]
    fn transpiles_assert_call_to_a_python_assert_statement() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert "), "expected a Python assert statement:\n{code}");
        assert!(code.contains(".approx_eq("), "expected approx_eq call:\n{code}");
        assert!(!code.contains("print(assert"), "must not fall through to the generic call path:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_equality_assert() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert ") && code.contains(") == ("), "expected an equality assert:\n{code}");
    }

    #[test]
    fn rejects_assert_used_as_a_nested_expression_py() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("x = assert(1.0 m, 1.0 m)").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-script transpiles_assert_call_to_a_python transpiles_exact_assert_call_to_equality rejects_assert_used_as_a_nested_expression_py -- --nocapture`
Expected: FAIL — today `assert(a, b)` generic-fallbacks to `print(assert(1.0, 1000.0))` (via `sanitize_identifier`), producing invalid/landmine Python, and the nested case does not currently error.

- [ ] **Step 3: Implement**

In `physure-script/src/codegen/python.rs`, replace the `Statement::Expr` arm inside `generate_program` (lines 29-32):

```rust
                Statement::Expr(expr) => {
                    if let Some((kind, a, b)) = super::as_assert_call(expr) {
                        let a_code = self.generate_expr(a)?;
                        let b_code = self.generate_expr(b)?;
                        let line = if kind == "assert" {
                            format!(
                                "assert ({a}).approx_eq({b}, 1e-9, 1e-12), f\"assert failed: {{{a}}} != {{{b}}}\"",
                                a = a_code, b = b_code
                            )
                        } else {
                            format!(
                                "assert ({a}) == ({b}), f\"exact_assert failed: {{{a}}} != {{{b}}}\"",
                                a = a_code, b = b_code
                            )
                        };
                        out.push_str(&line);
                        out.push('\n');
                    } else {
                        let expr_str = self.generate_expr(expr)?;
                        out.push_str(&format!("print({})\n", expr_str));
                    }
                }
```

Add a guard at the top of the `Expr::FunctionCall` arm in `generate_expr` (right before the `let mut arg_strs = Vec::new();` line, around line 185):

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if (name == "assert" || name == "exact_assert") && kwargs.is_empty() && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
                let mut arg_strs = Vec::new();
```

(The rest of that arm is unchanged.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-script transpiles_assert_call_to_a_python transpiles_exact_assert_call_to_equality rejects_assert_used_as_a_nested_expression_py`
Expected: PASS

- [ ] **Step 5: Run the full `physure-script` test suite to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/codegen/python.rs
git commit -m "feat(codegen): transpile assert/exact_assert to native Python asserts"
```

---

### Task 7: Java codegen (`java.rs`)

**Files:**
- Modify: `physure-script/src/codegen/java.rs:65-68` (`generate_program`'s `Statement::Expr` arm), `:193-205` (`generate_expr`'s generic `FunctionCall` fallback)
- Test: `physure-script/src/codegen/java.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-script/src/codegen/java.rs`, after `test_transpile_quantity_with_uncertainty`:

```rust
    #[test]
    fn transpiles_assert_call_to_phys_assert_call() {
        let transpiler = JavaTranspiler::default();
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".physAssert("), "expected physAssert call:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_phys_exact_assert_call() {
        let transpiler = JavaTranspiler::default();
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".physExactAssert("), "expected physExactAssert call:\n{code}");
    }

    #[test]
    fn rejects_assert_used_as_a_nested_expression_java() {
        let transpiler = JavaTranspiler::default();
        let program = crate::parser::parse_phs("x = assert(1.0 m, 1.0 m)").unwrap();
        assert!(transpiler.generate_program(&program).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-script transpiles_assert_call_to_phys transpiles_exact_assert_call_to_phys rejects_assert_used_as_a_nested_expression_java -- --nocapture`
Expected: FAIL — today `assert(a, b)` generic-fallbacks through `snake_to_camel("assert")` (still `"assert"`, a reserved Java keyword) to `System.out.println(assert(1.0, 1000.0));`, which does not compile as Java and contains no `.physAssert(`.

- [ ] **Step 3: Implement**

In `physure-script/src/codegen/java.rs`, replace the `Statement::Expr` arm inside `generate_program` (lines 65-68):

```rust
                Statement::Expr(expr) => {
                    if let Some((kind, a, b)) = super::as_assert_call(expr) {
                        let a_code = self.generate_expr(a)?;
                        let b_code = self.generate_expr(b)?;
                        let method = if kind == "assert" { "physAssert" } else { "physExactAssert" };
                        main_stmts.push(format!("        {}.{}({});", a_code, method, b_code));
                    } else {
                        let val = self.generate_expr(expr)?;
                        main_stmts.push(format!("        System.out.println({});", val));
                    }
                }
```

Add a guard at the top of the `Expr::FunctionCall` arm in `generate_expr` (right after the `if !kwargs.is_empty()` check, around line 199):

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Java codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
                let mut arg_strs = Vec::new();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-script transpiles_assert_call_to_phys transpiles_exact_assert_call_to_phys rejects_assert_used_as_a_nested_expression_java`
Expected: PASS

- [ ] **Step 5: Run the full `physure-script` test suite to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/codegen/java.rs
git commit -m "feat(codegen): transpile assert/exact_assert to physAssert/physExactAssert in Java"
```

---

### Task 8: JS/TS codegen (`js.rs`)

**Files:**
- Modify: `physure-script/src/codegen/js.rs:36-39` (`generate_program`'s `Statement::Expr` arm), `:166-178` (`generate_expr`'s generic `FunctionCall` fallback)
- Test: `physure-script/src/codegen/js.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-script/src/codegen/js.rs`, after `test_transpile_quantity_with_uncertainty_ts`:

```rust
    #[test]
    fn transpiles_assert_call_to_phys_assert_call_js() {
        let transpiler = JsTranspiler::default();
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".physAssert("), "expected physAssert call:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_phys_exact_assert_call_ts() {
        let transpiler = JsTranspiler { typed: true };
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = transpiler.generate_program(&program).unwrap();
        assert!(code.contains(".physExactAssert("), "expected physExactAssert call:\n{code}");
    }

    #[test]
    fn rejects_assert_used_as_a_nested_expression_js() {
        let transpiler = JsTranspiler::default();
        let program = crate::parser::parse_phs("x = assert(1.0 m, 1.0 m)").unwrap();
        assert!(transpiler.generate_program(&program).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-script transpiles_assert_call_to_phys_assert_call_js transpiles_exact_assert_call_to_phys_exact_assert_call_ts rejects_assert_used_as_a_nested_expression_js -- --nocapture`
Expected: FAIL — today `assert(a, b)` generic-fallbacks to `console.log(assert(1.0, 1000.0));`, a runtime `ReferenceError` in real JS, not a `.physAssert(` call.

- [ ] **Step 3: Implement**

In `physure-script/src/codegen/js.rs`, replace the `Statement::Expr` arm inside `generate_program` (lines 36-39):

```rust
                Statement::Expr(expr) => {
                    if let Some((kind, a, b)) = super::as_assert_call(expr) {
                        let a_code = self.generate_expr(a)?;
                        let b_code = self.generate_expr(b)?;
                        let method = if kind == "assert" { "physAssert" } else { "physExactAssert" };
                        main_stmts.push(format!("{}.{}({});", a_code, method, b_code));
                    } else {
                        let val = self.generate_expr(expr)?;
                        main_stmts.push(format!("console.log({});", val));
                    }
                }
```

Add a guard at the top of the `Expr::FunctionCall` arm in `generate_expr` (right after the `if !kwargs.is_empty()` check, around line 172):

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in JS/TS codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
                let mut arg_strs = Vec::new();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-script transpiles_assert_call_to_phys_assert_call_js transpiles_exact_assert_call_to_phys_exact_assert_call_ts rejects_assert_used_as_a_nested_expression_js`
Expected: PASS

- [ ] **Step 5: Run the full `physure-script` test suite to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS — all targets, including the two shared loop-based tests in `codegen/mod.rs`.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/codegen/js.rs
git commit -m "feat(codegen): transpile assert/exact_assert to physAssert/physExactAssert in JS/TS"
```

---

### Task 9: `physure-java` — native method declarations

**Files:**
- Modify: `physure-java/src/main/java/com/physure/NativeEngine.java:115-122`

There is no existing Java test harness in `physure-java` (no `src/test` directory) to TDD against, and these three files (native decl, JNI impl, Java wrapper) only link together once all three exist — so this task and Tasks 10–11 build the pieces in dependency order and defer verification to the end of Task 11.

- [ ] **Step 1: Add the two native method declarations**

In `physure-java/src/main/java/com/physure/NativeEngine.java`, add to the `// --- Quantity JNI math operations ---` block (after `convertQuantity`, before `getFunctionParams`):

```java
    public static native void assertQuantities(Quantity a, Quantity b);
    public static native void assertExactQuantities(Quantity a, Quantity b);
```

- [ ] **Step 2: Commit**

```bash
git add physure-java/src/main/java/com/physure/NativeEngine.java
git commit -m "feat(java): declare native assertQuantities/assertExactQuantities"
```

---

### Task 10: `physure-java` — JNI implementation

**Files:**
- Modify: `physure-java/src/lib.rs` (new functions after the existing `addQuantities`/`subQuantities` block, e.g. right after `subQuantities`'s closing brace)

- [ ] **Step 1: Add the two JNI extern functions**

In `physure-java/src/lib.rs`, add after the existing `Java_com_physure_NativeEngine_subQuantities` function:

```rust
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_assertQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    if let Err(e) = q1.phs_assert(&q2) {
        throw_physure_exception(&mut env, &format!("{}", e));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_assertExactQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    if let Err(e) = q1.phs_exact_assert(&q2) {
        throw_physure_exception(&mut env, &format!("{}", e));
    }
}
```

- [ ] **Step 2: Verify the crate compiles**

Run: `cargo build -p physure-java`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add physure-java/src/lib.rs
git commit -m "feat(java): implement assertQuantities/assertExactQuantities JNI bridge"
```

---

### Task 11: `physure-java` — `Quantity.java` wrapper methods and smoke test

**Files:**
- Modify: `physure-java/src/main/java/com/physure/Quantity.java` (after the existing `approxEquals`, before `to`)

- [ ] **Step 1: Add the two wrapper methods**

In `physure-java/src/main/java/com/physure/Quantity.java`, add right after `approxEquals` (line 185), before `public Quantity to(String targetUnit)`:

```java
    /**
     * PHS's {@code assert(actual, expected)}: passes when both quantities have compatible
     * dimensions and their magnitudes agree after unit conversion, within a fixed
     * tolerance. Throws {@link PhysureException} on failure — delegates to the same
     * {@code physure-core} comparison every other language target uses, rather than
     * reimplementing the tolerance logic here.
     */
    public void physAssert(Quantity expected) {
        NativeEngine.assertQuantities(this, expected);
    }

    /**
     * PHS's {@code exact_assert(actual, expected)}: passes only when both quantities carry
     * the literal same unit (aliases like {@code m}/{@code meter} still match) and the
     * magnitudes are bit-exact. Throws {@link PhysureException} on failure.
     */
    public void physExactAssert(Quantity expected) {
        NativeEngine.assertExactQuantities(this, expected);
    }
```

- [ ] **Step 2: Build the Java sources**

Run: `mvn -f physure-java/pom.xml compile`
Expected: PASS (this only compiles Java sources against the `native` declarations — it does not require the native library to be loadable)

- [ ] **Step 3: Manual smoke test**

There's no existing `physure-java` test harness to add an automated test to (per the codebase's own current state — this crate has zero Java or Rust tests today). Build the native library and run a throwaway smoke check instead:

```bash
cargo build -p physure-java --release
```

Then, from `D:\Projects\test_physure\phava_21` (or `phava_8`) — the existing external harness environment that already has `physure-java`'s Maven artifact wired up — write a 5-line scratch `.java` file that constructs two `Quantity`s and calls `.physAssert(...)`/`.physExactAssert(...)` once each (one passing pair, one failing pair expected to throw `PhysureException`), run it, confirm the pass case returns normally and the fail case throws with a message containing "assert failed" or "exact_assert failed", then delete the scratch file. This is a one-time manual verification, not a permanent test — `physure-java` has no test suite to extend today, and adding one is out of scope for this plan.

- [ ] **Step 4: Commit**

```bash
git add physure-java/src/main/java/com/physure/Quantity.java
git commit -m "feat(java): add physAssert/physExactAssert wrapper methods"
```

---

### Task 12: `physure-wasm` — `physAssert`/`physExactAssert`

**Files:**
- Modify: `physure-wasm/src/quantity.rs:67-70` (after `convert_to`)
- Test: `physure-wasm/src/quantity.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `physure-wasm/src/quantity.rs`, after `add_and_subtract_require_compatible_dimensions`:

```rust
    #[wasm_bindgen_test]
    fn phys_assert_passes_for_equal_dimension_and_magnitude() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        assert!(a.phs_assert(&b).is_ok());
    }

    #[wasm_bindgen_test]
    fn phys_assert_throws_a_js_error_on_mismatch() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.0, "s").unwrap();
        let err = a.phs_assert(&b).unwrap_err();
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("assert failed"));
    }

    #[wasm_bindgen_test]
    fn phys_exact_assert_throws_when_conversion_would_be_required() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        let err = a.phs_exact_assert(&b).unwrap_err();
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("exact_assert failed"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-wasm --target wasm32-unknown-unknown phys_assert phys_exact_assert` (or `wasm-pack test --node physure-wasm` if that's the project's established wasm test runner — check `physure-wasm/README.md` / CI config for the exact invocation already in use)
Expected: FAIL to compile — no `phs_assert`/`phs_exact_assert` methods exist yet on the wasm `Quantity`.

- [ ] **Step 3: Implement**

In `physure-wasm/src/quantity.rs`, add to the `#[wasm_bindgen] impl Quantity` block, right after `convert_to` (line 70, before the closing `}` of the impl block):

```rust
    #[wasm_bindgen(js_name = physAssert)]
    pub fn phs_assert(&self, other: &Quantity) -> Result<(), JsValue> {
        self.inner.phs_assert(&other.inner).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = physExactAssert)]
    pub fn phs_exact_assert(&self, other: &Quantity) -> Result<(), JsValue> {
        self.inner.phs_exact_assert(&other.inner).map_err(to_js_error)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run the same command as Step 2.
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/quantity.rs
git commit -m "feat(wasm): add physAssert/physExactAssert methods"
```

---

### Task 13: `physure-python` — confirm existing behavior backs the codegen contract

**Files:**
- Modify: `physure-python/tests/measurement_tests/test_quantity.py`

No PyO3/binding changes are needed — Task 6's generated Python calls `Quantity.approx_eq(other, rel_tol, abs_tol)` and `Quantity == Quantity` directly, both of which already exist. This task adds one test documenting that contract, so a future change to either method's semantics breaks a test instead of silently breaking generated code.

- [ ] **Step 1: Add the test**

Add to `physure-python/tests/measurement_tests/test_quantity.py` (check the file's existing imports first — it almost certainly already imports `Q_` or `Quantity` from `physure`; reuse whatever it already uses rather than adding a second import style):

```python
def test_approx_eq_and_eq_back_the_phs_assert_codegen_contract():
    # physure-script's Python codegen target emits `a.approx_eq(b, 1e-9, 1e-12)` for PHS's
    # `assert(a, b)` and `a == b` for `exact_assert(a, b)` — see
    # physure-script/src/codegen/python.rs. Both methods already exist on Quantity with
    # exactly these semantics; this test exists so a change to either one's behavior fails
    # loudly here instead of silently breaking every transpiled Python program's assertions.
    a = Q_(1.0, "km")
    b = Q_(1000.0, "m")
    assert a.approx_eq(b, 1e-9, 1e-12)

    c = Q_(1.0, "m")
    d = Q_(1.0, "s")
    assert not c.approx_eq(d, 1e-9, 1e-12)

    e = Q_(5.0, "m")
    f = Q_(5.0, "meter")
    assert e == f  # alias units compare equal

    g = Q_(1.0, "km")
    h = Q_(1000.0, "m")
    assert not (g == h)  # exact_assert requires the literal same unit, no conversion
```

- [ ] **Step 2: Run the test**

Run: `uv run pytest physure-python/tests/measurement_tests/test_quantity.py -k test_approx_eq_and_eq_back_the_phs_assert_codegen_contract -xvs`
Expected: PASS (this exercises pre-existing behavior, so it should pass immediately — if it doesn't, `python.rs`'s Task 6 assumptions are wrong and need to be revisited before continuing)

- [ ] **Step 3: Commit**

```bash
git add physure-python/tests/measurement_tests/test_quantity.py
git commit -m "test(python): confirm approx_eq/__eq__ back the PHS assert/exact_assert codegen contract"
```

---

### Task 14: Full workspace verification

- [ ] **Step 1: Run the full Rust workspace test suite**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 2: Run the full Python test suite**

Run: `uv run pytest`
Expected: PASS

- [ ] **Step 3: Lint check**

Run: `uv run ruff check .` and `uv run ruff format --check .`
Expected: PASS

- [ ] **Step 4: Manual end-to-end check of the interpreter and each transpile target**

```bash
echo 'assert(1.0 km, 1000.0 m)
assert(1.0 m, 2.0 m)' > scratch_assert.phs
```

Run: `cargo run -p physure-cli -- run scratch_assert.phs`
Expected: the interpreter runs the first `assert` silently (no output, since it returns `PhsValue::None`), then fails on the second with an `AssertionFailed`/"assert failed" error message.

Run: `cargo run -p physure-cli -- transpile scratch_assert.phs --target rust -o scratch_assert.rs` (repeat with `--target python -o scratch_assert.py`, `--target java -o scratch_assert.java`, `--target js -o scratch_assert.js`, `--target ts -o scratch_assert.ts`) and inspect each output file to confirm it contains the expected `phs_assert`/`approx_eq`/`physAssert` call shape from Tasks 5–8, then delete all the scratch files:

```bash
rm -f scratch_assert.phs scratch_assert.rs scratch_assert.py scratch_assert.java scratch_assert.js scratch_assert.ts
```

- [ ] **Step 5: No commit for this task** — it's verification only; nothing here should change tracked files. If Step 4 surfaces a bug, fix it as part of whichever Task's file it lives in and amend that task's commit workflow (new commit, not `--amend`, per this repo's commit conventions).

---

## Out of scope (per the design spec)

- Non-Quantity operands (bool/string/etc.) for `assert`/`exact_assert`.
- Inline/composable use inside larger expressions (explicitly rejected by the codegen guards added in Tasks 5–8).
- A configurable-tolerance argument (`assert(a, b, tol=...)`).
- A boolean-condition `assert(cond)` form.
- The `test_physure` verification harness itself (separate spec, depends on this plan).
