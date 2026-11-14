# PHS language semantics: precedence, global constants, zero needs a unit, text is not a condition, one-sided limits — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the five language-level findings the audit track left open (`docs/superpowers/audits/2026-11-14-code-resident-numbers.md` §11.2: B26, T11, T1, T6, T10), with the owner's decisions of 2026-11-14, so that PHS reads and compares the way mathematics and every formal language do, and nothing is decided by a convention the user cannot see.

**Owner decisions (2026-11-14):**
1. **Precedence (B26):** evaluate it and choose what is most acceptable scientifically.
2. **`pi` and the catalog constants become global identifiers.**
3. **Zero needs a compatible unit.** A quantity cannot be compared with a bare `0`: zeros of different scales (0 K, 0 °C, 0 °F) are different things, so the unit must say which zero is meant. The static checker is right; the runtime follows it.
4. **Text is text.** `"false"` and `false` are different things; a string is not a condition.
5. **One-sided limits are added now.**

**Process rules of this project:** tests first (see it fail, fix, see it pass); English everywhere; `CHANGELOG.md` (CRLF) gets a Fixed/Changed bullet per behaviour change with before/after measured through the real `phs`; update `PHYSURE_LANGUAGE_GUIDE.md` and the `phs doc` generator text in `physure-cli/src/main.rs` identically (they are kept equal; check with `phs --doc --save`); docs links per `CLAUDE.md` (no relative links to source files from `docs/`; `mkdocs build --strict`); never push; `physure-core` gets no new dependency; PHS semantics live only in `physure-script`; scripted edits are UTF-8 and CRLF-aware, backslashes via `chr(92)`. Earlier plan reviews of this project always found measured defects: every claim below about existing behaviour must be re-measured before the change and the result put in the commit message.

---

## Decision 1: the precedence PHS will have, and why

**Today** (`physure-script/src/phs.pest`, tightest first): `^` and unary minus (`factor`) → `* /` and implicit multiplication (`term`) → **comparison** (`comp_expr`) → **`+ -` and `=>` at one level, left-associative** (`conv_expr`) → `..` (range) → format spec → `not` → `and` → `or` → ternary → `where`. Consequences measured by the audit: `0.1 + 0.2 == 0.3` parses as `0.1 + (0.2 == 0.3)` and fails with "Invalid operand types"; `a + b > c` never means what a scientist reads.

**Proposed (standard mathematical order):** `^` and unary minus → `* /` and implicit multiplication → `+ -` → **`=>` (conversion)** → **comparison** → `..` → format → `not` → `and` → `or` → ternary → `where`.

Rationale, to be checked by the review:
- Mathematics, Python, Rust, Julia, MATLAB, R: arithmetic binds tighter than comparison, comparison tighter than logic. `a + b == c` is `(a + b) == c` everywhere. PHS is the outlier.
- Conversion `=>` is a *presentation/unit-change postfix*: `a + b => km` means "convert the sum to km", and `x => km < 5 km` means "convert, then compare". So `=>` sits **between** the additive level and the comparison level, left-associative with itself. This is the reading every written example of the language already uses (check the tutorials).
- The only breaking cases are expressions that parse today and mean something *different* under the standard order: `x => m + y` (today `(x => m) + y`; it will be a syntax/semantics error asking for parentheses, because the right side of `=>` is a unit, not an expression), and any expression mixing comparison with `+`/`-`/`=>` that today yields a type error or a Bool arithmetic mistake. The review must enumerate them from the repository's `.phs` text (tutorials, docs, tests, READMEs, the `phs doc` text) and from `physure-script/tests`; none may change meaning silently: either the expression was already an error or it now raises a clear parse/type error telling the user to parenthesise.

**Implementation shape** (decide against the real grammar and parser): split `conv_expr` into `add_expr = term ((+|-) term)*` and `conv_expr = add_expr (=> unit_expr)*`, and let `comp_expr = conv_expr (op_compare conv_expr)*`; keep `..`, format, logic, ternary, `where` where they are; update `physure-script/src/parser/*` (AST building must keep the left-associative shapes the interpreter, the static checker, the codegen targets, the LSP and the MCP/doc tooling read), and every other consumer of the grammar (grep for `conv_expr`, `comp_expr`, `Rule::`; any second PHS grammar: tree-sitter, editor syntax, docs highlighting, the Python/JS transpile targets). Chained comparisons `a < b < c` stay as they are today (read the code: do not invent semantics).

## Decision 2: `pi`, `π` and the catalog constants as global identifiers

- **Names:** `pi` and `π` (a Number, `std::f64::consts::PI`) and **every constant of the catalog in force** (`[Constants]` of the embedded catalog plus overrides, plus the frozen file's when one is in force) by its catalog name (e.g. `speed_of_light_in_vacuum`, `molar_gas_constant`, `bohr_radius`), valued as a Quantity with the unit the catalog gives (and its uncertainty when the catalog carries one). They resolve through the **hooked catalog** (`with_conf_registry` / `try_*`), so a frozen configuration governs them and an override changes them; they are never copied into the interpreter.
- **Resolution order** for a bare identifier: lexical (function parameters, `where` bindings, `for` variables) → variables in scope → user functions → **constants** → unit symbols where the grammar already allows them → *unknown* (today: a String; this stays, but a name that is a constant is no longer unknown). Say what happens with units that share a symbol with a constant name (check the catalog for clashes: list them; the long snake-case names should not clash, but verify; `e`, `c`, `h`, `G`, `k` must not be introduced as constants: only the catalog's own names).
- **Shadowing:** assigning to, or declaring a parameter/`for` variable/`where` binding named like, `pi`, `π` or a catalog constant is an **error** ("`pi` is a constant; choose another name") — a scientist who writes `pi = 3` has made a mistake, not a choice. Decide how this interacts with importing a user module defining the same name (error as well) and with `let`. The static checker must report it too.
- `integral` and `limit` free-name resolution (written in the previous track: variable → unit → `pi`/`π` → catalog constant) is **re-pointed to the same resolver** so there is one rule; the order inside those builtins must agree with the global order above (a variable named like a unit still wins, etc.). Remove the "resolve only inside integral/limit" special case and its changelog caveat.
- The LSP (hover/completion), `phs check`, the doc generator and the language guide list the constants; codegen targets (Rust/Python/Java/JS/TS transpile) must either map them or refuse with a clear message — read `codegen/*` and decide per target; no silent wrong numbers.

## Decision 3: zero needs a compatible unit

Remove the "dimensionless-zero exception" of comparisons (`physure-script/src/builtins/core.rs` ~54-66 and `physure-core` `comparable_magnitudes`): `x > 0`, `x == 0`, `0 < x`, `x >= 0` with `x` a quantity of any dimension other than dimensionless is a **dimension-mismatch error** whose message says to write the zero with its unit (`x > 0 m`), and, for temperatures and levels, why (a zero of an offset scale is a different value on each scale). A dimensionless quantity (unit `unity`, `%`, ratio) compares with plain numbers as today. Truthiness (`if x`) is a different operation and keeps the rule of the previous track (exactly zero is false; temperature points, levels, NaN are errors). The static checker (`check.rs` `comparison`) and the runtime must agree on every case: add a test matrix that runs the same snippets through `phs check`/`report::evaluate` and asserts both say the same. This is a **breaking change for scripts that wrote `x > 0` on quantities**: enumerate them in the repository (tutorials, docs, tests, `@requires`/`@ensures` guards, guard returns, the `phs doc` examples) and migrate each to a unit zero; the changelog lists the migration and the error text is the migration guide. Advice replacing the old idiom: `abs(x) < 1e-9 m`.

## Decision 4: a string is not a condition

Truthiness accepts **only Bool, and finite numbers and linear quantities by the exact-zero rule** of the previous track; everything else — strings (`"true"`, `"false"`, `"1"`, `""`), vectors, matrices, ranges, functions, plots — is a **type error** naming the type ("a String is not a condition: compare it, e.g. `s == \"true\"`"). Find and fix every caller of `is_truthy` (if/while/ternary/guard/`@requires`/`@ensures`/`and`/`or`/`not` operands — `and` already refuses quantities: keep the rules consistent and documented in one table) and every existing script that relied on string truthiness (grep).

## Decision 5: one-sided limits

`limit(expr, var, point[, side])` with `side` the string `"+"` (from the right, `x -> p+`) or `"-"` (from the left); absent means two-sided. Rules (to be challenged by the review): a **two-sided limit requires both sides defined** (the previous track's "a side undefined at every sample counts as absent" is withdrawn: with one-sided limits available it hides domain errors; `limit("x*ln(x)","x",0)` becomes an error saying "undefined for x < 0: use side \"+\""); a one-sided limit samples only that side and is an error if the expression is undefined there; an infinite point (`"inf"`, `"-inf"`) is inherently one-sided and rejects `side`; `side` with any other value is an error; the direction is part of the result's meaning and of the error messages ("from the left"/"from the right"); the value-at-the-point shortcut of the previous track applies only to two-sided limits of continuous expressions (a one-sided limit at a boundary of the domain must not evaluate at the point). Update the LSP signature/hover, the guide, `phs doc`, and the tests of the previous track that relied on domain-restricted two-sided answers (`x*ln(x)` at 0, `sin(sqrt x)/sqrt x` at 0, `sqrt(x)` at −1 error message).

---

## Tasks

### Task 1: precedence (Decision 1)

**Files:** `physure-script/src/phs.pest`, `physure-script/src/parser/*`, anything that consumes the changed rules, tests `physure-script/tests/precedence.rs` (new), the guide and `phs doc` text.

- [ ] **Step 0:** measure and record **before**: the table of the precedence probes below through `printf '<snippet>\n' | phs eval -` (the existing read-only binary `D:\Projects\physure\.claude\worktrees\audit-defects\target\debug\phs.exe` is current main), including every operator pair (`^`, unary `-`, `*`, `/`, implicit multiplication, `+`, `-`, `=>`, each comparison, `..`, format, `not`, `and`, `or`, ternary, `where`), a conversion with a following `+`, and chained comparison.
- [ ] **Step 1: tests first** (`precedence.rs`, through `report::evaluate` as `phs eval` does): `(0.1 + 0.2) == 0.3` written WITHOUT parentheses must be `true` once fixed (`0.1 + 0.2 == 0.3` and `0.1 m + 0.2 m == 0.3 m`); `a + b > c`; `2 m + 3 m => km == 0.005 km`; `x => km < 5 km`; `a - b == c - d`; `-2 ^ 2` unchanged; `a * b + c == d`; `not a + b > c` (not applies to the comparison); ranges `0 m .. 10 m => km` unchanged; format spec unchanged; ternary/where unchanged; the breaking case `x => m + y` now an error with a message about parentheses; a **property test** generating random well-formed expressions from the operator set and checking the parse tree against an independent reference precedence-climbing parser written in the test (so the tests do not merely mirror the grammar).
- [ ] **Step 2–4:** fail, implement, pass; run **every** snippet found in the repository text (tutorials, docs, READMEs, `phs doc` examples, `physure-script/tests` source strings: extract and run before/after; list each difference in the commit message; each difference must be explained as error→valid or an intentional break with a clear message).
- [ ] **Step 5:** commit `feat(script): arithmetic binds tighter than conversion and comparison (audit B26)`.

### Task 2: global constants (Decision 2)

**Files:** `physure-script/src/interpreter/*` (identifier resolution), `physure-script/src/builtins/{definite_integral,limit}.rs` (re-point to the resolver), `physure-script/src/check.rs`, `physure-script/src/codegen/*`, `physure-lsp/src/main.rs`, docs, tests `physure-script/tests/global_constants.rs`.

- [ ] **Step 1: tests first:** `pi`, `π`, `sin(pi/2) == 1`, `2 * pi * 1 m` (a Quantity), `molar_gas_constant * 300 K` (unit J/mol), each of a sample of catalog constants equals `Quantity` from the catalog with its unit; an override of `molar_gas_constant` in `physure.conf` or in an activated frozen file changes the PHS value (use the activation pattern of `physure-script/tests/frozen_phs.rs`, restore state); assigning `pi = 3`, `fn f(pi) = pi`, `for pi in ...`, `where pi = 3`, `let pi`, and an imported module defining `pi` are errors (and `phs check` reports them); a variable with a unit-like name still resolves as before; `integral("sin(x)","x",0,pi)` without quoting, and `limit("sin(x)/x","x",pi)`; the list of catalog constants whose name clashes with a unit symbol or keyword (assert the list is empty or document each); constants with uncertainty keep it; codegen: each target maps or refuses (test the refusal message).
- [ ] **Step 2–5:** fail, implement, pass, commit `feat(script): pi and the catalog constants are global identifiers`.

### Task 3: zero needs a unit (Decision 3) and text is not a condition (Decision 4)

**Files:** `physure-script/src/builtins/core.rs`, `physure-core/src/quantity.rs` (`comparable_magnitudes`), `physure-script/src/check.rs`, `physure-script/src/interpreter/helpers.rs` (`is_truthy`), callers, docs, tests `physure-script/tests/zero_and_conditions.rs`, `physure-core/tests/quantity_equality.rs` (update).

- [ ] **Step 0:** record before: `1 m > 0`, `0 < 1 m`, `1 m == 0`, `1 m >= 0 m`, `20 degC > 0`, `5 dB > 0`, `x = 1 %; x > 0`, `if "false" then 1 else 0`, `if "" then...`, `if vector(1,2) then...`, `"a" and True`, plus what `phs check` says for each.
- [ ] **Step 1: tests first:** the matrix of Decision 3 asserting runtime and static checker agree (error text mentions the unit zero and, for offset scales, the reason); dimensionless and `%` operands still compare with plain numbers; Decision 4's table of types × `if`/`while`/ternary/`@requires`/`and`/`or`/`not` (Bool ok, number/linear quantity by exact zero, everything else a type error naming the type).
- [ ] **Step 2–4:** fail, implement, pass; enumerate in the repository every script/test/doc snippet that compared a quantity with a bare `0` or used a string/other as a condition (grep, then run before/after) and migrate each with the unit zero / explicit comparison; list them in the commit message.
- [ ] **Step 5:** commit `fix(script,core): zero needs a unit and a text is not a condition`.

### Task 4: one-sided limits (Decision 5)

**Files:** `physure-script/src/builtins/limit.rs` (+ `calc.rs` dispatch), `physure-lsp/src/main.rs`, docs, tests `physure-script/tests/limit_semantics.rs` (update) and `limit_one_sided.rs` (new).

- [ ] **Step 1: tests first** (closed forms in the tests): `limit("1/x","x",0,"+")` = +inf, `"-"` = −inf, two-sided `1/x` at 0 still refused (sides disagree); `abs(x)/x` right = 1, left = −1; `x*ln(x)` at 0 with `"+"` = 0 and two-sided = error naming the undefined side; `sqrt(x)` at 0 with `"+"` = 0, at 0 `"-"` = error; `sin(sqrt(x))/sqrt(x)` right = 1; `floor(x)` at 1: right 1, left 0; infinite point with a `side` = error; bad `side` = error; a one-sided limit at a domain boundary never evaluates at the point; units/uncertainty behaviour of the previous track preserved; messages say "from the left/right".
- [ ] **Step 2–5:** fail, implement, pass; update the previous-track tests that relied on domain-restricted answers deliberately (list each with the reason). Commit `feat(script): one-sided limits; a two-sided limit needs both sides defined`.

### Task 5: documentation, changelog, audit status, verification

- [ ] **Step 1:** guide + `phs doc` (identical): the precedence table (one table, tightest first), the identifier resolution order, constants, zero-with-unit rule with the migration advice, the condition table, `limit` with `side`. `docs/tutorials/phs_primer.md` updated where it shows any of it.
- [ ] **Step 2:** `CHANGELOG.md` (CRLF; headings counted before/after; **Fixed**/**Changed**/**Added** bullets, each with the measured before/after and who is affected, and the migration notes: `x > 0` → `x > 0 m`, `x => m + y` → `(x => m) + y`, strings as conditions, `pi = ...` errors, two-sided limits needing both sides).
- [ ] **Step 3:** audit §11.2: mark B26, T1 (checker vs runtime), T6, T10, T11 as resolved with commit hashes; keep the rest open; update the roadmap files only if a phase statement changes (it should not).
- [ ] **Step 4:** full verification: `cargo test -p physure -p physure-cli -p physure-script -p physure-lsp`, `cargo check --workspace`, `cargo clippy -p physure-script --all-targets -A clippy::approx_constant` (nothing new in touched files), `cd physure-python && uv run mkdocs build --strict`, the repository snippets before/after. Commit `docs: language semantics - guide, changelog, audit status`.

---

## Self-review against the owner's decisions

| Decision | Task |
|---|---|
| Precedence evaluated; most acceptable scientifically | 1 (standard order, conversion between additive and comparison; measured corpus impact) |
| `pi` and catalog constants global | 2 |
| Zero needs a compatible unit; checker is right | 3 |
| Text is text | 3 |
| One-sided limits now | 4 |

Not in this plan (still open in the audit): unbound identifiers evaluating to strings (T5), the symbolic engine dropping comparisons, missing `sec`/`csc`/`cot`/`erf`, `preprocess_symbolic_expression` inlining one call; Phase 4b (data migration) follows this track.
