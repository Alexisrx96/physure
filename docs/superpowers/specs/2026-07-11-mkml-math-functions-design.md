# Structural grammar: built-in math functions (MKML Phase 2, scoped)

## Context

`measurekit/ext/grammar.py` currently supports one special-cased function: `sqrt(...)`, hardcoded
directly in `_ExprParser._atom()` (added in Phase 1, notation layer). There is no general
`name(args)` call syntax — every other identifier resolves either to a unit or to a previously
assigned variable.

This is a narrow slice of the full Phase 2 scope described in the 4-phase MKML roadmap (notation
layer / structural grammar / calculus / solve): general user-defined functions, typed function
defs, `let`, vectors, and blocks are **not** part of this spec. This spec adds only a fixed table
of built-in math functions, requested directly by the user: "agregar el valor absoluto y otras
funciones necesarias" (add absolute value and other necessary functions), for both `.mkml` files
and the REPL — both route through the same `GrammarInterpreter` in `grammar.py`, so no other file
needs to change.

## Findings that shrink scope

Verified live against the current code before designing:

- **Nearly all needed semantics already exist on `Quantity`.** `__abs__`, `__round__`, `__floor__`,
  `__ceil__` preserve units and sign correctly. `_compare` (powering `__lt__`/`__gt__`/etc.)
  auto-converts compatible-but-different units before comparing magnitudes and raises
  `IncompatibleUnitsError` on dimension mismatch — so Python's builtin `min()`/`max()` are already
  correct and safe across e.g. `min(3 m, 200 cm)`. `sin`/`cos`/`tan`/`exp`/`log` are bound methods
  that validate dimensionless-or-angle input via `_apply_transcendental` (auto-converting
  angle-dimensioned quantities to radians) and propagate uncertainty with explicit derivatives.
  The grammar layer's job is purely call-syntax parsing plus a thin dispatch table — no new unit
  logic.
- **No `log10` exists anywhere in the engine.** Both backends (`core_backend`, `python_backend`)
  only implement natural log as `"log"`. Introducing base-10 semantics for `log` would contradict
  the underlying engine and is out of scope. `ln` is therefore a plain alias for the same
  natural-log implementation as `log`.
- **`min` collides with an existing unit alias.** `measurekit.conf:123`:
  `minute = 60.0, T, [min, minute, minutes]`. Grepped every other proposed function name (`abs`,
  `max`, `round`, `floor`, `ceil`, `sin`, `cos`, `tan`, `exp`, `ln`, `sqrt`) against every `.conf`
  file — no other collisions. Because function dispatch only triggers when an `IDENT` token is
  *immediately* followed by `(` (whitespace is discarded by the tokenizer regardless, so `min (x)`
  and `min(x)` tokenize identically), the only affected input shape is a `min`-as-minutes value
  written with no operator directly before a parenthesized expression, e.g. `5 min(3 + 2)` intended
  as "5 minutes times (3+2)". This is rare, and after this change it fails loudly with an arity
  error rather than silently misbehaving. Decision (user-confirmed): keep `min`/`max` as functions,
  mark the dispatch table with a `# ponytail:` comment documenting the shadow.

## Scope

### 1. Tokenizer: comma

Add `,` to `_OP_PAT` so argument lists can be comma-separated. No new token type needed beyond
what the existing operator-token machinery already provides.

### 2. Parser: `_call_args()` and `_atom()` dispatch

- New `_ExprParser._call_args()`: parses `(` expr (`,` expr)* `)` into a `list` of parsed
  sub-expression nodes (reuses `_sum()` for each argument, the parser's existing top-level
  expression rule).
- `_atom()` gains a branch: when the current token is `IDENT` with a value that is a key in
  `_FUNCTIONS`, and the next token is `(`, consume it as a call node instead of falling through to
  the existing implicit-multiplication/unit-or-variable lookup. Falls through unchanged for every
  other identifier (existing unit/variable resolution is untouched).
- The existing bare `√` prefix-token path in `_atom()` is untouched — it is a distinct tokenizer
  rule (`SQRT` token), not part of this call-syntax mechanism.

### 3. Dispatch table `_FUNCTIONS`

A dict of `name -> (min_arity, max_arity, callable)`, evaluated during `_eval_expr` after each call
node's arguments have themselves been recursively evaluated to `Quantity`/number values:

| Function | Arity | Implementation |
|---|---|---|
| `abs` | 1 | `abs(x)` — `Quantity.__abs__` |
| `round` | 1–2 | `round(x[, ndigits])` — `Quantity.__round__` |
| `floor` | 1 | `math.floor(x)` — `Quantity.__floor__` |
| `ceil` | 1 | `math.ceil(x)` — `Quantity.__ceil__` |
| `min` | 2–∞ | `min(*args)` — uses `Quantity._compare` |
| `max` | 2–∞ | `max(*args)` — uses `Quantity._compare` |
| `sqrt` | 1 | migrated from the current `_atom()` special case: `x ** 0.5` |
| `sin`, `cos`, `tan` | 1 | `x.sin()`/`.cos()`/`.tan()` if `Quantity`, else `math.sin`/etc. for a bare number |
| `exp`, `log` | 1 | `x.exp()`/`.log()` if `Quantity`, else `math.exp`/`math.log` for a bare number |
| `ln` | 1 | same implementation as `log` |

Wrong argument count raises `GrammarError` naming the function and the expected/actual count
(matching the existing `GrammarError` usage pattern elsewhere in the file). Dimension errors (e.g.
`sin(3 kg)`) propagate `Quantity`'s own `DimensionError` unchanged — no catching/rewrapping needed.

### 4. Reserved-word generalization

`_split_assignment` currently has a single hardcoded check: `if lhs_tokens[0].value == "sqrt"`.
Generalize to `if lhs_tokens[0].value in _FUNCTIONS`, so no function name in the table can be used
as an assignment target. This subsumes and replaces the Phase 1 sqrt-specific check.

## Non-goals

- General user-defined functions, `let`, typed function defs, vectors, blocks — later phases.
- `log10` or any non-natural logarithm base.
- Renaming `min`/`max` to avoid the unit-alias shadow (explicitly rejected by user in favor of
  documenting it).
- Any change to `repl.py` or `application/parsing.py` — both are unaffected; `repl.py` picks up new
  grammar capabilities automatically since it just calls `GrammarInterpreter().run()`.

## Testing

Extend `tests/ext/test_grammar.py` (no new test files), following the same TDD failing-test-first
pattern as Phase 1:

- One basic-evaluation case per function.
- Unit-preservation cases for `abs`, `round`, `floor`, `ceil` (e.g. `abs(-3 m) == 3 m`).
- Cross-unit `min`/`max` case (e.g. `min(3 m, 200 cm)` picks the smaller regardless of unit).
- Dimensionless-or-angle enforcement for `sin`/`cos`/`tan` (a `DimensionError` case for e.g.
  `sin(3 kg)`, and an angle-unit auto-conversion case).
- Arity-error case for at least one multi-arity function (`round`) and one fixed-arity function.
- Reserved-word assignment rejection for each new function name (parametrized over `_FUNCTIONS`
  keys, replacing the old single sqrt-only test).
- `sqrt(...)` continues to work identically post-migration (regression case); bare `√x` prefix
  behavior is unaffected and already covered by Phase 1 tests.

## Risks / open questions

- None blocking. The `min`/minute shadow is the one judgment call, already discussed and confirmed
  with the user above.
