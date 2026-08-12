# Track A — Loops Design Spec (`ForExpr` & `Statement::While`)

**Date**: 2026-08-12  
**Status**: Approved (Updated with Large-Scale Iteration & Capacity Allocation)  
**Subsystem**: `physure-script` (Grammar, AST, Interpreter, Codegen)

---

## 1. Overview & Goals

Track A adds real iterative and convergence loop capabilities to PhysureScript (PHS):
1. **Functional Vectorized For-Expression (`ForExpr`)**: Evaluates a body expression over each element of a range or vector, producing a `PhsValue::Vector`. Optimized for large-scale iterations (millions of elements) via pre-allocated vector capacities (`Vec::with_capacity`).
2. **Imperative Convergence While-Statement (`Statement::While`)**: Repeatedly executes a block of statements while a condition is truthy, equipped with a configurable max-iteration cap (default: 10,000) to guarantee termination of non-convergent loops.
3. **Multi-Target Transpilation**: Emitters for Python, Java, Rust, and JavaScript/TypeScript code generators with execution-equivalence testing against the interpreter.

---

## 2. Syntax & Grammar (`phs.pest`)

### 2.1 Pest Grammar Rules

```pest
for_expr   = { "for" ~ identifier ~ "in" ~ expr ~ "{" ~ expr ~ "}" }
while_stmt = { "while" ~ expr ~ "{" ~ _nl* ~ (stmt ~ _nl*)* ~ "}" }
```

- `for_expr` is added as a `primary_expr` / `expr` variant.
- `while_stmt` is added as a `stmt` variant.

---

## 3. AST Data Structures (`ast.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    // ... existing variants
    While {
        cond: Expr,
        body: Vec<Statement>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    // ... existing variants
    ForExpr {
        var: String,
        iterable: Box<Expr>,
        body: Box<Expr>,
    },
}
```

---

## 4. Interpreter Semantics & Memory Performance (`interpreter.rs`)

### 4.1 `ForExpr` Evaluation & High-Volume Scaling
- Evaluates `iterable` to either a `PhsValue::Vector` or a range (`PhsValue::Range`).
- Obtains iteration count upfront (`len`).
- Pre-allocates target accumulator memory with `Vec::with_capacity(len)` so millions of loop iterations execute without heap re-allocation overhead.
- Iterates over each item, binding `var` in a child environment per iteration.
- Evaluates `body` for each element and collects values into `PhsValue::Vector`.
- `var` does not leak to the parent scope.
- Ready for **Track B (Concurrency)** parallelization via `rayon` when element count exceeds threshold (e.g., >10,000 items).

### 4.2 `While` Evaluation
- Evaluates `cond` before each iteration.
- Maintains an iteration counter. If iteration count exceeds `DEFAULT_MAX_LOOP_ITERATIONS` (10,000), raises `PhysureError::Generic("loop did not converge after 10000 iterations at line ...")`.
- Executes `body` statements in sequence.
- Variable assignments inside the loop body mutate existing variables in the enclosing scope if they were declared prior to the loop.

---

## 5. Codegen Emitters (`codegen/`)

1. **Python (`python.rs`)**:
   - `ForExpr`: Transpiles to list comprehension `[body for var in iterable]`.
   - `While`: Transpiles to `while cond:` loop.

2. **JavaScript / TypeScript (`js.rs`)**:
   - `ForExpr`: Transpiles to `iterable.map((var) => body)`.
   - `While`: Transpiles to `while (cond) { ... }`.

3. **Rust (`rust.rs`)**:
   - `ForExpr`: Transpiles to `iterable.into_iter().map(|var| body).collect()`.
   - `While`: Transpiles to `while cond { ... }`.

4. **Java (`java.rs`)**:
   - `ForExpr`: Transpiles to stream `.map()` or pre-allocated array loop.
   - `While`: Transpiles to `while (cond) { ... }`.

---

## 6. Testing & Verification Strategy

1. **Parser & AST Tests**: Verify round-trip parsing of nested `for` expressions and `while` statements.
2. **Interpreter Unit Tests**: Test vector generation from `for`, high-volume (e.g. 100,000+ items) `for` loop capacity allocation, convergence loops with `while`, and iteration cap overflow errors.
3. **Execution Parity Tests**: Verify identical numerical outputs when executing through the interpreter and through Python/JS/Java/Rust transpiled targets.
