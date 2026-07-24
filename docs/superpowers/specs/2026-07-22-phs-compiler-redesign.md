# PHS DSL Compiler & Multi-Target Transpiler Specification

> **Date:** 2026-07-22  
> **Status:** Approved  
> **Target Package:** `physure-script` (Rust crate in `physure` workspace)

---

## 1. Executive Summary & Purpose

The `.phs` (PhysureScript) Domain-Specific Language is designed for scientists and engineers to write, prototype, and validate physical and mathematical formulas using natural textbook notation, strict dimensional analysis, and first-class physical quantity handling with uncertainties.

To eliminate friction between domain experts and software developers, the `.phs` language architecture provides a **dual execution model**:
1. **Interactive Interpreter (Tree-Walk Engine)**: Fast, immediate formula validation with physical dimension checking and error propagation.
2. **Pluggable Transpiler Engine**: Emits production-ready, idiomatic code in **Python**, **Rust**, **Java**, and future target languages (C++, Julia, TypeScript).

---

## 2. Architectural Design & SOLID/DRY Invariants

The design strictly enforces SOLID principles and clean code invariants:

* **Single Responsibility Principle (SRP)**:
  * AST nodes (`ast.rs`) represent pure syntax data structures.
  * Grammar & Lexer (`phs.pest`) handle parsing exclusively.
  * Symbol resolution (`resolver.rs`) manages imports, scopes, and symbol tables.
  * Each target transpiler (`python_transpiler.rs`, `rust_transpiler.rs`, `java_transpiler.rs`) is isolated in its own code generator unit.
  * Data exporters (`exporter.rs`) specialize in serializing values to JSON, CSV, or Arrow buffers.
* **Open-Closed Principle (OCP)**:
  * New transpilation targets (e.g., C++, TypeScript) are added by implementing `trait CodeGenerator` without modifying existing parser, interpreter, or AST code.
* **Dependency Inversion Principle (DIP)**:
  * Module loading relies on `trait ModuleResolver`, enabling virtual modules, stdlib lookup, filesystem paths, and PyO3/C plugin registries.

---

## 3. Grammar Specification (`phs.pest`)

The parser uses PEG-based `pest` definitions supporting natural writing (implicit multiplication, Greek symbols, flexible uncertainty notation, single-line function expressions):

```pest
WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
COMMENT    = _{ "//" ~ (!"\n" ~ ANY)* ~ "\n" | "/*" ~ (!"*/" ~ ANY)* ~ "*/" }

// Identifiers, Constants & Greek symbols
identifier  = @{ (ASCII_ALPHA | "π" | "θ" | "λ" | "μ" | "Δ" | "σ" | "Ω" | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
number      = @{ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? ~ ( ^"e" ~ ("+" | "-")? ~ ASCII_DIGIT+ )? }

// Natural Uncertainty
uncertainty_op  = { "+/-" | "±" }
uncertainty_val = { number ~ "%"? }
uncertainty     = { uncertainty_op ~ uncertainty_val }

// Physical Units
unit_term       = @{ ASCII_ALPHA+ ~ ("^" ~ ("+" | "-")? ~ ASCII_DIGIT+)? }
unit_primary    = _{ unit_term | "(" ~ unit_expr ~ ")" }
unit_expr       = { unit_primary ~ (("*" | "/" | WHITESPACE+) ~ unit_primary)* }

// Quantity Literals
quantity = {
    ("(" ~ expr ~ uncertainty? ~ ")" ~ unit_expr?)
  | (number ~ uncertainty? ~ unit_expr?)
}

// Operators
op_add = { "+" }
op_sub = { "-" }
op_mul = { "*" }
op_div = { "/" }
op_pow = { "^" }

// Expression Hierarchy with Implicit Multiplication (e.g., 2 pi r, m a)
primary = _{ quantity | function_call | identifier | "(" ~ expr ~ ")" }
factor  = { primary ~ (op_pow ~ primary)? }
term    = { factor ~ ((op_mul | op_div) ~ factor | factor)* }
expr    = { term ~ ((op_add | op_sub) ~ term)* }

// Function Calls & Parameter Lists
function_call = { identifier ~ "(" ~ (expr ~ ("," ~ expr)*)? ~ ")" }
params        = { identifier ~ ("," ~ identifier)* }

// Statements & Import Syntax
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

---

## 4. AST & Symbol Resolution (`ast.rs`, `resolver.rs`)

### AST Structure

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Import(ImportNode),
    Export(ExportNode),
    FunctionDef(FunctionDefNode),
    Assignment(AssignmentNode),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportNode {
    pub path: String,
    pub specifier: ImportSpecifier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSpecifier {
    Wildcard,
    Symbols(Vec<ImportSymbol>),
    ModuleAlias(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportSymbol {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Quantity(QuantityNode),
    Identifier(String),
    BinaryOp {
        op: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}
```

### ModuleResolver & Symbol Table

```rust
pub trait ModuleResolver: Send + Sync {
    fn resolve_module(&self, path: &str) -> Result<ModuleExport, ModuleResolutionError>;
}

pub struct SymbolTable {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

pub struct SymbolInfo {
    pub name: String,
    pub is_function: bool,
    pub source_module: Option<String>,
}
```

---

## 5. Dual Execution Engine (Interpreter + Transpilers)

### Interpreter (`interpreter.rs`)
* Evaluates AST trees against an environment store.
* Wraps `physure::Quantity` (magnitude, `RationalUnit`, and `Uncertainty`).
* Validates physical dimensions at evaluation time and raises structured errors with source locations.

### Code Generator Trait & Implementations (`codegen/`)

```rust
pub trait CodeGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError>;
}
```

#### Targets:
1. **`PythonTranspiler`**: Generates Python code invoking `physure` (`Q_`, `uncertainty`, `constants`).
2. **`RustTranspiler`**: Generates zero-dependency native Rust code using `physure`.
3. **`JavaTranspiler`**: Generates Java class structures using `com.physure.Quantity`.

---

## 6. Data Export (`exporter.rs`)

Supports exporting evaluated symbols defined via `export` statements into:
* **JSON** (`export_json`)
* **CSV** (`export_csv`)
* **Python Dictionary / NumPy zero-copy memory buffers** (`export_py_dict`)

---

## 7. Verification & Testing Strategy

* **Grammar & Lexer Unit Tests**: Test parsing of implicit multiplication, Greek letters, physical units, and uncertainties.
* **Import Collision Tests**: Verify symbol resolution correctly handles wildcard imports, explicit symbol lists, and shadowing warnings.
* **Interpreter Integration Tests**: Evaluate `.phs` scripts against expected physical unit magnitudes and propagated uncertainties.
* **Codegen Tests**: Verify generated Python, Rust, and Java output compiles or executes accurately against reference formulas.
