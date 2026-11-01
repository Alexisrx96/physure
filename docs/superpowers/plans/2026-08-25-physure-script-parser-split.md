# physure-script: split parser.rs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Design spec:** `docs/superpowers/specs/2026-08-25-physure-script-parser-split-design.md` — read it first; this plan implements it exactly.

**Goal:** Split `physure-script/src/parser.rs` (1683 lines, the largest file in the crate) into `parser/{mod,statements,expressions,quantities,shadowing,tests}.rs`, following the same pattern already used for `builtins/` and `interpreter/` in this crate.

**Architecture:** `parser/mod.rs` keeps the public entry points (`PhsParser`, `parse_phs`, `parse_phs_with_lines`) exactly where external callers (`physure-lsp`, `physure-cli`) already reach them. Four siblings hold one parsing responsibility each. Unlike the earlier `builtins`/`interpreter` split, this one needs **no shared-helper file** — every candidate helper was grep-verified to have exactly one consumer group (see the design spec's "Why now" section). Every move is verbatim — no function body changes, no renamed identifiers, no behavior change.

**Tech Stack:** Rust (`physure-script` crate only).

**Deviation from the usual TDD step shape:** this plan moves existing, already-tested code — there is no new behavior to test-drive. Each task's "test" step is running the existing suite unchanged and confirming the same pass count as before the task. Every task must leave the crate compiling and the full suite green before its commit.

**Line ranges cited below are against `physure-script/src/parser.rs` as it stands after the `builtins`/`interpreter` split and the `function.rs`/`mod.rs` dead-code removal (commit `2d4034a` on branch `worktree-phs-modularization`).** If a prior task in this plan already moved code, later line-range citations for files that still exist refer to their state *after* the prior task's move — confirm with `grep -n "^pub fn \|^fn \|^struct \|^pub struct "` before cutting, don't trust a stale line number blindly.

**Ordering note:** `expressions.rs` (Task 3) and `quantities.rs` (Task 4) call each other (a quantity literal can appear inside an expression; a parenthesized quantity or a unit-conversion remainder needs general expression parsing — see the design spec). Extracting them in this order means Task 3 leaves a `super::parse_quantity(...)` reference that Task 4 has to update to `super::quantities::parse_quantity(...)` as part of moving `parse_quantity` out from under it — an expected, compiler-driven one-line fix, not a sign anything is wrong.

---

**Reconciliation (2026-08-26):** This plan was executed directly on `phs/correctness-and-readability`
by a separate/parallel process rather than through this file's own checkbox tracking, so the boxes
below were unchecked despite the work being done. Verified after the fact, task by task:

- Commits exist in this exact order with these exact messages: `66823d0` (Task 1), `e4cfe08`
  (Task 2), `945588e` (Task 3), `29339bd` (Task 4), `d526527` (Task 5), `57b6402` (Task 7).
  Task 6 has no commit, which is correct — it's a checkpoint task, and `parser/mod.rs`'s shape
  already matched by the time Task 6 would have run, so Step 3 ("only if Step 1 required a fix")
  legitimately did nothing.
- Every function named in Tasks 2-5's move lists lives in the file and under the visibility
  (`pub(crate)` vs private) the plan specifies — verified by grepping each file's top-level
  `fn`/`struct` signatures against the plan's lists directly, not by trusting the commit messages.
- One real gap **in this plan**, not in its execution: Task 4's move list for `quantities.rs`
  omits `is_known_unit_symbol` (it sat at line 783 of the pre-split baseline, right inside the
  cited range). It has an external consumer (`interpreter/expressions.rs`) that reaches it as
  `crate::parser::is_known_unit_symbol`, so moving it into `quantities.rs` needed a
  `pub(crate) use quantities::is_known_unit_symbol;` re-export in `mod.rs` that Task 4/6 never
  mention. The actual execution got this right anyway — function in the right file, re-export in
  place, external call site untouched — so no code changed here; noted for anyone reading this
  plan as a record of what happened, since the plan text alone would not explain that re-export.
- Task 8's doc anchors are repointed and correct: `language_readiness_roadmap.md:418` →
  `parser/statements.rs:284` (`parse_guard_if_stmt`, matching the plan's stated target), and the
  track-d plan's anchor → `parser/mod.rs:37` (`parse_phs_with_lines`). `grep -rn "parser\.rs#L"
  docs/` finds no remaining stale anchor outside this plan/its design spec's own prose.
- Task 9 re-verified fresh: `cargo test -p physure-script` all-green across every binary (lib
  count is higher than the plan's 219 baseline because later, unrelated work on this branch added
  tests on top of this split — not a discrepancy); `cargo build --workspace` clean, including
  `physure-wasm`/`physure-java` which hadn't been rebuilt earlier in the session; `cd
  physure-python && uv run mkdocs build --strict` exits 0.

No further action needed — the split matches its own design and plan in substance, the one
plan-text gap didn't become a real bug, and the boxes below are now checked to match reality.

---

## Task 1: `parser.rs` → `parser/mod.rs` (pure rename, zero content change)

**Files:**
- Rename: `physure-script/src/parser.rs` → `physure-script/src/parser/mod.rs`

- [x] **Step 1: Rename with git**

```bash
cd physure-script/src
mkdir -p parser
git mv parser.rs parser/mod.rs
```

(Task 1 of the earlier `builtins`/`interpreter` split found that plain `git mv` fails with "No such file or directory" on this Windows checkout unless the destination directory already exists — `mkdir -p` first avoids that.)

- [x] **Step 2: Build and run the full suite**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo build -p physure-script
cargo test -p physure-script
```

Expected: builds clean, `test result: ok. 219 passed` for the lib target and unchanged counts
for every integration test binary (7 `mathematical_correctness`, 4 `transpile_parity_tests`,
30 `unit_literal_regression`, 16 `unit_shadowing`, 3 `lsp_never_panics`, 0 doctests — 279
total).

- [x] **Step 3: Commit**

```bash
git add physure-script/src/parser.rs physure-script/src/parser/mod.rs
git commit -m "refactor(phs): parser.rs -> parser/mod.rs (pure rename, no content change)"
```

---

## Task 2: Extract `parser/statements.rs`

**Files:**
- Modify: `physure-script/src/parser/mod.rs`
- Create: `physure-script/src/parser/statements.rs`

- [x] **Step 1: Create `statements.rs`**

Confirm current line numbers with `grep -n "^fn parse_statement\|^fn parse_while_stmt\|^fn parse_import\|^fn parse_export\|^fn parse_function_def\|^fn parse_assignment\|^fn parse_decorated_stmt\|^fn parse_documented_stmt\|^fn parse_decorator\|^fn parse_guard_if_stmt\|^fn parse_return_stmt" physure-script/src/parser/mod.rs`, then cut, in this order, into a new `physure-script/src/parser/statements.rs`:

- `fn parse_statement` (originally lines 57-74)
- `fn parse_while_stmt` (originally lines 75-95)
- `fn parse_import` (originally lines 96-154)
- `fn parse_export` (originally lines 155-174)
- `fn parse_function_def` (originally lines 175-236)
- `fn parse_assignment` (originally lines 237-259)
- `fn parse_decorated_stmt` (originally lines 260-297)
- `fn parse_documented_stmt` (originally lines 298-324)
- `fn parse_decorator` (originally lines 325-334)
- `fn parse_guard_if_stmt` (originally lines 335-342)
- `fn parse_return_stmt` (originally lines 343-346)

With this `use` block:

```rust
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;
use super::Rule;
```

`parse_statement` and several others call `parse_expr`, which stays in `mod.rs` until Task 3 —
reference it as `super::parse_expr(...)` at each call site inside `statements.rs` for now.
Mark `parse_statement` `pub(crate)` (it's the one function this group needs reachable from
`mod.rs`'s `parse_phs`/`parse_phs_with_lines`); leave the rest private `fn` (each is called
only from within this same group, per the earlier grep audit).

- [x] **Step 2: Wire `statements.rs` into `mod.rs`**

```rust
pub(crate) mod statements;
```

Update `parse_phs`/`parse_phs_with_lines`'s calls to `parse_statement(...)` to
`statements::parse_statement(...)`.

- [x] **Step 3: Build and fix unused-import / missing-import issues**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo build -p physure-script 2>&1 | grep -A2 "warning: unused import"
```

Fix anything named. Also fix any outright compile errors the same way — a genuinely missing
import shows as `error[E0433]` or similar "cannot find"; add whatever the compiler names.

- [x] **Step 4: Run the full suite**

```bash
cargo test -p physure-script
```

Expected: same 219/279 as Task 1.

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser/mod.rs physure-script/src/parser/statements.rs
git commit -m "refactor(phs): extract parser/statements.rs"
```

---

## Task 3: Extract `parser/expressions.rs`

**Files:**
- Modify: `physure-script/src/parser/mod.rs`
- Modify: `physure-script/src/parser/statements.rs`
- Create: `physure-script/src/parser/expressions.rs`

- [x] **Step 1: Create `expressions.rs`**

Confirm current line numbers, then cut, in this order, into a new
`physure-script/src/parser/expressions.rs`:

- `fn parse_expr` (originally lines 347-390)
- `fn parse_base_expr` (originally lines 391-422)
- `fn parse_conv_expr` (originally lines 423-446)
- `fn parse_comp_expr` (originally lines 447-477)
- `fn as_bare_identifier` (originally lines 478-484)
- `fn parse_term` (originally lines 485-548)
- `fn parse_factor` (originally lines 549-600)
- `fn parse_if_expr` (originally lines 601-612)
- `fn parse_for_expr` (originally lines 613-630)
- `fn parse_where_expr` (originally lines 631-651)
- `fn parse_vector_literal` (originally lines 652-684)
- `fn parse_number_quantity` (originally lines 685-700)
- `fn parse_function_call` (originally lines 701-728)
- `fn parse_method_call` (originally lines 729-774)

With this `use` block:

```rust
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;
use super::Rule;
```

Mark `parse_expr` `pub(crate)` — it's called from `statements.rs` (Task 2) and, once Task 4
lands, from `quantities.rs` too. Leave everything else in this file private `fn` (each is
called only from within this group, per the earlier grep audit — `as_bare_identifier` and
`parse_term`/`parse_factor` are the trio the earlier `x y` / `1/2 m v^2` ambiguity fix
touched, all three stay together here).

`parse_factor` and `parse_method_call` call `parse_quantity`, which is still in `mod.rs` until
Task 4 — reference it as `super::parse_quantity(...)` at each call site inside
`expressions.rs` for now.

- [x] **Step 2: Wire `expressions.rs` into `mod.rs`, and fix `statements.rs`'s forward reference**

In `mod.rs`:

```rust
pub(crate) mod expressions;
```

In `statements.rs`, every `super::parse_expr(...)` call site (added as a stopgap in Task 2)
becomes `super::expressions::parse_expr(...)`.

- [x] **Step 3: Build and fix unused-import / missing-import issues**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo build -p physure-script 2>&1 | grep -A2 "warning: unused import"
```

- [x] **Step 4: Run the full suite**

```bash
cargo test -p physure-script
```

Expected: same 219/279 as Task 2.

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser/mod.rs physure-script/src/parser/statements.rs physure-script/src/parser/expressions.rs
git commit -m "refactor(phs): extract parser/expressions.rs"
```

---

## Task 4: Extract `parser/quantities.rs`

**Files:**
- Modify: `physure-script/src/parser/mod.rs`
- Modify: `physure-script/src/parser/expressions.rs`
- Create: `physure-script/src/parser/quantities.rs`

- [x] **Step 1: Create `quantities.rs`**

Confirm current line numbers, then cut, in this order, into a new
`physure-script/src/parser/quantities.rs`:

- `fn unit_registry` (originally lines 775-782)
- `fn unit_term_is_registered` (originally lines 798-816)
- `fn unit_expr_all_valid` (originally lines 817-835)
- `fn split_unit_expr` (originally lines 836-875)
- `fn parse_unit_leftover` (originally lines 876-887)
- `fn parse_uncertainty_val` (originally lines 888-904)
- `fn parse_quantity` (originally lines 905-1035)

With this `use` block:

```rust
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::UnitRegistry;
use std::sync::OnceLock;
use crate::ast::*;
use super::Rule;
use super::expressions::parse_expr;
```

Mark `parse_quantity` `pub(crate)` — called only from `expressions.rs` (`parse_factor` and
`parse_method_call`; verified via `grep -n "parse_quantity("` against the pre-task file —
`shadowing.rs`'s functions operate on the already-built `Statement`/`Expr` AST, not on pest
`Pair`s, and never call it). Leave `unit_registry`/`unit_term_is_registered`/`unit_expr_all_valid`/
`split_unit_expr`/`parse_unit_leftover`/`parse_uncertainty_val` private `fn` (each called only
from within this group).

`parse_quantity` and `parse_unit_leftover` call `parse_expr` — already in `expressions.rs`
from Task 3, so reference it directly via the `use super::expressions::parse_expr;` above, no
stopgap `super::` needed here (unlike Task 2/3's forward references, this one has no
forward-reference problem since `expressions.rs` already exists by the time this task runs).

- [x] **Step 2: Wire `quantities.rs` into `mod.rs`, and fix `expressions.rs`'s forward reference**

In `mod.rs`:

```rust
pub(crate) mod quantities;
```

In `expressions.rs`, every `super::parse_quantity(...)` call site (added as a stopgap in
Task 3) becomes `super::quantities::parse_quantity(...)`.

- [x] **Step 3: Build and fix unused-import / missing-import issues**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo build -p physure-script 2>&1 | grep -A2 "warning: unused import"
```

- [x] **Step 4: Run the full suite**

```bash
cargo test -p physure-script
```

Expected: same 219/279 as Task 3.

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser/mod.rs physure-script/src/parser/expressions.rs physure-script/src/parser/quantities.rs
git commit -m "refactor(phs): extract parser/quantities.rs"
```

---

## Task 5: Extract `parser/shadowing.rs`

**Files:**
- Modify: `physure-script/src/parser/mod.rs`
- Create: `physure-script/src/parser/shadowing.rs`

- [x] **Step 1: Create `shadowing.rs`**

Confirm current line numbers, then cut, in this order, into a new
`physure-script/src/parser/shadowing.rs`:

- `struct UnitToken` (originally lines 1036-1050)
- `fn unit_chain_tokens` (originally lines 1051-1110)
- `fn unit_shadowing_error` (originally lines 1111-1151)
- `fn validate_unit_shadowing` (originally lines 1152-1160)
- `fn check_statement_shadowing` (originally lines 1161-1193)
- `fn check_expr_shadowing` (originally lines 1194-1246)

With this `use` block:

```rust
use physure_core::error::{PhysureError, PhysureResult};
use std::collections::HashSet;
use crate::ast::*;
```

Mark `validate_unit_shadowing` `pub(crate)` — it's called once, from `mod.rs`'s `parse_phs`/
`parse_phs_with_lines`, after the AST is built. Leave `UnitToken`, `unit_chain_tokens`,
`unit_shadowing_error`, `check_statement_shadowing`, `check_expr_shadowing` private (all
called only from within this group). This file does not need `super::Rule` — everything in
it operates on the already-parsed `Statement`/`Expr` AST, not on pest `Pair`s.

- [x] **Step 2: Wire `shadowing.rs` into `mod.rs`**

```rust
pub(crate) mod shadowing;
```

Update `parse_phs`/`parse_phs_with_lines`'s calls to `validate_unit_shadowing(...)` to
`shadowing::validate_unit_shadowing(...)`.

- [x] **Step 3: Build and fix unused-import / missing-import issues**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo build -p physure-script 2>&1 | grep -A2 "warning: unused import"
```

- [x] **Step 4: Run the full suite**

```bash
cargo test -p physure-script
```

Expected: same 219/279 as Task 4.

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser/mod.rs physure-script/src/parser/shadowing.rs
git commit -m "refactor(phs): extract parser/shadowing.rs"
```

---

## Task 6: `mod.rs` should now hold only the public entry points

After Tasks 2-5, `parser/mod.rs` should contain exactly: `#[derive(Parser)] pub struct
PhsParser;` (and its `Rule` enum, generated by the `#[grammar = "phs.pest"]` attribute), `pub
fn parse_phs`, `pub fn parse_phs_with_lines`, the four `pub(crate) mod` lines, and the test
module declaration (added in Task 7).

**Files:**
- Modify: `physure-script/src/parser/mod.rs`

- [x] **Step 1: Confirm the file's remaining shape**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization/physure-script/src
grep -n "^pub fn \|^fn \|^pub struct \|^pub(crate) mod " parser/mod.rs
```

Expected: `parse_phs`, `parse_phs_with_lines`, `PhsParser`, plus the four `pub(crate) mod`
lines from Tasks 2-5. If anything else still appears here, it was missed in an earlier task —
go back and move it into the file its callers actually live in before continuing.

- [x] **Step 2: No code change needed if Step 1 matches** — this task is a checkpoint. If it
      already matches, skip to Step 3 (no commit needed for a checkpoint with nothing to fix).

- [x] **Step 3: Commit** (only if Step 1 required a fix)

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
git add physure-script/src/parser/
git commit -m "refactor(phs): parser/mod.rs holds only the public entry points"
```

---

## Task 7: Move the test module to `parser/tests.rs`

**Files:**
- Modify: `physure-script/src/parser/mod.rs`
- Create: `physure-script/src/parser/tests.rs`

- [x] **Step 1: Cut the test module**

Cut the entire `#[cfg(test)] mod tests { ... }` block (originally lines 1247-1683) out of
`mod.rs` into a new `physure-script/src/parser/tests.rs`. Drop the `#[cfg(test)]`/`mod tests {
}` wrapper — the file's content becomes the module's content directly, same as
`symbolic/tests.rs`, `builtins/tests.rs`, and `interpreter/tests.rs` already do.

The test module already does `use super::*;` — after the move, `super` refers to
`parser::mod`, so this needs explicit paths to reach the re-organized siblings:

```rust
use crate::parser::*;
use crate::parser::statements::*;
use crate::parser::expressions::*;
use crate::parser::quantities::*;
use crate::parser::shadowing::*;
```

- [x] **Step 2: Wire `tests.rs` into `mod.rs`**

```rust
#[cfg(test)]
mod tests;
```

- [x] **Step 3: Build and fix unresolved-path errors**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo test -p physure-script --lib --no-run 2>&1 | head -60
```

Expected: either compiles clean, or names exactly which test function can't resolve which
symbol — add the matching explicit `use` for that symbol's real path and re-run until it
compiles. A private (non-`pub(crate)`) helper a test wants to call directly may need its
visibility bumped to `pub(crate)` — if so, that's expected (test code reaching into a sibling
module's internals is exactly the case `pub(crate)` exists for), not a design problem.

- [x] **Step 4: Run the full suite**

```bash
cargo test -p physure-script
```

Expected: identical pass counts to Task 6 — same tests, same assertions, new location only.

- [x] **Step 5: Commit**

```bash
git add physure-script/src/parser/mod.rs physure-script/src/parser/tests.rs
git commit -m "refactor(phs): move parser test module to parser/tests.rs"
```

---

## Task 8: Fix the stale `parser.rs#L<line>` doc anchors

**Files:**
- Modify: `docs/language_readiness_roadmap.md`
- Modify: `docs/superpowers/plans/2026-08-14-track-d-incremental-lsp.md`

- [x] **Step 1: Find each anchor's target and its new location**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
grep -n "parser\.rs#L" docs/language_readiness_roadmap.md docs/superpowers/plans/2026-08-14-track-d-incremental-lsp.md
```

Re-run this to get the current, authoritative citation list before editing (line numbers as
of this plan being written: `parser.rs:339` in `language_readiness_roadmap.md`; `parser.rs:35`
in the track-d plan, cited for `parse_phs_with_lines`). Cross-reference each against this
plan's Task 2-5 line-range tables to find which new file the cited line now lives in, then
find its new line number:

```bash
grep -n "fn parse_phs_with_lines" physure-script/src/parser/mod.rs
grep -n "fn parse_guard_if_stmt" physure-script/src/parser/statements.rs
```

Note: `parser.rs:339` (in the pre-split file) falls inside `parse_guard_if_stmt`'s body — the
surrounding prose in `language_readiness_roadmap.md` talks about comparisons lowering to
`FunctionCall`s like `op_<`, which isn't obviously what that line does. That mismatch predates
this refactor and isn't something this task should try to silently correct by guessing a
"better" target — just repoint the link to wherever `parse_guard_if_stmt`'s body (specifically
its `parse_expr` call, since that's the line-339 content) now lives, preserving whatever the
citation already pointed at. If it's worth fixing to point somewhere more accurate, that's a
separate call for whoever owns that doc, not part of this mechanical repoint.

- [x] **Step 2: Update each link**

Replace both the visible label and the URL, e.g.:

```markdown
([parser.rs:35](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/parser.rs#L35))
```

becomes (with whatever line number Step 1 found):

```markdown
([parser/mod.rs:N](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/parser/mod.rs#LN))
```

- [x] **Step 3: Verify no anchor was missed**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
grep -rn "parser\.rs#L" docs/
```

Expected: no output.

- [x] **Step 4: Docs build sanity check**

```bash
cd physure-python
uv run mkdocs build --strict
```

- [x] **Step 5: Commit**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
git add docs/language_readiness_roadmap.md docs/superpowers/plans/2026-08-14-track-d-incremental-lsp.md
git commit -m "docs(phs): repoint parser.rs line anchors at their new files after the module split"
```

---

## Task 9: Final cross-crate verification

**Files:** none — verification only.

- [x] **Step 1: Full physure-script suite from a clean build**

```bash
cd d:/Projects/physure/.claude/worktrees/phs-modularization
cargo clean -p physure-script
cargo test -p physure-script
```

Expected: `test result: ok. 219 passed` (lib) and 279 total across all binaries, identical to
the baseline recorded at the start of this plan.

- [x] **Step 2: Downstream crates**

```bash
cargo test -p physure-cli -p physure-lsp
```

Expected: same pass counts as before this plan started (6 + 5 + 1 + 0-ignored-2 for
`physure-cli`, 36 for `physure-lsp`).

- [x] **Step 3: Whole-workspace build**

```bash
cargo build --workspace
```

- [x] **Step 4: Rebuild and test the Python native extension**

```bash
cd physure-python
maturin develop
uv run pytest tests/test_transpiler_parity.py -q
```

Expected: `8 passed`.

- [x] **Step 5: No commit for this task** — verification-only. If any step fails, stop and fix
      the specific task whose move caused it before proceeding.
