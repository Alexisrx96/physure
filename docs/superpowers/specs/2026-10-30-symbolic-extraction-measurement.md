# Extracting the symbolic engine (N3) — measurement and recommendation

**Date:** 2026-10-30
**Status:** recommendation **A, do nothing**; the maintainer confirms or sets a threshold (below).
**Base:** local `main` @ `eea89d6`
**Follows:** the draft `2026-10-30-draft-symbolic-extraction.md` (next step N3 of `2026-10-30-post-a1-inventory.md`), whose recommendation was "measure before deciding".

## The question the draft left open

Would a binding get lighter if `physure-script/src/symbolic/` moved out of the script crate (options B and C of the draft)? That depends on one thing: **does any binding use symbolic math without also running the PHS interpreter?** If none does, no binding can drop the interpreter and pest, and the bundle-size and build-time measurement the draft proposed cannot find a saving.

## What the code says

| Consumer | How it reaches symbolic math | Needs the interpreter anyway |
|---|---|---|
| `physure-wasm` | `deriv`, `integral` and `solve` build a PHS call and run `self.evaluate(...)` (`physure-wasm/src/registry.rs:154-163`); `phy_function.rs` also uses `PhsInterpreter` | yes |
| `physure-java` | `UnitRegistry.java:132,142,152` call `evaluateRaw("solve(...)")`, `deriv(...)`, `integral(...)`; the native side is `physure_script::{parse_phs, PhsInterpreter}` (`physure-java/src/lib.rs:5`) | yes |
| `physure-python` | `Expr` wraps `symbolic::Expr` directly (`lib.rs:38`), but the same extension exposes `PhsInterpreter` (`lib.rs:1314`), `eval_phs` and `transpile` | yes |
| `physure-cli` | pipeline, codegen, `check` | yes |
| `physure-lsp` | parser, interpreter, `check` | yes |
| `physure-core` | nothing from the script | no symbolic use |

Every binding that offers symbolic math offers PHS evaluation too, and most of them offer it *through* PHS evaluation.

## What a split would remove, and from whom

- Crates the script adds over the core (`cargo tree -e normal`, unique crates: core 129, script 139): `pest`, `pest_derive`, `pest_generator`, `pest_meta`, `ucd-trie`, plus `libloading` and `windows-link` on native targets.
- Source: the script crate is 19,585 lines, `symbolic/` is 4,210. A symbolic-only dependency would drop the other 15,375 (interpreter, parser, codegen, builtins, checker).

Both are removed only for a consumer that does not run PHS. There is none in this repository. Option C would therefore add an eighth workspace member (`structure.md`, the `Cargo.toml` hook, release workflows and versioning, docs) for a saving no current binding collects. Option B would move 4,210 lines and two numeric crates into the core for the same absent benefit.

## Not measured, on purpose

The draft's build of `physure-wasm` and the Python extension with and without the interpreter was **not run**: since no binding can drop the interpreter, its result cannot change the decision, and a clean build here costs a lot of compute. (The only `.wasm` on disk, `target/wasm32-unknown-unknown/release/physure_wasm.wasm`, is 2.4 MB from 2026-08-11 and predates the current tree, so it is not a baseline.)

## Recommendation

**A: leave `symbolic/` where it is.** Revisit if either becomes true:

- a binding or product appears that needs symbolic math and not PHS (then measure the wasm bundle and a clean build for that consumer, with the threshold the maintainer sets: the draft's example was a fifth), or
- a third party asks for the symbolic engine as a Rust crate.

If that happens the move is already cheap to describe: `symbolic/parser.rs` is the only file that uses the PHS lexer, and the rest of the module uses `physure_core::error`, `physure_core::units::RationalUnit`, `num-rational` and `num-traits`.

## A correction to the draft

The draft says "`physure-core` holds only `equation.rs` on the symbolic side". That file is **not compiled**: `physure-core/src/lib.rs` has no `mod equation`, nothing uses `PhyEquation`, and the file calls `physure_script::eval_phs`, which the core cannot depend on (the script depends on the core). It was added by `80ed1c2` and never wired. Compiled as it stands it would not build; it is dead code that contradicts the layering (the core sits below the script and cannot name it). Deleting it is a separate one-file change and is not part of this recommendation.
