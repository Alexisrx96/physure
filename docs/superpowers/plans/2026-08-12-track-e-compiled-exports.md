# Track E — Compiled Export Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn an already-`export`ed PHS function into a `.proto` interface contract and a `.md`
doc file (generated from new `///` doc comments) unconditionally, plus an optional compiled
`.dll`/`.so`/`.dylib` via `phs export <script.phs> --fn <name> [--native] [-o <dir>]`.

**Architecture:** Three small, independent codegen backends (`codegen::proto::ProtoGenerator`,
`codegen::md::MdGenerator`, and a new `generate_export_shim` method on the existing
`codegen::rust::RustTranspiler`) share the existing `CodeGenerator` trait / synthetic-`Program`
calling convention already used by the four transpile targets. A new `///` doc-comment grammar
form feeds `FunctionDefNode.doc`, which `MdGenerator` renders. A new `physure-cli/src/export.rs`
subcommand wires script parsing, the three generators, and (for `--native`) a throwaway `cdylib`
crate scaffold + `cargo build --release`.

**Tech Stack:** Rust, `pest`/`pest_derive` grammar, existing `physure-script` codegen module,
`physure-cli`; `libloading` (new, test-only) for the native round-trip integration test.

**Spec:** `docs/superpowers/specs/2026-08-12-track-e-compiled-exports-design.md` (approved).

---

### Task 1: AST — `doc` field on `FunctionDefNode`

**Files:**
- Modify: `physure-script/src/ast.rs:45-57` (struct), `physure-script/src/ast.rs:194-204` (test)
- Modify: `physure-script/src/parser.rs:212-218`
- Modify: `physure-script/src/codegen/mod.rs:481-487`
- Modify: `physure-script/src/codegen/rust.rs:256-263` (test fixture)
- Modify: `physure-script/src/codegen/java.rs:308` area (test fixture)
- Modify: `physure-script/src/codegen/python.rs:295` area (test fixture)
- Modify: `physure-script/src/codegen/js.rs:279`, `physure-script/src/codegen/js.rs:299` (test fixtures)
- Modify: `physure-script/src/interpreter.rs:623`, `physure-script/src/interpreter.rs:971`, `physure-script/src/interpreter.rs:1345` (test fixture)
- Modify: `physure-script/src/decorators.rs:186`, `physure-script/src/decorators.rs:212` (test fixtures)

`codegen/mod.rs:148` (`inline_bindings_stmt`'s `Statement::FunctionDef` arm) uses `..def.clone()`
struct-update syntax — it carries the new field automatically and needs **no edit**.

- [ ] **Step 1: Confirm the current failing state**

Add the field first, which will break every other construction site — that compile failure is
this task's "red" state. In `physure-script/src/ast.rs`, change:

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

to:

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
    /// Consecutive `///` lines immediately above the definition, newline-joined, `///` prefix
    /// and one leading space stripped per line. `None` if the function has no doc comment.
    #[serde(default)]
    pub doc: Option<String>,
}
```

Run: `cd physure-script && cargo build 2>&1 | grep "missing field"`
Expected: a `missing field \`doc\`` (or similar E0063) error at every other construction site
listed above.

- [ ] **Step 2: Fix every construction site**

In `physure-script/src/ast.rs:194-204`, add `doc: None,` to `test_construct_function_def`:

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
            doc: None,
        };
```

In `physure-script/src/parser.rs:212-218`, change:

```rust
    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        decorators: Vec::new(),
    }))
```

to:

```rust
    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        decorators: Vec::new(),
        doc: None,
    }))
```

In `physure-script/src/codegen/mod.rs:481-487`, change:

```rust
                    functions.push(FunctionDefNode {
                        name: name.clone(),
                        params: kwarg_names.clone(),
                        param_units: vec![None; kwarg_names.len()],
                        body_stmts: vec![Statement::Expr(node_to_expr(chosen))],
                        decorators: Vec::new(),
                    });
```

to:

```rust
                    functions.push(FunctionDefNode {
                        name: name.clone(),
                        params: kwarg_names.clone(),
                        param_units: vec![None; kwarg_names.len()],
                        body_stmts: vec![Statement::Expr(node_to_expr(chosen))],
                        decorators: Vec::new(),
                        doc: None,
                    });
```

In `physure-script/src/interpreter.rs`, both function-composition synthesis sites (around line 623
and line 971) build a `FunctionDefNode { name: ..., params, param_units, body_stmts: vec![body],
decorators: Vec::new() }` — in each, add `doc: None,` after `decorators: Vec::new(),`. The test
fixture around line 1345 (`test_kinetic_energy`'s `FunctionDefNode` literal) gets the same
one-line addition after its `decorators:` field.

In `physure-script/src/codegen/rust.rs` (`test_transpile_function_def`'s fixture, ~line 256-263),
`physure-script/src/codegen/java.rs` (`~line 308`), `physure-script/src/codegen/python.rs`
(`~line 295`), and `physure-script/src/codegen/js.rs` (both fixtures, `~line 279` and `~line 299`):
each is a `FunctionDefNode { name: "kinetic_energy"..., decorators: vec![]/Vec::new(), }` test
literal — add `doc: None,` as the last field in each.

In `physure-script/src/decorators.rs`, `function_with_decorators` (a test helper, ~line 186) and
`validate_decorators_rejects_ensures_on_param_named_result` (~line 212) both build a
`FunctionDefNode` directly — add `doc: None,` as the last field in each.

`AssignmentNode` construction sites (`ast.rs:213-217`, `parser.rs:237-241`, `codegen/mod.rs:140`,
`codegen/python.rs` `while_stmt` fixture, `interpreter.rs` `test_kinetic_energy`'s three bindings
and `test_uncertainty_propagation`'s `m` binding, `decorators.rs`'s two `Statement::Assignment`
test fixtures) need **no change** — `doc` is not a field on `AssignmentNode` (§2.3 of the spec:
doc comments only document functions).

- [ ] **Step 3: Confirm it compiles and existing tests pass**

Run: `cd physure-script && cargo build 2>&1 | tail -30`
Expected: no errors.

Run: `cd physure-script && cargo test --lib 2>&1 | tail -20`
Expected: all pass, including `test_construct_function_def`.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/ast.rs physure-script/src/parser.rs physure-script/src/codegen/mod.rs \
  physure-script/src/codegen/rust.rs physure-script/src/codegen/java.rs \
  physure-script/src/codegen/python.rs physure-script/src/codegen/js.rs \
  physure-script/src/interpreter.rs physure-script/src/decorators.rs
git commit -m "feat(phs): add doc field to FunctionDefNode (Track E prep)"
```

---

### Task 2: Grammar — `///` doc comments

**Files:**
- Modify: `physure-script/src/phs.pest:3` (COMMENT), `physure-script/src/phs.pest:166-173` (new rules + `stmt`)

- [ ] **Step 1: Write the failing test**

In `physure-script/src/parser.rs`, add to the existing `#[cfg(test)] mod tests` block (create one
at the end of the file if none exists yet — check with
`grep -n "mod tests" physure-script/src/parser.rs` first):

```rust
    #[test]
    fn doc_comment_attaches_to_function_def() {
        let program = parse_phs("/// Computes kinetic energy.\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn multiline_doc_comment_joins_with_newline() {
        let program = parse_phs(
            "/// Line one.\n/// Line two.\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Line one.\nLine two."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn doc_comment_stacks_above_decorators() {
        let program = parse_phs(
            "/// Computes kinetic energy.\n@stable\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn plain_double_slash_comment_still_parses() {
        let program = parse_phs("// just a comment\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => assert_eq!(node.doc, None),
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd physure-script && cargo test doc_comment -- --nocapture`
Expected: FAIL — `///` is still swallowed by `COMMENT`, so `ke` parses with no leading doc and
`node.doc` is `None` in every case (or the whole `parse_phs` call errors, depending on how pest
handles the orphaned `///` text — either way, not the expected `Some(...)`).

- [ ] **Step 3: Redefine `COMMENT` and add the new grammar rules**

In `physure-script/src/phs.pest:3`, change:

```pest
COMMENT    = _{ ("//" | "#") ~ (!"\n" ~ !"\r" ~ ANY)* }
```

to:

```pest
COMMENT    = _{ ("//" ~ !"/" ~ (!"\n" ~ !"\r" ~ ANY)*) | ("#" ~ (!"\n" ~ !"\r" ~ ANY)*) }
```

In `physure-script/src/phs.pest`, immediately after the existing `decorated_stmt` rule (currently
lines 166-170), add:

```pest
doc_comment     = @{ "///" ~ (!NEWLINE ~ ANY)* }
// One or more `///` lines immediately above the definition they document. Stacks outside
// decorators — source order is docs, then decorators, then the def:
//   /// Computes kinetic energy.
//   @stable
//   fn kinetic_energy(m, v) = 0.5 * m * v^2
documented_stmt = { (doc_comment ~ _nl)+ ~ (decorated_stmt | function_def | assignment_fn | assignment) }
```

Then change the `stmt` rule (currently line 173) from:

```pest
stmt            = { import_stmt | export_stmt | decorated_stmt | function_def | assignment_fn | assignment | guard_if_stmt | return_stmt | while_stmt | raw_block | expr }
```

to:

```pest
stmt            = { import_stmt | export_stmt | documented_stmt | decorated_stmt | function_def | assignment_fn | assignment | guard_if_stmt | return_stmt | while_stmt | raw_block | expr }
```

- [ ] **Step 4: Run the tests again**

Run: `cd physure-script && cargo build 2>&1 | tail -20`
Expected: fails to build — `Rule::documented_stmt` and `Rule::doc_comment` exist now but nothing
in `parser.rs` handles them yet (Task 3). This is expected; proceed to Task 3 before re-running
the doc-comment tests.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/phs.pest
git commit -m "feat(phs): add /// doc-comment grammar (Track E prep)"
```

---

### Task 3: Parser — `parse_documented_stmt`

**Files:**
- Modify: `physure-script/src/parser.rs:55-70` (`parse_statement` match), add new function near `parse_decorated_stmt` (`physure-script/src/parser.rs:244-275`)

- [ ] **Step 1: Wire `Rule::documented_stmt` into `parse_statement`**

In `physure-script/src/parser.rs:55-70`, change:

```rust
fn parse_statement(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    match pair.as_rule() {
        Rule::stmt => parse_statement(pair.into_inner().next().unwrap()),
        Rule::import_stmt => parse_import(pair),
        Rule::export_stmt => parse_export(pair),
        Rule::decorated_stmt => parse_decorated_stmt(pair),
        Rule::function_def | Rule::assignment_fn => parse_function_def(pair),
```

to:

```rust
fn parse_statement(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    match pair.as_rule() {
        Rule::stmt => parse_statement(pair.into_inner().next().unwrap()),
        Rule::import_stmt => parse_import(pair),
        Rule::export_stmt => parse_export(pair),
        Rule::documented_stmt => parse_documented_stmt(pair),
        Rule::decorated_stmt => parse_decorated_stmt(pair),
        Rule::function_def | Rule::assignment_fn => parse_function_def(pair),
```

- [ ] **Step 2: Add `parse_documented_stmt`**

Immediately after `parse_decorated_stmt` (which ends at `physure-script/src/parser.rs:275`, right
before `parse_decorator`), add:

```rust
/// Collects consecutive `doc_comment` pairs (stripping the `///` prefix and one leading space
/// per line, joining with `\n`), parses the wrapped target via the existing parse functions, and
/// attaches the joined text to `FunctionDefNode.doc`. A doc comment stacked on a bare
/// `Statement::Assignment` parses without error but the text is dropped — `AssignmentNode` has no
/// `doc` field (§2.4 of the Track E design spec: doc comments document functions, not constants).
fn parse_documented_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut doc_lines = Vec::new();
    let mut target = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::doc_comment => {
                let text = inner.as_str().trim_start_matches("///");
                let text = text.strip_prefix(' ').unwrap_or(text);
                doc_lines.push(text.to_string());
            }
            Rule::decorated_stmt => target = Some(parse_decorated_stmt(inner)?),
            Rule::function_def | Rule::assignment_fn => target = Some(parse_function_def(inner)?),
            Rule::assignment => target = Some(parse_assignment(inner)?),
            _ => {}
        }
    }

    let mut stmt = target.ok_or_else(|| {
        PhysureError::Generic("documented statement is missing its function or assignment".to_string())
    })?;
    if let Statement::FunctionDef(node) = &mut stmt {
        node.doc = Some(doc_lines.join("\n"));
    }
    Ok(stmt)
}
```

- [ ] **Step 3: Run the Task 2 doc-comment tests**

Run: `cd physure-script && cargo test doc_comment plain_double_slash -- --nocapture`
Expected: PASS — all four tests from Task 2 Step 1 now pass.

- [ ] **Step 4: Run the full test suite**

Run: `cd physure-script && cargo test 2>&1 | tail -20`
Expected: all pass (regression check for the `COMMENT` rule change).

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/parser.rs
git commit -m "feat(phs): parse /// doc comments into FunctionDefNode.doc"
```

---

### Task 4: `codegen/mod.rs` — shared PascalCase helper + module wiring

**Files:**
- Modify: `physure-script/src/codegen/mod.rs:26-29` (module list), add helper near `as_comparison_op` (`physure-script/src/codegen/mod.rs:324-341`)

- [ ] **Step 1: Add `to_pascal_case`**

In `physure-script/src/codegen/mod.rs`, immediately before the `as_comparison_op` function
(currently starting at line 329), add:

```rust
/// `kinetic_energy` -> `KineticEnergy`. Used by `proto::ProtoGenerator` and
/// `rust::RustTranspiler::generate_export_shim` for message/service/struct names, so it lives
/// here rather than being duplicated in both.
pub(crate) fn to_pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod pascal_case_tests {
    use super::to_pascal_case;

    #[test]
    fn converts_snake_case_to_pascal_case() {
        assert_eq!(to_pascal_case("kinetic_energy"), "KineticEnergy");
        assert_eq!(to_pascal_case("ke"), "Ke");
        assert_eq!(to_pascal_case("a_b_c"), "ABC");
    }
}
```

- [ ] **Step 2: Add the two new module declarations**

In `physure-script/src/codegen/mod.rs:26-29`, change:

```rust
pub mod python;
pub mod rust;
pub mod java;
pub mod js;
```

to:

```rust
pub mod python;
pub mod rust;
pub mod java;
pub mod js;
pub mod proto;
pub mod md;
```

Both new files are created empty-but-valid in Tasks 5 and 6 — this step alone will fail to
compile until then, which is expected.

- [ ] **Step 3: Create placeholder files so the crate compiles**

Create `physure-script/src/codegen/proto.rs` with just:

```rust
```

Create `physure-script/src/codegen/md.rs` with just:

```rust
```

(Empty files — replaced with real content in Tasks 5 and 6.)

- [ ] **Step 4: Run tests**

Run: `cd physure-script && cargo test pascal_case -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/codegen/mod.rs physure-script/src/codegen/proto.rs physure-script/src/codegen/md.rs
git commit -m "feat(phs): add to_pascal_case helper and proto/md codegen module stubs"
```

---

### Task 5: `codegen/proto.rs` — `ProtoGenerator`

**Files:**
- Modify: `physure-script/src/codegen/proto.rs` (replace placeholder)

- [ ] **Step 1: Write the failing tests**

Replace the contents of `physure-script/src/codegen/proto.rs` with:

```rust
use crate::ast::{FunctionDefNode, Program, Statement};
use crate::codegen::{CodeGenerator, CodegenError};

pub struct ProtoGenerator;

impl CodeGenerator for ProtoGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let node = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::FunctionDef(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| {
                CodegenError::Generic("no function definition found to generate a .proto contract for".to_string())
            })?;
        Ok(self.generate_function(node))
    }
}

impl ProtoGenerator {
    fn generate_function(&self, node: &FunctionDefNode) -> String {
        let pascal = super::to_pascal_case(&node.name);
        let has_contract = node.decorators.iter().any(|d| d.name == "requires" || d.name == "ensures");

        let mut out = String::from("syntax = \"proto3\";\n\n");

        out.push_str(&format!("message {}Request {{\n", pascal));
        for (i, param) in node.params.iter().enumerate() {
            out.push_str(&format!("  double {} = {};\n", param, i + 1));
        }
        out.push_str("}\n\n");

        out.push_str(&format!("message {}Response {{\n", pascal));
        out.push_str("  double value = 1;\n");
        if has_contract {
            out.push_str("  bool ok = 2;      // present only if the function has >=1 @requires/@ensures\n");
            out.push_str("  string error = 3; // present only if the function has >=1 @requires/@ensures\n");
        }
        out.push_str("}\n\n");

        out.push_str(&format!("service {}Service {{\n", pascal));
        out.push_str(&format!("  rpc Compute({}Request) returns ({}Response);\n", pascal, pascal));
        out.push_str("}\n");

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DecoratorNode;
    use crate::ast::Expr;

    fn program_with(node: FunctionDefNode) -> Program {
        Program { statements: vec![Statement::FunctionDef(node)] }
    }

    fn base_node() -> FunctionDefNode {
        FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            param_units: vec![None, Some("m/s".to_string())],
            body_stmts: vec![],
            decorators: vec![],
            doc: None,
        }
    }

    #[test]
    fn generates_request_response_service_without_contract_fields() {
        let out = ProtoGenerator.generate_program(&program_with(base_node())).unwrap();
        assert!(out.contains("message KineticEnergyRequest {"));
        assert!(out.contains("double m = 1;"));
        assert!(out.contains("double v = 2;"));
        assert!(out.contains("message KineticEnergyResponse {"));
        assert!(out.contains("double value = 1;"));
        assert!(!out.contains("bool ok"));
        assert!(out.contains("service KineticEnergyService {"));
        assert!(out.contains("rpc Compute(KineticEnergyRequest) returns (KineticEnergyResponse);"));
    }

    #[test]
    fn adds_ok_and_error_fields_when_function_has_contracts() {
        let mut node = base_node();
        node.decorators = vec![DecoratorNode {
            name: "requires".to_string(),
            args: vec![Expr::Identifier("m".to_string()), Expr::Str("m must be positive".to_string())],
        }];
        let out = ProtoGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("bool ok = 2;"));
        assert!(out.contains("string error = 3;"));
    }

    #[test]
    fn errors_on_empty_program() {
        let out = ProtoGenerator.generate_program(&Program { statements: vec![] });
        assert!(out.is_err());
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd physure-script && cargo test --lib proto:: -- --nocapture`
Expected: PASS (this is a "write it, then verify" task rather than red/green, since the module
was compiling-but-empty before this step — the tests exercise real, freshly-written logic, so a
first-run pass is the expected and correct outcome here).

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/codegen/proto.rs
git commit -m "feat(phs): add ProtoGenerator (.proto codegen for Track E exports)"
```

---

### Task 6: `codegen/md.rs` — `MdGenerator`

**Files:**
- Modify: `physure-script/src/codegen/md.rs` (replace placeholder)

- [ ] **Step 1: Write the generator and its tests**

Replace the contents of `physure-script/src/codegen/md.rs` with:

```rust
use crate::ast::{Expr, FunctionDefNode, Program, Statement};
use crate::codegen::{CodeGenerator, CodegenError};

pub struct MdGenerator;

impl CodeGenerator for MdGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let node = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::FunctionDef(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| CodegenError::Generic("no function definition found to document".to_string()))?;
        Ok(self.generate_function(node))
    }
}

impl MdGenerator {
    fn generate_function(&self, node: &FunctionDefNode) -> String {
        let mut out = format!("# {}\n\n", node.name);

        if let Some(doc) = &node.doc {
            out.push_str(doc);
            out.push_str("\n\n");
        }

        out.push_str("## Signature\n\n");
        out.push_str(&format!("`{}({}) -> Quantity`\n\n", node.name, node.params.join(", ")));
        out.push_str("| Parameter | Unit |\n| :-- | :-- |\n");
        for (i, param) in node.params.iter().enumerate() {
            let unit = match node.param_units.get(i).and_then(|u| u.as_deref()) {
                Some(u) if !u.is_empty() => u.to_string(),
                _ => "*(none declared)*".to_string(),
            };
            out.push_str(&format!("| `{}` | {} |\n", param, unit));
        }
        out.push('\n');

        if node.decorators.iter().any(|d| d.name == "stable") {
            out.push_str("## Stability\n\n`@stable`\n\n");
        } else if node.decorators.iter().any(|d| d.name == "experimental") {
            out.push_str("## Stability\n\n`@experimental`\n\n");
        }

        let requires: Vec<String> = node
            .decorators
            .iter()
            .filter(|d| d.name == "requires")
            .map(|d| message_text(&d.args[1]))
            .collect();
        if !requires.is_empty() {
            out.push_str("## Preconditions\n\n");
            for msg in &requires {
                out.push_str(&format!("- `{}`\n", msg));
            }
            out.push('\n');
        }

        let ensures: Vec<String> = node
            .decorators
            .iter()
            .filter(|d| d.name == "ensures")
            .map(|d| message_text(&d.args[1]))
            .collect();
        if !ensures.is_empty() {
            out.push_str("## Postconditions\n\n");
            for msg in &ensures {
                out.push_str(&format!("- `{}`\n", msg));
            }
            out.push('\n');
        }

        out.trim_end().to_string() + "\n"
    }
}

fn message_text(expr: &Expr) -> String {
    match expr {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DecoratorNode;

    fn program_with(node: FunctionDefNode) -> Program {
        Program { statements: vec![Statement::FunctionDef(node)] }
    }

    fn base_node() -> FunctionDefNode {
        FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            param_units: vec![None, Some("m/s".to_string())],
            body_stmts: vec![],
            decorators: vec![],
            doc: None,
        }
    }

    #[test]
    fn bare_function_has_signature_table_only() {
        let out = MdGenerator.generate_program(&program_with(base_node())).unwrap();
        assert!(out.contains("# kinetic_energy"));
        assert!(out.contains("`kinetic_energy(m, v) -> Quantity`"));
        assert!(out.contains("| `m` | *(none declared)* |"));
        assert!(out.contains("| `v` | m/s |"));
        assert!(!out.contains("## Stability"));
        assert!(!out.contains("## Preconditions"));
        assert!(!out.contains("## Postconditions"));
    }

    #[test]
    fn doc_only_adds_description_section() {
        let mut node = base_node();
        node.doc = Some("Computes kinetic energy.".to_string());
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("# kinetic_energy\n\nComputes kinetic energy.\n\n## Signature"));
    }

    #[test]
    fn decorators_only_add_stability_and_condition_sections() {
        let mut node = base_node();
        node.decorators = vec![
            DecoratorNode { name: "stable".to_string(), args: vec![] },
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be positive".to_string())],
            },
            DecoratorNode {
                name: "ensures".to_string(),
                args: vec![Expr::Identifier("result".to_string()), Expr::Str("result must be positive".to_string())],
            },
        ];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("## Stability\n\n`@stable`"));
        assert!(out.contains("## Preconditions\n\n- `v must be positive`"));
        assert!(out.contains("## Postconditions\n\n- `result must be positive`"));
    }

    #[test]
    fn doc_and_decorators_together_render_all_sections() {
        let mut node = base_node();
        node.doc = Some("Computes kinetic energy.".to_string());
        node.decorators = vec![DecoratorNode { name: "experimental".to_string(), args: vec![] }];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("Computes kinetic energy."));
        assert!(out.contains("`@experimental`"));
    }

    #[test]
    fn range_lowered_requires_render_as_separate_bullets() {
        let mut node = base_node();
        node.decorators = vec![
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be >= the @range lower bound".to_string())],
            },
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be <= the @range upper bound".to_string())],
            },
        ];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("- `v must be >= the @range lower bound`"));
        assert!(out.contains("- `v must be <= the @range upper bound`"));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd physure-script && cargo test --lib md:: -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/codegen/md.rs
git commit -m "feat(phs): add MdGenerator (.md codegen for Track E exports)"
```

---

### Task 7: `codegen/rust.rs` — `generate_export_shim`

**Files:**
- Modify: `physure-script/src/codegen/rust.rs` (add method to `impl RustTranspiler` block, add tests)

- [ ] **Step 1: Write the failing tests**

In `physure-script/src/codegen/rust.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn export_shim_bare_function_returns_flat_f64() {
        let node = FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            param_units: vec![Some("kg".to_string()), Some("m/s".to_string())],
            body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Quantity(QuantityNode {
                    magnitude: 0.5,
                    uncertainty: None,
                    uncertainty_lower: None,
                    is_sigma: false,
                    unit: None,
                    format_spec: None,
                })),
                right: Box::new(Expr::Identifier("m".to_string())),
            })],
            decorators: vec![],
            doc: None,
        };
        let shim = RustTranspiler.generate_export_shim(&node).unwrap();
        assert!(shim.contains("pub fn kinetic_energy_impl(m: Quantity, v: Quantity) -> Quantity"));
        assert!(shim.contains("pub extern \"C\" fn kinetic_energy(m: f64, v: f64) -> f64"));
        assert!(shim.contains("Quantity::new(m, \"kg\").unwrap()"));
        assert!(shim.contains("kinetic_energy_impl(m, v).value.mean()"));
        assert!(!shim.contains("KineticEnergyResult"));
    }

    #[test]
    fn export_shim_decorated_function_returns_result_struct() {
        let node = FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string()],
            param_units: vec![Some("kg".to_string())],
            body_stmts: vec![Statement::Expr(Expr::Identifier("m".to_string()))],
            decorators: vec![
                DecoratorNode {
                    name: "requires".to_string(),
                    args: vec![
                        Expr::FunctionCall {
                            name: "op_>".to_string(),
                            args: vec![
                                Expr::Identifier("m".to_string()),
                                Expr::Quantity(QuantityNode {
                                    magnitude: 0.0,
                                    uncertainty: None,
                                    uncertainty_lower: None,
                                    is_sigma: false,
                                    unit: Some("kg".to_string()),
                                    format_spec: None,
                                }),
                            ],
                            kwargs: vec![],
                        },
                        Expr::Str("m must be positive".to_string()),
                    ],
                },
                DecoratorNode {
                    name: "ensures".to_string(),
                    args: vec![
                        Expr::FunctionCall {
                            name: "op_>".to_string(),
                            args: vec![
                                Expr::Identifier("result".to_string()),
                                Expr::Quantity(QuantityNode {
                                    magnitude: 0.0,
                                    uncertainty: None,
                                    uncertainty_lower: None,
                                    is_sigma: false,
                                    unit: Some("kg".to_string()),
                                    format_spec: None,
                                }),
                            ],
                            kwargs: vec![],
                        },
                        Expr::Str("result must be positive".to_string()),
                    ],
                },
            ],
            doc: None,
        };
        let shim = RustTranspiler.generate_export_shim(&node).unwrap();
        assert!(shim.contains("pub struct KineticEnergyResult"));
        assert!(shim.contains("pub extern \"C\" fn kinetic_energy(m: f64) -> KineticEnergyResult"));
        assert!(shim.contains("\"m must be positive\""));
        assert!(shim.contains("\"result must be positive\""));
        assert!(shim.contains("let result = kinetic_energy_impl(m);"));
        assert!(shim.contains("pub extern \"C\" fn kinetic_energy_last_error() -> *const std::os::raw::c_char"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd physure-script && cargo test --lib export_shim -- --nocapture`
Expected: FAIL with `no method named \`generate_export_shim\` found`.

- [ ] **Step 3: Implement `generate_export_shim`**

In `physure-script/src/codegen/rust.rs`, inside the `impl RustTranspiler { ... }` block (after
`generate_expr`, before the closing `}` of the impl), add:

```rust
    /// Wraps the ordinary transpiled function (renamed `<name>_impl`) in a `#[no_mangle] extern
    /// "C"` shim: flat `f64` params (unit baked in from `param_units` at generation time), flat
    /// `f64` return for a function with no `@requires`/`@ensures`, or a `#[repr(C)]
    /// <Name>Result { value: f64, ok: bool }` for one that has them — running each `@requires`
    /// check before the call and each `@ensures` check after, via the exact same `generate_expr`
    /// path already used for the function body, so compiled and interpreted pass/fail can never
    /// diverge by construction. On the first failing check, the message is stashed in a
    /// thread-local string readable via `<name>_last_error()`, mirroring `errno`/`GetLastError`.
    pub fn generate_export_shim(&self, node: &FunctionDefNode) -> Result<String, CodegenError> {
        let impl_name = format!("{}_impl", node.name);
        let mut impl_node = node.clone();
        impl_node.name = impl_name.clone();
        let impl_fn = self.generate_function_def(&impl_node)?;

        let has_contract = node.decorators.iter().any(|d| d.name == "requires" || d.name == "ensures");

        let params_sig: Vec<String> = node.params.iter().map(|p| format!("{}: f64", p)).collect();
        let mut binds = Vec::new();
        let mut call_args = Vec::new();
        for (i, param) in node.params.iter().enumerate() {
            let unit = node.param_units.get(i).and_then(|u| u.as_deref()).unwrap_or("");
            binds.push(format!("    let {p} = Quantity::new({p}, {u:?}).unwrap();", p = param, u = unit));
            call_args.push(param.clone());
        }
        let binds = binds.join("\n");
        let call_expr = format!("{}({})", impl_name, call_args.join(", "));

        let mut shim = String::new();
        shim.push_str(&impl_fn);
        shim.push_str("\n\n");

        if !has_contract {
            shim.push_str(&format!(
                "#[no_mangle]\npub extern \"C\" fn {name}({params}) -> f64 {{\n{binds}\n    {call}.value.mean()\n}}\n",
                name = node.name,
                params = params_sig.join(", "),
                binds = binds,
                call = call_expr,
            ));
            return Ok(shim);
        }

        let pascal = super::to_pascal_case(&node.name);
        let error_static = format!("{}_LAST_ERROR", node.name.to_uppercase());

        shim.push_str(&format!(
            "#[repr(C)]\npub struct {pascal}Result {{\n    pub value: f64,\n    pub ok: bool,\n}}\n\n",
            pascal = pascal,
        ));
        shim.push_str(&format!(
            "thread_local! {{\n    static {error_static}: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());\n}}\n\n",
            error_static = error_static,
        ));

        let mut body = String::new();
        body.push_str(&binds);
        body.push('\n');
        for dec in &node.decorators {
            if dec.name == "requires" {
                let cond = self.generate_expr(&dec.args[0])?;
                let msg = message_literal(&dec.args[1]);
                body.push_str(&format!(
                    "    if !({cond}) {{\n        {error_static}.with(|e| *e.borrow_mut() = {msg}.to_string());\n        return {pascal}Result {{ value: 0.0, ok: false }};\n    }}\n",
                    cond = cond, error_static = error_static, msg = msg, pascal = pascal,
                ));
            }
        }
        body.push_str(&format!("    let result = {};\n", call_expr));
        for dec in &node.decorators {
            if dec.name == "ensures" {
                let cond = self.generate_expr(&dec.args[0])?;
                let msg = message_literal(&dec.args[1]);
                body.push_str(&format!(
                    "    if !({cond}) {{\n        {error_static}.with(|e| *e.borrow_mut() = {msg}.to_string());\n        return {pascal}Result {{ value: 0.0, ok: false }};\n    }}\n",
                    cond = cond, error_static = error_static, msg = msg, pascal = pascal,
                ));
            }
        }
        body.push_str(&format!("    {pascal}Result {{ value: result.value.mean(), ok: true }}\n", pascal = pascal));

        shim.push_str(&format!(
            "#[no_mangle]\npub extern \"C\" fn {name}({params}) -> {pascal}Result {{\n{body}}}\n\n",
            name = node.name,
            params = params_sig.join(", "),
            pascal = pascal,
            body = body,
        ));

        shim.push_str(&format!(
            "#[no_mangle]\npub extern \"C\" fn {name}_last_error() -> *const std::os::raw::c_char {{\n    {error_static}.with(|e| std::ffi::CString::new(e.borrow().clone()).unwrap_or_default().into_raw())\n}}\n",
            name = node.name,
            error_static = error_static,
        ));

        Ok(shim)
    }
```

Then, above `impl RustTranspiler` (module level), add the small literal-rendering helper it calls:

```rust
/// Renders a decorator's message argument as a Rust string literal. `{:?}` on a `&str` already
/// produces a correctly escaped, double-quoted Rust literal, so no hand-rolled escaping is
/// needed. Every existing `@requires`/`@ensures` message is built as `Expr::Str` (see
/// `decorators.rs`), so the fallback branch is unreached in practice; it exists so a non-literal
/// message can never panic codegen.
fn message_literal(expr: &Expr) -> String {
    match expr {
        Expr::Str(s) => format!("{:?}", s),
        _ => "\"contract violated\"".to_string(),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cd physure-script && cargo test --lib export_shim -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Run the full `physure-script` test suite**

Run: `cd physure-script && cargo test 2>&1 | tail -20`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/codegen/rust.rs
git commit -m "feat(phs): add generate_export_shim for compiled FFI exports"
```

---

### Task 8: `physure-cli/src/export.rs` — the `phs export` subcommand

**Files:**
- Create: `physure-cli/src/export.rs`
- Modify: `physure-cli/src/main.rs:7-16` (module list), `physure-cli/src/main.rs:648-655` (dispatch), `physure-cli/src/main.rs:22-51` (help text)

- [ ] **Step 1: Create `export.rs`**

Create `physure-cli/src/export.rs`:

```rust
//! `phs export <script.phs> --fn <name> [--native] [-o <dir>]`
//!
//! Always writes `<fn>.proto` and `<fn>.md` for the named, already-`export`ed function.
//! `--native` additionally scaffolds a throwaway `cdylib` crate wrapping the compiled FFI shim,
//! builds it with `cargo build --release`, and copies the resulting `.dll`/`.so`/`.dylib` next
//! to the `.proto`/`.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use physure_script::ast::FunctionDefNode;
use physure_script::codegen::md::MdGenerator;
use physure_script::codegen::proto::ProtoGenerator;
use physure_script::codegen::rust::RustTranspiler;
use physure_script::codegen::CodeGenerator;
use physure_script::{parse_phs, Program, Statement};

use crate::get_flag_value;

/// Baked in at `phs`'s own compile time — the same relationship this crate's own `Cargo.toml`
/// already has to `physure-core` (`path = "../physure-core", package = "physure"`), just
/// available at runtime so a scaffolded crate anywhere on disk can resolve back to the exact
/// `physure-core` this binary was built from. No publishing, no vendoring, no drift from the
/// single source of truth for unit logic.
const PHYSURE_CORE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../physure-core");

pub fn run_export(args: &[String]) {
    let script_path = match args.get(2) {
        Some(p) if !p.starts_with('-') => p.clone(),
        _ => {
            eprintln!("Usage: phs export <script.phs> --fn <name> [--native] [-o <dir>]");
            process::exit(1);
        }
    };
    let fn_name = match get_flag_value(args, "--fn") {
        Some(n) => n,
        None => {
            eprintln!("error: --fn <name> is required");
            process::exit(1);
        }
    };
    let is_native = args.iter().any(|a| a == "--native");

    let code = match fs::read_to_string(&script_path) {
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

    let node = match find_function(&program, &fn_name) {
        Some(n) => n.clone(),
        None => {
            eprintln!("error: no function named '{}' in '{}'", fn_name, script_path);
            process::exit(1);
        }
    };
    if !is_exported(&program, &fn_name) {
        eprintln!("error: '{}' exists but was never `export`ed; add `export {}`", fn_name, fn_name);
        process::exit(1);
    }

    let out_dir = PathBuf::from(get_flag_value(args, "-o").or_else(|| get_flag_value(args, "--output")).unwrap_or_else(|| {
        Path::new(&script_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ".".to_string())
    }));
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("error creating output directory '{}': {}", out_dir.display(), e);
        process::exit(1);
    }

    let single_fn_program = Program { statements: vec![Statement::FunctionDef(node.clone())] };

    let proto = match ProtoGenerator.generate_program(&single_fn_program) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating .proto: {}", e);
            process::exit(1);
        }
    };
    write_output(&out_dir.join(format!("{}.proto", fn_name)), &proto);

    let md = match MdGenerator.generate_program(&single_fn_program) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating .md: {}", e);
            process::exit(1);
        }
    };
    write_output(&out_dir.join(format!("{}.md", fn_name)), &md);

    if is_native {
        build_native(&out_dir, &fn_name, &node);
    }
}

fn find_function<'a>(program: &'a Program, name: &str) -> Option<&'a FunctionDefNode> {
    program.statements.iter().find_map(|s| match s {
        Statement::FunctionDef(f) if f.name == name => Some(f),
        _ => None,
    })
}

fn is_exported(program: &Program, name: &str) -> bool {
    program.statements.iter().any(|s| matches!(s, Statement::Export(e) if e.symbol == name))
}

fn write_output(path: &Path, contents: &str) {
    if let Err(e) = fs::write(path, contents) {
        eprintln!("error writing '{}': {}", path.display(), e);
        process::exit(1);
    }
    println!("wrote {}", path.display());
}

fn build_native(out_dir: &Path, fn_name: &str, node: &FunctionDefNode) {
    let shim = match RustTranspiler.generate_export_shim(node) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating native shim: {}", e);
            process::exit(1);
        }
    };
    let crate_name = format!("{}_export", fn_name);
    let crate_dir = out_dir.join(&crate_name);
    let src_dir = crate_dir.join("src");
    if let Err(e) = fs::create_dir_all(&src_dir) {
        eprintln!("error creating '{}': {}", src_dir.display(), e);
        process::exit(1);
    }

    let lib_rs = format!("// Generated by PhysureScript (PHS) Compiler\nuse physure_core::Quantity;\n\n{}", shim);
    write_output(&src_dir.join("lib.rs"), &lib_rs);

    let cargo_toml = format!(
        "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nname = \"{crate_name}\"\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nphysure-core = {{ path = '{core_path}', package = \"physure\" }}\n",
        crate_name = crate_name,
        core_path = PHYSURE_CORE_PATH,
    );
    write_output(&crate_dir.join("Cargo.toml"), &cargo_toml);

    println!("running cargo build --release in {}...", crate_dir.display());
    let output = Command::new("cargo").args(["build", "--release"]).current_dir(&crate_dir).output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error running cargo: {}", e);
            process::exit(1);
        }
    };
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        process::exit(1);
    }

    let built_name = if cfg!(target_os = "windows") {
        format!("{}.dll", crate_name)
    } else if cfg!(target_os = "macos") {
        format!("lib{}.dylib", crate_name)
    } else {
        format!("lib{}.so", crate_name)
    };
    let dest_ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let built = crate_dir.join("target").join("release").join(&built_name);
    let dest = out_dir.join(format!("{}.{}", fn_name, dest_ext));
    if let Err(e) = fs::copy(&built, &dest) {
        eprintln!("error copying built library from '{}' to '{}': {}", built.display(), dest.display(), e);
        process::exit(1);
    }
    println!("wrote {}", dest.display());
}
```

This requires `Program`/`Statement` to be reachable as `physure_script::{Program, Statement}` (they
already are — `lib.rs` re-exports `pub use ast::{Expr, Program, Statement};`) and
`physure_script::ast::FunctionDefNode`, `physure_script::codegen::{proto, md, rust, CodeGenerator}`
to be `pub` (they already are, per Task 4/5/6/`lib.rs`'s existing `pub mod ast;`/`pub mod codegen;`).

- [ ] **Step 2: Wire the module and dispatch into `main.rs`**

In `physure-cli/src/main.rs:7-16`, change:

```rust
mod config;
mod html;
mod katex_assets;
mod latex;
mod protocol;
mod rich;
mod scaffold;
mod step;
mod tui;
mod web;
```

to:

```rust
mod config;
mod export;
mod html;
mod katex_assets;
mod latex;
mod protocol;
mod rich;
mod scaffold;
mod step;
mod tui;
mod web;
```

In `physure-cli/src/main.rs:648-655`, change:

```rust
    if args[1] == "new-plugin" {
        scaffold::run_new_plugin(&args);
        return;
    }

    if handle_transpile(&args) {
        return;
    }
```

to:

```rust
    if args[1] == "new-plugin" {
        scaffold::run_new_plugin(&args);
        return;
    }

    if args[1] == "export" {
        export::run_export(&args);
        return;
    }

    if handle_transpile(&args) {
        return;
    }
```

In `physure-cli/src/main.rs:22-51` (`print_help`), add a usage line after the `transpile` line and
an example after the `new-plugin` example:

```rust
    println!("    phs transpile <script.phs> [--target <rust|python|java|js|ts>] [--output <file>]");
    println!("    phs export <script.phs> --fn <name> [--native] [-o <dir>]");
```

and:

```rust
    println!("    phs new-plugin myplugin --lang rust");
    println!("    phs export orbit_sim.phs --fn kinetic_energy --native -o dist/");
```

- [ ] **Step 3: Manually verify against the example script**

Run:
```bash
cd physure-cli
cat > /tmp/ke.phs <<'EOF'
/// Computes the kinetic energy of a moving mass.
/// `m` is mass, `v` is velocity, both must be positive.
@stable
@ensures(result > 0.0 J, "kinetic energy must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
EOF
cargo run --bin phs -- export /tmp/ke.phs --fn kinetic_energy -o /tmp/ke_out
cat /tmp/ke_out/kinetic_energy.proto
cat /tmp/ke_out/kinetic_energy.md
```
Expected: both files are written and contain the request/response/service and the doc/signature/
stability/postcondition sections respectively, with no `--native` build attempted.

- [ ] **Step 4: Commit**

```bash
git add physure-cli/src/export.rs physure-cli/src/main.rs
git commit -m "feat(cli): add phs export subcommand (.proto/.md, optional --native)"
```

---

### Task 9: `--native` round-trip integration test

**Files:**
- Create: `physure-cli/tests/export_native_roundtrip.rs`
- Modify: `physure-cli/Cargo.toml` (add `[dev-dependencies]`)

`physure-cli` is a `[[bin]]`-only crate (no `[lib]`), so this integration test shells out to the
built `phs` binary via `env!("CARGO_BIN_EXE_phs")` rather than calling `export::run_export`
directly — the standard cargo pattern for testing binary-only crates.

- [ ] **Step 1: Add the `libloading` dev-dependency**

In `physure-cli/Cargo.toml`, after the existing `[target.'cfg(windows)'.dependencies]` block, add:

```toml

[dev-dependencies]
libloading = "0.8"
```

- [ ] **Step 2: Write the test**

Create `physure-cli/tests/export_native_roundtrip.rs`:

```rust
//! Slow (real `cargo build --release`) — run explicitly with:
//!   cargo test --test export_native_roundtrip -- --ignored --nocapture

use std::fs;
use std::process::Command;

const SCRIPT: &str = r#"
/// Computes the kinetic energy of a moving mass.
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
"#;

const CONTRACT_SCRIPT: &str = r#"
@requires(m > 0.0 kg, "m must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
"#;

#[test]
#[ignore]
fn native_export_matches_interpreter_output() {
    let dir = std::env::temp_dir().join("phs_export_roundtrip_bare");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("ke.phs");
    fs::write(&script_path, SCRIPT).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["export", script_path.to_str().unwrap(), "--fn", "kinetic_energy", "--native", "-o"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success());

    let lib_ext = if cfg!(target_os = "windows") { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let lib_path = dir.join(format!("kinetic_energy.{}", lib_ext));
    assert!(lib_path.exists(), "expected {} to exist", lib_path.display());

    let m = 2.0_f64;
    let v = 3.0_f64;
    let compiled_value = unsafe {
        let lib = libloading::Library::new(&lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(f64, f64) -> f64> = lib.get(b"kinetic_energy").unwrap();
        func(m, v)
    };

    let interpreted = physure_script::eval_phs(&format!(
        "fn kinetic_energy(m, v) = 0.5 * m * v^2\nkinetic_energy({m} kg, {v} m/s)"
    ))
    .unwrap();
    let expected = match interpreted.last().unwrap() {
        physure_script::PhsValue::Quantity(q) => q.value.mean(),
        other => panic!("expected Quantity, got {:?}", other),
    };

    assert!(
        (compiled_value - expected).abs() < 1e-9,
        "compiled={} interpreted={}",
        compiled_value,
        expected
    );
}

#[test]
#[ignore]
fn native_export_contract_violation_matches_interpreter() {
    let dir = std::env::temp_dir().join("phs_export_roundtrip_contract");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("ke.phs");
    fs::write(&script_path, CONTRACT_SCRIPT).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["export", script_path.to_str().unwrap(), "--fn", "kinetic_energy", "--native", "-o"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success());

    let lib_ext = if cfg!(target_os = "windows") { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let lib_path = dir.join(format!("kinetic_energy.{}", lib_ext));

    #[repr(C)]
    struct KineticEnergyResult {
        value: f64,
        ok: bool,
    }

    let (ok_valid, ok_invalid) = unsafe {
        let lib = libloading::Library::new(&lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(f64, f64) -> KineticEnergyResult> =
            lib.get(b"kinetic_energy").unwrap();
        (func(2.0, 3.0).ok, func(-1.0, 3.0).ok)
    };

    assert!(ok_valid, "positive mass should satisfy @requires");
    assert!(!ok_invalid, "negative mass should violate @requires");

    let interpreter_rejects = physure_script::eval_phs(
        "@requires(m > 0.0 kg, \"m must be positive\")\nfn kinetic_energy(m, v) = 0.5 * m * v^2\nkinetic_energy(-1.0 kg, 3.0 m/s)",
    )
    .is_err();
    assert!(interpreter_rejects, "interpreter should also reject the negative-mass call");
}
```

- [ ] **Step 3: Run it explicitly**

Run: `cd physure-cli && cargo test --test export_native_roundtrip -- --ignored --nocapture`
Expected: both tests PASS (this invokes a real `cargo build --release` for the scaffolded crate,
so it is slow — minutes, not seconds — which is exactly why it's `#[ignore]`d by default).

- [ ] **Step 4: Confirm the default (non-ignored) test run is unaffected**

Run: `cd physure-cli && cargo test 2>&1 | tail -10`
Expected: the two new tests are skipped (reported as "ignored"), everything else passes.

- [ ] **Step 5: Commit**

```bash
git add physure-cli/Cargo.toml physure-cli/tests/export_native_roundtrip.rs
git commit -m "test(cli): add ignored native export round-trip test (libloading)"
```

---

### Task 10: Update `docs/language_readiness_roadmap.md`

**Files:**
- Modify: `docs/language_readiness_roadmap.md:489-501`

- [ ] **Step 1: Mark the Track E checklist items done, documenting the two deviations from the original sketch**

In `docs/language_readiness_roadmap.md`, change the block at lines 489-501 from:

```markdown
- [ ] **Track E: Compiled Export Artifacts** *(non-blocking for LAB-READY — a distribution/interop
      capability, not a language execution capability)*
  - [ ] `codegen::proto::ProtoGenerator`: Request/Response messages + `rpc Compute` per exported
        function, derived from `param_units` and the inferred return type.
  - [ ] FFI shim generator: flat `f64` in/out wrapping `RustTranspiler` output, unit baked in at
        generation time.
  - [ ] Scaffold-and-build pipeline: throwaway `cdylib` crate, `cargo build --release`, artifact
        copied to the output dir.
  - [ ] `phs export` CLI subcommand (`--fn`, `--target proto|native|all`, `-o`).
  - [ ] Round-trip test: compiled `.so`/`.dll` output matches interpreter output for the same
        function and input, within floating-point tolerance.
  - [ ] *(Parked, out of scope)* Curated multi-formula repository, hosted on-demand builds,
        generated docs site.
```

to:

```markdown
- [x] **Track E: Compiled Export Artifacts** *(non-blocking for LAB-READY — a distribution/interop
      capability, not a language execution capability)* — see
      [`docs/superpowers/specs/2026-08-12-track-e-compiled-exports-design.md`](superpowers/specs/2026-08-12-track-e-compiled-exports-design.md)
      and [`docs/superpowers/plans/2026-08-12-track-e-compiled-exports.md`](superpowers/plans/2026-08-12-track-e-compiled-exports.md)
      for the design and executed plan. Two scope changes from this section's original
      description, both intentional and tracked below:
      (1) `.proto` **and** `.md` generation are mandatory on every `phs export` run, not one of
      several `--target` choices — the CLI flag surface simplified to a single optional
      `--native`, in favor of a new `///` doc-comment language feature that feeds the `.md`;
      (2) contract propagation (`@requires`/`@ensures`) into the compiled shim, which Track F's
      own plan explicitly deferred to this track, is implemented via `generate_export_shim`
      reusing `RustTranspiler::generate_expr` — never a second, independently-written condition
      evaluator.
  - [x] `codegen::proto::ProtoGenerator`: Request/Response messages + `rpc Compute` per exported
        function, derived from `param_units`; `Response` gains `ok`/`error` fields only when the
        function carries `@requires`/`@ensures` (return-value unit is not statically known in PHS
        today, so the `Response.value` field is untyped `double` rather than unit-annotated).
  - [x] `///` doc-comment grammar/AST/parser (new prerequisite, not in the original sketch):
        `FunctionDefNode.doc: Option<String>`, rendered by `codegen::md::MdGenerator`.
  - [x] FFI shim generator (`RustTranspiler::generate_export_shim`): flat `f64` in/out, unit baked
        in at generation time, `@requires`/`@ensures` enforced via the shared `generate_expr` path,
        `<name>_last_error()` thread-local getter for the failing contract's message.
  - [x] Scaffold-and-build pipeline: throwaway `cdylib` crate, `cargo build --release`, artifact
        copied to the output dir; `physure-core` resolved via a `CARGO_MANIFEST_DIR`-derived
        absolute path baked into `phs` at its own compile time (not published, not vendored).
  - [x] `phs export` CLI subcommand (`--fn`, `--native`, `-o`; `.proto`/`.md` unconditional).
  - [x] Round-trip test: compiled `.so`/`.dll` output matches interpreter output for the same
        function and input, within floating-point tolerance, including a paired
        contract-violation case (`#[ignore]`d — real `cargo build --release`, run explicitly).
  - [ ] *(Parked, out of scope)* Curated multi-formula repository, hosted on-demand builds,
        generated docs site.
```

- [ ] **Step 2: Commit**

```bash
git add docs/language_readiness_roadmap.md
git commit -m "docs: mark Track E complete in language readiness roadmap"
```

---

## Self-Review Notes

- **Spec coverage**: §2 (grammar/AST/parser) → Tasks 1-3. §3 (`.proto`) → Task 5. §4 (`.md`) →
  Task 6. §5 (FFI shim) → Task 7. §6 (CLI subcommand, scaffold-and-build, tests) → Tasks 8-9. §7
  (scope boundaries) — nothing added beyond spec; `generate_function_def` and the Python/Java/JS
  backends are untouched by every task above. §8 (example script) → Task 8 Step 3's manual check
  uses the same script.
- **Placeholder scan**: no "TBD"/"handle appropriately" — every step has literal code, exact
  commands, and expected output.
- **Type consistency**: `FunctionDefNode.doc: Option<String>` (Task 1) is read the same way in
  Task 3 (`node.doc = Some(...)`) and Task 6 (`node.doc: &Option<String>` matched via `if let
  Some(doc) = &node.doc`). `generate_export_shim(&self, node: &FunctionDefNode) -> Result<String,
  CodegenError>` (Task 7) is called identically in Task 8 (`RustTranspiler.generate_export_shim(node)`).
  `to_pascal_case` (Task 4) is called as `super::to_pascal_case` from both `proto.rs` (Task 5) and
  `rust.rs` (Task 7).
