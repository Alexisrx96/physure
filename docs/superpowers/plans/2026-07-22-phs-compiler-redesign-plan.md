# PHS DSL Compiler & Multi-Target Transpiler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a production-ready `pest`-based PEG compiler and multi-target transpiler (Python, Rust, Java) for the `.phs` Physics/Math DSL within `physure-script`.

**Architecture:** A three-stage compiler pipeline (Pest Parser → Symbol Resolver → Tree-Walk Interpreter) coupled with a pluggable `CodeGenerator` trait engine emitting target code for Python, Rust, and Java. Follows SOLID (SRP, DIP, OCP) and DRY principles.

**Tech Stack:** Rust (edition 2021), `pest` 2.7, `pest_derive` 2.7, `physure-core`, `physure-script`, `num-rational`, `serde`.

---

## File Map

- `physure-script/Cargo.toml`: Add `pest` and `pest_derive` dependencies.
- `physure-script/src/phs.pest`: PEG grammar definitions for natural math notation, physical quantities, units, and explicit imports.
- `physure-script/src/ast.rs`: SOLID AST nodes (`Program`, `Statement`, `Expr`, `ImportNode`, `QuantityNode`, etc.).
- `physure-script/src/parser.rs`: Pest pair visitor mapping CST pairs to `ast::Program`.
- `physure-script/src/resolver.rs`: `ModuleResolver` trait, `FsModuleResolver`, and `SymbolTable` collision detection.
- `physure-script/src/interpreter.rs`: Tree-walk execution engine operating on resolved AST.
- `physure-script/src/codegen/mod.rs`: `CodeGenerator` trait definition and dispatch module.
- `physure-script/src/codegen/python.rs`: Python target transpiler emitting `physure` library code.
- `physure-script/src/codegen/rust.rs`: Rust target transpiler emitting `physure` crate code.
- `physure-script/src/codegen/java.rs`: Java target transpiler emitting `com.physure.Quantity` code.
- `physure-script/src/exporter.rs`: `DataExporter` trait and implementation (JSON, CSV, Python dicts).
- `physure-script/src/lib.rs`: Public module exports.

---

### Task 1: Add `pest` Dependencies to `physure-script/Cargo.toml`

**Files:**
- Modify: `physure-script/Cargo.toml`

- [ ] **Step 1: Update Cargo.toml dependencies**

Add `pest` and `pest_derive` to `physure-script/Cargo.toml`:

```toml
[dependencies]
physure-core = { path = "../physure-core" }
num-rational.workspace = true
num-traits.workspace   = true
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
pest        = "2.7"
pest_derive = "2.7"
```

- [ ] **Step 2: Verify cargo compilation**

Run: `cargo check -p physure-script`
Expected: PASS clean compilation.

- [ ] **Step 3: Commit**

```bash
git add physure-script/Cargo.toml
git commit -m "build(phs): add pest and pest_derive dependencies"
```

---

### Task 2: Create Pest Grammar Specification (`physure-script/src/phs.pest`)

**Files:**
- Create: `physure-script/src/phs.pest`

- [ ] **Step 1: Write `phs.pest` grammar**

Create `physure-script/src/phs.pest` with natural math, implicit multiplication, physical units, and explicit import rules:

```pest
WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
COMMENT    = _{ "//" ~ (!"\n" ~ ANY)* ~ "\n" | "/*" ~ (!"*/" ~ ANY)* ~ "*/" }

identifier  = @{ (ASCII_ALPHA | "π" | "θ" | "λ" | "μ" | "Δ" | "σ" | "Ω" | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
number      = @{ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? ~ ( ^"e" ~ ("+" | "-")? ~ ASCII_DIGIT+ )? }

uncertainty_op  = { "+/-" | "±" }
uncertainty_val = { number ~ "%"? }
uncertainty     = { uncertainty_op ~ uncertainty_val }

unit_term       = @{ ASCII_ALPHA+ ~ ("^" ~ ("+" | "-")? ~ ASCII_DIGIT+)? }
unit_primary    = _{ unit_term | "(" ~ unit_expr ~ ")" }
unit_expr       = { unit_primary ~ (("*" | "/" | WHITESPACE+) ~ unit_primary)* }

quantity = {
    ("(" ~ expr ~ uncertainty? ~ ")" ~ unit_expr?)
  | (number ~ uncertainty? ~ unit_expr?)
}

op_add = { "+" }
op_sub = { "-" }
op_mul = { "*" }
op_div = { "/" }
op_pow = { "^" }

primary = _{ quantity | function_call | identifier | "(" ~ expr ~ ")" }
factor  = { primary ~ (op_pow ~ primary)? }
term    = { factor ~ ((op_mul | op_div) ~ factor | factor)* }
expr    = { term ~ ((op_add | op_sub) ~ term)* }

function_call = { identifier ~ "(" ~ (expr ~ ("," ~ expr)*)? ~ ")" }
params        = { identifier ~ ("," ~ identifier)* }

import_symbol_item = { identifier ~ ("as" ~ identifier)? }
import_symbols     = { "*" | (import_symbol_item ~ ("," ~ import_symbol_item)*) }

import_stmt = {
    ("use" ~ import_symbols ~ "from" ~ (string_lit | identifier))
  | ("import" ~ (string_lit | identifier) ~ ("as" ~ identifier)?)
}

export_stmt  = { "export" ~ identifier ~ ("as" ~ (string_lit | identifier))? }
assignment   = { identifier ~ "=" ~ expr }
function_def = { "fn" ~ identifier ~ "(" ~ params? ~ ")" ~ "=" ~ expr }
string_lit   = @{ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }

stmt    = _{ import_stmt | export_stmt | function_def | assignment | expr }
program = _{ SOI ~ stmt* ~ EOI }
```

- [ ] **Step 2: Check pest syntax validity**

Run: `cargo check -p physure-script`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/phs.pest
git commit -m "feat(phs): add phs.pest PEG grammar definition"
```

---

### Task 3: Redesign AST Data Structures (`physure-script/src/ast.rs`)

**Files:**
- Modify: `physure-script/src/ast.rs`

- [ ] **Step 1: Write failing AST test**

In `physure-script/src/ast.rs`, add a test for constructing explicit import AST nodes and quantity AST nodes:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_construction() {
        let import = Statement::Import(ImportNode {
            path: "physics/constants".to_string(),
            specifier: ImportSpecifier::Symbols(vec![
                ImportSymbol { name: "g".to_string(), alias: None }
            ]),
        });
        assert_eq!(matches!(import, Statement::Import(_)), true);
    }
}
```

- [ ] **Step 2: Update `ast.rs` definitions**

Implement `Program`, `Statement`, `ImportNode`, `ImportSpecifier`, `ImportSymbol`, `ExportNode`, `FunctionDefNode`, `AssignmentNode`, `Expr`, `QuantityNode`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Import(ImportNode),
    Export(ExportNode),
    FunctionDef(FunctionDefNode),
    Assignment(AssignmentNode),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportNode {
    pub path: String,
    pub specifier: ImportSpecifier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportSpecifier {
    Wildcard,
    Symbols(Vec<ImportSymbol>),
    ModuleAlias(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportSymbol {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportNode {
    pub symbol: String,
    pub export_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefNode {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentNode {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Quantity(QuantityNode),
    Identifier(String),
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityNode {
    pub magnitude: f64,
    pub uncertainty: Option<f64>,
    pub unit: Option<String>,
}
```

- [ ] **Step 3: Run cargo test**

Run: `cargo test -p physure-script --lib ast::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/ast.rs
git commit -m "feat(phs): update AST nodes following Single Responsibility Principle"
```

---

### Task 4: Implement Pest Parser Visitor (`physure-script/src/parser.rs`)

**Files:**
- Modify: `physure-script/src/parser.rs`

- [ ] **Step 1: Write test for parsing `.phs` code string into AST**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_natural_quantity_and_import() {
        let code = r#"
            use g, c as speed_of_light from "physics/constants"
            fn kinetic_energy(m, v) = 1/2 m v^2
            m = 75.0 ± 0.5 kg
        "#;
        let program = parse_phs(code).unwrap();
        assert_eq!(program.statements.len(), 3);
    }
}
```

- [ ] **Step 2: Implement `pest` Parser struct & `parse_phs` function**

Use `pest_derive::Parser` on `PhsParser` referencing `"phs.pest"`. Convert Pest `Pair<Rule>` into `ast::Program`, `ast::Statement`, and `ast::Expr`.

- [ ] **Step 3: Run parser tests**

Run: `cargo test -p physure-script --lib parser::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/parser.rs
git commit -m "feat(phs): implement pest parser mapping CST to AST"
```

---

### Task 5: Module Resolver & Symbol Table (`physure-script/src/resolver.rs`)

**Files:**
- Create: `physure-script/src/resolver.rs`
- Modify: `physure-script/src/lib.rs`

- [ ] **Step 1: Write failing test for ModuleResolver**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_resolver() {
        let mut resolver = MemoryModuleResolver::new();
        resolver.add_module("math", vec![("pi", 3.14159265)]);
        let export = resolver.resolve_module("math").unwrap();
        assert!(export.symbols.contains_key("pi"));
    }
}
```

- [ ] **Step 2: Implement `ModuleResolver` trait, `FsModuleResolver`, and `SymbolTable`**

- `pub trait ModuleResolver: Send + Sync`
- `pub struct SymbolTable` tracking symbol binding and checking collisions during wildcard or selective imports.

- [ ] **Step 3: Run resolver tests**

Run: `cargo test -p physure-script --lib resolver::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/resolver.rs physure-script/src/lib.rs
git commit -m "feat(phs): add ModuleResolver trait and SymbolTable collision handling"
```

---

### Task 6: Interpreter Updates (`physure-script/src/interpreter.rs`)

**Files:**
- Modify: `physure-script/src/interpreter.rs`

- [ ] **Step 1: Write failing test for evaluating AST with implicit multiplication and physical units**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_kinetic_energy() {
        let code = r#"
            fn kinetic_energy(m, v) = 1/2 m v^2
            m = 10 kg
            v = 2 m/s
            E = kinetic_energy(m, v)
        "#;
        let mut interp = PhsInterpreter::default();
        let res = interp.eval_str(code).unwrap();
        let e_val = res.get("E").unwrap();
        assert_eq!(e_val.magnitude(), 20.0);
    }
}
```

- [ ] **Step 2: Update `PhsInterpreter` to evaluate the redesigned AST**

Implement tree-walk evaluation using `physure-core::measurement::Quantity` and `RationalUnit`.

- [ ] **Step 3: Run interpreter tests**

Run: `cargo test -p physure-script --lib interpreter::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "feat(phs): update interpreter tree-walk evaluation engine for new AST"
```

---

### Task 7: Pluggable Code Generator Trait & Python Transpiler (`physure-script/src/codegen/`)

**Files:**
- Create: `physure-script/src/codegen/mod.rs`
- Create: `physure-script/src/codegen/python.rs`

- [ ] **Step 1: Write failing test for Python transpilation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_transpiler() {
        let code = "m = 75.0 ± 0.5 kg";
        let program = parse_phs(code).unwrap();
        let py_code = PythonTranspiler.generate_program(&program).unwrap();
        assert!(py_code.contains("Q_(75.0, \"kg\", uncertainty=0.5)"));
    }
}
```

- [ ] **Step 2: Implement `CodeGenerator` trait and `PythonTranspiler`**

Emit clean Python code referencing `physure.Q_` and explicit symbol imports.

- [ ] **Step 3: Run codegen tests**

Run: `cargo test -p physure-script --lib codegen::python::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/codegen/
git commit -m "feat(phs): implement CodeGenerator trait and PythonTranspiler target"
```

---

### Task 8: Rust Transpiler Target (`physure-script/src/codegen/rust.rs`)

**Files:**
- Create: `physure-script/src/codegen/rust.rs`

- [ ] **Step 1: Write failing test for Rust transpilation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_transpiler() {
        let code = "fn kinetic_energy(m, v) = 1/2 m v^2";
        let program = parse_phs(code).unwrap();
        let rs_code = RustTranspiler.generate_program(&program).unwrap();
        assert!(rs_code.contains("pub fn kinetic_energy("));
        assert!(rs_code.contains("use physure::measurement::Quantity;"));
    }
}
```

- [ ] **Step 2: Implement `RustTranspiler`**

Emit clean native Rust functions and `physure::measurement::Quantity` assignments.

- [ ] **Step 3: Run test**

Run: `cargo test -p physure-script --lib codegen::rust::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/codegen/rust.rs
git commit -m "feat(phs): implement RustTranspiler target"
```

---

### Task 9: Java Transpiler Target (`physure-script/src/codegen/java.rs`)

**Files:**
- Create: `physure-script/src/codegen/java.rs`

- [ ] **Step 1: Write failing test for Java transpilation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_java_transpiler() {
        let code = "fn kinetic_energy(m, v) = 1/2 m v^2";
        let program = parse_phs(code).unwrap();
        let java_code = JavaTranspiler.generate_program(&program).unwrap();
        assert!(java_code.contains("public static Quantity kineticEnergy("));
    }
}
```

- [ ] **Step 2: Implement `JavaTranspiler`**

Emit clean Java class structures and static method definitions.

- [ ] **Step 3: Run test**

Run: `cargo test -p physure-script --lib codegen::java::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/codegen/java.rs
git commit -m "feat(phs): implement JavaTranspiler target"
```

---

### Task 10: Data Exporter Implementation (`physure-script/src/exporter.rs`)

**Files:**
- Create: `physure-script/src/exporter.rs`

- [ ] **Step 1: Write failing test for JSON/CSV data export**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_export_json() {
        let mut exports = HashMap::new();
        exports.insert("E".to_string(), PhsValue::from_magnitude_and_unit(20.0, "J"));
        let json = Exporter::export_json(&exports).unwrap();
        assert!(json.contains("\"E\""));
        assert!(json.contains("20.0"));
    }
}
```

- [ ] **Step 2: Implement `DataExporter` trait and `Exporter` struct**

- [ ] **Step 3: Run tests**

Run: `cargo test -p physure-script --lib exporter::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/exporter.rs
git commit -m "feat(phs): implement DataExporter for JSON and CSV exports"
```

---

### Task 11: End-to-End Verification Across Cargo and PyO3 Workspace

- [ ] **Step 1: Run full Rust workspace test suite**

Run: `cargo test`
Expected: PASS (all crates green).

- [ ] **Step 2: Run Python integration tests**

Run: `uv run pytest`
Expected: PASS.

- [ ] **Step 3: Commit full feature completion**

```bash
git add .
git commit -m "feat(phs): complete PHS compiler redesign and multi-target transpiler"
```
