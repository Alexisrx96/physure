# physure-script: split parser.rs into focused modules

## Context

Following the `builtins.rs`/`interpreter.rs` split (see
`docs/superpowers/specs/2026-08-25-physure-script-modularization-design.md` and the plan at
`docs/superpowers/plans/2026-08-25-physure-script-modularization.md`, both already
implemented and merged into this branch), `physure-script/src/parser.rs` is now the largest
file in the crate at 1683 lines — larger than any single file produced by the earlier split.

This is a pure reorganization — no behavior change, no public API change. The full
`physure-script` test suite (279 tests: 219 lib + 60 across five integration test files) must
pass unchanged, with zero test assertions modified. `physure-cli` and `physure-lsp`, which
depend on `physure_script::parser::parse_phs` directly, must also keep passing unchanged.

## Why now

Unlike `builtins.rs`/`interpreter.rs`, which needed a call-site investigation to find their
real seams, `parser.rs`'s internal structure was already grep-verified to split cleanly along
five responsibilities with **zero shared private helpers across the boundaries** — every
candidate "could this be needed by two siblings" helper (`unit_registry`,
`unit_term_is_registered`, `unit_expr_all_valid`, `split_unit_expr`, `parse_unit_leftover`,
`parse_uncertainty_val`, `as_bare_identifier`, `unit_chain_tokens`) is called only from within
its own group. This makes the split lower-risk than the earlier one, which needed a
`helpers.rs`/shared-cluster file in both `builtins/` and `interpreter/`.

## Target layout

```
parser/
  mod.rs          — PhsParser struct, parse_phs, parse_phs_with_lines (the public entry
                    points; unchanged external path — physure_script::parser::parse_phs is
                    used directly by physure-lsp and physure-cli)
  statements.rs   — parse_statement, parse_while_stmt, parse_import, parse_export,
                    parse_function_def, parse_assignment, parse_decorated_stmt,
                    parse_documented_stmt, parse_decorator, parse_guard_if_stmt,
                    parse_return_stmt
  expressions.rs  — parse_expr, parse_base_expr, parse_conv_expr, parse_comp_expr,
                    as_bare_identifier, parse_term, parse_factor, parse_if_expr,
                    parse_for_expr, parse_where_expr, parse_vector_literal,
                    parse_number_quantity, parse_function_call, parse_method_call
  quantities.rs   — unit_registry, unit_term_is_registered, unit_expr_all_valid,
                    split_unit_expr, parse_unit_leftover, parse_uncertainty_val,
                    parse_quantity
  shadowing.rs    — UnitToken, unit_chain_tokens, unit_shadowing_error,
                    validate_unit_shadowing, check_statement_shadowing,
                    check_expr_shadowing
  tests.rs        — the existing `#[cfg(test)] mod tests` content, consolidated (same
                    precedent as `symbolic/tests.rs`, `builtins/tests.rs`,
                    `interpreter/tests.rs`)
```

`shadowing.rs` is kept separate from `quantities.rs` rather than merged, even though every
shadowing check is about quantity/unit ambiguity: it's a semantic validation *pass* over an
already-parsed AST (`&[Statement]` in, `PhysureResult<()>` out), not part of the grammar-to-AST
parsing `quantities.rs` does. It's also the exact code this branch's earlier work (the `x y`
and `1/2 m v^2` silent-ambiguity fixes, commit `53a89be`) touched — giving it its own file
makes it easy to find next time.

## Expected cross-file calls (not a placement problem)

Two of the new siblings call each other, which is normal for a recursive-descent parser and
not the "shared helper" pattern the earlier split had to solve for:

- `expressions.rs`'s `parse_factor`/`parse_method_call` call `quantities.rs::parse_quantity`
  (a quantity literal can appear anywhere an expression can).
- `quantities.rs`'s `parse_quantity`/`parse_unit_leftover` call back into
  `expressions.rs::parse_expr` (a quantity's parenthesized form, `(expr) unit`, and a unit
  conversion's remainder both need general expression parsing).

Both directions resolve with `use super::expressions::parse_expr;` /
`use super::quantities::parse_quantity;` — ordinary sibling-module references, no circular
*compilation* issue since Rust resolves all items in a crate together.

`statements.rs` calls into `expressions.rs::parse_expr` (a statement's value is an
expression) but nothing calls back from `expressions.rs` into `statements.rs` — one-directional.

`shadowing.rs`'s `validate_unit_shadowing` is called once, from `parser/mod.rs`'s
`parse_phs`/`parse_phs_with_lines` (after the AST is built, as a post-parse pass) — not from
any of the other four siblings.

## Docs to update

Two files hold `parser.rs#L<line>` GitHub anchor links:

- `docs/language_readiness_roadmap.md` (1 link, to the comparison-operator desugaring point
  cited as context for Track F decorator transpilation)
- `docs/superpowers/plans/2026-08-14-track-d-incremental-lsp.md` (1 link, to
  `parse_phs_with_lines`)

Same treatment as the earlier split's Task 14: update both anchors to the new file + line once
the split lands and line numbers are known, verified against the surrounding prose (not just
"a link exists" — confirm the cited line actually lands on the function/statement described).

## Verification

1. `cargo test -p physure-script` and `cargo test -p physure-cli -p physure-lsp` must show
   the same pass counts as today (279 total for physure-script; unchanged counts for the
   other two), with no test assertions modified — a pure move, not a rewrite.
2. `cargo build --workspace` to confirm no downstream crate broke on a
   `use physure_script::parser::...` path.
3. Rebuild the Python native extension (`cd physure-python && maturin develop`) and run
   `uv run pytest tests/test_transpiler_parity.py -q` — the same end-to-end check the earlier
   split's Task 15 used, since it's the only check that actually executes `.phs` scripts
   through the rebuilt engine.
4. Grep the whole repo for `parser\.rs#L` after the split to catch any anchor the docs sweep
   missed.

## Explicitly out of scope

- Any change to the grammar (`phs.pest`), to any function's logic, control flow, or error
  messages — every move is verbatim.
- Any change to `parse_phs`/`parse_phs_with_lines`/`PhsParser`'s signatures or the
  `physure_script::parser::` path external crates already depend on.
- `lexer.rs` (284 lines) and `resolver.rs` (276 lines) — both already reasonably sized, not
  part of this pass.
