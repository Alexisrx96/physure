# Track A — Loops Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `Expr::ForExpr` (vectorized functional loop) and `Statement::While` (imperative convergence loop) across `physure-script`'s AST, grammar, parser, interpreter, and all 4 transpiler backends (Python, JS/TS, Rust, Java), supporting large-scale loop iterations with `Vec::with_capacity` pre-allocation.

**Architecture:** Extend AST with `Expr::ForExpr` and `Statement::While`. Update `phs.pest` rules and `parser.rs` to build AST nodes. In `interpreter.rs`, evaluate `ForExpr` by obtaining length upfront, pre-allocating accumulator memory with `Vec::with_capacity(len)` for scaling to millions of elements, and evaluating `While` by looping up to 10,000 iterations while mutating outer scope bindings. Extend Python, JS, Rust, and Java code generators with corresponding loop constructs and verify with cross-target execution parity tests.

**Tech Stack:** Rust (Pest parser, Serde, Transpilers), Python/Node.js/Java CLI targets for parity verification.

---

### Task 1: AST Data Structures & Grammar Rules

**Files:**
- Modify: `physure-script/src/ast.rs`
- Modify: `physure-script/src/phs.pest`

- [ ] **Step 1: Write AST tests for `Expr::ForExpr` and `Statement::While`**

In `physure-script/src/ast.rs`:
```rust
#[test]
fn test_ast_for_expr_and_while_stmt() {
    let for_expr = Expr::ForExpr {
        var: "x".to_string(),
        iterable: Box::new(Expr::Identifier("range".to_string())),
        body: Box::new(Expr::Identifier("x".to_string())),
    };
    let while_stmt = Statement::While {
        cond: Expr::Identifier("flag".to_string()),
        body: vec![Statement::Expr(for_expr)],
    };
    assert!(matches!(while_stmt, Statement::While { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-script --lib ast::tests::test_ast_for_expr_and_while_stmt`
Expected: FAIL with missing enum variants `Expr::ForExpr` and `Statement::While`.

- [ ] **Step 3: Update `ast.rs` and `phs.pest`**

In `physure-script/src/ast.rs`, add variants:
```rust
pub enum Statement {
    // ...
    While {
        cond: Expr,
        body: Vec<Statement>,
    },
}

pub enum Expr {
    // ...
    ForExpr {
        var: String,
        iterable: Box<Expr>,
        body: Box<Expr>,
    },
}
```

In `physure-script/src/phs.pest`:
```pest
for_expr = { "for" ~ identifier ~ "in" ~ expr ~ "{" ~ expr ~ "}" }
while_stmt = { "while" ~ expr ~ "{" ~ _nl* ~ (stmt ~ _nl*)* ~ "}" }
```

- [ ] **Step 4: Run test to verify AST test passes**

Run: `cargo test -p physure-script --lib ast::tests::test_ast_for_expr_and_while_stmt`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/ast.rs physure-script/src/phs.pest
git commit -m "feat(script): add AST nodes and pest rules for for-expr and while-stmt"
```

---

### Task 2: Parser Implementation

**Files:**
- Modify: `physure-script/src/parser.rs`

- [ ] **Step 1: Write parser tests for `for` expression and `while` statement**

In `physure-script/src/parser.rs`:
```rust
#[test]
fn test_parse_for_expr_and_while_stmt() {
    let script = "for t in 1 .. 5 { t * 2 }\nwhile x > 0 { x = x - 1 }";
    let stmts = parse_phs(script).unwrap();
    assert_eq!(stmts.len(), 2);
    assert!(matches!(&stmts[0], Statement::Expr(Expr::ForExpr { .. })));
    assert!(matches!(&stmts[1], Statement::While { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-script --lib parser::tests::test_parse_for_expr_and_while_stmt`
Expected: FAIL.

- [ ] **Step 3: Implement parsing for `for_expr` and `while_stmt`**

In `physure-script/src/parser.rs`, handle `Rule::for_expr` inside `parse_expr` / `parse_primary_expr`, and `Rule::while_stmt` inside `parse_stmt`.

- [ ] **Step 4: Run parser test to verify it passes**

Run: `cargo test -p physure-script --lib parser::tests::test_parse_for_expr_and_while_stmt`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/parser.rs
git commit -m "feat(script): implement parser rules for for-expr and while-stmt"
```

---

### Task 3: Interpreter Evaluation & Max Iteration Cap

**Files:**
- Modify: `physure-script/src/interpreter.rs`

- [ ] **Step 1: Write interpreter unit tests for small and large-scale iterations**

In `physure-script/src/interpreter.rs`:
```rust
#[test]
fn test_interpreter_for_expr() {
    let mut interp = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs("res = for i in 1..4 { i * 2 }").unwrap();
    interp.run_statements(&stmts).unwrap();
    let val = interp.get_var("res").unwrap();
    assert!(matches!(val, PhsValue::Vector(_)));
}

#[test]
fn test_interpreter_for_expr_large_scale() {
    let mut interp = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs("res = for i in 1..100000 { i + 1 }").unwrap();
    interp.run_statements(&stmts).unwrap();
    let val = interp.get_var("res").unwrap();
    if let PhsValue::Vector(v) = val {
        assert_eq!(v.len(), 99999);
    } else {
        panic!("expected vector");
    }
}

#[test]
fn test_interpreter_while_loop_and_max_iter() {
    let mut interp = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs("i = 0\nwhile i < 5 { i = i + 1 }").unwrap();
    interp.run_statements(&stmts).unwrap();
    let val = interp.get_var("i").unwrap();
    assert_eq!(val.to_string(), "5");

    let infinite = crate::parser::parse_phs("i = 0\nwhile true { i = i + 1 }").unwrap();
    assert!(interp.run_statements(&infinite).is_err());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p physure-script --lib interpreter::tests::test_interpreter_for_expr`
Expected: FAIL.

- [ ] **Step 3: Implement `eval_expr` for `ForExpr` and `run_statement` for `While`**

In `physure-script/src/interpreter.rs`:
- For `ForExpr`: obtain length upfront, use `Vec::with_capacity(len)` for accumulator allocation, iterate elements, bind `var` in scoped child env, evaluate `body`, collect into `PhsValue::Vector`.
- For `While`: loop while `is_truthy(cond)`, increment iteration counter (fail if > 10,000), execute statements in `body` mutating outer env.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p physure-script --lib interpreter::tests::test_interpreter_for_expr`
Run: `cargo test -p physure-script --lib interpreter::tests::test_interpreter_for_expr_large_scale`
Run: `cargo test -p physure-script --lib interpreter::tests::test_interpreter_while_loop_and_max_iter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "feat(script): implement interpreter evaluation for for-expr and while-stmt with pre-allocated vector scaling"
```

---

### Task 4: Codegen Emitters (Python, JS/TS, Rust, Java)

**Files:**
- Modify: `physure-script/src/codegen/python.rs`
- Modify: `physure-script/src/codegen/js.rs`
- Modify: `physure-script/src/codegen/rust.rs`
- Modify: `physure-script/src/codegen/java.rs`

- [ ] **Step 1: Write transpiler unit tests**

Add transpiler tests for `for` and `while` across all 4 transpiler test modules.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p physure-script --lib codegen`
Expected: FAIL.

- [ ] **Step 3: Implement emitters**

- Python (`python.rs`): list comprehension for `ForExpr`, `while cond:` for `While`.
- JS (`js.rs`): `.map((var) => body)` for `ForExpr`, `while (cond)` for `While`.
- Rust (`rust.rs`): `.into_iter().map(...)` for `ForExpr`, `while cond` for `While`.
- Java (`java.rs`): stream `.map()` or loop for `ForExpr`, `while (cond)` for `While`.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p physure-script --lib codegen`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/codegen/
git commit -m "feat(script): add for/while transpilation support for Python, JS, Rust, and Java"
```

---

### Task 5: Integration & Execution Parity Tests

**Files:**
- Modify: `physure-script/tests/transpile_parity_tests.rs`

- [ ] **Step 1: Add loop scripts to execution parity test suite**

In `physure-script/tests/transpile_parity_tests.rs`, add a test case evaluating a convergence loop (e.g. Newton's method for square root) across interpreter and transpiled targets.

- [ ] **Step 2: Run workspace test suite**

Run: `cargo test --workspace`
Expected: ALL PASS.

- [ ] **Step 3: Commit**

```bash
git add physure-script/tests/transpile_parity_tests.rs
git commit -m "test(script): add execution parity tests for Track A loop constructs"
```
