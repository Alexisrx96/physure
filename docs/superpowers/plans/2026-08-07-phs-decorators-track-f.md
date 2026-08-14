# PHS Decorators (Track F) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a generic `@name(args)` decorator mechanism to PHS, plus the approved Phase 1 catalog — `@requires`/`@ensures` (function pre/postconditions), `@range` (sugar over two `@requires` bound checks), and `@stable`/`@experimental` (metadata) — as specified in [`docs/language_readiness_roadmap.md`](../../language_readiness_roadmap.md) Track F.

**Architecture:** A new `decorator`/`decorated_stmt` grammar rule wraps `function_def`/`assignment_fn`/`assignment` and produces a `Vec<DecoratorNode>` attached directly to the existing `FunctionDefNode`/`AssignmentNode` AST structs (no new AST node types for functions — decorators just ride along on the node they annotate, so every existing consumer that already clones/threads a `FunctionDefNode` — including `PhsValue::Function` — carries them for free). A new `physure-script/src/decorators.rs` module owns the `KNOWN_DECORATORS` registry, `@range` desugaring, and a post-parse validation pass (unknown names, arity, `@stable`/`@experimental` mutual exclusion, `@ensures`-vs-`result`-param collision) modeled directly on the existing `validate_unit_shadowing` pass in `parser.rs`. `@requires`/`@ensures` are enforced at call time inside `PhsInterpreter::call_function_node` by evaluating their condition `Expr` (comparisons already desugar to ordinary `FunctionCall`s like `op_>`, so no new expression machinery is needed) and raising a new `PhysureError::ContractViolation` on failure. `@stable`/`@experimental` are pure metadata in this track — carried on the AST, not yet enforced or surfaced anywhere (that's for a later consumer, e.g. Track E's FFI shim or an LSP hover).

**Tech Stack:** Rust, `pest`/`pest_derive` (existing grammar), `serde` (existing AST serialization), `cargo test` (existing workspace test runner, no new dev-dependencies).

---

## Scope boundary (read before starting)

This plan implements Track F **only**. It does **not** touch `physure-script/src/codegen/{python,rust,java}.rs` code generation logic, and does **not** implement the Track E FFI-shim propagation of `@requires`/`@ensures` into compiled `.dll`/`.so` artifacts — both are explicitly Track E's job later in the approved sequence (Track F → A → B → C → E → D). Everywhere this plan touches a codegen file, it is only to fix a struct-literal compile error caused by the new `decorators` field — never to add decorator-aware behavior to a transpiler.

---

## File Structure

- **Modify:** `physure-script/src/ast.rs` — add `DecoratorNode` struct; add `decorators: Vec<DecoratorNode>` field to `FunctionDefNode` and `AssignmentNode`.
- **Modify:** `physure-script/src/phs.pest` — add `decorator` and `decorated_stmt` rules; add `decorated_stmt` to the `stmt` alternation.
- **Modify:** `physure-script/src/parser.rs` — parse `decorated_stmt` into a `Vec<DecoratorNode>` attached to the wrapped `Statement`; call the new validation pass from `parse_phs`/`parse_phs_with_lines`.
- **Create:** `physure-script/src/decorators.rs` — `KNOWN_DECORATORS`, `lower_range`, `validate_decorators` (and its recursive helpers).
- **Modify:** `physure-script/src/lib.rs` — declare `pub mod decorators;`.
- **Modify:** `physure-core/src/error.rs` — add `PhysureError::ContractViolation { decorator: String, message: String }`.
- **Modify:** `physure-script/src/interpreter.rs` — enforce `@requires`/`@ensures` inside `call_function_node` via two new private helpers, `check_requires` and `check_ensures`.
- **Modify (compile fixes only, no behavior change):** `physure-script/src/codegen/java.rs`, `physure-script/src/codegen/mod.rs`, `physure-script/src/codegen/python.rs`, `physure-script/src/codegen/rust.rs` — add the new `decorators` field to existing `FunctionDefNode`/`AssignmentNode` struct literals.

---

### Task 1: `DecoratorNode` AST type and `decorators` fields

**Files:**
- Modify: `physure-script/src/ast.rs`
- Modify: `physure-script/src/codegen/mod.rs:110-128` (inline_bindings_stmt), `physure-script/src/codegen/mod.rs:336-353` (rewrite_equation_calls loop)
- Modify: `physure-script/src/codegen/java.rs:236-245`
- Modify: `physure-script/src/codegen/python.rs:247-256`
- Modify: `physure-script/src/codegen/rust.rs:190-219`
- Modify: `physure-script/src/interpreter.rs:595-600`, `:843-848`, `:1066-1158` (four sites in `test_kinetic_energy`/`test_uncertainty_propagation`)
- Modify: `physure-script/src/parser.rs:189-217` (`parse_function_def`, `parse_assignment`)
- Test: `physure-script/src/ast.rs` (extends the existing `test_construct_function_def`)

- [x] **Step 1: Write/extend the failing test**

In `physure-script/src/ast.rs`, replace the existing test:

```rust
    #[test]
    fn test_construct_function_def() {
        let node = FunctionDefNode {
            name: "square".to_string(),
            params: vec!["x".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("x".to_string()))],
        };
        let stmt = Statement::FunctionDef(node);
        assert!(matches!(stmt, Statement::FunctionDef(_)));
```

with:

```rust
    #[test]
    fn test_construct_function_def() {
        let node = FunctionDefNode {
            name: "square".to_string(),
            params: vec!["x".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("x".to_string()))],
            decorators: vec![DecoratorNode {
                name: "stable".to_string(),
                args: vec![],
            }],
        };
        assert_eq!(node.decorators.len(), 1);
        assert_eq!(node.decorators[0].name, "stable");
        let stmt = Statement::FunctionDef(node);
        assert!(matches!(stmt, Statement::FunctionDef(_)));
```

- [x] **Step 2: Run the test to confirm it fails to compile**

Run: `cargo test -p physure-script test_construct_function_def`
Expected: FAIL to compile — `no field \`decorators\` on type \`FunctionDefNode\`` and `cannot find struct \`DecoratorNode\``.

- [x] **Step 3: Add `DecoratorNode` and the `decorators` fields**

In `physure-script/src/ast.rs`, add the new struct directly after `AssignmentNode` (before the `Expr` enum):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentNode {
    pub name: String,
    pub value: Expr,
    #[serde(default)]
    pub decorators: Vec<DecoratorNode>,
}

/// A single `@name(args...)` annotation attached to a `FunctionDefNode` or an
/// `AssignmentNode`. `args` are ordinary expressions — a decorator that takes a
/// condition (`@requires`, `@ensures`) reuses the same `Expr` machinery as any other
/// call, since comparisons already desugar to `FunctionCall { name: "op_>", .. }` at
/// parse time and need no new evaluator support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecoratorNode {
    pub name: String,
    pub args: Vec<Expr>,
}
```

Replace the old `AssignmentNode` definition (the one without `decorators`) with the version above, and update `FunctionDefNode`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefNode {
    pub name: String,
    pub params: Vec<String>,
    /// Optional declared unit constraint for each parameter, aligned by index with `params`.
    /// `None` (or a missing/short entry, for backward compatibility) means the parameter has
    /// no declared unit, so its argument is bound as-is with no conversion attempted.
    #[serde(default)]
    pub param_units: Vec<Option<String>>,
    pub body_stmts: Vec<Statement>,
    #[serde(default)]
    pub decorators: Vec<DecoratorNode>,
}
```

- [x] **Step 4: Run the test again to confirm the new failures are only the other construction sites**

Run: `cargo build -p physure-script`
Expected: FAIL — a `missing field \`decorators\`` compile error at each of the following sites (this is expected; each is fixed in the next steps): `physure-script/src/codegen/java.rs:236`, `physure-script/src/codegen/mod.rs:112` and `:344`, `physure-script/src/codegen/python.rs:247`, `physure-script/src/codegen/rust.rs:190`, `physure-script/src/interpreter.rs:595`, `:843`, `:1066`, `:1096`, `:1106`, `:1116`, `:1148`, `physure-script/src/parser.rs:189`, `:213`.

- [x] **Step 5: Fix every other `FunctionDefNode`/`AssignmentNode` construction site**

In `physure-script/src/parser.rs`, `parse_function_def` (around line 189) — this is the *base* parse with no decorators yet; `decorated_stmt` parsing (Task 3) fills them in afterward:

```rust
    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        decorators: Vec::new(),
    }))
```

And `parse_assignment` (around line 213), same reasoning:

```rust
    Ok(Statement::Assignment(AssignmentNode {
        name,
        value: value.unwrap(),
        decorators: Vec::new(),
    }))
```

In `physure-script/src/codegen/mod.rs`, `inline_bindings_stmt` (around line 110-128) is a rewrite of an *existing* node, so it must carry decorators forward rather than drop them:

```rust
fn inline_bindings_stmt(stmt: &Statement) -> Statement {
    match stmt {
        Statement::Assignment(node) => Statement::Assignment(AssignmentNode {
            name: node.name.clone(),
            value: inline_bindings(&node.value),
            decorators: node.decorators.clone(),
        }),
```

(The `Statement::FunctionDef(def) => ... ..def.clone()` arm a few lines below already uses struct-update syntax and needs no change — `decorators` is carried automatically.)

Still in `physure-script/src/codegen/mod.rs`, the `rewrite_equation_calls` loop (around line 336-353) is likewise a rewrite of an existing node:

```rust
        statements.push(match stmt {
            Statement::Assignment(node) => Statement::Assignment(AssignmentNode {
                name: node.name.clone(),
                value: rewrite_equation_calls(&node.value, &equations, &mut functions, &mut signatures)?,
                decorators: node.decorators.clone(),
            }),
```

And the `functions.push(FunctionDefNode { ... })` a few lines above that (around line 387) synthesizes a brand-new function with no decorators of its own:

```rust
                    functions.push(FunctionDefNode {
                        name: name.clone(),
                        params: kwarg_names.clone(),
                        param_units: vec![None; kwarg_names.len()],
                        body_stmts: vec![Statement::Expr(node_to_expr(chosen))],
                        decorators: Vec::new(),
                    });
```

In `physure-script/src/codegen/java.rs`, the test at line 236 constructs a fresh test fixture:

```rust
        let func = Statement::FunctionDef(FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            param_units: vec![None, None],
            body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Identifier("m".to_string())),
                right: Box::new(Expr::Identifier("v".to_string())),
            })],
            decorators: Vec::new(),
        });
```

In `physure-script/src/codegen/python.rs`, the test at line 247:

```rust
        let fn_node = FunctionDefNode {
            name: "foo".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            param_units: vec![None, None],
            body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Identifier("a".to_string())),
                right: Box::new(Expr::Identifier("b".to_string())),
            })],
            decorators: Vec::new(),
        };
```

In `physure-script/src/codegen/rust.rs`, the test at line 190:

```rust
            statements: vec![Statement::FunctionDef(FunctionDefNode {
                name: "kinetic_energy".to_string(),
                params: vec!["m".to_string(), "v".to_string()],
                param_units: vec![None, None],
                body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Identifier("m".to_string())),
                        right: Box::new(Expr::BinaryOp {
                            op: BinaryOp::Pow,
                            left: Box::new(Expr::Identifier("v".to_string())),
                            right: Box::new(Expr::Quantity(QuantityNode {
                                magnitude: 2.0,
                                uncertainty: None,
                                uncertainty_lower: None,
                                is_sigma: false,
                                unit: None,
                            })),
                        }),
                    }),
                    right: Box::new(Expr::Quantity(QuantityNode {
                        magnitude: 0.5,
                        uncertainty: None,
                        uncertainty_lower: None,
                        is_sigma: false,
                        unit: None,
                    })),
                })],
                decorators: Vec::new(),
            })],
```

In `physure-script/src/interpreter.rs`, the two runtime function-composition sites (around line 595 and 843) synthesize new functions with no decorators of their own:

```rust
                            return Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                                name: format!("{}_{}", func.name, arg_func.name),
                                params,
                                param_units,
                                body_stmts: vec![body],
                                decorators: Vec::new(),
                            }));
```

```rust
                Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                    name,
                    params,
                    param_units,
                    body_stmts: vec![body],
                    decorators: Vec::new(),
                }))
```

And in the `#[cfg(test)] mod tests` block of `physure-script/src/interpreter.rs`, `test_kinetic_energy` (around line 1066) and `test_uncertainty_propagation` (around line 1148) each need `decorators: Vec::new(),` added to their `FunctionDefNode { .. }` / `AssignmentNode { .. }` literals — there are four literals total (one `FunctionDefNode`, three `AssignmentNode`s named `m`, `v`, `E` in `test_kinetic_energy`, and one `AssignmentNode` named `m` in `test_uncertainty_propagation`). For each, add `decorators: Vec::new(),` as the last field before the closing `}`. For example, the `m` assignment in `test_uncertainty_propagation`:

```rust
                Statement::Assignment(AssignmentNode {
                    name: "m".to_string(),
                    value: Expr::Quantity(QuantityNode {
                        magnitude: 75.0,
                        uncertainty: Some(0.5),
                        uncertainty_lower: None,
                        is_sigma: false,
                        unit: Some("kg".to_string()),
                    }),
                    decorators: Vec::new(),
                }),
```

- [x] **Step 6: Run the test to confirm it passes and the whole crate builds**

Run: `cargo test -p physure-script test_construct_function_def`
Expected: PASS

Run: `cargo build --workspace`
Expected: builds cleanly (no more missing-field errors anywhere in the workspace).

- [x] **Step 7: Commit**

```bash
git add physure-script/src/ast.rs physure-script/src/codegen/mod.rs physure-script/src/codegen/java.rs physure-script/src/codegen/python.rs physure-script/src/codegen/rust.rs physure-script/src/interpreter.rs physure-script/src/parser.rs
git commit -m "feat(phs): add DecoratorNode AST type and decorators field"
```

---

### Task 2: Grammar — `decorator` and `decorated_stmt` rules

**Files:**
- Modify: `physure-script/src/phs.pest`
- Test: `physure-script/src/parser.rs` (new `#[cfg(test)] mod tests` entries)

- [x] **Step 1: Write the failing test**

In `physure-script/src/parser.rs`, inside the existing `#[cfg(test)] mod tests` block (near the other `PhsParser::parse(Rule::..., ...)` tests, e.g. next to `test_assignment_fn_standalone`), add:

```rust
    #[test]
    fn test_decorated_stmt_rule_parses() {
        let code = "@stable\nfn f(x) = x";
        let pairs = PhsParser::parse(Rule::decorated_stmt, code);
        assert!(pairs.is_ok(), "expected decorated_stmt to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_decorator_with_args_rule_parses() {
        let pairs = PhsParser::parse(Rule::decorator, "@requires(x > 0.0, \"x must be positive\")");
        assert!(pairs.is_ok(), "expected decorator with args to parse: {:?}", pairs.err());
    }
```

- [x] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p physure-script test_decorated_stmt_rule_parses`
Expected: FAIL to compile — `no variant or associated item named \`decorated_stmt\` found for enum \`Rule\`` (and likewise for `decorator`).

- [x] **Step 3: Add the grammar rules**

In `physure-script/src/phs.pest`, insert the two new rules directly above the existing `stmt_term`/`stmt` rules, and add `decorated_stmt` to the `stmt` alternation. Find:

```
stmt_term       = _{ ";" | NEWLINE }
stmt            = { import_stmt | export_stmt | function_def | assignment_fn | assignment | guard_if_stmt | return_stmt | raw_block | expr }
```

Replace with:

```
decorator       = { "@" ~ identifier ~ ("(" ~ (expr ~ ("," ~ expr)*)? ~ ")")? }
// One or more `@name(...)` lines immediately above the definition they annotate. `_nl`
// (already used by `if_expr`/`where_expr`) absorbs the newline between each decorator
// and the next, and between the last decorator and the definition itself.
decorated_stmt  = { (decorator ~ _nl)+ ~ (function_def | assignment_fn | assignment) }

stmt_term       = _{ ";" | NEWLINE }
stmt            = { import_stmt | export_stmt | decorated_stmt | function_def | assignment_fn | assignment | guard_if_stmt | return_stmt | raw_block | expr }
```

- [x] **Step 4: Run the test to confirm it passes**

Run: `cargo test -p physure-script test_decorated_stmt_rule_parses test_decorator_with_args_rule_parses`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add physure-script/src/phs.pest physure-script/src/parser.rs
git commit -m "feat(phs): add decorator and decorated_stmt grammar rules"
```

---

### Task 3: Parser — build `DecoratorNode`s and attach them to the wrapped statement

**Files:**
- Modify: `physure-script/src/parser.rs`
- Test: `physure-script/src/parser.rs`

- [x] **Step 1: Write the failing test**

Add to `physure-script/src/parser.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn test_parse_phs_attaches_decorators_to_function_def() {
        let program = parse_phs("@stable\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
                assert!(node.decorators[0].args.is_empty());
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_decorator_args() {
        let program = parse_phs("@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "requires");
                assert_eq!(node.decorators[0].args.len(), 2);
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }
```

- [x] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p physure-script test_parse_phs_attaches_decorators_to_function_def`
Expected: FAIL — `left: 0, right: 1` (parses fine as a plain `fn`, decorator text is silently swallowed) or a parse error, depending on how the unreached `decorated_stmt` rule behaves; either way the assertion on `decorators.len()` fails.

- [x] **Step 3: Implement `parse_decorated_stmt` and `parse_decorator`, and wire them in**

In `physure-script/src/parser.rs`, update `parse_statement`'s match (around line 53-66) to add a `decorated_stmt` arm:

```rust
fn parse_statement(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    match pair.as_rule() {
        Rule::stmt => parse_statement(pair.into_inner().next().unwrap()),
        Rule::import_stmt => parse_import(pair),
        Rule::export_stmt => parse_export(pair),
        Rule::decorated_stmt => parse_decorated_stmt(pair),
        Rule::function_def | Rule::assignment_fn => parse_function_def(pair),
        Rule::assignment => parse_assignment(pair),
        Rule::guard_if_stmt => parse_guard_if_stmt(pair),
        Rule::return_stmt => parse_return_stmt(pair),
        Rule::raw_block => Ok(Statement::Expr(Expr::Identifier(pair.as_str().to_string()))),
        Rule::expr => Ok(Statement::Expr(parse_expr(pair)?)),
        _ => Err(PhysureError::Generic(format!("Unexpected statement rule: {:?}", pair.as_rule()))),
    }
}
```

Then add the two new functions directly after `parse_assignment` (around line 217):

```rust
fn parse_decorated_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut decorators = Vec::new();
    let mut target = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::decorator => {
                let raw = parse_decorator(inner)?;
                for lowered in crate::decorators::lower_range(raw)? {
                    decorators.push(lowered);
                }
            }
            Rule::function_def | Rule::assignment_fn => {
                target = Some(parse_function_def(inner)?);
            }
            Rule::assignment => {
                target = Some(parse_assignment(inner)?);
            }
            _ => {}
        }
    }

    let mut stmt = target.ok_or_else(|| {
        PhysureError::Generic("decorated statement is missing its function or assignment".to_string())
    })?;
    match &mut stmt {
        Statement::FunctionDef(node) => node.decorators = decorators,
        Statement::Assignment(node) => node.decorators = decorators,
        _ => unreachable!("decorated_stmt only ever wraps function_def, assignment_fn, or assignment"),
    }
    Ok(stmt)
}

fn parse_decorator(pair: pest::iterators::Pair<Rule>) -> PhysureResult<DecoratorNode> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let mut args = Vec::new();
    for arg_pair in inner {
        args.push(parse_expr(arg_pair)?);
    }
    Ok(DecoratorNode { name, args })
}
```

`DecoratorNode` is already in scope via the existing `use crate::ast::*;` at the top of `parser.rs`. `crate::decorators::lower_range` does not exist yet — that is Task 5. For this step, temporarily stub it inline so the crate compiles and this task's tests can pass on their own:

```rust
            Rule::decorator => {
                let raw = parse_decorator(inner)?;
                decorators.push(raw);
            }
```

(Task 5 will replace this two-line arm with the `lower_range` call shown above once the `decorators` module exists.)

- [x] **Step 4: Run the test to confirm it passes**

Run: `cargo test -p physure-script test_parse_phs_attaches_decorators_to_function_def test_parse_phs_attaches_decorator_args`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser.rs
git commit -m "feat(phs): parse decorated_stmt into DecoratorNodes attached to the AST"
```

---

### Task 4: `PhysureError::ContractViolation`

**Files:**
- Modify: `physure-core/src/error.rs`
- Test: `physure-core/src/error.rs`

- [x] **Step 1: Write the failing test**

In `physure-core/src/error.rs`, add a test module (there isn't one yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_violation_displays_decorator_and_message() {
        let err = PhysureError::ContractViolation {
            decorator: "requires".to_string(),
            message: "x must be positive".to_string(),
        };
        assert_eq!(err.to_string(), "@requires violated: x must be positive");
    }
}
```

- [x] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p physure-core contract_violation_displays_decorator_and_message`
Expected: FAIL to compile — `no variant \`ContractViolation\` found for enum \`PhysureError\``.

- [x] **Step 3: Add the variant**

In `physure-core/src/error.rs`, add the variant to the enum:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PhysureError {
    UnitMismatch { expected: String, actual: String },
    UnknownUnit { symbol: String, suggestion: Option<String> },
    IncompatibleDimensions { op: &'static str, dim1: String, dim2: String },
    DivisionByZero(String),
    NonConstantExponent(String),
    NonLinearArgument { function: &'static str },
    UnsupportedIntegration(String),
    ArrowError(String),
    CovarianceError(String),
    ParseError(String),
    /// A `@requires`/`@ensures` condition evaluated to false. `decorator` is the
    /// decorator name without the `@` (`"requires"` or `"ensures"`); `message` is the
    /// user-supplied explanation string from the decorator's second argument.
    ContractViolation { decorator: String, message: String },
    Generic(String),
}
```

And the `Display` arm, next to `ParseError`:

```rust
            PhysureError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            PhysureError::ContractViolation { decorator, message } => {
                write!(f, "@{} violated: {}", decorator, message)
            }
            PhysureError::Generic(msg) => write!(f, "{}", msg),
```

- [x] **Step 4: Run the test to confirm it passes**

Run: `cargo test -p physure-core contract_violation_displays_decorator_and_message`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add physure-core/src/error.rs
git commit -m "feat(core): add PhysureError::ContractViolation"
```

---

### Task 5: `decorators.rs` — registry, `@range` lowering, and validation

**Files:**
- Create: `physure-script/src/decorators.rs`
- Modify: `physure-script/src/lib.rs`
- Modify: `physure-script/src/parser.rs` (wire `validate_decorators` in, replace the Task 3 stub with the real `lower_range` call)
- Test: `physure-script/src/decorators.rs`

- [x] **Step 1: Write the failing tests**

Create `physure-script/src/decorators.rs` with only the test module first:

```rust
use crate::ast::{DecoratorNode, Expr, FunctionDefNode, Statement};
use physure_core::error::PhysureResult;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::QuantityNode;

    fn quantity(magnitude: f64) -> Expr {
        Expr::Quantity(QuantityNode {
            magnitude,
            uncertainty: None,
            uncertainty_lower: None,
            is_sigma: false,
            unit: None,
        })
    }

    #[test]
    fn lower_range_expands_into_two_requires() {
        let raw = DecoratorNode {
            name: "range".to_string(),
            args: vec![Expr::Identifier("v".to_string()), quantity(0.0), quantity(10.0)],
        };
        let lowered = lower_range(raw).unwrap();
        assert_eq!(lowered.len(), 2);
        assert!(lowered.iter().all(|d| d.name == "requires"));
    }

    #[test]
    fn lower_range_rejects_wrong_arity() {
        let raw = DecoratorNode {
            name: "range".to_string(),
            args: vec![Expr::Identifier("v".to_string()), quantity(0.0)],
        };
        assert!(lower_range(raw).is_err());
    }

    #[test]
    fn lower_range_passes_through_non_range_decorators() {
        let raw = DecoratorNode { name: "stable".to_string(), args: vec![] };
        let lowered = lower_range(raw.clone()).unwrap();
        assert_eq!(lowered, vec![raw]);
    }

    fn function_with_decorators(decorators: Vec<DecoratorNode>) -> Statement {
        Statement::FunctionDef(FunctionDefNode {
            name: "f".to_string(),
            params: vec!["x".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("x".to_string()))],
            decorators,
        })
    }

    #[test]
    fn validate_decorators_rejects_unknown_name() {
        let stmt = function_with_decorators(vec![DecoratorNode { name: "bogus".to_string(), args: vec![] }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_stable_and_experimental_together() {
        let stmt = function_with_decorators(vec![
            DecoratorNode { name: "stable".to_string(), args: vec![] },
            DecoratorNode { name: "experimental".to_string(), args: vec![] },
        ]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_ensures_on_param_named_result() {
        let stmt = Statement::FunctionDef(FunctionDefNode {
            name: "f".to_string(),
            params: vec!["result".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("result".to_string()))],
            decorators: vec![DecoratorNode {
                name: "ensures".to_string(),
                args: vec![Expr::Identifier("result".to_string()), Expr::Str("must hold".to_string())],
            }],
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_accepts_requires_with_two_args() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "requires".to_string(),
            args: vec![Expr::Identifier("x".to_string()), Expr::Str("x must be positive".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_ok());
    }
}
```

- [x] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p physure-script decorators::tests`
Expected: FAIL to compile — `cannot find function \`lower_range\`` / `\`validate_decorators\`` in this scope.

- [x] **Step 3: Implement the module**

Still in `physure-script/src/decorators.rs`, add the implementation above the `#[cfg(test)]` block:

```rust
use crate::ast::{DecoratorNode, Expr, FunctionDefNode, Statement};
use physure_core::error::{PhysureError, PhysureResult};

/// Every decorator name Track F's interpreter/validator understands. `@range` is
/// deliberately absent: it is desugared into `requires` by `lower_range` before this
/// registry is ever consulted, so nothing downstream needs to know it existed.
const KNOWN_DECORATORS: &[&str] = &["requires", "ensures", "stable", "experimental"];

/// Expands `@range(var, min, max)` into two `@requires` decorators — `var >= min` and
/// `var <= max` — reusing `@requires`'s own runtime enforcement (Task 6) instead of
/// giving `@range` a code path of its own. Any other decorator passes through unchanged.
pub fn lower_range(raw: DecoratorNode) -> PhysureResult<Vec<DecoratorNode>> {
    if raw.name != "range" {
        return Ok(vec![raw]);
    }
    if raw.args.len() != 3 {
        return Err(PhysureError::Generic(format!(
            "@range expects 3 arguments (variable, min, max), got {}",
            raw.args.len()
        )));
    }
    let var_name = match &raw.args[0] {
        Expr::Identifier(name) => name.clone(),
        _ => {
            return Err(PhysureError::Generic(
                "@range's first argument must be a bare parameter name".to_string(),
            ))
        }
    };
    let var = raw.args[0].clone();
    let lo = raw.args[1].clone();
    let hi = raw.args[2].clone();

    let lower = DecoratorNode {
        name: "requires".to_string(),
        args: vec![
            Expr::FunctionCall { name: "op_>=".to_string(), args: vec![var.clone(), lo], kwargs: Vec::new() },
            Expr::Str(format!("{} must be >= the @range lower bound", var_name)),
        ],
    };
    let upper = DecoratorNode {
        name: "requires".to_string(),
        args: vec![
            Expr::FunctionCall { name: "op_<=".to_string(), args: vec![var, hi], kwargs: Vec::new() },
            Expr::Str(format!("{} must be <= the @range upper bound", var_name)),
        ],
    };
    Ok(vec![lower, upper])
}

/// Walks every statement (recursing into function bodies, so a decorated nested `fn`
/// is checked too) and validates its `decorators`. Mirrors `validate_unit_shadowing`
/// in `parser.rs`: called once, after the whole `Program` has been parsed.
pub fn validate_decorators(statements: &[Statement]) -> PhysureResult<()> {
    for stmt in statements {
        check_statement_decorators(stmt)?;
    }
    Ok(())
}

fn check_statement_decorators(stmt: &Statement) -> PhysureResult<()> {
    match stmt {
        Statement::FunctionDef(node) => {
            check_decorator_list(&node.decorators, Some(node))?;
            for body_stmt in &node.body_stmts {
                check_statement_decorators(body_stmt)?;
            }
        }
        Statement::Assignment(node) => {
            check_decorator_list(&node.decorators, None)?;
        }
        _ => {}
    }
    Ok(())
}

fn check_decorator_list(decorators: &[DecoratorNode], func: Option<&FunctionDefNode>) -> PhysureResult<()> {
    if decorators.is_empty() {
        return Ok(());
    }
    let mut has_stable = false;
    let mut has_experimental = false;
    let mut has_ensures = false;

    for dec in decorators {
        if !KNOWN_DECORATORS.contains(&dec.name.as_str()) {
            return Err(PhysureError::Generic(format!("Unknown decorator '@{}'", dec.name)));
        }
        match dec.name.as_str() {
            "requires" | "ensures" => {
                if func.is_none() {
                    return Err(PhysureError::Generic(format!(
                        "@{} is only valid on a function definition, not a variable assignment",
                        dec.name
                    )));
                }
                if dec.args.len() != 2 {
                    return Err(PhysureError::Generic(format!(
                        "@{} expects 2 arguments (condition, message), got {}",
                        dec.name,
                        dec.args.len()
                    )));
                }
                if dec.name == "ensures" {
                    has_ensures = true;
                }
            }
            "stable" => {
                if !dec.args.is_empty() {
                    return Err(PhysureError::Generic("@stable takes no arguments".to_string()));
                }
                has_stable = true;
            }
            "experimental" => {
                if !dec.args.is_empty() {
                    return Err(PhysureError::Generic("@experimental takes no arguments".to_string()));
                }
                has_experimental = true;
            }
            _ => unreachable!("checked against KNOWN_DECORATORS above"),
        }
    }

    if has_stable && has_experimental {
        return Err(PhysureError::Generic(
            "A function cannot be both @stable and @experimental".to_string(),
        ));
    }
    if has_ensures {
        if let Some(f) = func {
            if f.params.iter().any(|p| p == "result") {
                return Err(PhysureError::Generic(format!(
                    "function '{}' cannot use @ensures because it has a parameter literally named \
                     'result', which the postcondition needs to refer to the return value",
                    f.name
                )));
            }
        }
    }
    Ok(())
}
```

Register the module in `physure-script/src/lib.rs`. Find:

```
pub mod ast;
pub mod lexer;
pub mod parser;
```

Replace with:

```
pub mod ast;
pub mod decorators;
pub mod lexer;
pub mod parser;
```

(`lib.rs` is a single-line module list in this codebase; add `pub mod decorators;` anywhere in that list, alphabetically next to `ast`.)

- [x] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-script decorators::tests`
Expected: PASS (7 tests)

- [x] **Step 5: Wire `validate_decorators` into `parse_phs` and `parse_phs_with_lines`, and replace the Task 3 stub**

In `physure-script/src/parser.rs`, update `parse_phs` (around line 13-30):

```rust
pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push(parse_statement(inner)?);
            statement_pos.push((line, col));
        }
    }

    validate_unit_shadowing(&statements, &statement_pos)?;
    crate::decorators::validate_decorators(&statements)?;
    Ok(Program { statements })
}
```

And `parse_phs_with_lines` (around line 32-51):

```rust
pub fn parse_phs_with_lines(code: &str) -> PhysureResult<Vec<(usize, Statement)>> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;

    let mut statements = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push((line - 1, parse_statement(inner)?));
            statement_pos.push((line, col));
        }
    }

    let stmts_only: Vec<Statement> = statements.iter().map(|(_, s)| s.clone()).collect();
    validate_unit_shadowing(&stmts_only, &statement_pos)?;
    crate::decorators::validate_decorators(&stmts_only)?;

    Ok(statements)
}
```

Then replace the Task 3 stub in `parse_decorated_stmt` — find:

```rust
            Rule::decorator => {
                let raw = parse_decorator(inner)?;
                decorators.push(raw);
            }
```

Replace with the real lowering call:

```rust
            Rule::decorator => {
                let raw = parse_decorator(inner)?;
                for lowered in crate::decorators::lower_range(raw)? {
                    decorators.push(lowered);
                }
            }
```

- [x] **Step 6: Add end-to-end parse-level tests for the wiring**

Add to `physure-script/src/parser.rs`'s test module:

```rust
    #[test]
    fn test_parse_phs_rejects_unknown_decorator() {
        assert!(parse_phs("@bogus\nfn f(x) = x").is_err());
    }

    #[test]
    fn test_parse_phs_lowers_range_into_two_requires() {
        let program = parse_phs("@range(v, 0.0, 10.0)\nfn f(v) = v").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 2);
                assert!(node.decorators.iter().all(|d| d.name == "requires"));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }
```

- [x] **Step 7: Run the tests to confirm they pass**

Run: `cargo test -p physure-script`
Expected: PASS (all `physure-script` tests, including the new ones)

- [x] **Step 8: Commit**

```bash
git add physure-script/src/decorators.rs physure-script/src/lib.rs physure-script/src/parser.rs
git commit -m "feat(phs): add decorator registry, @range lowering, and post-parse validation"
```

---

### Task 6: Interpreter enforcement of `@requires`/`@ensures`

**Files:**
- Modify: `physure-script/src/interpreter.rs`
- Test: `physure-script/src/interpreter.rs`

- [x] **Step 1: Write the failing tests**

Add to `physure-script/src/interpreter.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn requires_violation_returns_contract_violation_error() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str(
                "@requires(m > 0.0, \"mass must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(-1.0)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"));
    }

    #[test]
    fn requires_satisfied_returns_normally() {
        let mut interp = PhsInterpreter::default();
        let results = interp
            .eval_str(
                "@requires(m > 0.0, \"mass must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(1.0)",
            )
            .unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 2.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 2.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn ensures_violation_returns_contract_violation_error() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str(
                "@ensures(result > 100.0, \"result must exceed 100\")\nfn small(m) = m\nsmall(1.0)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "ensures"));
    }

    #[test]
    fn ensures_satisfied_returns_normally() {
        let mut interp = PhsInterpreter::default();
        let results = interp
            .eval_str(
                "@ensures(result > 0.0, \"result must be positive\")\nfn small(m) = m\nsmall(1.0)",
            )
            .unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 1.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 1.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn range_lowered_to_requires_is_enforced_at_call_time() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str("@range(v, 0.0, 10.0)\nfn identity(v) = v\nidentity(20.0)")
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"));
    }
```

This test module needs `PhysureError` in scope; confirm the existing `use super::*;` (or equivalent) at the top of the test module already re-exports it via `physure_core::error::PhysureError` used elsewhere in `interpreter.rs` — if not already imported in the test module, add `use physure_core::error::PhysureError;` to the `mod tests` block's `use` list.

- [x] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p physure-script requires_violation_returns_contract_violation_error`
Expected: FAIL — `assertion failed` (the call currently succeeds because nothing checks `@requires`/`@ensures` yet).

- [x] **Step 3: Implement enforcement in `call_function_node`**

In `physure-script/src/interpreter.rs`, update `call_function_node` (around line 712-746):

```rust
    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        if func.params.len() != arg_vals.len() {
            return Err(PhysureError::Generic(format!("Function {} expects {} args, got {}", func.name, func.params.len(), arg_vals.len())));
        }
        let mut local_env = env.clone();
        for (i, (param_name, arg_val)) in func.params.iter().zip(arg_vals.into_iter()).enumerate() {
            let bound_val = self.bind_param_value(&func.name, param_name, func.param_units.get(i).and_then(|u| u.as_ref()), arg_val)?;
            local_env.insert(param_name.clone(), bound_val);
        }
        self.check_requires(func, &local_env)?;
        let mut last_val = PhsValue::None;
        for stmt in &func.body_stmts {
            match stmt {
                Statement::Return(expr) => {
                    last_val = self.eval_expr(expr, &local_env)?;
                    break;
                }
                Statement::GuardReturn { cond, value } => {
                    let cond_val = self.eval_expr(cond, &local_env)?;
                    if is_truthy(&cond_val) {
                        last_val = self.eval_expr(value, &local_env)?;
                        break;
                    }
                }
                _ => {
                    last_val = self.eval_statement_with_env(stmt, &mut local_env)?;
                }
            }
        }
        self.check_ensures(func, &local_env, &last_val)?;
        Ok(last_val)
    }

    /// Evaluates every `@requires` condition against the already-bound parameters,
    /// erroring on the first one that is not truthy. Conditions are ordinary `Expr`s —
    /// a comparison like `m > 0.0` is a `FunctionCall { name: "op_>", .. }` under the
    /// hood, so this needs no evaluator support beyond `eval_expr`/`is_truthy`.
    fn check_requires(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        for dec in &func.decorators {
            if dec.name == "requires" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, local_env)?) {
                    let message = self.eval_expr(&dec.args[1], local_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "requires".to_string(), message });
                }
            }
        }
        Ok(())
    }

    /// Evaluates every `@ensures` condition with `result` bound to the function's
    /// return value. `validate_decorators` (Task 5) already rejects `@ensures` on any
    /// function with a parameter literally named `result`, so this insert can never
    /// silently shadow a caller-visible binding.
    fn check_ensures(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>, result: &PhsValue) -> PhysureResult<()> {
        if !func.decorators.iter().any(|d| d.name == "ensures") {
            return Ok(());
        }
        let mut result_env = local_env.clone();
        result_env.insert("result".to_string(), result.clone());
        for dec in &func.decorators {
            if dec.name == "ensures" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, &result_env)?) {
                    let message = self.eval_expr(&dec.args[1], &result_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "ensures".to_string(), message });
                }
            }
        }
        Ok(())
    }
```

- [x] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-script requires_violation_returns_contract_violation_error requires_satisfied_returns_normally ensures_violation_returns_contract_violation_error ensures_satisfied_returns_normally range_lowered_to_requires_is_enforced_at_call_time`
Expected: PASS (5 tests)

- [x] **Step 5: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS — no regressions in `physure-core`, `physure-script`, `physure-cli`, `physure-python`, `physure-lsp`, or `physure-java`.

- [x] **Step 6: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "feat(phs): enforce @requires/@ensures contracts at call time"
```

---

### Task 7: `@stable`/`@experimental` are inert metadata — confirm and document with a test

**Files:**
- Test: `physure-script/src/interpreter.rs`

- [x] **Step 1: Write the test**

`@stable`/`@experimental` should not affect evaluation at all in this track — they only need to survive parsing (already covered by Task 5's `validate_decorators` tests) and not interfere with a normal call. Add to `physure-script/src/interpreter.rs`'s test module:

```rust
    #[test]
    fn stable_and_experimental_decorators_do_not_affect_evaluation() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("@stable\nfn f(x) = x * 2.0\nf(3.0)").unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 6.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 6.0),
            other => panic!("expected numeric value, got {other:?}"),
        }

        let mut interp2 = PhsInterpreter::default();
        let results2 = interp2.eval_str("@experimental\nfn g(x) = x * 3.0\ng(2.0)").unwrap();
        match results2.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 6.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 6.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }
```

- [x] **Step 2: Run the test to confirm it passes immediately**

Run: `cargo test -p physure-script stable_and_experimental_decorators_do_not_affect_evaluation`
Expected: PASS — no interpreter change needed; this test documents and locks in the "metadata only, no runtime behavior yet" contract from the roadmap so a future Track E/LSP change that starts reading these decorators can't silently break plain evaluation.

- [x] **Step 3: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "test(phs): lock in that @stable/@experimental are inert at call time"
```

---

## Self-Review

**Spec coverage** (against `docs/language_readiness_roadmap.md` Track F, "Phase 1 decorator catalog"):
- Generic `@name(args)` grammar + `DecoratorNode` AST + `known_decorators` registry → Tasks 1-3, 5.
- `@requires`/`@ensures` → Tasks 5 (validation/arity) and 6 (enforcement).
- `@range` as sugar over two `@requires`-equivalent checks → Task 5 (`lower_range`), reusing Task 6's enforcement with zero extra interpreter code.
- `@stable`/`@experimental` (metadata only) + mutual exclusion → Task 5 (validation), Task 7 (locks in no-runtime-effect).
- "Propagation into Track E's compiled artifact is mandatory" → explicitly out of scope for this plan (see "Scope boundary" section); left for Track E's own plan.
- `@review_required`/`@approved_by` and the dev-time/system-integration validation category → correctly absent from every task; not part of Phase 1.

**Placeholder scan:** no `TODO`/`TBD`/"add appropriate error handling" phrases; every step shows the exact code to write and the exact command to run.

**Type consistency:** `DecoratorNode { name: String, args: Vec<Expr> }` is used identically in Tasks 1, 3, 5, 6. `PhysureError::ContractViolation { decorator: String, message: String }` (Task 4) matches its two call sites in `check_requires`/`check_ensures` (Task 6) and its display test (Task 4). `lower_range(DecoratorNode) -> PhysureResult<Vec<DecoratorNode>>` (Task 5) matches its call site in `parse_decorated_stmt` (Task 3, updated in Task 5 Step 5). `validate_decorators(&[Statement]) -> PhysureResult<()>` (Task 5) matches both call sites in `parse_phs`/`parse_phs_with_lines`.

---

## Example script this plan makes valid

```phs
@stable
@range(v, 0.0 m/s, 2.998e8 m/s)
@ensures(result > 0.0 J, "kinetic energy must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

@experimental
@requires(t > 0.0 K, "temperature must be positive")
fn boltzmann_factor(e, t) = exp(-e / (1.380649e-23 J/K * t))
```
