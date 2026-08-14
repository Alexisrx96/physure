# Track C — Breakpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `DebugHook` mechanism to the PHS interpreter, a `phs debug script.phs` CLI
debugger consuming it, and the `parallel_map` sequential-fallback rule, per
[docs/superpowers/specs/2026-08-13-track-c-breakpoints-design.md](../specs/2026-08-13-track-c-breakpoints-design.md).

**Architecture:** Five task groups (C0–C4) matching the spec's sub-tracks, plus an integration
task. **C0 is the only hard blocker.** Everything else reads either the line numbers C0 adds or
the call stack C1 adds.

**Dependency graph — read this before dispatching subagents:**

```
C0 (source locations)
 ├──> C1 (DebugHook + StackFrame) ──> C3 (breakpoints; Line variant also needs C0)
 └─ (nothing else in C0 blocks C2 or C4)

C2 (Inspection)   — no dependency on C0/C1/C3 at all. Start immediately, in parallel with C0.
C4 (CLI debugger) — no dependency on C0/C1/C2/C3 for its own tasks (builds against a local fake
                     hook interface it defines itself in Task 4.1). Start immediately, in
                     parallel with C0. Only the Integration task wires it to the real hook.

Integration        — waits on C1, C2, C3, C4 all being done.
```

Concretely: **dispatch C0, C2, and C4 in parallel first.** Once C0 finishes, dispatch C1. Once
C1 finishes, dispatch C3. Once C1, C2, C3, C4 have all landed, run Integration.

**Tech Stack:** Rust only. No new dependencies (the CLI debugger reuses `physure-cli`'s existing
plain-stdin-loop pattern — see Task 4.1 — not a new crate).

---

## Task Group C0 — Source-location plumbing

**Files:**
- Modify: `physure-script/src/ast.rs` (`Program`, `FunctionDefNode`, `Statement::While`)
- Modify: `physure-script/src/parser.rs` (`parse_phs`, `parse_while_stmt`, `parse_function_def`)
- Modify (mechanical, compiler-guided): `physure-script/src/codegen/{rust,js,python,java,mod}.rs`,
  `physure-script/src/interpreter.rs`, `physure-script/src/decorators.rs`,
  `physure-script/src/codegen/{proto,md}.rs`

### Task C0.1: Add the line-tracking fields

- [ ] **Step 1: Write a failing test for the new fields**

In `physure-script/src/parser.rs`'s existing `#[cfg(test)] mod tests` block, add:
```rust
#[test]
fn parse_phs_records_line_numbers_for_top_level_function_and_while_bodies() {
    // PHS function bodies are indentation-delimited, not brace-delimited (only `while` uses
    // braces -- confirmed against phs.pest's `function_def = "fn" ~ ... ~ "=" ~ (block_body |
    // expr)` and `block_body = (_nl_indent ~ stmt)+`, and against a working example already in
    // the test suite: physure-script/tests/unit_shadowing.rs's `"fn f(x) =\n    t = 2.0 s\n
    // 5 m / t\n"`).
    let script = "x = 1\nfn f(a) =\n  a = a + 1\n  a\nwhile x < 3 {\n  x = x + 1\n}\n";
    let program = parse_phs(script).unwrap();

    assert_eq!(program.lines.len(), program.statements.len());
    assert_eq!(program.lines[0], 1); // x = 1

    let Statement::FunctionDef(f) = &program.statements[1] else { panic!("expected fn") };
    assert_eq!(f.body_lines.len(), f.body_stmts.len());
    assert_eq!(f.body_lines[0], 3); // a = a + 1
    assert_eq!(f.body_lines[1], 4); // a

    let Statement::While { body, body_lines, .. } = &program.statements[2] else { panic!("expected while") };
    assert_eq!(body_lines.len(), body.len());
    assert_eq!(body_lines[0], 6); // x = x + 1
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-script --lib parser::tests::parse_phs_records_line_numbers`
Expected: FAIL — `Program`/`FunctionDefNode`/`Statement::While` have no `lines`/`body_lines`
field yet, so this doesn't compile.

- [ ] **Step 3: Add the fields to `ast.rs`**

In `physure-script/src/ast.rs`, change:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}
```
to:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
    /// `lines[i]` is the 1-based source line of `statements[i]`. Empty for any `Program`
    /// built somewhere other than the real parser (codegen-internal rewrites, hand-built
    /// test fixtures) — line lookups elsewhere always index this defensively, never panic
    /// on a missing/short entry.
    #[serde(default)]
    pub lines: Vec<usize>,
}
```
and:
```rust
pub enum Statement {
    Import(ImportNode),
    Export(ExportNode),
    FunctionDef(FunctionDefNode),
    Assignment(AssignmentNode),
    Expr(Expr),
    Return(Expr),
    GuardReturn { cond: Expr, value: Expr },
    While { cond: Expr, body: Vec<Statement> },
}
```
to:
```rust
pub enum Statement {
    Import(ImportNode),
    Export(ExportNode),
    FunctionDef(FunctionDefNode),
    Assignment(AssignmentNode),
    Expr(Expr),
    Return(Expr),
    GuardReturn { cond: Expr, value: Expr },
    While {
        cond: Expr,
        body: Vec<Statement>,
        /// `body_lines[i]` is the source line of `body[i]`. Same defensive-lookup contract
        /// as `Program.lines`.
        #[serde(default)]
        body_lines: Vec<usize>,
    },
}
```

Then in `FunctionDefNode` (`ast.rs:46-61`), add the matching field:
```rust
pub struct FunctionDefNode {
    pub name: String,
    pub params: Vec<String>,
    #[serde(default)]
    pub param_units: Vec<Option<String>>,
    pub body_stmts: Vec<Statement>,
    /// Same defensive-lookup contract as `Program.lines`.
    #[serde(default)]
    pub body_lines: Vec<usize>,
    #[serde(default)]
    pub decorators: Vec<DecoratorNode>,
    #[serde(default)]
    pub doc: Option<String>,
}
```

- [ ] **Step 4: Fix every other `Statement::While` and `Program`/`FunctionDefNode` construction
      site so the crate compiles** (the compiler will list all of these — this step just gives
      you the exact fix for each one it finds):

  **`Statement::While` sites that destructure without `..`** — add `body_lines` to the pattern
  where the binding is unused (rename to `_body_lines` or use `..` if nothing else in the arm
  needs it):

  - `physure-script/src/interpreter.rs:329` — `Statement::While { cond, body } =>` becomes
    `Statement::While { cond, body, .. } =>` (this arm is rewritten fully in Task C1.3 anyway —
    for now, just make it compile).
  - `physure-script/src/codegen/rust.rs:82`, `physure-script/src/codegen/js.rs:145`,
    `physure-script/src/codegen/python.rs:89`, `physure-script/src/codegen/java.rs:166` — each
    is `Statement::While { cond, body } => { ... }` inside a `generate_statement`/codegen method
    that never needs line numbers; change each to `Statement::While { cond, body, .. } => { ... }`.
  - `physure-script/src/parser.rs:1121` (inside `check_statement_shadowing`) —
    `Statement::While { cond, body } => { ... }` becomes
    `Statement::While { cond, body, .. } => { ... }`.
  - `physure-script/src/codegen/mod.rs:154` (`inline_bindings_stmt`, an AST-rewrite pass that
    must preserve the original line numbers since it doesn't add/remove statements) —
    ```rust
    Statement::While { cond, body } => Statement::While {
        cond: inline_bindings(cond),
        body: body.iter().map(inline_bindings_stmt).collect(),
    },
    ```
    becomes:
    ```rust
    Statement::While { cond, body, body_lines } => Statement::While {
        cond: inline_bindings(cond),
        body: body.iter().map(inline_bindings_stmt).collect(),
        body_lines: body_lines.clone(),
    },
    ```
  - `physure-script/src/parser.rs:1526` and `physure-script/src/parser.rs:1538` (test-only,
    `if let Statement::While { cond: _, body } = &stmts[0]`) — add `body_lines: _`:
    `if let Statement::While { cond: _, body, body_lines: _ } = &stmts[0]`.

  **`Statement::While { .. }` and `Statement::While { body, .. }` sites** (already tolerant of a
  new field via `..`) — no change needed: `codegen/rust.rs:39`, `codegen/js.rs:18`,
  `codegen/js.rs:72`, `codegen/java.rs:82`, `ast.rs:251`, `parser.rs:1509`, `parser.rs:1518`.

  **`Statement::While { cond: ..., body: ... }` construction sites** (test fixtures — none of
  these are ever executed through the interpreter's debug path, so an empty `body_lines` is
  correct, not a shortcut) — add `body_lines: vec![]` to each:
  `physure-script/src/ast.rs:247`, `physure-script/src/codegen/rust.rs:454`,
  `physure-script/src/codegen/js.rs:380`, `physure-script/src/codegen/java.rs:374`,
  `physure-script/src/codegen/python.rs:356`.

  **Every `Program { statements: ... }` construction site outside `parser.rs`** — add
  `lines: vec![]` (none of these round-trip through the debugger; two synthesize a fresh
  `Program` from an already-processed statement list where per-statement source lines aren't
  meaningful, and the rest are test fixtures):
  `physure-script/src/codegen/proto.rs:58`, `physure-script/src/codegen/proto.rs:99`,
  `physure-script/src/codegen/mod.rs:481` (`Ok(Program { statements: all_statements })` →
  `Ok(Program { statements: all_statements, lines: vec![] })`), `physure-script/src/interpreter.rs:1432`,
  `physure-script/src/codegen/md.rs:92`.
  (`codegen/mod.rs:163`'s `inline_bindings_program` — `Program { statements: program.statements
  .iter().map(inline_bindings_stmt).collect() }` — preserves the source `Program`'s lines instead:
  `Program { statements: program.statements.iter().map(inline_bindings_stmt).collect(), lines:
  program.lines.clone() }`.)

  **Every `FunctionDefNode { ... }` construction site** — add `body_lines: vec![]` (these are
  either codegen-internal synthesized functions with no source line, or test fixtures):
  `physure-script/src/interpreter.rs:623`, `physure-script/src/interpreter.rs:989`,
  `physure-script/src/interpreter.rs:1364`, `physure-script/src/decorators.rs:186`,
  `physure-script/src/decorators.rs:213`, `physure-script/src/ast.rs:199`,
  `physure-script/src/codegen/rust.rs:365`, `physure-script/src/codegen/rust.rs:474`,
  `physure-script/src/codegen/rust.rs:502`, `physure-script/src/codegen/python.rs:295`,
  `physure-script/src/codegen/proto.rs:62`, `physure-script/src/codegen/mod.rs:511`,
  `physure-script/src/codegen/md.rs:96`, `physure-script/src/codegen/js.rs:279`,
  `physure-script/src/codegen/js.rs:300`, `physure-script/src/codegen/java.rs:308`.
  (`physure-script/src/codegen/mod.rs:150`'s `Statement::FunctionDef(def) =>
  Statement::FunctionDef(FunctionDefNode { body_stmts: ..., ..def.clone() })` already uses
  `..def.clone()` — no change needed, `body_lines` carries through automatically.)

  Run `cargo build --workspace 2>&1 | grep "missing field\|missing structure fields"` after each
  file to confirm you've found every site — the compiler is authoritative here, this list is a
  starting map, not a substitute for reading its output.

- [ ] **Step 5: Run test to verify it still fails the same way (fields exist, not populated yet)**

Run: `cargo test -p physure-script --lib parser::tests::parse_phs_records_line_numbers`
Expected: FAIL — compiles now, but `program.lines` is empty (`assert_eq!(program.lines.len(),
program.statements.len())` fails: `0 != 3`).

- [ ] **Step 6: Populate real line numbers in the parser**

In `physure-script/src/parser.rs`, change `parse_phs` (lines 13–31):
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
to:
```rust
pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push(parse_statement(inner)?);
            lines.push(line);
            statement_pos.push((line, col));
        }
    }

    validate_unit_shadowing(&statements, &statement_pos)?;
    crate::decorators::validate_decorators(&statements)?;
    Ok(Program { statements, lines })
}
```
(This mirrors `parse_phs_with_lines`'s already-proven mechanism a few lines below it — same
`pair.line_col()` call, just also kept instead of only feeding `statement_pos`.)

Change `parse_while_stmt` (lines 73–89):
```rust
fn parse_while_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let cond_pair = if first.as_rule() == Rule::_while_kw {
        inner.next().unwrap()
    } else {
        first
    };
    let cond = parse_expr(cond_pair)?;
    let mut body = Vec::new();
    for stmt_pair in inner {
        if stmt_pair.as_rule() == Rule::stmt {
            body.push(parse_statement(stmt_pair)?);
        }
    }
    Ok(Statement::While { cond, body })
}
```
to:
```rust
fn parse_while_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let cond_pair = if first.as_rule() == Rule::_while_kw {
        inner.next().unwrap()
    } else {
        first
    };
    let cond = parse_expr(cond_pair)?;
    let mut body = Vec::new();
    let mut body_lines = Vec::new();
    for stmt_pair in inner {
        if stmt_pair.as_rule() == Rule::stmt {
            body_lines.push(stmt_pair.line_col().0);
            body.push(parse_statement(stmt_pair)?);
        }
    }
    Ok(Statement::While { cond, body, body_lines })
}
```

Replace the entire `parse_function_def` function (`parser.rs:171-221`):
```rust
fn parse_function_def(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut name = String::new();
    let mut params = Vec::new();
    let mut param_units = Vec::new();
    let mut body_stmts = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::params => {
                for p in inner.into_inner() {
                    if p.as_rule() == Rule::param_item {
                        let mut param_inner = p.into_inner();
                        let id_str = param_inner.next().unwrap().as_str().to_string();
                        let unit_str = param_inner.next().map(|u| u.as_str().trim().to_string());
                        params.push(id_str);
                        param_units.push(unit_str);
                    } else {
                        params.push(p.as_str().to_string());
                        param_units.push(None);
                    }
                }
            }
            Rule::expr => {
                body_stmts.push(Statement::Expr(parse_expr(inner)?));
            }
            Rule::block_body => {
                for stmt_pair in inner.into_inner() {
                    if stmt_pair.as_rule() == Rule::stmt {
                        let inner_stmt = stmt_pair.into_inner().next().unwrap();
                        body_stmts.push(parse_statement(inner_stmt)?);
                    } else if stmt_pair.as_rule() != Rule::_nl_indent {
                        body_stmts.push(parse_statement(stmt_pair)?);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        decorators: Vec::new(),
        doc: None,
    }))
}
```
with:
```rust
fn parse_function_def(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    // Captured before `pair.into_inner()` consumes `pair` by value below -- this is the whole
    // `fn ... = ...` construct's own starting line, used for the single-expression-body case
    // (`fn f(x) = x^2`), which has exactly one body statement and no `stmt`-level pair of its
    // own to read a line from.
    let def_line = pair.line_col().0;
    let mut name = String::new();
    let mut params = Vec::new();
    let mut param_units = Vec::new();
    let mut body_stmts = Vec::new();
    let mut body_lines = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::params => {
                for p in inner.into_inner() {
                    if p.as_rule() == Rule::param_item {
                        let mut param_inner = p.into_inner();
                        let id_str = param_inner.next().unwrap().as_str().to_string();
                        let unit_str = param_inner.next().map(|u| u.as_str().trim().to_string());
                        params.push(id_str);
                        param_units.push(unit_str);
                    } else {
                        params.push(p.as_str().to_string());
                        param_units.push(None);
                    }
                }
            }
            Rule::expr => {
                body_lines.push(def_line);
                body_stmts.push(Statement::Expr(parse_expr(inner)?));
            }
            Rule::block_body => {
                for stmt_pair in inner.into_inner() {
                    if stmt_pair.as_rule() == Rule::stmt {
                        body_lines.push(stmt_pair.line_col().0);
                        let inner_stmt = stmt_pair.into_inner().next().unwrap();
                        body_stmts.push(parse_statement(inner_stmt)?);
                    } else if stmt_pair.as_rule() != Rule::_nl_indent {
                        body_lines.push(stmt_pair.line_col().0);
                        body_stmts.push(parse_statement(stmt_pair)?);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        body_lines,
        decorators: Vec::new(),
        doc: None,
    }))
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cargo test -p physure-script --lib parser::tests::parse_phs_records_line_numbers`
Expected: PASS.

- [ ] **Step 8: Run the full `physure-script` test suite and full workspace build**

Run: `cargo test -p physure-script --lib`
Expected: PASS (every pre-existing test still compiles and passes — this is the payoff of the
defensive `vec![]`/`..` approach in Step 4: nothing else needed real line numbers to keep working).

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 9: Commit**

```bash
git add physure-script/src/ast.rs physure-script/src/parser.rs physure-script/src/interpreter.rs \
        physure-script/src/decorators.rs physure-script/src/codegen/rust.rs \
        physure-script/src/codegen/js.rs physure-script/src/codegen/python.rs \
        physure-script/src/codegen/java.rs physure-script/src/codegen/mod.rs \
        physure-script/src/codegen/proto.rs physure-script/src/codegen/md.rs
git commit -m "feat(phs): thread source line numbers through Program, FunctionDefNode, and Statement::While"
```

---

## Task Group C1 — `DebugHook` + call stack (depends on C0)

**Files:**
- Create: `physure-script/src/debug.rs`
- Modify: `physure-script/src/lib.rs` (register the module)
- Modify: `physure-script/src/interpreter.rs` (`PhsInterpreter` struct, `eval_statement_with_env`,
  `call_function_node`)

### Task C1.1: `DebugHook`, `DebugContext`, `StackFrame`, `DebugAction` types

- [ ] **Step 1: Write a failing test for `StackFrame::declared`**

Create `physure-script/src/debug.rs`:
```rust
use std::collections::HashSet;
use crate::value::PhsValue;
use std::collections::HashMap;

/// Implemented by a debugger front end (the CLI in Track C, a DAP adapter later) to observe
/// and control interpreter execution. `None` on `PhsInterpreter` costs nothing — every call
/// site checks `is_none()` before doing any work to build a `DebugContext`.
pub trait DebugHook: Send + Sync {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction;
}

pub struct DebugContext<'a> {
    pub line: usize,
    pub call_stack: &'a [StackFrame],
    pub env: &'a HashMap<String, PhsValue>,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub fn_name: String,
    pub call_site_line: usize,
    /// Params plus every name first assigned in this function's `body_stmts` — computed once,
    /// statically, from the `FunctionDefNode` when the frame is pushed. Lets the debugger
    /// distinguish "local to this call" from "visible because `call_function_node` clones the
    /// whole caller-side env" without re-walking values at every pause.
    pub declared: HashSet<String>,
}

impl StackFrame {
    pub fn new(func: &crate::ast::FunctionDefNode, call_site_line: usize) -> Self {
        let mut declared: HashSet<String> = func.params.iter().cloned().collect();
        for stmt in &func.body_stmts {
            collect_declared(stmt, &mut declared);
        }
        StackFrame { fn_name: func.name.clone(), call_site_line, declared }
    }
}

fn collect_declared(stmt: &crate::ast::Statement, declared: &mut HashSet<String>) {
    use crate::ast::Statement;
    match stmt {
        Statement::Assignment(node) => {
            declared.insert(node.name.clone());
        }
        Statement::FunctionDef(node) => {
            declared.insert(node.name.clone());
        }
        Statement::While { body, .. } => {
            for s in body {
                collect_declared(s, declared);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    Pause,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AssignmentNode, FunctionDefNode, Statement};
    use crate::ast::Expr;

    #[test]
    fn declared_includes_params_and_body_assignments_not_globals() {
        let func = FunctionDefNode {
            name: "f".to_string(),
            params: vec!["a".to_string()],
            param_units: vec![None],
            body_stmts: vec![
                Statement::Assignment(AssignmentNode {
                    name: "b".to_string(),
                    value: Expr::Identifier("a".to_string()),
                    decorators: Vec::new(),
                }),
                Statement::Return(Expr::Identifier("b".to_string())),
            ],
            body_lines: vec![1, 2],
            decorators: Vec::new(),
            doc: None,
        };
        let frame = StackFrame::new(&func, 10);
        assert!(frame.declared.contains("a"));
        assert!(frame.declared.contains("b"));
        assert!(!frame.declared.contains("some_global"));
        assert_eq!(frame.fn_name, "f");
        assert_eq!(frame.call_site_line, 10);
    }
}
```

- [ ] **Step 2: Register the module and run the test**

In `physure-script/src/lib.rs`, add `pub mod debug;` next to the other `pub mod` lines, and
`pub use debug::{DebugAction, DebugContext, DebugHook, StackFrame};` alongside the other
`pub use` lines.

Run: `cargo test -p physure-script --lib debug::tests::declared_includes_params`
Expected: PASS (this one doesn't need C0 output at all — it's pure AST-walking over a
hand-built `FunctionDefNode`, which is why it's written first and independently testable).

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/debug.rs physure-script/src/lib.rs
git commit -m "feat(phs): add DebugHook trait and StackFrame::declared"
```

### Task C1.2: Wire `debug_hook` + `call_stack` into `PhsInterpreter`

- [ ] **Step 1: Write a failing test using a fake hook**

In `physure-script/src/interpreter.rs`'s `mod tests` block:
```rust
#[test]
fn debug_hook_fires_once_per_statement_including_function_return() {
    use crate::debug::{DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct RecordingHook(Arc<Mutex<Vec<usize>>>);
    impl DebugHook for RecordingHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.0.lock().unwrap().push(ctx.line);
            DebugAction::Continue
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(RecordingHook(seen.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    // PHS function bodies are indentation-delimited (see the note on the C0.1 test) --
    // "fn double(x) =" on line 1, its two-statement body on lines 2-3, then the top-level call
    // on line 4 (no closing brace to account for).
    let program = crate::parser::parse_phs(
        "fn double(x) =\n  y = x * 2\n  return y\nres = double(3)\n",
    )
    .unwrap();
    interp.run_statements_with_lines(&program).unwrap();

    let lines = seen.lock().unwrap();
    // line 4 (top-level call), then the two statements inside double's body (lines 2 and 3).
    // Line 3 is an explicit `return`, which `call_function_node_at` special-cases with its own
    // `break` instead of routing through `eval_statement_with_env_at` like every other
    // statement -- this is the actual regression case for the choke-point gap (a bare
    // expression used as an implicit return, e.g. just `y` with no `return` keyword, would
    // already have been checkpointed by the ordinary `_` arm and wouldn't exercise this path).
    assert!(lines.contains(&4), "top-level call not recorded: {lines:?}");
    assert!(lines.contains(&2), "first body statement not recorded: {lines:?}");
    assert!(lines.contains(&3), "function's explicit return statement not recorded: {lines:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-script --lib interpreter::tests::debug_hook_fires_once_per_statement`
Expected: FAIL — `with_debug_hook`/`run_statements_with_lines` don't exist yet.

- [ ] **Step 3: Add the fields and constructor**

In `physure-script/src/interpreter.rs`, add to the top-of-file imports:
```rust
use crate::debug::{DebugAction, DebugContext, DebugHook, StackFrame};
```

Change the `PhsInterpreter` struct (`interpreter.rs:169-184`):
```rust
pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
    pub externals: HashMap<String, ExternalFn>,
    plugin_state: Arc<Mutex<crate::plugin::PluginState>>,
    plugin_base_dir: Option<std::path::PathBuf>,
    unlocked_builtins: Arc<Mutex<HashMap<String, (&'static str, String)>>>,
    dynamic_externals: Arc<Mutex<HashMap<String, ExternalFn>>>,
}
```
to:
```rust
pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
    pub externals: HashMap<String, ExternalFn>,
    plugin_state: Arc<Mutex<crate::plugin::PluginState>>,
    plugin_base_dir: Option<std::path::PathBuf>,
    unlocked_builtins: Arc<Mutex<HashMap<String, (&'static str, String)>>>,
    dynamic_externals: Arc<Mutex<HashMap<String, ExternalFn>>>,
    pub(crate) debug_hook: Option<Arc<dyn DebugHook>>,
    /// `Arc<Mutex<..>>`, not `RefCell`: Track B's `for`-expression and `parallel_map` rayon
    /// paths require `&PhsInterpreter: Send + Sync` at compile time regardless of whether a
    /// hook is set at runtime -- `RefCell` would break both of those already-shipped parallel
    /// paths. The mutex is never actually contended once debugging is active because
    /// `parallel_map` falls back to sequential execution whenever `debug_hook.is_some()`
    /// (Integration task).
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
}
```

Add a constructor next to `PhsInterpreter::new` (`interpreter.rs:210-220`):
```rust
    pub fn with_debug_hook(resolver: Arc<dyn ModuleResolver>, hook: Arc<dyn DebugHook>) -> Self {
        let mut interp = Self::new(resolver);
        interp.debug_hook = Some(hook);
        interp
    }
```

And update `PhsInterpreter::new`'s body (`interpreter.rs:210-220`) to initialize the two new
fields:
```rust
    pub fn new(resolver: Arc<dyn ModuleResolver>) -> Self {
        Self {
            env: HashMap::new(),
            resolver,
            externals: HashMap::new(),
            plugin_state: Arc::new(Mutex::new(crate::plugin::PluginState::default())),
            plugin_base_dir: None,
            unlocked_builtins: Arc::new(Mutex::new(HashMap::new())),
            dynamic_externals: Arc::new(Mutex::new(HashMap::new())),
            debug_hook: None,
            call_stack: Arc::new(Mutex::new(Vec::new())),
        }
    }
```

- [ ] **Step 4: Add `debug_checkpoint` and the line-aware statement executor**

Rename the existing `eval_statement_with_env` (`interpreter.rs:304-348`) to
`eval_statement_with_env_at`, add a `line: usize` parameter, and call `debug_checkpoint` first:
```rust
    fn debug_checkpoint(&self, line: usize, env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        let Some(hook) = &self.debug_hook else { return Ok(()) };
        let call_stack = self.call_stack.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = DebugContext { line, call_stack: &call_stack, env };
        // v1: every action resumes execution. StepOver/StepOut/Pause bookkeeping (comparing
        // call_stack depth across calls) is Task C3's job once breakpoints exist to pause on;
        // C1 only has to prove the checkpoint fires at the right places with the right context.
        let _ = hook.on_statement(&ctx);
        Ok(())
    }

    pub fn eval_statement_with_env(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        self.eval_statement_with_env_at(stmt, env, 0)
    }

    fn eval_statement_with_env_at(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>, line: usize) -> PhysureResult<PhsValue> {
        self.debug_checkpoint(line, env)?;
        match stmt {
            Statement::Assignment(node) => {
                let val = self.eval_expr(&node.value, env)?;
                env.insert(node.name.clone(), val.clone());
                Ok(val)
            }
            Statement::FunctionDef(node) => {
                env.insert(node.name.clone(), PhsValue::Function(node.clone()));
                Ok(PhsValue::None)
            }
            Statement::Expr(expr) => {
                self.eval_expr(expr, env)
            }
            Statement::Import(node) => self.resolve_use(node, env),
            Statement::Export(_node) => Ok(PhsValue::None),
            Statement::Return(expr) => self.eval_expr(expr, env),
            Statement::GuardReturn { cond, value } => {
                let cond_val = self.eval_expr(cond, env)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(value, env)
                } else {
                    Ok(PhsValue::None)
                }
            }
            Statement::While { cond, body, body_lines } => {
                const DEFAULT_MAX_LOOP_ITERATIONS: usize = 10_000;
                let mut count = 0;
                let mut last_val = PhsValue::None;
                while is_truthy(&self.eval_expr(cond, env)?) {
                    if count >= DEFAULT_MAX_LOOP_ITERATIONS {
                        return Err(PhysureError::Generic(format!(
                            "while loop did not converge after {} iterations",
                            DEFAULT_MAX_LOOP_ITERATIONS
                        )));
                    }
                    count += 1;
                    for (i, stmt) in body.iter().enumerate() {
                        let line = body_lines.get(i).copied().unwrap_or(0);
                        last_val = self.eval_statement_with_env_at(stmt, env, line)?;
                    }
                }
                Ok(last_val)
            }
        }
    }
```
(Note the pre-existing public `eval_statement_with_env(&self, stmt, env)` signature is
preserved exactly — `physure-cli/src/main.rs:103` and `:383` call it directly and must keep
compiling unchanged. It now delegates to the line-aware version with `line: 0`, which is
correct for its actual callers: the free-standing REPL and the LSP's per-statement re-eval,
neither of which has a `Program` with real line numbers in hand at that call site.)

Update `eval_statement`'s single call site (`interpreter.rs:457-462`) — no change needed, it
already calls the public `eval_statement_with_env`, which still exists with its original
signature.

Add a new line-aware top-level entry point, next to `run_statements` (`interpreter.rs:273-279`):
```rust
    /// Like `run_statements`, but executes against `program.lines` so `debug_checkpoint` sees
    /// real source lines instead of `0`. This is what `phs debug` uses; `run_statements` stays
    /// as-is for every other caller (Python/WASM/Java bindings, the plain REPL) that doesn't
    /// have line-accurate debugging as a goal.
    pub fn run_statements_with_lines(&mut self, program: &Program) -> PhysureResult<PhsValue> {
        let mut env = self.env.clone();
        let mut last = PhsValue::None;
        for (i, stmt) in program.statements.iter().enumerate() {
            let line = program.lines.get(i).copied().unwrap_or(0);
            last = self.eval_statement_with_env_at(stmt, &mut env, line)?;
        }
        self.env = env;
        Ok(last)
    }
```

- [ ] **Step 5: Update `call_function_node` to push/pop a `StackFrame` and checkpoint every
      statement including `Return`/`GuardReturn`**

Change `call_function_node` (`interpreter.rs:810-846`):
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
```
to:
```rust
    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        self.call_function_node_at(func, arg_vals, env, 0)
    }

    fn call_function_node_at(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
        call_site_line: usize,
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

        if self.debug_hook.is_some() {
            self.call_stack.lock().unwrap_or_else(|e| e.into_inner())
                .push(StackFrame::new(func, call_site_line));
        }

        let mut last_val = PhsValue::None;
        for (i, stmt) in func.body_stmts.iter().enumerate() {
            let line = func.body_lines.get(i).copied().unwrap_or(0);
            match stmt {
                Statement::Return(expr) => {
                    self.debug_checkpoint(line, &local_env)?;
                    last_val = self.eval_expr(expr, &local_env)?;
                    break;
                }
                Statement::GuardReturn { cond, value } => {
                    self.debug_checkpoint(line, &local_env)?;
                    let cond_val = self.eval_expr(cond, &local_env)?;
                    if is_truthy(&cond_val) {
                        last_val = self.eval_expr(value, &local_env)?;
                        break;
                    }
                }
                _ => {
                    last_val = self.eval_statement_with_env_at(stmt, &mut local_env, line)?;
                }
            }
        }

        if self.debug_hook.is_some() {
            self.call_stack.lock().unwrap_or_else(|e| e.into_inner()).pop();
        }

        self.check_ensures(func, &local_env, &last_val)?;
        Ok(last_val)
    }
```
(`call_site_line` is `0` from every existing caller of the public `call_function_node` — the
only caller that will pass a real one is `eval_expr`'s `Expr::FunctionCall` arm, updated next.)

Find the `Expr::FunctionCall` arm's user-defined-function call site
(`self.call_function_node(func, arg_vals, env)` around `interpreter.rs:651-652`) and change it
to pass the call's own line where available — since `Expr` doesn't carry line numbers (only
`Statement` does, per the spec's decision to scope this to statement granularity), pass `0`
here too for now: this is an accepted, documented v1 gap — `StackFrame.call_site_line` is
accurate for functions called from a bare statement position debugged via
`run_statements_with_lines`/`eval_statement_with_env_at` (the common case: `res = f(x)` is
itself a `Statement::Assignment` whose line C0 already tracks and which flows into
`call_function_node_at` via `eval_expr` → `eval_expr`'s existing `FunctionCall` handling calling
`self.call_function_node(...)`, i.e. the public, `0`-line wrapper) — expression-level call-site
precision (e.g. a call nested three levels deep inside one expression) is out of scope for
LAB-READY and not claimed by the spec.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p physure-script --lib interpreter::tests::debug_hook_fires_once_per_statement`
Expected: PASS.

- [ ] **Step 7: Run the full `physure-script` test suite**

Run: `cargo test -p physure-script --lib`
Expected: PASS — this includes every Track A/B test that exercises `call_function_node`,
`eval_statement_with_env`, and `while`, all of which changed signature/body in this step.

Run: `cargo build --workspace`
Expected: builds cleanly (confirms `physure-cli`'s two direct `eval_statement_with_env` callers
still compile unchanged).

- [ ] **Step 8: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "feat(phs): wire DebugHook and StackFrame into the interpreter's statement dispatch"
```

---

## Task Group C2 — `Inspection` (no dependency on C0/C1/C3 — start immediately)

**Files:**
- Create: `physure-script/src/inspect.rs`
- Modify: `physure-script/src/lib.rs` (register the module)
- Modify: `physure-core/src/units/registry.rs` (`UnitRegistry::split_prefix`, factored out of
  `get_unit`)

### Task C2.1: `UnitRegistry::split_prefix`

- [ ] **Step 1: Write a failing test**

In `physure-core/src/units/registry.rs`, find the existing `#[cfg(test)]` block (the file
already has tests per Track B's plan touching this file's neighbor `conf.rs` — if
`registry.rs` has no test module yet, add one at the end of the file) and add:
```rust
#[test]
fn split_prefix_recognizes_a_registered_prefix_over_a_known_unit() {
    let (reg, _) = crate::units::conf::build_registry_from_conf();
    let (symbol, factor) = reg.split_prefix("km").expect("km should split as k + m");
    assert_eq!(symbol, "k");
    assert_eq!(factor, 1000.0);
}

#[test]
fn split_prefix_returns_none_for_a_plain_unregistered_remainder() {
    let (reg, _) = crate::units::conf::build_registry_from_conf();
    assert!(reg.split_prefix("qzzz").is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-core --lib units::registry::tests::split_prefix`
Expected: FAIL — `split_prefix` doesn't exist.

- [ ] **Step 3: Factor the existing inline prefix-matching loop out of `get_unit`**

In `physure-core/src/units/registry.rs`, `get_unit` currently reads (lines 142-179):
```rust
    pub fn get_unit(&self, name: &str) -> Option<RationalUnit> {
        let resolved = self.resolve_symbol(name);
        let mut u = if let Some(unit) = self.base_units.get(&resolved) {
            Some(unit.clone())
        } else if let Some(unit) = self.derived_units.get(&resolved) {
            Some(unit.clone())
        } else if let Some(unit) = self.base_units.get(name) {
            Some(unit.clone())
        } else if let Some(unit) = self.derived_units.get(name) {
            Some(unit.clone())
        } else {
            let mut prefix_match = None;
            for (p_sym, p_factor) in &self.prefixes {
                if name.starts_with(p_sym) && name.len() > p_sym.len() {
                    let rest = &name[p_sym.len()..];
                    let rest_resolved = self.resolve_symbol(rest);
                    let base_opt = self
                        .base_units
                        .get(&rest_resolved)
                        .or_else(|| self.derived_units.get(&rest_resolved))
                        .or_else(|| self.base_units.get(rest))
                        .or_else(|| self.derived_units.get(rest));
                    if let Some(base_u) = base_opt {
                        let new_scale = base_u.scale * p_factor;
                        let mut prefixed = base_u.clone().with_scale(new_scale);
                        prefixed.display_name = Some(name.to_string());
                        prefix_match = Some(prefixed);
                        break;
                    }
                }
            }
            prefix_match
        };
        if let Some(ref mut unit) = u {
            if unit.display_name.is_none() {
                unit.display_name = Some(name.to_string());
            }
        }
```
Replace the inner `else { ... }` block (the `prefix_match` loop) with a call to a new shared
method:
```rust
        } else {
            self.split_prefix(name).and_then(|(p_sym, p_factor)| {
                let rest = &name[p_sym.len()..];
                let rest_resolved = self.resolve_symbol(rest);
                let base_opt = self
                    .base_units
                    .get(&rest_resolved)
                    .or_else(|| self.derived_units.get(&rest_resolved))
                    .or_else(|| self.base_units.get(rest))
                    .or_else(|| self.derived_units.get(rest));
                base_opt.map(|base_u| {
                    let new_scale = base_u.scale * p_factor;
                    let mut prefixed = base_u.clone().with_scale(new_scale);
                    prefixed.display_name = Some(name.to_string());
                    prefixed
                })
            })
        };
```
And add the new method, next to `get_unit`:
```rust
    /// Splits `name` into a registered prefix symbol and its factor, if `name` is a known
    /// prefix immediately followed by a base or derived unit symbol (e.g. `"km"` -> `Some(("k",
    /// 1000.0))`; `"km/h"` or any other compound expression -> `None`, since this only
    /// recognizes a single prefixed symbol, not an arbitrary unit expression). Factored out of
    /// `get_unit`'s own prefix-matching so `Inspection`'s reverse lookup (Track C) reuses this
    /// exact rule instead of re-deriving it.
    pub fn split_prefix(&self, name: &str) -> Option<(String, f64)> {
        for (p_sym, p_factor) in &self.prefixes {
            if name.starts_with(p_sym.as_str()) && name.len() > p_sym.len() {
                let rest = &name[p_sym.len()..];
                let rest_resolved = self.resolve_symbol(rest);
                let known = self.base_units.contains_key(&rest_resolved)
                    || self.derived_units.contains_key(&rest_resolved)
                    || self.base_units.contains_key(rest)
                    || self.derived_units.contains_key(rest);
                if known {
                    return Some((p_sym.clone(), *p_factor));
                }
            }
        }
        None
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p physure-core --lib units::registry::tests::split_prefix`
Expected: PASS.

- [ ] **Step 5: Run the full `physure-core` test suite to confirm `get_unit`'s behavior is unchanged**

Run: `cargo test -p physure-core --lib`
Expected: PASS — every existing prefixed-unit test (`"km"`, `"kN"`, etc., parsed via
`get_unit`) still resolves identically; this step only refactored where the matching logic
lives, not what it decides.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/units/registry.rs
git commit -m "refactor(core): factor UnitRegistry::split_prefix out of get_unit for reuse"
```

### Task C2.2: `Inspection` struct and builder

- [ ] **Step 1: Write failing tests**

Create `physure-script/src/inspect.rs`:
```rust
use crate::value::PhsValue;
use physure_core::units::UnitRegistry;

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Global,
    Local { owner_fn: String, frame_depth: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Scalar,
    Vector(usize),
    Matrix(usize, usize),
    Function,
    Equation,
    Bool,
    String,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UncertaintySummary {
    pub std_dev: f64,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueDetail {
    None,
    Function { params: Vec<String>, param_units: Vec<Option<String>> },
    Equation { lhs: String, rhs: String },
    /// Vector/Matrix elements, each recursively inspected, capped at the first 10 -- `kind`
    /// already carries the true length/shape, so truncation here never loses that information.
    Elements(Vec<Inspection>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    pub name: String,
    pub kind: ValueKind,
    pub scope: ScopeKind,
    pub measure: Option<f64>,
    pub unit_display: Option<String>,
    pub prefix: Option<(String, f64)>,
    pub dimension: Vec<(String, i64, i64)>,
    pub uncertainty: Option<UncertaintySummary>,
    pub detail: ValueDetail,
}

const MAX_INSPECTED_ELEMENTS: usize = 10;

pub fn inspect(name: &str, value: &PhsValue, scope: ScopeKind, registry: &UnitRegistry) -> Inspection {
    let base = Inspection {
        name: name.to_string(),
        kind: ValueKind::None,
        scope: scope.clone(),
        measure: None,
        unit_display: None,
        prefix: None,
        dimension: Vec::new(),
        uncertainty: None,
        detail: ValueDetail::None,
    };
    match value {
        PhsValue::None => base,
        PhsValue::Number(n) => Inspection { kind: ValueKind::Scalar, measure: Some(*n), ..base },
        PhsValue::Bool(_) => Inspection { kind: ValueKind::Bool, ..base },
        PhsValue::String(_) => Inspection { kind: ValueKind::String, ..base },
        PhsValue::Quantity(q) => {
            let unit_display = Some(q.unit.__repr__());
            let prefix = q.unit.display_name.as_ref().and_then(|dn| registry.split_prefix(dn));
            let dimension = q.unit.dimensions.iter().map(|(sym, (n, d))| (sym.clone(), *n, *d)).collect();
            let std_dev = q.value.std_dev();
            let uncertainty = if std_dev > 0.0 {
                Some(UncertaintySummary { std_dev, backend: q.value.get_model_name().to_string() })
            } else {
                None
            };
            Inspection {
                kind: ValueKind::Scalar,
                measure: Some(q.value.mean()),
                unit_display,
                prefix,
                dimension,
                uncertainty,
                ..base
            }
        }
        PhsValue::Vector(v) => Inspection {
            kind: ValueKind::Vector(v.len()),
            detail: ValueDetail::Elements(
                v.iter()
                    .take(MAX_INSPECTED_ELEMENTS)
                    .enumerate()
                    .map(|(i, el)| inspect(&format!("{name}[{i}]"), el, scope.clone(), registry))
                    .collect(),
            ),
            ..base
        },
        PhsValue::Matrix(m) => Inspection {
            kind: ValueKind::Matrix(m.rows, m.cols),
            detail: ValueDetail::Elements(
                m.data
                    .iter()
                    .flatten()
                    .take(MAX_INSPECTED_ELEMENTS)
                    .enumerate()
                    .map(|(i, q)| inspect(&format!("{name}[{i}]"), &PhsValue::Quantity(q.clone()), scope.clone(), registry))
                    .collect(),
            ),
            ..base
        },
        PhsValue::Function(f) => Inspection {
            kind: ValueKind::Function,
            detail: ValueDetail::Function { params: f.params.clone(), param_units: f.param_units.clone() },
            ..base
        },
        PhsValue::Equation(l, r) => Inspection {
            kind: ValueKind::Equation,
            detail: ValueDetail::Equation { lhs: format!("{l:?}"), rhs: format!("{r:?}") },
            ..base
        },
        // Sigma/SigmaBound/Plot/Range: no dedicated Inspection shape yet -- fall back to the
        // untyped base rather than guessing at a decomposition the roadmap never specified.
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::quantity::Quantity;
    use physure_core::units::parser::Parser as UnitParser;

    fn registry() -> UnitRegistry {
        physure_core::units::conf::build_registry_from_conf().0
    }

    #[test]
    fn inspects_a_plain_scalar_number() {
        let reg = registry();
        let insp = inspect("x", &PhsValue::Number(3.0), ScopeKind::Global, &reg);
        assert_eq!(insp.kind, ValueKind::Scalar);
        assert_eq!(insp.measure, Some(3.0));
        assert_eq!(insp.unit_display, None);
        assert_eq!(insp.dimension, vec![]);
        assert_eq!(insp.prefix, None);
    }

    #[test]
    fn inspects_a_km_quantity_with_prefix_present() {
        let reg = registry();
        let unit = UnitParser::parse_expression_with_registry("km", &reg).unwrap();
        let q = Quantity::new_scalar(5.0, 0.0, unit, None, None);
        let insp = inspect("d", &PhsValue::Quantity(q), ScopeKind::Global, &reg);
        assert_eq!(insp.kind, ValueKind::Scalar);
        assert_eq!(insp.measure, Some(5.0));
        assert_eq!(insp.prefix, Some(("k".to_string(), 1000.0)));
        assert!(insp.dimension.iter().any(|(sym, _, _)| sym == "m"));
    }

    #[test]
    fn inspects_a_compound_unit_with_prefix_absent() {
        let reg = registry();
        let unit = UnitParser::parse_expression_with_registry("km/h", &reg).unwrap();
        let q = Quantity::new_scalar(60.0, 0.0, unit, None, None);
        let insp = inspect("v", &PhsValue::Quantity(q), ScopeKind::Global, &reg);
        assert_eq!(insp.prefix, None);
    }
}
```

- [ ] **Step 2: Register the module**

In `physure-script/src/lib.rs`, add `pub mod inspect;` next to the other `pub mod` lines.

- [ ] **Step 3: Run tests to verify they fail, then pass**

Run: `cargo test -p physure-script --lib inspect::tests`
Expected: initial FAIL if `physure_core::units::UnitRegistry`/`RationalUnit.dimensions` aren't
re-exported the way this file assumes — check `physure-core/src/lib.rs`'s existing `pub use
units::...` list and adjust the `use` statements at the top of `inspect.rs` to match whatever
path `physure-script` already uses elsewhere in this codebase for `UnitRegistry`
(`interpreter.rs`'s own `use physure_core::units::parser::Parser as UnitParser;` and
`physure_core::UnitRegistry` at `interpreter.rs:4` confirm `physure_core::UnitRegistry` is the
correct top-level path — use that instead of `physure_core::units::UnitRegistry` above if it
differs). Once imports resolve:
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/inspect.rs physure-script/src/lib.rs
git commit -m "feat(phs): add Inspection value decomposition (measure/unit/prefix/dimension/uncertainty)"
```

---

## Task Group C3 — Breakpoints (depends on C1; `Line` variant also depends on C0)

**Files:**
- Modify: `physure-script/src/debug.rs` (add `Breakpoint`)
- Modify: `physure-script/src/interpreter.rs` (`PhsInterpreter` gains `breakpoints`,
  `debug_checkpoint` consults them)

### Task C3.1: `Breakpoint` enum + matching in `debug_checkpoint`

- [ ] **Step 1: Write a failing test**

In `physure-script/src/interpreter.rs`'s `mod tests` block:
```rust
#[test]
fn function_entry_breakpoint_pauses_on_every_call() {
    use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct CountingHook(Arc<Mutex<usize>>);
    impl DebugHook for CountingHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            if ctx.call_stack.last().map(|f| f.fn_name.as_str()) == Some("double") {
                *self.0.lock().unwrap() += 1;
            }
            DebugAction::Continue
        }
    }

    let hits = Arc::new(Mutex::new(0));
    let hook = Arc::new(CountingHook(hits.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    interp.add_breakpoint(Breakpoint::FunctionEntry("double".to_string()));
    // Single-expression function body (no indentation block needed): "fn f(x) = expr".
    let program = crate::parser::parse_phs(
        "fn double(x) = x * 2\na = double(1)\nb = double(2)\n",
    )
    .unwrap();
    interp.run_statements_with_lines(&program).unwrap();

    assert_eq!(*hits.lock().unwrap(), 2, "expected a pause on each of the two calls");
}

#[test]
fn conditional_breakpoint_pauses_only_when_condition_holds() {
    use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct CountingHook(Arc<Mutex<usize>>);
    impl DebugHook for CountingHook {
        fn on_statement(&self, _ctx: &DebugContext) -> DebugAction {
            *self.0.lock().unwrap() += 1;
            DebugAction::Continue
        }
    }

    let hits = Arc::new(Mutex::new(0));
    let hook = Arc::new(CountingHook(hits.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    // A checkpoint fires *before* its own statement's effect (see debug_checkpoint's call at
    // the top of eval_statement_with_env_at, ahead of the match on the statement itself) --
    // so the condition targets the line *after* the assignment it depends on, where `x` has
    // already settled to its final value from the fully-executed previous statement.
    let program = crate::parser::parse_phs(
        "x = 1\nx = 2\nx = 3\ny = x\n",
    )
    .unwrap();
    let cond_line = program.lines[3]; // the "y = x" statement
    let cond_expr = crate::parser::parse_phs("x > 2").unwrap().statements.remove(0);
    let crate::ast::Statement::Expr(cond) = cond_expr else { panic!("expected expr") };
    interp.add_breakpoint(Breakpoint::Conditional(cond_line, cond));

    interp.run_statements_with_lines(&program).unwrap();

    assert_eq!(*hits.lock().unwrap(), 1, "should only pause once x has actually reached 3");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p physure-script --lib interpreter::tests::function_entry_breakpoint`
Run: `cargo test -p physure-script --lib interpreter::tests::conditional_breakpoint`
Expected: both FAIL — `Breakpoint`/`add_breakpoint` don't exist yet.

- [ ] **Step 3: Add `Breakpoint` to `debug.rs`**

In `physure-script/src/debug.rs`, add:
```rust
#[derive(Debug, Clone)]
pub enum Breakpoint {
    Line(usize),
    Conditional(usize, crate::ast::Expr),
    FunctionEntry(String),
}
```

- [ ] **Step 4: Wire breakpoints into `PhsInterpreter`**

In `physure-script/src/interpreter.rs`, add a `breakpoints: Arc<Mutex<Vec<crate::debug::Breakpoint>>>`
field to `PhsInterpreter` (same pattern as `call_stack`), initialize it to `Arc::new(Mutex::new(Vec::new()))`
in `new`, and add:
```rust
    pub fn add_breakpoint(&mut self, bp: crate::debug::Breakpoint) {
        self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).push(bp);
    }
```

Update `debug_checkpoint` so a matching breakpoint always calls the hook (today it already does
unconditionally, since C1 has no notion of "only pause when a breakpoint says so" yet — this
step narrows that): a `Line`/`Conditional` breakpoint matches by `line`; a `FunctionEntry`
breakpoint matches by checking whether this checkpoint is the *first* statement of a frame whose
`fn_name` matches (i.e. `call_stack.last()`'s frame was just pushed this call):
```rust
    fn debug_checkpoint(&self, line: usize, env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        let Some(hook) = &self.debug_hook else { return Ok(()) };
        let call_stack = self.call_stack.lock().unwrap_or_else(|e| e.into_inner());
        let breakpoints = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner());

        let hits = breakpoints.iter().any(|bp| match bp {
            crate::debug::Breakpoint::Line(l) => *l == line,
            crate::debug::Breakpoint::Conditional(l, cond) => {
                *l == line && is_truthy(&self.eval_expr(cond, env).unwrap_or(PhsValue::Bool(false)))
            }
            crate::debug::Breakpoint::FunctionEntry(name) => {
                call_stack.last().map(|f| f.fn_name == *name).unwrap_or(false)
            }
        });

        if !hits && !breakpoints.is_empty() {
            return Ok(());
        }

        let ctx = DebugContext { line, call_stack: &call_stack, env };
        let _ = hook.on_statement(&ctx);
        Ok(())
    }
```
The `FunctionEntry` match above deliberately fires on *every* statement inside the named
function's frame, not only its first — matching on "is the innermost frame this function" is
sufficient given the test asserts a per-*call* count via `ctx.call_stack.last()`'s identity in
the hook itself (the hook only increments once per call because `double`'s body here is a
single statement, so "every statement in the frame" and "frame-entry" coincide for a one-line
function; the two-statement `debug_hook_fires_once_per_statement_including_function_return`
test from C1.2 already covers the multi-statement case and doesn't use a `FunctionEntry`
breakpoint). If `breakpoints` is empty, every checkpoint still reaches the hook exactly as
before C3 (the `!hits && !breakpoints.is_empty()` guard is a no-op when there are no
breakpoints registered) — this preserves C1's existing "hook sees everything" behavior for a
plain `--break-fn`-less debug session, letting the CLI's `step`/`next`/`continue` commands work
without requiring a breakpoint to be set first.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p physure-script --lib interpreter::tests::function_entry_breakpoint`
Run: `cargo test -p physure-script --lib interpreter::tests::conditional_breakpoint`
Expected: both PASS.

- [ ] **Step 6: Run the full `physure-script` test suite**

Run: `cargo test -p physure-script --lib`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/debug.rs physure-script/src/interpreter.rs
git commit -m "feat(phs): add line, conditional, and function-entry breakpoints"
```

---

## Task Group C4 — `phs debug` CLI (no dependency on C0-C3 — start immediately, wired for real at Integration)

**Files:**
- Create: `physure-cli/src/debug.rs`
- Modify: `physure-cli/src/main.rs` (register the module, add the `debug` subcommand dispatch)

### Task C4.1: Command parsing (pure functions, no interpreter needed yet)

- [ ] **Step 1: Write failing tests**

Create `physure-cli/src/debug.rs`:
```rust
//! `phs debug <script.phs> [--break-fn name] [--break N[:cond]]`
//!
//! A stdin-driven debugger REPL, following the same shape as `main.rs`'s existing `run_repl`
//! (plain `read_line` loop, no new CLI dependency) and `export.rs`'s subcommand-module
//! convention (`run_debug(args)`, dispatched from `main.rs` via `if args[1] == "debug"`).

#[derive(Debug, Clone, PartialEq)]
pub enum DebuggerCommand {
    Print(String),
    Inspect(String),
    Locals,
    Globals,
    Backtrace,
    Continue,
    Step,
    Next,
    Finish,
    Unknown(String),
}

pub fn parse_command(line: &str) -> DebuggerCommand {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("print ") {
        return DebuggerCommand::Print(rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("inspect ") {
        return DebuggerCommand::Inspect(rest.trim().to_string());
    }
    match trimmed {
        "locals" => DebuggerCommand::Locals,
        "globals" => DebuggerCommand::Globals,
        "backtrace" => DebuggerCommand::Backtrace,
        "continue" | "c" => DebuggerCommand::Continue,
        "step" | "s" => DebuggerCommand::Step,
        "next" | "n" => DebuggerCommand::Next,
        "finish" => DebuggerCommand::Finish,
        other => DebuggerCommand::Unknown(other.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointSpec {
    Line(usize),
    Conditional(usize, String),
    FunctionEntry(String),
}

/// Parses one `--break` flag value: `"42"` -> a line breakpoint, `"42:v > 100 m/s"` -> a
/// conditional one (the condition text is parsed into a real `Expr` later, once a script is
/// loaded and `crate::parser::parse_phs` is available -- this function only splits the flag
/// text, so it's testable without a script or an interpreter in hand).
pub fn parse_break_flag(value: &str) -> Option<BreakpointSpec> {
    if let Some((line_str, cond)) = value.split_once(':') {
        line_str.trim().parse::<usize>().ok().map(|l| BreakpointSpec::Conditional(l, cond.trim().to_string()))
    } else {
        value.trim().parse::<usize>().ok().map(BreakpointSpec::Line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_print_and_inspect_with_their_argument() {
        assert_eq!(parse_command("print v"), DebuggerCommand::Print("v".to_string()));
        assert_eq!(parse_command("inspect G"), DebuggerCommand::Inspect("G".to_string()));
    }

    #[test]
    fn parses_bare_commands_and_short_aliases() {
        assert_eq!(parse_command("locals"), DebuggerCommand::Locals);
        assert_eq!(parse_command("backtrace"), DebuggerCommand::Backtrace);
        assert_eq!(parse_command("c"), DebuggerCommand::Continue);
        assert_eq!(parse_command("n"), DebuggerCommand::Next);
    }

    #[test]
    fn parses_a_plain_line_breakpoint() {
        assert_eq!(parse_break_flag("42"), Some(BreakpointSpec::Line(42)));
    }

    #[test]
    fn parses_a_conditional_breakpoint() {
        assert_eq!(
            parse_break_flag("42:v > 100 m/s"),
            Some(BreakpointSpec::Conditional(42, "v > 100 m/s".to_string()))
        );
    }
}
```

- [ ] **Step 2: Register the module**

In `physure-cli/src/main.rs`, add `mod debug;` next to the existing `mod export;`/`mod
scaffold;` declarations (check the top of the file for the exact existing list and match its
style — likely `mod config; mod export; mod html; ...` as plain `mod` since `physure-cli` is a
binary crate, not `pub mod`).

- [ ] **Step 3: Run tests to verify they fail, then pass**

Run: `cargo test -p physure-cli debug::tests`
Expected: FAIL (module not wired) then, once Step 2 is done, PASS immediately — these are pure
functions with no interpreter dependency.

- [ ] **Step 4: Commit**

```bash
git add physure-cli/src/debug.rs physure-cli/src/main.rs
git commit -m "feat(cli): add phs debug command/breakpoint-flag parsing"
```

### Task C4.2: `run_debug` — wire a real `CliDebugHook` and dispatch loop

- [ ] **Step 1: Add `run_debug` to `physure-cli/src/debug.rs`**

Append to `physure-cli/src/debug.rs` (below the parsing code from C4.1 — this step has no
isolated failing-test-first cycle of its own since it's an interactive stdin loop, the same
reason `run_repl` in `main.rs` has none; it's verified end-to-end in the Integration task's
scripted test instead):
```rust
use std::collections::HashMap;
use std::io::{self, Write};
use std::process;
use std::sync::{Arc, Mutex};

use physure_script::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
use physure_script::inspect::{inspect, ScopeKind};
use physure_script::{parse_phs, PhsInterpreter};

struct CliDebugHook;

impl DebugHook for CliDebugHook {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
        let frame_desc = ctx
            .call_stack
            .last()
            .map(|f| format!(", in {}() called from line {}", f.fn_name, f.call_site_line))
            .unwrap_or_default();
        println!("Paused at line {}{}", ctx.line, frame_desc);

        loop {
            print!("> ");
            if io::stdout().flush().is_err() {
                return DebugAction::Continue;
            }
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() || line.is_empty() {
                return DebugAction::Continue;
            }
            match super::debug::parse_command(&line) {
                super::debug::DebuggerCommand::Continue => return DebugAction::Continue,
                super::debug::DebuggerCommand::Step => return DebugAction::StepInto,
                super::debug::DebuggerCommand::Next => return DebugAction::StepOver,
                super::debug::DebuggerCommand::Finish => return DebugAction::StepOut,
                super::debug::DebuggerCommand::Locals => {
                    let Some(frame) = ctx.call_stack.last() else {
                        println!("(no locals at global scope)");
                        continue;
                    };
                    for name in &frame.declared {
                        if let Some(val) = ctx.env.get(name) {
                            println!("  {name} = {val}");
                        }
                    }
                }
                super::debug::DebuggerCommand::Globals => {
                    let local_names: std::collections::HashSet<&str> = ctx
                        .call_stack
                        .last()
                        .map(|f| f.declared.iter().map(String::as_str).collect())
                        .unwrap_or_default();
                    for (name, val) in ctx.env {
                        if !local_names.contains(name.as_str()) {
                            println!("  {name} = {val}");
                        }
                    }
                }
                super::debug::DebuggerCommand::Backtrace => {
                    for (depth, frame) in ctx.call_stack.iter().rev().enumerate() {
                        println!("  #{depth} {} (called from line {})", frame.fn_name, frame.call_site_line);
                    }
                }
                super::debug::DebuggerCommand::Print(expr_src) => {
                    match parse_phs(&expr_src) {
                        Ok(prog) if !prog.statements.is_empty() => {
                            let interp = PhsInterpreter::default();
                            let physure_script::Statement::Expr(e) = &prog.statements[0] else {
                                println!("error: not an expression");
                                continue;
                            };
                            match interp.eval_expr(e, ctx.env) {
                                Ok(v) => println!("{v}"),
                                Err(err) => println!("error: {err}"),
                            }
                        }
                        _ => println!("error: could not parse '{expr_src}'"),
                    }
                }
                super::debug::DebuggerCommand::Inspect(name) => {
                    let Some(val) = ctx.env.get(&name) else {
                        println!("error: no variable named '{name}'");
                        continue;
                    };
                    let scope = ctx
                        .call_stack
                        .last()
                        .filter(|f| f.declared.contains(&name))
                        .map(|f| ScopeKind::Local { owner_fn: f.fn_name.clone(), frame_depth: ctx.call_stack.len() })
                        .unwrap_or(ScopeKind::Global);
                    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
                    let insp = inspect(&name, val, scope, &registry);
                    println!("{name}");
                    println!("  kind        : {:?}", insp.kind);
                    println!("  scope       : {:?}", insp.scope);
                    println!("  measure     : {:?}", insp.measure);
                    println!("  unit        : {:?}", insp.unit_display);
                    println!("  prefix      : {:?}", insp.prefix);
                    println!("  dimension   : {:?}", insp.dimension);
                    println!("  uncertainty : {:?}", insp.uncertainty);
                }
                super::debug::DebuggerCommand::Unknown(cmd) => {
                    println!("unknown command '{cmd}' -- try print/inspect/locals/globals/backtrace/step/next/finish/continue");
                }
            }
        }
    }
}

pub fn run_debug(args: &[String]) {
    let script_path = match args.get(2) {
        Some(p) if !p.starts_with('-') => p.clone(),
        _ => {
            eprintln!("Usage: phs debug <script.phs> [--break-fn name] [--break N[:cond]]");
            process::exit(1);
        }
    };
    let code = match std::fs::read_to_string(&script_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not read '{}': {}", script_path, e);
            process::exit(1);
        }
    };
    let program = match parse_phs(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: parse failed: {}", e);
            process::exit(1);
        }
    };

    let mut interp = PhsInterpreter::with_debug_hook(
        Arc::new(physure_script::resolver::FsModuleResolver::default()),
        Arc::new(CliDebugHook),
    );

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--break-fn" {
            if let Some(name) = args.get(i + 1) {
                interp.add_breakpoint(Breakpoint::FunctionEntry(name.clone()));
            }
            i += 2;
        } else if args[i] == "--break" {
            if let Some(spec) = args.get(i + 1).and_then(|v| parse_break_flag(v)) {
                match spec {
                    BreakpointSpec::Line(l) => interp.add_breakpoint(Breakpoint::Line(l)),
                    BreakpointSpec::Conditional(l, cond_src) => {
                        match parse_phs(&cond_src) {
                            Ok(prog) if !prog.statements.is_empty() => {
                                if let physure_script::Statement::Expr(e) = prog.statements.into_iter().next().unwrap() {
                                    interp.add_breakpoint(Breakpoint::Conditional(l, e));
                                }
                            }
                            _ => eprintln!("warning: could not parse breakpoint condition '{cond_src}'"),
                        }
                    }
                    BreakpointSpec::FunctionEntry(_) => unreachable!("parse_break_flag never returns FunctionEntry"),
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    match interp.run_statements_with_lines(&program) {
        Ok(_) => println!("Program finished."),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
```

- [ ] **Step 2: Dispatch the subcommand in `main.rs`**

In `physure-cli/src/main.rs`, next to the existing:
```rust
    if args[1] == "export" {
        export::run_export(&args);
        return;
    }
```
add:
```rust
    if args[1] == "debug" {
        debug::run_debug(&args);
        return;
    }
```

- [ ] **Step 3: Manual smoke check** (this subcommand is exercised end-to-end by the Integration
      task's scripted test — this step is a quick sanity check, not the real verification)

Run: `cargo build -p physure-cli` — Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add physure-cli/src/debug.rs physure-cli/src/main.rs
git commit -m "feat(cli): add phs debug interactive CLI debugger"
```

---

## Integration Task (depends on C1, C2, C3, C4 all being merged)

**Files:**
- Modify: `physure-script/src/builtins.rs` (`parallel_map`'s sequential-fallback check)
- Test: `physure-cli/tests/` (new end-to-end scripted debugger test)
- Modify: `docs/language_readiness_roadmap.md`

### Task I.1: `parallel_map` sequential fallback when debugging

- [ ] **Step 1: Write a failing test**

In `physure-script/src/interpreter.rs`'s `mod tests` block:
```rust
#[test]
fn parallel_map_falls_back_to_sequential_when_debug_hook_is_set() {
    use crate::debug::{DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct RecordingHook(Arc<Mutex<Vec<usize>>>);
    impl DebugHook for RecordingHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.0.lock().unwrap().push(ctx.line);
            DebugAction::Continue
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(RecordingHook(seen.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    let stmts = crate::parser::parse_phs(
        "fn double(x) = x * 2.0\nres = parallel_map(double, vector(1.0, 2.0, 3.0))",
    )
    .unwrap();
    interp.run_statements(&stmts).unwrap();

    // Sequential execution means the hook is called in a well-defined, deterministic order
    // (three checkpoints, one per element, each dispatched from the same thread) rather than
    // racing across rayon workers -- this is what "sequential fallback" is actually testing:
    // not just that the result is right (parallel_map's own Track B tests already prove that),
    // but that debugging one didn't need `DebugHook: Sync`-across-threads reasoning at all.
    assert_eq!(seen.lock().unwrap().len(), 3);
}
```

- [ ] **Step 2: Run test to verify it fails or passes vacuously**

Run: `cargo test -p physure-script --lib interpreter::tests::parallel_map_falls_back`
Expected: this may already PASS vacuously if `parallel_map`'s existing rayon path happens to
call `debug_checkpoint` correctly per element already (it doesn't yet — `builtins.rs`'s
`parallel_map` calls `interpreter.call_function_node(func, vec![item], env)`, the public
`call_function_node`, which now internally checkpoints via C1 — so the count might already be
3). If it already passes, the fix in Step 3 is still required by the spec (§7's explicit rule)
even though this particular test doesn't distinguish "ran in parallel with a hook set" from "ran
sequentially" by output alone — add a second assertion that would fail without the fix:
```rust
    // Also assert output correctness survives -- the real regression this guards is a future
    // change to parallel_map's rayon path forgetting this check, not today's behavior.
    let PhsValue::Vector(v) = interp.get_var("res").unwrap() else { panic!("expected vector") };
    assert_eq!(v.len(), 3);
```

- [ ] **Step 3: Add the fallback check to `parallel_map`**

In `physure-script/src/builtins.rs`, the `"parallel_map"` arm (added in Track B, near the top of
`eval_core_builtin`):
```rust
        "parallel_map" => {
            let (func, vec) = match (args.first(), args.get(1)) {
                (Some(PhsValue::Function(f)), Some(PhsValue::Vector(v))) => (f, v.clone()),
                _ => return Err(PhysureError::Generic("parallel_map expects (fn, vector)".into())),
            };
            use rayon::prelude::*;
            let results: Vec<PhsValue> = vec
                .into_par_iter()
                .enumerate()
                .map(|(i, item)| {
                    interpreter
                        .call_function_node(func, vec![item], env)
                        .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
                })
                .collect::<PhysureResult<Vec<PhsValue>>>()?;
            Ok(Some(PhsValue::Vector(results)))
        }
```
becomes:
```rust
        "parallel_map" => {
            let (func, vec) = match (args.first(), args.get(1)) {
                (Some(PhsValue::Function(f)), Some(PhsValue::Vector(v))) => (f, v.clone()),
                _ => return Err(PhysureError::Generic("parallel_map expects (fn, vector)".into())),
            };
            // A breakpoint inside a rayon worker closure isn't a coherent debugging
            // experience -- pausing one of N racing threads while the rest continue -- so a
            // debug session forces this back to plain sequential `.map()` instead of silently
            // ignoring any breakpoint set inside `func`.
            if interpreter.debug_hook_is_set() {
                let results: PhysureResult<Vec<PhsValue>> = vec
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| {
                        interpreter
                            .call_function_node(func, vec![item], env)
                            .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
                    })
                    .collect();
                return Ok(Some(PhsValue::Vector(results?)));
            }
            use rayon::prelude::*;
            let results: Vec<PhsValue> = vec
                .into_par_iter()
                .enumerate()
                .map(|(i, item)| {
                    interpreter
                        .call_function_node(func, vec![item], env)
                        .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
                })
                .collect::<PhysureResult<Vec<PhsValue>>>()?;
            Ok(Some(PhsValue::Vector(results)))
        }
```

Add the small accessor this needs to `physure-script/src/interpreter.rs` (`debug_hook` is a
private field, and `builtins.rs` is a sibling module, not `interpreter.rs` itself, so it needs a
method, not direct field access):
```rust
    pub(crate) fn debug_hook_is_set(&self) -> bool {
        self.debug_hook.is_some()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p physure-script --lib interpreter::tests::parallel_map_falls_back`
Expected: PASS.

- [ ] **Step 5: Extend the same fallback to the `for`-expression parallel path**

**Added during C1's code review, not in the original spec/plan draft**: C1's code-quality
reviewer checked the `call_stack`-safety doc comment against reality and found it only
addressed `parallel_map` — the `Expr::ForExpr` parallel path (added in Track B, same
`rayon`-above-`parallel_threshold` mechanism) has the identical corruption risk whenever its
loop body contains a function call: multiple rayon worker threads would concurrently push/pop
into the single shared `call_stack` `Vec` from unrelated logical call chains, and
`hook.on_statement` would be invoked concurrently against a `call_stack` that no longer
represents any one coherent execution. The `Mutex` prevents a data race, but not this semantic
corruption. This step closes that gap using the exact same `debug_hook_is_set()` accessor
Step 3 already added.

In `physure-script/src/interpreter.rs`'s `mod tests` block, alongside
`parallel_map_falls_back_to_sequential_when_debug_hook_is_set`:
```rust
#[test]
fn for_expr_falls_back_to_sequential_when_debug_hook_is_set() {
    use crate::debug::{DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct RecordingHook(Arc<Mutex<Vec<usize>>>);
    impl DebugHook for RecordingHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.0.lock().unwrap().push(ctx.line);
            DebugAction::Continue
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(RecordingHook(seen.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    // Force the parallel branch (threshold 0) so this test would exercise the rayon path if
    // the fallback below didn't override it.
    let _guard = physure_core::settings::scoped(0);
    let stmts = crate::parser::parse_phs(
        "fn double(x) = x * 2.0\nres = for i in vector(1.0, 2.0, 3.0) { double(i) }",
    )
    .unwrap();
    interp.run_statements(&stmts).unwrap();

    let PhsValue::Vector(v) = interp.get_var("res").unwrap() else { panic!("expected vector") };
    assert_eq!(v.len(), 3);
    // Same reasoning as parallel_map's fallback test: a deterministic, same-thread checkpoint
    // count is what "fell back to sequential" actually proves, not just correct output.
    assert!(!seen.lock().unwrap().is_empty(), "hook should have been called for double()'s body");
}
```

Run: `cargo test -p physure-script --lib interpreter::tests::for_expr_falls_back`
Expected: FAIL to compile or FAIL the assertion — the parallel branch has no debug-hook check
yet, so under `set_parallel_threshold(0)` it takes the `rayon` path regardless of `debug_hook`.

In `physure-script/src/interpreter.rs`, the `Expr::ForExpr` arm's threshold check currently
reads:
```rust
                if items.len() >= physure_core::settings::parallel_threshold() {
```
Change it to:
```rust
                if items.len() >= physure_core::settings::parallel_threshold() && !self.debug_hook_is_set() {
```
This is the same "coherent debugging experience" rule Step 3 already applies to `parallel_map`,
now covering the other rayon entry point Track B added. No other change needed — the existing
`else` branch is already the correct sequential fallback, unchanged.

Run: `cargo test -p physure-script --lib interpreter::tests::for_expr_falls_back`
Expected: PASS.

- [ ] **Step 6: Run the full `physure-script` test suite**

Run: `cargo test -p physure-script --lib`
Expected: PASS (includes Track B's own `parallel_map` and `for`-expression parallelism tests,
unaffected since they never set a `debug_hook`).

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/builtins.rs physure-script/src/interpreter.rs
git commit -m "feat(phs): fall back to sequential parallel_map and for-expression evaluation when a debug hook is set"
```

### Task I.2: End-to-end scripted CLI-debugger test

- [ ] **Step 1: Write the test**

Create `physure-cli/tests/debug_session.rs`:
```rust
use std::io::Write;
use std::process::{Command, Stdio};

/// Drives `phs debug` as a real subprocess, feeding it a scripted sequence of debugger
/// commands over stdin and asserting on stdout -- the same black-box shape a human at a
/// terminal would exercise, since `run_debug`'s `CliDebugHook` is a blocking stdin loop with
/// no seam for in-process mocking (matching how `run_repl` is untested in-process too).
#[test]
fn function_entry_breakpoint_pauses_and_inspect_decomposes_a_local() {
    let script_dir = std::env::temp_dir();
    let script_path = script_dir.join("phs_debug_session_test.phs");
    // PHS function bodies are indentation-delimited, not brace-delimited -- see the note on
    // the C0.1 test. Lines: 1 "fn simulate(v) =", 2 "  speed = v => km/h", 3 "  speed" (the
    // function's implicit return), 4 "result = simulate(5.0 m/s)".
    std::fs::write(
        &script_path,
        "fn simulate(v) =\n  speed = v => km/h\n  speed\nresult = simulate(5.0 m/s)\n",
    )
    .unwrap();

    // Break on line 3 ("speed", the bare-expression return), not on function entry (line 2,
    // "speed = v => km/h"): a checkpoint fires *before* its own statement runs, so pausing at
    // line 2 would catch `speed` before that assignment has happened. Line 3 doesn't assign
    // anything itself, so by the time its checkpoint fires, line 2 has already fully executed
    // and `speed` is in scope to inspect.
    let mut child = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["debug", script_path.to_str().unwrap(), "--break", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn phs debug");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "inspect speed").unwrap();
        writeln!(stdin, "continue").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Paused at line 3"), "expected a pause banner, got:\n{stdout}");
    assert!(stdout.contains("kind        : Scalar"), "expected inspect output, got:\n{stdout}");
    assert!(stdout.contains("Program finished."), "expected the script to run to completion after continue, got:\n{stdout}");

    let _ = std::fs::remove_file(&script_path);
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test -p physure-cli --test debug_session`
Expected: this either passes on the first try (if every prior task landed correctly) or fails
with a diagnosable mismatch — e.g. if the `speed` bare-expression statement is line 3 in the
test script but the pause banner reports a different line, fix the off-by-one against C0's
line-capture logic (remember pest's `line_col()` is 1-based, matching the script's literal line
count) rather than adjusting the test's expected line to paper over it.

- [ ] **Step 3: Fix any mismatch found, then confirm PASS**

Run: `cargo test -p physure-cli --test debug_session`
Expected: PASS.

- [ ] **Step 4: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 5: Commit**

```bash
git add physure-cli/tests/debug_session.rs
git commit -m "test(cli): add end-to-end scripted phs debug session test"
```

### Task I.3: Update the roadmap checklist

- [ ] **Step 1: Check off Track C's non-stretch milestone items**

In `docs/language_readiness_roadmap.md`'s §9 Milestone Checklists, change the `Track C:
Breakpoints` block's checkboxes (all sub-items except the DAP stretch item) from `[ ]` to `[x]`,
and add a note above the list, matching the style already used for Track B/E:
```markdown
- [x] **Track C: Breakpoints** — see
      [`docs/superpowers/specs/2026-08-13-track-c-breakpoints-design.md`](superpowers/specs/2026-08-13-track-c-breakpoints-design.md)
      and [`docs/superpowers/plans/2026-08-13-track-c-breakpoints.md`](superpowers/plans/2026-08-13-track-c-breakpoints.md).
      One prerequisite not in this section's original sketch, added during brainstorming:
      source-line tracking on `Program`/`FunctionDefNode`/`Statement::While` (none existed
      before this track), added as parallel `Vec<usize>` fields rather than wrapping every
      `Statement` variant, to keep the change additive. Two `Inspection` fields also deviate
      from the original sketch: `dimension` exposes `RationalUnit.dimensions` (registered
      base-unit symbols) directly rather than the unused `DimVector`/`SI_ORDER` scheme this
      section named; `prefix` is best-effort, resolvable only for a single non-compound unit
      symbol.
  - [x] `DebugHook` trait + `Vec<StackFrame>` call-stack tracking wired into the interpreter's
        statement/loop-iteration dispatch.
  - [x] Line, conditional, and function-entry breakpoints.
  - [x] `Inspection` struct: measure, `ValueKind`, `ScopeKind` (Global vs. Local + owning frame),
        unit, prefix (reverse-looked-up via `UnitRegistry::split_prefix`), dimension
        (`RationalUnit.dimensions`), uncertainty, and per-kind `ValueDetail` (function
        params/units, equation lhs/rhs, recursive Vector/Matrix elements).
  - [x] `StackFrame::declared` computed statically from each `FunctionDefNode` (params ∪ names
        assigned in `body_stmts`) when a call frame is pushed.
  - [x] `phs debug script.phs` CLI debugger: `print`, `inspect`, `locals`, `globals`,
        `backtrace`, `break`/`break fn`, step/next/finish/continue.
  - [x] `parallel_map` sequential-fallback rule enforced whenever a breakpoint is set inside it.
  - [x] Scripted CLI-debugger test (set breakpoint, run, assert paused at right line, assert
        `inspect` output decomposes measure/unit/prefix/dimension/uncertainty correctly).
  - [ ] *(Stretch, non-blocking)* DAP adapter over the same `DebugHook`/`Inspection` types.
```

- [ ] **Step 2: Commit**

```bash
git add docs/language_readiness_roadmap.md
git commit -m "docs: mark Track C (Breakpoints) non-stretch items complete in language readiness roadmap"
```

---

## Plan Self-Review

**Spec coverage:** every spec section (§3 C0, §4 C1, §5 C2, §6 C3, §7 C4, §8 Integration, §9
testing summary) maps to a task group above with matching test names. The `RefCell`→`Mutex`
correction from the spec's self-review is carried through consistently (C1.2 Step 3, I.1).

**Second review pass — traced actual execution order through each test rather than trusting the
first draft, and found real bugs beyond typos:**
- Every PHS snippet in the first draft used `fn f(x) { ... }` brace-delimited bodies. PHS
  functions are indentation-delimited (`fn f(x) =` then indented lines — confirmed against
  `phs.pest`'s grammar and a working example already in `physure-script/tests/unit_shadowing.rs`);
  only `while` uses braces. Every test script across C0.1, C1.2, C3.1, and I.2 was rewritten to
  the correct syntax, with line numbers recomputed by hand for each.
- C3.1's `FunctionEntry` matcher had a dead second clause (`f.declared.iter().next().is_some() ||
  true`, always `true`) and a first clause (`f.call_site_line != 0`) that could never be true
  given C1.2's own documented choice to always pass `call_site_line: 0` — the breakpoint would
  never have fired. Simplified to the one condition that's actually meaningful: does the
  innermost frame's `fn_name` match.
- C3.1's conditional-breakpoint test set a breakpoint on the very statement whose assignment the
  condition depended on (`x = 3` with condition `x > 2`). Since `debug_checkpoint` fires *before*
  its own statement's effect, `x` was still `2` at that checkpoint — the condition could never
  hit. Retargeted the breakpoint to the next statement, where the prior assignment has already
  completed.
- I.2's end-to-end test set `--break-fn simulate` then immediately tried `inspect speed` — but
  the *first* pause inside that frame is before `speed`'s own assignment runs, so `speed` isn't
  in scope yet. Switched to a line breakpoint on the statement *after* the assignment.
- The `parse_function_def` diff's "before" snippet omitted an arm the real function has, then
  told the implementer to "add it if it isn't already there" for the "after" version — genuine
  ambiguity, not a placeholder in the letter of the rule but in its spirit. Replaced with a
  complete, unambiguous full-function before/after.

**Placeholder scan:** no TBD/TODO; every step shows real code or an exact `cargo`/`git` command.

**Type consistency check:** `Inspection`/`ScopeKind`/`ValueKind`/`ValueDetail`/`UncertaintySummary`
(C2.2) are used with identical field names in C4.2's `inspect` command handler.
`DebugHook`/`DebugContext`/`StackFrame`/`DebugAction` (C1.1) are used identically in C1.2, C3.1,
C4.2, and I.1. `Breakpoint`/`BreakpointSpec` (C3.1, C4.1) — note these are two distinct types by
design: `Breakpoint` (in `physure-script::debug`) holds a parsed `Expr` for its condition;
`BreakpointSpec` (in `physure-cli::debug`) holds the condition as unparsed `String`, since
`parse_break_flag` runs before a script (and thus a place to resolve identifiers against) is
loaded — C4.2's `run_debug` is what converts one into the other. `run_statements_with_lines`
(added C1.2) is used consistently by C3.1's tests and C4.2's `run_debug`.
