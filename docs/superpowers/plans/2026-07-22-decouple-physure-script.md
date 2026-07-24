# Decouple Physure-Core, Physure-Script, and Physure-CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-architect the Cargo workspace into modular crates: `physure-core` (pure physical quantities & math engine), `physure-script` (PHS DSL parser, AST, interpreter, codegen transpilers), and `physure-cli` (CLI binary tool `phs`).

**Architecture:**
- `physure-core`: Single source of truth for physical quantities, dimensional vectors, uncertainty propagation, and unit registry.
- `physure-script`: Language engine containing `phs`, `ast`, `interpreter`, `builtins`, `function`, and `codegen` transpiler modules. Depends on `physure-core`.
- `physure-cli`: Command-line tool `phs` with runner, REPL, and transpiler subcommands. Depends on `physure-script` and `physure-core`.

**Tech Stack:** Rust (`physure-core`, `physure-script`, `physure-cli`), Cargo Workspace.

---

### Task 1: Create `physure-script` Crate Structure

**Files:**
- Create: `D:\Projects\physure\physure-script\Cargo.toml`
- Create: `D:\Projects\physure\physure-script\src\lib.rs`
- Move PHS modules (`phs/`, `codegen/`) into `physure-script/src/`

- [ ] **Step 1: Create `physure-script/Cargo.toml`**

```toml
[package]
name        = "physure-script"
description = "PhysureScript (PHS) language engine: Lexer, Parser, AST, Interpreter, and Codegen."
version.workspace    = true
edition.workspace    = true
license.workspace    = true
repository.workspace = true
homepage.workspace   = true
authors.workspace    = true

[lib]
name       = "physure_script"
crate-type = ["rlib"]

[dependencies]
physure-core = { path = "../physure-core" }
num-rational.workspace = true
num-traits.workspace   = true
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
```

- [ ] **Step 2: Create `physure-script/src/lib.rs`**

```rust
pub mod ast;
pub mod builtins;
pub mod codegen;
pub mod function;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod value;

pub use codegen::{transpile, Target};
pub use function::PhyFunction;
pub use interpreter::PhsInterpreter;
pub use lexer::{PhsLexer, PhsToken, TokenKind};
pub use parser::parse_phs;
pub use value::PhsValue;
```

---

### Task 2: Create `physure-cli` Crate Structure

**Files:**
- Create: `D:\Projects\physure\physure-cli\Cargo.toml`
- Create: `D:\Projects\physure\physure-cli\src\main.rs`

- [ ] **Step 1: Create `physure-cli/Cargo.toml`**

```toml
[package]
name        = "physure-cli"
description = "Command line interface for PhysureScript (.phs)."
version.workspace    = true
edition.workspace    = true
license.workspace    = true
repository.workspace = true
homepage.workspace   = true
authors.workspace    = true

[[bin]]
name = "phs"
path = "src/main.rs"

[dependencies]
physure-core   = { path = "../physure-core" }
physure-script = { path = "../physure-script" }
```

- [ ] **Step 2: Implement CLI binary in `physure-cli/src/main.rs`**

```rust
use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, Target};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("PhysureScript (PHS) CLI");
        eprintln!("Usage: phs <script.phs>");
        eprintln!("       phs transpile <script.phs> --target <rust|python|java>");
        process::exit(1);
    }

    if args[1] == "transpile" && args.len() >= 3 {
        let file_path = &args[2];
        let target_str = if args.len() >= 5 && args[3] == "--target" { &args[4] } else { "rust" };
        let target = match target_str.to_lowercase().as_str() {
            "python" | "py" => Target::Python,
            "java" => Target::Java,
            _ => Target::Rust,
        };
        let code = fs::read_to_string(file_path).expect("Failed to read script file");
        let result = transpile(target, &code).expect("Transpilation failed");
        println!("{}", result);
        return;
    }

    let file_path = &args[1];
    let code = fs::read_to_string(file_path).expect("Failed to read script file");
    let stmts = parse_phs(&code).expect("Failed to parse script");
    let mut interp = PhsInterpreter::new();
    for stmt in stmts {
        if let Err(e) = interp.run_statement(&stmt) {
            eprintln!("Error executing statement: {:?}", e);
            process::exit(1);
        }
    }
}
```

---

### Task 3: Update Root `Cargo.toml` and Workspace Members

**Files:**
- Modify: `D:\Projects\physure\Cargo.toml`
- Modify: `D:\Projects\physure\physure-core\Cargo.toml`

- [ ] **Step 1: Update workspace members in root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members  = [
    "physure-core",
    "physure-script",
    "physure-cli",
    "physure-python",
    "physure-lsp",
    "physure-java",
]
```

- [ ] **Step 2: Run `cargo check --workspace` to verify workspace compilation.**

---

### Task 4: Run Test Battery & Verification

- [ ] **Step 1: Run `cargo test --workspace`**
- [ ] **Step 2: Verify `phrust.bat`, `phython.bat`, `phava21.bat`, `phava8.bat` all pass 100% cleanly.**
