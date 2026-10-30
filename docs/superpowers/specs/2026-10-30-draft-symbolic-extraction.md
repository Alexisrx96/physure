# Draft: extracting the symbolic engine from `physure-script` (N3)

**Status:** DRAFT, not approved. Source: next step N3 of `2026-10-30-post-a1-inventory.md`. The proposal came from an outside review of the repository; this draft keeps what the code supports and corrects what it does not.

## What the code says

- `physure-script/src/symbolic/` is 14 files and 4210 lines: `ast`, `compiler`, `diff`, `display`, `expr`, `factor`, `integrate`, `mod`, `ode`, `parser`, `series`, `solve`, `sym_matrix`, `transforms` (and `tests`). `physure-core` holds only `equation.rs` on the symbolic side.
- Its dependencies are light: `physure_core::error`, `physure_core::units::RationalUnit`, `num-rational`, `num-traits`.
- Its coupling to the rest of `physure-script` is small:
  - `symbolic/parser.rs` uses the PHS lexer (`PhsLexer`, `PhsToken`, `TokenKind`); it does not use pest.
  - `symbolic/tests.rs` calls the interpreter in one test.
  - Inside the crate, `builtins/calc.rs`, `codegen/mod.rs`, `interpreter/binary_ops.rs`, `interpreter/expressions.rs` and `value.rs` use it (`PhsValue` holds an `Expr`).
- **It is already exposed to every binding.** `physure-python`, `physure-java`, `physure-wasm`, `physure-cli` and `physure-lsp` all depend on `physure-script`. Python exposes `physure._core.Expr`, a wrapper of `physure_script::symbolic::Expr`; Java and WASM offer `deriv`, `integral` and `solve`.

So the premise "symbolic math cannot reach the bindings without the script's parser" does not hold. What extraction would buy is the dependency weight of bindings that want symbolic and not the interpreter: build time, the size of the WASM bundle, and a cleaner picture of the crate graph.

## Options

| Option | Move | For | Against |
|---|---|---|---|
| A | Nothing | no work | the crate graph keeps `physure-script` as a general dependency |
| B | `symbolic/` into `physure-core` | core stays free of FFI (the rule is about FFI, not size) | the core grows by 4210 lines and two numeric crates; `parser.rs` needs the PHS lexer, so it stays in the script crate and the module is split |
| C | A new crate `physure-symbolic` between core and script; `parser.rs` stays in `physure-script` as the PHS front end | the clearest boundary; bindings can depend on it alone | an eighth workspace member: `structure.md`, the `PostToolUse` hook on `Cargo.toml`, the release workflows, the version policy, and docs |

## Recommendation

**Measure before deciding.** Build `physure-wasm` and the Python extension with and without the interpreter and pest in the dependency tree (a throwaway branch that stubs what the bindings use from `physure-script`) and record the WASM bundle size and clean-build time. If the saving is below a threshold the maintainer sets, choose A. If above it, choose C: it is the only option that lets a binding drop the interpreter.

## Open questions

- What saving justifies an eighth crate? (An example threshold to react to: a WASM bundle smaller by a fifth, or a clean build faster by a fifth.)
- Do the bindings want symbolic without the interpreter at all? `physure-java` and `physure-wasm` expose `evaluate` of PHS, which needs the interpreter anyway.
- Would the crate be versioned with the core (one release) or on its own (a fifth release flow beside Python, core, Java, WASM)?

## Invariants to keep

`physure-core` has no FFI dependency, and PHS has no fallback (`CLAUDE.md`). Neither is endangered by B or C.

## Acceptance

- The measurement above, written into this file or its successor.
- If C: every crate that used `physure_script::symbolic` compiles against the new path, `cargo test --workspace` keeps its 767 tests, and the Python suite is unchanged.

## Size and dependencies

Measurement S; the move M; risk medium for C (release and documentation machinery). Independent of N1, N2, N4.
