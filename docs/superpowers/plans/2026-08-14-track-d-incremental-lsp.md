# Track D — Incremental LSP Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `physure-lsp`'s full-file reparse-and-re-execute on every keystroke with
per-statement, dependency-aware incremental re-evaluation, so an edit to one statement doesn't
re-run statements nothing it touches depends on.

**Architecture:** A new `physure-lsp/src/incremental.rs` module owns a persisted `DocState`
(interpreter + last-parsed statements + cached diagnostics), a pure AST-level read/write analysis,
a common-prefix/suffix statement diff, and a dirty-set sweep that decides which statements need
re-running. `main.rs`'s `on_change` calls into it instead of today's `analyze()`, which this plan
deletes.

**Tech Stack:** Rust, `physure-script` (parser/interpreter/AST), `tower-lsp` (`lsp_types::Diagnostic`).
No new dependencies.

**Design spec:** [`docs/superpowers/specs/2026-08-14-track-d-incremental-lsp-design.md`](../specs/2026-08-14-track-d-incremental-lsp-design.md)
— read it before starting; every task below cites the section it implements.

---

## Task 1: Expose `collect_declared` for cross-crate reuse

**Files:**
- Modify: `physure-script/src/debug.rs:39`

Track C already built the exact "params ∪ every name assigned anywhere in this body" computation
Track D needs (spec §4.1/§4.4). It's currently private to `physure-script`'s `debug` module.
Making it `pub` avoids re-deriving the same logic in `physure-lsp`.

- [ ] **Step 1: Make the function public**

In `physure-script/src/debug.rs`, change:

```rust
fn collect_declared(stmt: &crate::ast::Statement, declared: &mut HashSet<String>) {
```

to:

```rust
pub fn collect_declared(stmt: &crate::ast::Statement, declared: &mut HashSet<String>) {
```

- [ ] **Step 2: Confirm the workspace still builds**

Run: `cargo build -p physure-script`
Expected: builds cleanly (visibility widening never breaks existing callers).

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/debug.rs
git commit -m "feat(script): expose collect_declared for reuse outside the debug module"
```

---

## Task 2: `DocState` skeleton and module wiring

**Files:**
- Create: `physure-lsp/src/incremental.rs`
- Modify: `physure-lsp/src/main.rs:1` (add `mod incremental;`)

**Confirmed API shapes used below** (already verified against the current source):
- `physure_script::parser::parse_phs_with_lines(text: &str) -> PhysureResult<Vec<(usize, Statement)>>`
  ([parser/mod.rs:37](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/parser/mod.rs#L37))
- `physure_script::interpreter::PhsInterpreter` — `#[derive(Clone)]`, `Default`, `pub env: HashMap<String, PhsValue>`,
  `pub fn run_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue>`

- [ ] **Step 1: Create the module with `DocState`**

Create `physure-lsp/src/incremental.rs`:

```rust
use physure_script::ast::Statement;
use physure_script::interpreter::PhsInterpreter;
use tower_lsp::lsp_types::Diagnostic;

/// Everything Track D persists for one open document across edits: the last successfully
/// parsed statement list (diffed against on the next change), the interpreter whose `env`
/// carries forward instead of being rebuilt from scratch, and each statement's last-known
/// diagnostic so an unchanged statement doesn't need to re-run to keep reporting it.
pub struct DocState {
    pub statements: Vec<Statement>,
    pub lines: Vec<usize>,
    pub interp: PhsInterpreter,
    pub diagnostics: Vec<Option<Diagnostic>>,
}

impl DocState {
    pub fn empty() -> Self {
        DocState {
            statements: Vec::new(),
            lines: Vec::new(),
            interp: PhsInterpreter::default(),
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_state_has_no_statements_or_diagnostics() {
        let state = DocState::empty();
        assert!(state.statements.is_empty());
        assert!(state.diagnostics.is_empty());
    }
}
```

- [ ] **Step 2: Register the module in `main.rs`**

In `physure-lsp/src/main.rs`, add as the first line:

```rust
mod incremental;
```

- [ ] **Step 3: Run the new test**

Run: `cargo test -p physure-lsp empty_doc_state_has_no_statements_or_diagnostics`
Expected: PASS (1 test)

- [ ] **Step 4: Commit**

```bash
git add physure-lsp/src/incremental.rs physure-lsp/src/main.rs
git commit -m "feat(lsp): add DocState skeleton for Track D incremental evaluation"
```

---

## Task 3: Read/write analysis (spec §3 step 3-4, §4.1, §4.3, §4.4, §4.5)

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

This is the pure-AST pass with no execution: for each top-level statement, what name(s) does it
write into `env`, and what names does its full (recursive) expression tree read. Implements the
`FunctionCall.name`-is-a-read fix (§4.1), `while`'s unconditional writes (§4.4), `where`/`let`
local scoping (§4.3), and template-string `{expr}` reads (§4.5) in one pass.

- [ ] **Step 1: Write the failing tests**

Add to `physure-lsp/src/incremental.rs`, inside the existing `#[cfg(test)] mod tests` block:

```rust
    use physure_script::parser::parse_phs;

    fn stmts(src: &str) -> Vec<Statement> {
        parse_phs(src).unwrap().statements
    }

    #[test]
    fn assignment_writes_its_name_and_reads_its_expression() {
        let s = stmts("y = x + 1");
        let deps = analyze_one(&s[0]);
        assert_eq!(deps.writes, vec!["y".to_string()]);
        assert!(deps.reads.contains("x"));
    }

    #[test]
    fn function_def_reads_a_global_used_only_in_its_body_and_writes_its_own_name() {
        // The body's `g` must show up as a read of the *FunctionDef* statement -- nothing
        // else in the script mentions `g` at the top level. Confirms the "expression tree
        // includes nested body statements" reading of the spec (§4.1).
        let s = stmts("fn compute(m) = m * g");
        let deps = analyze_one(&s[0]);
        assert_eq!(deps.writes, vec!["compute".to_string()]);
        assert!(deps.reads.contains("g"));
        assert!(!deps.reads.contains("m"), "param must not read as a global");
    }

    #[test]
    fn function_call_name_counts_as_a_read_not_just_its_args() {
        // Expr::FunctionCall.name is a bare String, not an Expr::Identifier -- a walker that
        // only visits Identifier nodes would miss this (§4.1).
        let s = stmts("result = compute(2.0)");
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("compute"));
    }

    #[test]
    fn while_writes_every_body_assigned_name_unconditionally() {
        // Verified against the real interpreter (spec §4.4) that while-body writes always
        // leak, regardless of whether the name existed before the loop -- no filtering here.
        let s = stmts("while i < 5 { i = i + 1\nbrand_new = 99 }");
        let deps = analyze_one(&s[0]);
        let mut writes = deps.writes.clone();
        writes.sort();
        assert_eq!(writes, vec!["brand_new".to_string(), "i".to_string()]);
        assert!(deps.reads.contains("i"), "cond and body must read the pre-loop i");
    }

    #[test]
    fn where_bound_name_does_not_read_as_an_outer_dependency() {
        // `expr where a = value` desugars to let(a, value, expr) -- `a` is local to the
        // `let`'s body argument only (§4.3).
        let s = stmts("y = a * 2 where a = 3 m");
        let deps = analyze_one(&s[0]);
        assert!(!deps.reads.contains("a"), "where-bound name must not read as a global");
    }

    #[test]
    fn where_value_expr_still_reads_normally() {
        let s = stmts("y = a * 2 where a = base_value");
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("base_value"));
    }

    #[test]
    fn template_string_interpolation_reads_the_interpolated_name() {
        // interpolate() parses {expr} as real PHS source and evaluates it against env --
        // skipping this would under-count reads for any statement using string
        // interpolation (§4.5).
        let s = stmts(r#"msg = "v is {v * 2}""#);
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("v"));
    }

    #[test]
    fn import_symbols_write_their_aliased_or_own_names() {
        let s = stmts("use solve as slv, deriv from calc");
        let deps = analyze_one(&s[0]);
        let mut writes = deps.writes.clone();
        writes.sort();
        assert_eq!(writes, vec!["deriv".to_string(), "slv".to_string()]);
    }
```

- [ ] **Step 2: Run the tests to confirm they fail to compile**

Run: `cargo test -p physure-lsp analyze_one -- --list`
Expected: fails to compile — `analyze_one`, `StmtDeps` not found.

- [ ] **Step 3: Implement the analysis**

Add to `physure-lsp/src/incremental.rs`, above the `#[cfg(test)]` block:

```rust
use physure_script::ast::{Expr, ImportSpecifier};
use physure_script::debug::collect_declared;
use std::collections::HashSet;

/// What one top-level statement writes into `env` when it runs successfully, and what names
/// its evaluation reads from `env`. Purely a static AST walk -- no execution.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StmtDeps {
    pub writes: Vec<String>,
    pub reads: HashSet<String>,
}

pub fn analyze(statements: &[Statement]) -> Vec<StmtDeps> {
    statements.iter().map(analyze_one).collect()
}

fn analyze_one(stmt: &Statement) -> StmtDeps {
    match stmt {
        Statement::Assignment(node) => {
            let mut reads = HashSet::new();
            collect_expr_reads(&node.value, &HashSet::new(), &mut reads);
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, &HashSet::new(), &mut reads);
                }
            }
            StmtDeps { writes: vec![node.name.clone()], reads }
        }
        Statement::FunctionDef(node) => {
            let mut declared: HashSet<String> = node.params.iter().cloned().collect();
            for s in &node.body_stmts {
                collect_declared(s, &mut declared);
            }
            let mut reads = HashSet::new();
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, &declared, &mut reads);
                }
            }
            for s in &node.body_stmts {
                collect_stmt_reads(s, &declared, &mut reads);
            }
            StmtDeps { writes: vec![node.name.clone()], reads }
        }
        Statement::Import(node) => {
            let writes = match &node.specifier {
                ImportSpecifier::Symbols(syms) => syms
                    .iter()
                    .map(|s| s.alias.clone().unwrap_or_else(|| s.name.clone()))
                    .collect(),
                // Wildcard/ModuleAlias: statically unknowable which names get bound -- under-
                // report rather than guess, same rule collect_declared already uses.
                ImportSpecifier::Wildcard | ImportSpecifier::ModuleAlias(_) => Vec::new(),
            };
            StmtDeps { writes, reads: HashSet::new() }
        }
        Statement::While { cond, body, .. } => {
            // §4.4: verified against the interpreter that a while body's writes always leak
            // to the top level, unconditionally -- no "did this name already exist" filter.
            let mut declared = HashSet::new();
            for s in body {
                collect_declared(s, &mut declared);
            }
            // Reads are NOT scoped by `declared` the way a function's params/locals are: a
            // while loop introduces no fresh child scope (§4.4 confirmed the body executes
            // directly against the shared top-level env, unlike a function call's separate
            // env clone), so even a name the body itself assigns must still count as reading
            // whatever came from before the loop -- most importantly `cond`'s own use of a
            // name the body reassigns (`i` in `while i < 5 { i = i + 1 }` reads the pre-loop
            // `i`; it is not something the body "locally" owns the way a param would be).
            let mut reads = HashSet::new();
            collect_expr_reads(cond, &HashSet::new(), &mut reads);
            for s in body {
                collect_stmt_reads(s, &HashSet::new(), &mut reads);
            }
            StmtDeps { writes: declared.into_iter().collect(), reads }
        }
        Statement::Expr(expr) | Statement::Return(expr) => {
            let mut reads = HashSet::new();
            collect_expr_reads(expr, &HashSet::new(), &mut reads);
            StmtDeps { writes: Vec::new(), reads }
        }
        Statement::GuardReturn { cond, value } => {
            let mut reads = HashSet::new();
            collect_expr_reads(cond, &HashSet::new(), &mut reads);
            collect_expr_reads(value, &HashSet::new(), &mut reads);
            StmtDeps { writes: Vec::new(), reads }
        }
        Statement::Export(_) => StmtDeps::default(),
    }
}

/// Reads inside a nested statement (a function body or while body), relative to the
/// enclosing statement's already-computed `locals` set. Deliberately does not add a nested
/// FunctionDef's own params as further-local: over-reporting a param name as a "read" is
/// harmless (nothing at the top level is ever named after a stranger's parameter, and if it
/// coincidentally is, the worst case is one extra safe re-run) -- ponytail: known ceiling,
/// ok to leave as-is; upgrade only if this over-reporting is ever observed to matter.
fn collect_stmt_reads(stmt: &Statement, locals: &HashSet<String>, out: &mut HashSet<String>) {
    match stmt {
        Statement::Assignment(node) => {
            collect_expr_reads(&node.value, locals, out);
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, locals, out);
                }
            }
        }
        Statement::FunctionDef(node) => {
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, locals, out);
                }
            }
            for s in &node.body_stmts {
                collect_stmt_reads(s, locals, out);
            }
        }
        Statement::Import(_) => {}
        Statement::While { cond, body, .. } => {
            collect_expr_reads(cond, locals, out);
            for s in body {
                collect_stmt_reads(s, locals, out);
            }
        }
        Statement::Expr(expr) | Statement::Return(expr) => collect_expr_reads(expr, locals, out),
        Statement::GuardReturn { cond, value } => {
            collect_expr_reads(cond, locals, out);
            collect_expr_reads(value, locals, out);
        }
        Statement::Export(_) => {}
    }
}

fn collect_expr_reads(expr: &Expr, locals: &HashSet<String>, out: &mut HashSet<String>) {
    match expr {
        Expr::Quantity(_) => {}
        Expr::Identifier(name) => {
            if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
                collect_template_reads(name, locals, out);
            } else if !locals.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::Str(text) => collect_template_reads(text, locals, out),
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_reads(left, locals, out);
            collect_expr_reads(right, locals, out);
        }
        Expr::FunctionCall { name, args, kwargs } => {
            // `where`/`let` desugars to this internal 3-arg pseudo-call: args[0] is a bare
            // name local to args[2] only, evaluated against the *outer* scope for args[1]
            // (§4.3). Not a real callable -- "let" itself must not be treated as a read.
            if name == "let" && args.len() == 3 {
                if let Expr::Identifier(bound_name) = &args[0] {
                    collect_expr_reads(&args[1], locals, out);
                    let mut inner = locals.clone();
                    inner.insert(bound_name.clone());
                    collect_expr_reads(&args[2], &inner, out);
                    return;
                }
            }
            // §4.1: the callee's name is a bare String field, not a nested Identifier --
            // resolution goes through env.get(name) exactly like an Identifier read does.
            if !locals.contains(name) {
                out.insert(name.clone());
            }
            for arg in args {
                collect_expr_reads(arg, locals, out);
            }
            for (_, arg) in kwargs {
                collect_expr_reads(arg, locals, out);
            }
        }
        Expr::ForExpr { var, iterable, body } => {
            collect_expr_reads(iterable, locals, out);
            let mut inner = locals.clone();
            inner.insert(var.clone());
            collect_expr_reads(body, &inner, out);
        }
    }
}

/// §4.5: scans `text` for every `{...}` span the same way `interpolate` does at eval time,
/// parses each as PHS source, and folds in whatever it reads. A span that fails to parse
/// contributes nothing, matching `interpolate`'s own behavior of leaving it untouched.
fn collect_template_reads(text: &str, locals: &HashSet<String>, out: &mut HashSet<String>) {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let expr_str = rest[..end].trim();
        rest = &rest[end + 1..];
        if let Ok(prog) = physure_script::parser::parse_phs(expr_str) {
            for stmt in &prog.statements {
                collect_stmt_reads(stmt, locals, out);
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-lsp incremental::tests`
Expected: PASS (all tests in the module, including Task 2's)

- [ ] **Step 5: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "feat(lsp): add per-statement read/write analysis for incremental evaluation"
```

---

## Task 4: Common-prefix/suffix statement diff (spec §3 step 2)

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn diff_bounds_finds_a_single_inserted_statement() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nx = 9\nb = 2\nc = 3");
        assert_eq!(diff_bounds(&old, &new), (1, 2));
    }

    #[test]
    fn diff_bounds_finds_a_single_edited_statement() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 5\nc = 3");
        assert_eq!(diff_bounds(&old, &new), (1, 1));
    }

    #[test]
    fn diff_bounds_of_identical_lists_covers_everything() {
        let old = stmts("a = 1\nb = 2");
        let new = stmts("a = 1\nb = 2");
        assert_eq!(diff_bounds(&old, &new), (2, 0));
    }

    #[test]
    fn diff_bounds_handles_empty_old_list() {
        let old: Vec<Statement> = Vec::new();
        let new = stmts("a = 1\nb = 2");
        assert_eq!(diff_bounds(&old, &new), (0, 0));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail to compile**

Run: `cargo test -p physure-lsp diff_bounds -- --list`
Expected: fails to compile — `diff_bounds` not found.

- [ ] **Step 3: Implement the diff**

Add to `physure-lsp/src/incremental.rs`, above the `#[cfg(test)]` block:

```rust
/// Longest common prefix length, then longest common suffix length (not overlapping the
/// prefix), by structural equality. Everything between the two matched regions -- on the old
/// side and the new side independently, since insertions/deletions change list length -- is
/// the changed span (spec §3 step 2). O(n), safe in the over-inclusive direction for any edit
/// this doesn't perfectly isolate (e.g. a whole statement moving from top to bottom).
fn diff_bounds(old: &[Statement], new: &[Statement]) -> (usize, usize) {
    let max_prefix = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < max_prefix && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = 0;
    while suffix < max_suffix && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix] {
        suffix += 1;
    }
    (prefix, suffix)
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-lsp incremental::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "feat(lsp): add common-prefix/suffix statement diff"
```

---

## Task 5: Dirty-set computation (spec §3 steps 3-4)

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

This is the core of the track: given the old and new statement lists, which new-list indices
must be re-run. Combines Task 3's read/write analysis with Task 4's diff. This single function is
directly the roadmap's mandated "execution-count" test target — a statement's index being in the
returned set *is* "this statement re-executes," so asserting the set is exact is a stronger,
simpler check than instrumenting an actual counter.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn editing_an_unread_statement_dirties_only_itself() {
        // Roadmap-mandated execution-count test: a statement whose result nothing downstream
        // reads, edited, re-executes alone.
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 99\nc = 3");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([1]));
    }

    #[test]
    fn editing_the_first_of_two_writes_reruns_only_correctly_scoped_dependents() {
        // Roadmap-mandated rebinding-correctness test: x written twice, y and z read
        // in between/after. Editing the *first* x must not touch the second write (x = 2,
        // a fresh write reading nothing) but must touch both the direct and transitive
        // readers of the first write.
        let old = stmts("x = 1\ny = x\nx = 2\nz = y");
        let new = stmts("x = 5\ny = x\nx = 2\nz = y");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1, 3]));
    }

    #[test]
    fn editing_a_global_only_a_called_functions_body_reads_propagates_to_the_call_site() {
        // §4.1: g never appears in the call site's own text -- only inside compute's body.
        let old = stmts("g = 9.8\nfn compute(m) = m * g\nresult = compute(2.0)");
        let new = stmts("g = 10.0\nfn compute(m) = m * g\nresult = compute(2.0)");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1, 2]));
    }

    #[test]
    fn unrelated_statements_stay_clean() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 2\nc = 3");
        let result = compute_dirty(&old, &new);
        assert!(result.dirty.is_empty());
    }

    #[test]
    fn a_deleted_write_dirties_its_former_readers() {
        // touched_names must come from *both* sides of the changed span: the write to x
        // disappears entirely (statement removed), so whatever used to read it needs
        // re-resolving even though nothing at its new position writes x anymore.
        let old = stmts("x = 1\ny = x");
        let new = stmts("y = x");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0]));
    }

    #[test]
    fn statement_after_a_while_loop_reruns_when_the_loops_body_write_changes() {
        // §4.4: c reads i, which the while loop (re)assigns -- edit the loop body, c must
        // re-run too, regardless of whether i existed before the loop.
        let old = stmts("i = 0\nwhile i < 5 { i = i + 1 }\nc = i");
        let new = stmts("i = 0\nwhile i < 5 { i = i + 2 }\nc = i");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([1, 2]));
    }

    #[test]
    fn editing_a_variable_used_only_in_a_template_string_dirties_that_statement() {
        // §4.5
        let old = stmts("v = 1\nmsg = \"v is {v * 2}\"");
        let new = stmts("v = 2\nmsg = \"v is {v * 2}\"");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1]));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail to compile**

Run: `cargo test -p physure-lsp compute_dirty -- --list`
Expected: fails to compile — `compute_dirty`, `DirtyAnalysis` not found.

- [ ] **Step 3: Implement the dirty-set sweep**

Add to `physure-lsp/src/incremental.rs`, above the `#[cfg(test)]` block:

```rust
use std::collections::HashMap;

/// Result of diffing an old statement list against a new one: which new-list indices need
/// re-running, the common-prefix length (statements before it can never be dirty by
/// construction -- everything they read resolves within the unchanged prefix), and every
/// name touched by the changed span on either side (needed to invalidate stale `env` entries
/// before re-running -- see `apply_change`).
pub struct DirtyAnalysis {
    pub dirty: HashSet<usize>,
    pub prefix: usize,
    pub touched_names: HashSet<String>,
}

pub fn compute_dirty(old: &[Statement], new: &[Statement]) -> DirtyAnalysis {
    let (prefix, suffix) = diff_bounds(old, new);
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid_end = new.len() - suffix;
    let new_mid = &new[prefix..new_mid_end];

    // Union of both sides: a write that's purely deleted (old side only) still needs every
    // downstream reader re-resolved, which an index-only check over the new list would miss.
    let mut touched_names: HashSet<String> = HashSet::new();
    for stmt in old_mid.iter().chain(new_mid.iter()) {
        touched_names.extend(analyze_one(stmt).writes);
    }

    let deps = analyze(new);
    let mut dirty = HashSet::new();
    let mut last_writer: HashMap<String, usize> = HashMap::new();

    for (i, d) in deps.iter().enumerate() {
        // Statements before `prefix` are unchanged content whose reads resolve entirely
        // within the equally-unchanged prefix -- never dirty, regardless of a same-named
        // write appearing later in the changed span.
        if i >= prefix {
            let in_changed_span = i < new_mid_end;
            let touches = d.reads.iter().any(|n| touched_names.contains(n));
            let depends_on_dirty = d
                .reads
                .iter()
                .any(|n| last_writer.get(n).map_or(false, |w| dirty.contains(w)));
            if in_changed_span || touches || depends_on_dirty {
                dirty.insert(i);
            }
        }
        for name in &d.writes {
            last_writer.insert(name.clone(), i);
        }
    }

    DirtyAnalysis { dirty, prefix, touched_names }
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-lsp incremental::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "feat(lsp): add dirty-set computation for incremental re-evaluation"
```

---

## Task 6: Diagnostic construction helpers, moved from `main.rs`

**Files:**
- Modify: `physure-lsp/src/incremental.rs`
- Modify: `physure-lsp/src/main.rs`

`main.rs` already has `extract_line_col_from_err`, `clean_error_message`, and the diagnostic
construction logic inline in `analyze()`. Task 7's `apply_change` needs the same logic, so this
task moves it into `incremental.rs` as reusable functions rather than duplicating it — `main.rs`'s
existing test for it moves along with the code it tests.

- [ ] **Step 1: Move the two helper functions and their test**

In `physure-lsp/src/main.rs`, delete these two functions in full (currently above `impl Backend`):

```rust
fn extract_line_col_from_err(err_str: &str) -> (u32, u32) {
    if let Some(pos) = err_str.find("--> ") {
        let after = &err_str[pos + 4..];
        if let Some(colon) = after.find(':') {
            let line_part = after[..colon].trim();
            let rest = &after[colon + 1..];
            let end_pos = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let col_part = rest[..end_pos].trim();

            if let (Ok(l), Ok(c)) = (line_part.parse::<u32>(), col_part.parse::<u32>()) {
                return (l.saturating_sub(1), c.saturating_sub(1));
            }
        }
    }
    (0, 0)
}

fn clean_error_message(err_str: &str) -> String {
    let mut s = err_str.trim();

    // Remove leading "--> line:col\n" header if present
    if let Some(pos) = s.find("--> ") {
        if let Some(nl) = s[pos..].find('\n') {
            s = s[pos + nl + 1..].trim();
        }
    }

    // Strip Generic("...") wrapper if present
    if s.starts_with("Generic(\"") {
        s = &s[9..];
        if s.ends_with("\")") {
            s = &s[..s.len() - 2];
        } else if s.ends_with('"') {
            s = &s[..s.len() - 1];
        }
    }

    // Strip "Parse error: " prefix if present
    if let Some(stripped) = s.strip_prefix("Parse error: ") {
        s = stripped;
    }

    // Strip secondary Generic("...") if nested
    if s.starts_with("Generic(\"") {
        s = &s[9..];
        if s.ends_with("\")") {
            s = &s[..s.len() - 2];
        } else if s.ends_with('"') {
            s = &s[..s.len() - 1];
        }
    }

    s.replace("\\\"", "\"")
     .replace("\\n", "\n")
     .replace("␊", "")
     .trim()
     .to_string()
}
```

Also delete the test that exercises them from `main.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn test_unit_shadowing_lsp_diagnostic_location_and_cleaning() {
        let script = "s = 3.0 s\ng = 9.81 m / s ^ 2\n";
        let err = physure_script::parser::parse_phs_with_lines(script).unwrap_err();
        let err_str = err.to_string();
        let (line, col) = extract_line_col_from_err(&err_str);
        assert_eq!(line, 1, "Should point to line 2 (0-indexed 1)");
        assert_eq!(col, 0, "Should point to col 1 (0-indexed 0)");

        let cleaned = clean_error_message(&err_str);
        assert!(!cleaned.contains("Generic("));
        assert!(!cleaned.contains("-->"));
        assert!(cleaned.contains("Ambiguous 's' in the quantity literal `9.81 m / s ^ 2`"));
        assert!(cleaned.contains("Write `(9.81 m) / s ^ 2`"));
    }
```

Add both functions, plus two new diagnostic-builders that factor out `analyze()`'s two
diagnostic-construction blocks, to `physure-lsp/src/incremental.rs` (above `#[cfg(test)]`):

```rust
use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

fn extract_line_col_from_err(err_str: &str) -> (u32, u32) {
    if let Some(pos) = err_str.find("--> ") {
        let after = &err_str[pos + 4..];
        if let Some(colon) = after.find(':') {
            let line_part = after[..colon].trim();
            let rest = &after[colon + 1..];
            let end_pos = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let col_part = rest[..end_pos].trim();

            if let (Ok(l), Ok(c)) = (line_part.parse::<u32>(), col_part.parse::<u32>()) {
                return (l.saturating_sub(1), c.saturating_sub(1));
            }
        }
    }
    (0, 0)
}

fn clean_error_message(err_str: &str) -> String {
    let mut s = err_str.trim();

    if let Some(pos) = s.find("--> ") {
        if let Some(nl) = s[pos..].find('\n') {
            s = s[pos + nl + 1..].trim();
        }
    }

    if s.starts_with("Generic(\"") {
        s = &s[9..];
        if s.ends_with("\")") {
            s = &s[..s.len() - 2];
        } else if s.ends_with('"') {
            s = &s[..s.len() - 1];
        }
    }

    if let Some(stripped) = s.strip_prefix("Parse error: ") {
        s = stripped;
    }

    if s.starts_with("Generic(\"") {
        s = &s[9..];
        if s.ends_with("\")") {
            s = &s[..s.len() - 2];
        } else if s.ends_with('"') {
            s = &s[..s.len() - 1];
        }
    }

    s.replace("\\\"", "\"")
     .replace("\\n", "\n")
     .replace("␊", "")
     .trim()
     .to_string()
}

/// Diagnostic for a parse failure -- location comes from the error text itself (no known
/// statement to anchor it on).
fn parse_error_diagnostic(err_str: &str, text: &str) -> Diagnostic {
    let (line, col) = extract_line_col_from_err(err_str);
    let line_text = text.lines().nth(line as usize).unwrap_or("");
    let end_col = if line_text.is_empty() { 10 } else { (line_text.len() as u32).max(col + 1) };
    Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: end_col },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("physure-lsp".to_string()),
        message: clean_error_message(err_str),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Diagnostic for a statement that failed during execution -- location is the statement's own
/// known source line.
fn execution_error_diagnostic(err_str: &str, line: usize, text: &str) -> Diagnostic {
    let line = line as u32;
    let line_text = text.lines().nth(line as usize).unwrap_or("");
    let end_col = (line_text.len() as u32).max(1);
    Diagnostic {
        range: Range {
            start: Position { line, character: 0 },
            end: Position { line, character: end_col },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("physure-lsp".to_string()),
        message: format!("Execution Error: {}", clean_error_message(err_str)),
        related_information: None,
        tags: None,
        data: None,
    }
}
```

Add the moved test to `incremental.rs`'s `tests` module (identical body, just relocated):

```rust
    #[test]
    fn test_unit_shadowing_lsp_diagnostic_location_and_cleaning() {
        let script = "s = 3.0 s\ng = 9.81 m / s ^ 2\n";
        let err = physure_script::parser::parse_phs_with_lines(script).unwrap_err();
        let err_str = err.to_string();
        let (line, col) = extract_line_col_from_err(&err_str);
        assert_eq!(line, 1, "Should point to line 2 (0-indexed 1)");
        assert_eq!(col, 0, "Should point to col 1 (0-indexed 0)");

        let cleaned = clean_error_message(&err_str);
        assert!(!cleaned.contains("Generic("));
        assert!(!cleaned.contains("-->"));
        assert!(cleaned.contains("Ambiguous 's' in the quantity literal `9.81 m / s ^ 2`"));
        assert!(cleaned.contains("Write `(9.81 m) / s ^ 2`"));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p physure-lsp`
Expected: PASS — the relocated test passes from its new home; `main.rs` still compiles since
nothing else in it referenced the two moved functions except `analyze()`, which Task 7 deletes.

Note: this step intentionally isn't a strict "write failing test, watch it fail, then implement"
cycle — the test's *behavior* isn't new, it's being relocated with its implementation in one
atomic move so there's never a commit where the crate doesn't compile.

- [ ] **Step 3: Commit**

```bash
git add physure-lsp/src/incremental.rs physure-lsp/src/main.rs
git commit -m "refactor(lsp): move diagnostic construction into incremental.rs"
```

---

## Task 7: `apply_change` orchestrator (spec §3 steps 1, 5; §4.2)

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

Ties Tasks 3-6 together: parse, bootstrap or diff against `prev`, invalidate stale `env` entries,
re-run exactly the dirty statements, and return the updated `DocState` plus the full diagnostics
list `main.rs` needs to publish.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    fn run(prev: Option<DocState>, text: &str) -> (DocState, Vec<Diagnostic>) {
        let outcome = apply_change(prev, text);
        (outcome.state, outcome.diagnostics)
    }

    #[test]
    fn first_open_with_valid_text_runs_every_statement_and_reports_no_errors() {
        let (state, diagnostics) = run(None, "a = 1\nb = a + 1");
        assert!(diagnostics.is_empty());
        assert_eq!(state.interp.env.get("b").unwrap().to_string(), "2.0");
    }

    #[test]
    fn first_open_with_a_parse_error_reports_one_diagnostic_and_keeps_empty_state() {
        let (state, diagnostics) = run(None, "a = ");
        assert_eq!(diagnostics.len(), 1);
        assert!(state.statements.is_empty());
    }

    #[test]
    fn a_later_parse_error_leaves_the_previous_good_state_untouched() {
        let (state, _) = run(None, "a = 1");
        assert_eq!(state.interp.env.get("a").unwrap().to_string(), "1.0");

        let (state, diagnostics) = run(Some(state), "a = ");
        assert_eq!(diagnostics.len(), 1);
        // Untouched: still has the old value and the old (valid) statement list.
        assert_eq!(state.interp.env.get("a").unwrap().to_string(), "1.0");
        assert_eq!(state.statements.len(), 1);
    }

    #[test]
    fn editing_one_statement_only_recomputes_its_own_value() {
        let (state, _) = run(None, "a = 1\nb = 2\nc = 3");
        let (state, diagnostics) = run(Some(state), "a = 1\nb = 99\nc = 3");
        assert!(diagnostics.is_empty());
        assert_eq!(state.interp.env.get("b").unwrap().to_string(), "99.0");
        assert_eq!(state.interp.env.get("a").unwrap().to_string(), "1.0");
        assert_eq!(state.interp.env.get("c").unwrap().to_string(), "3.0");
    }

    #[test]
    fn a_rewrite_that_starts_failing_removes_its_stale_value_for_downstream_readers() {
        // §4.2: x was written successfully, then edited into a form that now errors. A
        // downstream reader of x must not see the old value.
        let (state, diagnostics) = run(None, "x = 1\ny = x");
        assert!(diagnostics.is_empty());
        assert_eq!(state.interp.env.get("y").unwrap().to_string(), "1.0");

        let (state, diagnostics) = run(Some(state), "x = undefined_fn()\ny = x");
        assert!(!diagnostics.is_empty(), "x's statement must report its new error");
        assert!(state.interp.env.get("x").is_none(), "stale x must not survive the failed rewrite");
    }

    #[test]
    fn renaming_which_variable_a_statement_writes_invalidates_the_old_name() {
        // §4.2's renamed-write case: x = 1 edited to y = 1 at the same position. The old x
        // has no statement left to invalidate it except touched_names.
        let (state, _) = run(None, "x = 1");
        assert!(state.interp.env.get("x").is_some());

        let (state, _) = run(Some(state), "y = 1");
        assert!(state.interp.env.get("x").is_none(), "old name must be invalidated");
        assert_eq!(state.interp.env.get("y").unwrap().to_string(), "1.0");
    }
```

- [ ] **Step 2: Run the tests to confirm they fail to compile**

Run: `cargo test -p physure-lsp apply_change -- --list`
Expected: fails to compile — `apply_change` not found.

- [ ] **Step 3: Implement `apply_change`**

Add to `physure-lsp/src/incremental.rs`, above the `#[cfg(test)]` block:

```rust
pub struct ChangeOutcome {
    pub state: DocState,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn apply_change(prev: Option<DocState>, text: &str) -> ChangeOutcome {
    let pairs = match physure_script::parser::parse_phs_with_lines(text) {
        Ok(p) => p,
        Err(err) => {
            // A syntactically invalid buffer can't be diffed meaningfully -- leave the last
            // known-good state untouched so the next successful parse resumes incremental
            // diffing from it rather than from scratch.
            return ChangeOutcome {
                state: prev.unwrap_or_else(DocState::empty),
                diagnostics: vec![parse_error_diagnostic(&err.to_string(), text)],
            };
        }
    };
    let new_lines: Vec<usize> = pairs.iter().map(|(l, _)| *l).collect();
    let new_statements: Vec<Statement> = pairs.into_iter().map(|(_, s)| s).collect();

    let mut state = prev.unwrap_or_else(DocState::empty);
    let old_statements = std::mem::take(&mut state.statements);
    let old_diagnostics = std::mem::take(&mut state.diagnostics);

    let DirtyAnalysis { dirty, prefix, touched_names } =
        compute_dirty(&old_statements, &new_statements);
    let len_diff = new_statements.len() as isize - old_statements.len() as isize;

    // Non-dirty statements keep their cached diagnostic, remapped from its old index (the
    // suffix region can be at a different index than before if the changed span's length
    // differs -- an insertion or deletion). Dirty slots get filled in by the run loop below.
    let mut diagnostics_by_stmt: Vec<Option<Diagnostic>> = Vec::with_capacity(new_statements.len());
    for i in 0..new_statements.len() {
        if dirty.contains(&i) {
            diagnostics_by_stmt.push(None);
        } else {
            let old_i = if i < prefix { i } else { (i as isize - len_diff) as usize };
            diagnostics_by_stmt.push(old_diagnostics.get(old_i).cloned().flatten());
        }
    }

    // §4.2: invalidate every name either side of the changed span used to write, once, up
    // front -- subsumes per-statement invalidation and correctly handles a renamed write too.
    for name in &touched_names {
        state.interp.env.remove(name);
    }

    for (i, stmt) in new_statements.iter().enumerate() {
        if !dirty.contains(&i) {
            continue;
        }
        let line = new_lines.get(i).copied().unwrap_or(0);
        diagnostics_by_stmt[i] = match state.interp.run_statement(stmt) {
            Ok(_) => None,
            Err(e) => Some(execution_error_diagnostic(&e.to_string(), line, text)),
        };
    }

    let final_diagnostics: Vec<Diagnostic> = diagnostics_by_stmt.iter().flatten().cloned().collect();
    state.statements = new_statements;
    state.lines = new_lines;
    state.diagnostics = diagnostics_by_stmt;

    ChangeOutcome { state, diagnostics: final_diagnostics }
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p physure-lsp incremental::tests`
Expected: PASS (every test added across Tasks 2-7)

- [ ] **Step 5: Run the full physure-lsp test suite**

Run: `cargo test -p physure-lsp`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "feat(lsp): add apply_change orchestrator tying incremental evaluation together"
```

---

## Task 8: Wire `main.rs` to the incremental pipeline

**Files:**
- Modify: `physure-lsp/src/main.rs`

Replaces `Backend.documents`'s role in diagnostics with `doc_states: RwLock<HashMap<Url,
incremental::DocState>>`, rewrites `on_change` to call `incremental::apply_change`, adds a
`did_close` handler (there is none today — `documents`/`doc_states` would otherwise never evict an
entry for the process lifetime), and deletes the now-dead `analyze` function. `documents` (raw
text) stays exactly as-is — hover and completion still read from it and are untouched by this
track.

- [ ] **Step 1: Add the `doc_states` field**

In `physure-lsp/src/main.rs`, change:

```rust
struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
}
```

to:

```rust
struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
    doc_states: RwLock<HashMap<Url, incremental::DocState>>,
}
```

- [ ] **Step 2: Initialize the new field**

In `main()`, change:

```rust
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
    });
```

to:

```rust
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
        doc_states: RwLock::new(HashMap::new()),
    });
```

- [ ] **Step 3: Rewrite `on_change` to use `apply_change`, and delete `analyze`**

Replace the existing `on_change` method:

```rust
    async fn on_change(&self, uri: Url, text: String) {
        // Analysing a half-typed buffer must never take the process down. A panic here used to
        // exit(101); the client restarts a few times, then gives up and the user loses
        // diagnostics for the rest of the session. Degrade to one diagnostic instead.
        let diagnostics = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            analyze(&text)
        })) {
            Ok(diagnostics) => diagnostics,
            Err(_) => vec![Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("physure-lsp".to_string()),
                message: "Internal error while analysing this file — \
                          diagnostics are unavailable until it changes again. \
                          Please report the buffer contents."
                    .to_string(),
                related_information: None,
                tags: None,
                data: None,
            }],
        };

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
```

with:

```rust
    async fn on_change(&self, uri: Url, text: String) {
        // Take ownership of any previous state before the panic guard: on a panic the
        // closure's argument is dropped along with the unwind, which correctly leaves no
        // entry behind (next edit falls back to a full bootstrap run, the same graceful
        // degradation as today).
        let prev = self.doc_states.write().unwrap().remove(&uri);

        // Analysing a half-typed buffer must never take the process down. A panic here used to
        // exit(101); the client restarts a few times, then gives up and the user loses
        // diagnostics for the rest of the session. Degrade to one diagnostic instead.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            incremental::apply_change(prev, &text)
        }));

        let diagnostics = match outcome {
            Ok(outcome) => {
                self.doc_states.write().unwrap().insert(uri.clone(), outcome.state);
                outcome.diagnostics
            }
            Err(_) => vec![Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("physure-lsp".to_string()),
                message: "Internal error while analysing this file — \
                          diagnostics are unavailable until it changes again. \
                          Please report the buffer contents."
                    .to_string(),
                related_information: None,
                tags: None,
                data: None,
            }],
        };

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
```

Delete the now-unused `analyze` function entirely (it duplicated what `incremental::apply_change`
now does, and its two diagnostic-construction blocks were already moved into `incremental.rs`'s
`parse_error_diagnostic`/`execution_error_diagnostic` in Task 6):

```rust
fn analyze(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    match physure_script::parser::parse_phs_with_lines(text) {
            Ok(statements) => {
                let mut interp = physure_script::interpreter::PhsInterpreter::default();
                for (line_idx, stmt) in statements {
                    if let Err(e) = interp.run_statement(&stmt) {
                        let err_str = e.to_string();
                        let clean_msg = clean_error_message(&err_str);
                        let line = line_idx as u32;
                        let line_text = text.lines().nth(line as usize).unwrap_or("");
                        let end_col = (line_text.len() as u32).max(1);

                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position { line, character: 0 },
                                end: Position { line, character: end_col },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: None,
                            code_description: None,
                            source: Some("physure-lsp".to_string()),
                            message: format!("Execution Error: {}", clean_msg),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
            Err(err) => {
                let err_str = err.to_string();
                let (line, col) = extract_line_col_from_err(&err_str);
                let line_text = text.lines().nth(line as usize).unwrap_or("");
                let end_col = if line_text.is_empty() {
                    10
                } else {
                    (line_text.len() as u32).max(col + 1)
                };
                let clean_msg = clean_error_message(&err_str);

                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position { line, character: col },
                        end: Position { line, character: end_col },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("physure-lsp".to_string()),
                    message: clean_msg,
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }

    diagnostics
}
```

This deletion is also why Task 6 had to move `extract_line_col_from_err`/`clean_error_message` out
first — `analyze` was their only remaining caller in `main.rs` once this step removes it.

- [ ] **Step 4: Add a `did_close` handler**

Add to the `impl LanguageServer for Backend` block, next to the other `did_*` methods:

```rust
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().unwrap().remove(&uri);
        self.doc_states.write().unwrap().remove(&uri);
    }
```

- [ ] **Step 5: Build and run the full test suite**

Run: `cargo build -p physure-lsp`
Expected: builds cleanly — confirms `analyze`'s deletion left nothing dangling and
`DidCloseTextDocumentParams` resolves via the existing `use tower_lsp::lsp_types::*;`.

Run: `cargo test -p physure-lsp`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add physure-lsp/src/main.rs
git commit -m "feat(lsp): route on_change through incremental::apply_change, add did_close"
```

---

## Task 9: Workspace verification and changelog

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 2: Run clippy on the touched crates**

Run: `cargo clippy -p physure-lsp -p physure-script -- -D warnings`
Expected: no new warnings (fix any that appear before proceeding).

- [ ] **Step 3: Add the changelog entry**

In `CHANGELOG.md`, under `## [Unreleased]` (create an `### Added` subsection if none exists),
add:

```markdown
- **`physure-lsp` re-evaluates a document incrementally instead of re-running every statement on
  every keystroke.** Editing one statement now only re-runs it and whatever transitively depends
  on it, tracked via a per-statement read/write dependency graph built from the AST (no execution
  needed to build it). A persisted interpreter carries `env` forward across edits instead of being
  rebuilt from scratch each time. Handles dynamic scoping correctly — a function call depends on
  every global its body reads, not just names in the call site's own text — and invalidates stale
  values left behind by a statement that used to succeed and now fails, including when an edit
  renames which variable a statement writes.
```

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): document Track D incremental LSP evaluation"
```

---

## Self-Review Notes (for the implementer)

- **Spec coverage**: §3 steps 1-5 → Tasks 3-7. §4.1 (FunctionCall.name reads) → Task 3. §4.2
  (stale invalidation) → Task 7. §4.3 (where/let scoping) → Task 3. §4.4 (while's unconditional
  writes, corrected against the real interpreter) → Task 3. §4.5 (template reads) → Task 3. §5
  testing list items 1-6 → Task 5 (items 1, 2, 3, 5, 6) and Task 7 (item 4, which needs actual
  execution to observe the stale value, not just the dirty-set).
- **Out of scope, confirmed untouched by this plan**: cross-file incrementality, `salsa`, any new
  LSP-visible feature. `documents` (raw text for hover/completion) is not modified.
