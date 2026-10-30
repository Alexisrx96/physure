# Specification: Native Rust PHS Engine & Standalone Binary

**Date**: 2026-07-20  
**Status**: Implemented  

## Context

PHS (Physure Script / MKML) was previously interpreted purely in Python via `physure.ext.grammar`. To achieve maximum execution performance and move towards Python independence, the core lexer, parser, AST, and evaluator have been migrated to pure Rust inside `physure-core`.

## Architecture & Implementation

### 1. `physure-core/src/phs/`
- `lexer.rs`: Tokenizer (`PhsLexer`) for PHS syntax, string literals, operators, and Unicode subscripts/superscripts.
- `ast.rs`: Abstract Syntax Tree (`Expr`, `Statement`, `ParamDef`, `UnaryOp`, `BinaryOp`).
- `parser.rs`: Recursive descent parser matching MKML precedence (`sum < product < implicit multiplication < power < atom`) and statement splitting.
- `value.rs`: `PhsValue` runtime types (`Number`, `Quantity`, `Bool`, `String`, `Vector`).
- `interpreter.rs`: Environment management (`PhsInterpreter`), scope resolution, function definitions, ternary expressions, and `let` bindings.
- `builtins.rs`: Built-in mathematical functions (`sqrt`, `sin`, `cos`, `exp`, `ln`, `abs`).

### 2. Standalone Binary CLI (`physure-core/src/bin/phs.rs`)
- Compiles to native executable binary `phs` via `cargo build --bin phs`.
- Evaluates script files (`.phs`), single CLI expressions (`phs "10 + 20"`), piped input, or interactive REPL.
- Operates with **zero Python runtime dependencies**.

### 3. PyO3 Binding (`physure-python/src/lib.rs`)
- Exposes `evaluate_phs_native` to Python via `physure._core`.
- Marshals native `PhsValue` instances to Python primitives (`float`, `bool`, `str`, `PyQuantity`, `list`).

## Verification

- **Rust Unit Tests**: `cargo test` passes 94/94 tests.
- **Python Integration Tests**: `pytest` passes 989/989 tests including `tests/ext/test_phs_native.py`.
- **Formatting & Quality**: `ruff check .` and `ruff format --check .` clean.
