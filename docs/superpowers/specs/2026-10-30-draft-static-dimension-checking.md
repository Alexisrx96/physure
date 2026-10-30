# Draft: static dimensional checking of PHS (N2)

**Status:** DRAFT, not approved. Source: next step N2 of `2026-10-30-post-a1-inventory.md`.

## Where things stand

- The language server (`physure-lsp/src/main.rs`) answers hover requests from a fixed table of a few units (`m`, `kg`, `s`, `N`, ...). It reports no dimensional diagnostics. `physure-script/src/resolver.rs` resolves names and has no unit logic.
- PHS evaluates dynamically: `m + s` is an error only when the line runs.
- The grammar already lets a parameter carry a unit: `param_item = { identifier ~ (":" ~ unit_expr)? }` in `physure-script/src/phs.pest`.
- The core now has what a checker needs from units: `UnitRegistry::facts`, `dimension_of`, `knows`, `conversion`, exact rational powers, points and intervals for temperatures, levels.
- `physure-script/src/codegen/rust.rs` emits code over `physure_core::Quantity` (parameters typed `Quantity`), so generated Rust still pays for units at run time.
- The repository holds one `.phs` file outside tests (a tutorial under `docs/tutorials`), so a corpus of real programs for false positives does not exist yet; the Rust and Python tests carry PHS in strings.

## Goal

Tell the author, at edit time and without running the script, that a sum, a comparison or a conversion mixes incompatible dimensions. In later tiers, remove the unit cost from generated code.

## Tiers

| Tier | What is checked | Needs |
|---|---|---|
| T1 | quantity literals, assignments, `+`, `-`, comparisons, `=> unit`, powers and products, with dimensions known from literals | a pass over the AST with a dimension per expression; the registry from the active conf |
| T2 | function calls whose parameters are annotated (`fn f(m: kg, v: m/s)`) and their return dimension | signatures in the resolver |
| T3 | unannotated parameters | either a declared policy ("unknown dimension: never an error") or inference by specialising each function at its call sites |
| T4 | generated Rust and JS without units at run time, after T1 to T3 proved the program | a typed IR; the codegen emitting `f64` and the exact scale factors |

## Design sketch (T1, T2)

- A `check` module in `physure-script` that yields diagnostics `{span, kind, message}` and has no dependency on the interpreter (the symbolic module shows how little coupling is needed).
- It builds a registry the way the interpreter does (embedded catalog plus conf settings), so the checker and the interpreter cannot disagree about what a unit is.
- `physure-lsp` calls it on each change (the server is already incremental) and turns diagnostics into LSP ones. `physure-cli` gets `phs check <file> [--json]`, which N5 reuses.
- Soundness policy: an error only when both sides are known and incompatible; unknown stays silent.

## Open questions

- Unknown unit or unknown name: warning, error, or silent?
- Unannotated parameters (T3): is "silent" acceptable for the first release, with inference later?
- Affine units: `degC + K` is a point plus an interval (allowed), `degC + degC` is not. The checker has to carry the point or interval kind, not only the dimension.
- Units introduced by a conf file or a plugin: does the checker see the same conf as the runtime?
- Dynamic corners (native plugins, `eval`-like builtins): excluded from the check by a documented rule?

## Acceptance

- A test file of PHS programs that the interpreter accepts produces no diagnostic (no false positives), and a file of programs the interpreter rejects with a dimensional error produces one at the same span.
- The language server answers a keystroke in under a few milliseconds on the sample programs (to be measured, no target is claimed here).
- T4 is judged by the benchmark of generated code against `Quantity`-based code, not by assertion.

## Size and dependencies

Size L, risk medium (a checker that cries wolf is switched off). Builds on A1; N5 benefits from it; independent of N3 and N4.
