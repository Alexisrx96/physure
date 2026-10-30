# Structural grammar: user-defined functions, typed params, `let`, display-text blocks (MKML Phase 2, slice 2)

## Context

`physure/ext/grammar.py` currently supports variable assignment (`x = 5 m`) and a fixed table
of built-in functions (`_FUNCTIONS`, added in the previous Phase 2 slice: abs, round, sqrt, sin,
etc. — see `docs/superpowers/specs/2026-07-11-mkml-math-functions-design.md`). There is no way for
a user to define their own named, callable expression.

This is the next slice of the 4-phase MKML roadmap's Phase 2 (structural grammar). Per user
decision, **blocks are explicitly out of scope** for this phase — `let...in` covers the one
legitimate use case blocks would have served (a named intermediate value inside a single-expression
function body) without introducing a multi-statement block construct. Vectors are also out of scope
here, tracked as a separate spec/plan (a distinct subsystem: a new value type built on `Quantity`'s
existing array-magnitude support, vs. this spec's pure grammar/evaluation-control work).

This slice also adds **display-text blocks**: a way to embed human-facing explanatory text directly
in a `.mkml` script, distinct from `#` comments (which are silently stripped — for the script's
author only). Unrelated to functions/`let` mechanically, but small enough, and part of the same
statement-level layer of `GrammarInterpreter`, to fold into this slice rather than open a third
spec.

## Scope

### 1. User-defined function definitions: `f(x) = expr`

Reuses the existing `=` assignment operator. `_split_assignment` currently recognizes a bare
`IDENT` on the LHS as a variable target. It gains a second case: `IDENT ( params )` on the LHS is a
function definition, storing the raw parameter list and the RHS token stream (not evaluating it) in
a new `GrammarInterpreter._functions: dict[str, UserFunction]`, parallel to the existing `_vars`
dict.

```python
@dataclass
class UserFunction:
    params: list[tuple[str, str | None]]  # (name, unit_symbol_or_None)
    body_tokens: list[Token]
```

No AST is introduced. The function body is stored as its raw token list and **re-parsed with a
fresh `_ExprParser` on every call** — the same "parse and eval in one pass" architecture the
codebase already uses everywhere else. This sidesteps the AST-vs-parse-and-eval decision the
original notation-layer spec flagged as a Phase 2+ concern (it stays unneeded).

Calling: `_atom()` already special-cases `IDENT` immediately followed by `(` for built-ins
(`_is_function_call`/`_FUNCTIONS`). It gains a second lookup against
`self._functions` (checked after `_FUNCTIONS`, since built-in names can never appear in
`_functions` — see reserved-word rule below). On a hit: parse arguments via the existing
`_call_args()`, arity-check against `len(params)` (exact match, no variadic user functions), bind
each argument to its parameter name in a new local scope dict, and evaluate `body_tokens` with a
fresh `_ExprParser` whose `resolve` closure checks the local scope first, then falls through to the
interpreter's normal `_resolve` (global variables, then units/constants).

### 2. Typed parameters: `f(x: m, k) = expr`

New `:` token added to `_OP_PAT`. Parameter list parsing (a new `_param_list()` method, mirroring
`_call_args()`'s shape but for the definition side) accepts `IDENT` optionally followed by `: IDENT`
— the second `IDENT` is a unit symbol resolved once, at definition time, to its `Dimension` via the
interpreter's `UnitSystem`.

Typing is **optional per parameter** — untyped parameters accept any value unchecked, exactly like
today's plain variables. At call time, for each typed parameter: the bound argument must have a
`.dimension` equal to the annotation's dimension (auto-converts compatible units, e.g. `cm` accepted
for a `: m` parameter — same "physical dimension, not exact unit" rule `sin`/`cos`/`tan` already use
for angle-vs-dimensionless). Mismatch raises the existing `DimensionError`, unwrapped. Passing a
bare number (no unit) to a typed parameter also raises `DimensionError`, since a bare number carries
no dimension to match against.

### 3. `let y = expr1 in expr2`

New reserved words `let`/`in`, recognized **only in their exact expected grammatical position** —
the same mechanism `sqrt`/`min`/`max`/etc. already use (position-gated, not a global reserved-word
list). This matters because `in` is already a unit alias for inches
(`physure.conf:102: inch = 0.0254, L, noprefix, [in, inch]`) — outside the `let...in` position,
`in` continues to resolve as inches exactly as it does today (e.g. `5 in`). Documented inline with a
`# ponytail:` comment, same style as the existing `min`/minute shadow note.

Grammar: `let` IDENT `=` expr `in` expr. Single binding per `let` — multiple intermediate values
nest: `let y = x^2 in let z = y + 1 in z * 2`. Evaluates by binding the `IDENT` to `expr1`'s value in
a local scope layered on top of whatever scope is currently active (mirrors user-function param
binding — same local-scope-then-fallback resolve mechanism), then evaluates `expr2` in that scope.

**Restricted to function bodies.** `let` is parsed as part of `_ExprParser`'s grammar (so it works
anywhere an `_ExprParser` runs, including nested inside function bodies and nested `let`s), but
`GrammarInterpreter._eval_statement` rejects a top-level statement that isn't a function definition
and begins with the `let` token, raising `GrammarError("'let' is only valid inside a function body")`
before invoking the parser. A bare `let...in` line typed directly into a script or the REPL hits this
check.

### 4. Recursion

Supported without special-casing: since a function's body is re-resolved (not pre-evaluated) on
every call, and `_functions[name]` is already populated by the time a `def` statement completes,
`f` can reference itself (or, incidentally, mutually-recursive functions can reference each other,
as long as both are defined before either is called).

A call-depth counter is threaded through nested `_ExprParser` invocations (incremented once per
user-function call, checked before evaluating the body). The counter starts at 0 for each top-level
statement evaluated by `GrammarInterpreter._eval_statement` and is not shared across statements —
so a script calling the same function many times in sequence (not recursively) never approaches the
limit. Limit is read from
`UnitSystem.settings["mkml_recursion_limit"]` (new key in `physure.conf`'s `[Settings]` section,
default `100` if the active system's `.conf` doesn't define it — keeps user-local `.conf` overrides
working without a hard requirement to update them). Exceeding the limit raises
`GrammarError(f"recursion limit ({limit}) exceeded calling {name!r}")` — a controlled MKML-level
error, not an uncaught Python `RecursionError`.

### 5. Reserved-word / namespace rules

- Function and variable names **share one namespace**: `f = 5` then `f(x) = x^2` (or the reverse
  order) raises the same "reserved" `GrammarError` `_split_assignment` already raises for built-in
  names, generalized to also check `self._functions` / `self._vars` against each other.
- Built-in names (`_FUNCTIONS` keys) remain reserved and cannot be shadowed by a user function
  definition — same existing check, unchanged.
- Redefining an existing user function (same name, new params/body) is allowed and simply
  overwrites the `_functions` entry — consistent with variables already being freely reassignable.

### 6. Display-text blocks: ```` ```text``` ````

A triple-backtick-delimited block, inline (```` ```text``` ````) or spanning multiple lines,
marks literal text to be shown to whoever runs the script — the opposite of `#` comments, which are
silently discarded. Markdown-fenced-code-block syntax, deliberately, since that's already familiar
notation for "verbatim text block."

`GrammarInterpreter.run(source)` extracts every ```` ```...``` ```` span from the raw source with a
single non-greedy, `DOTALL` regex match (`` r"```(.*?)```" ``), *before* the existing per-line
`\n`/`;` splitting and `#`-comment stripping — so text inside a block is never tokenized as MKML and
never has a `#` inside it treated as a comment marker. Each extracted block becomes one statement in
its original source position, evaluating to the enclosed text as a plain `str` (only a single
leading/trailing blank line, if present, is trimmed — no dedenting, no other normalization).

`GrammarValue` (currently `Quantity[Any, Any, Any] | int | float`) widens to include `str`. Since
`run()` performs no I/O itself — it only returns `list[GrammarValue | None]` — a display-text block
needs no interpreter-level printing logic: it's just a `str` entry in that list, and `repl.py`'s
existing `_print_results` (`if result is not None: print(result)`) already prints it unchanged.

**Scope:** top-level only, exactly where `#` comments are valid today — never inside a function body
or a `let...in` expression (both are single expressions; allowing a display block inside either
reopens the "blocks" question already rejected for this phase). The interactive REPL (`_repl()` in
`repl.py`, which reads one line at a time via `input()`) only supports the inline single-line form;
true multi-line blocks require the whole source to be evaluated at once (piped stdin, a `.mkml` file,
or a direct `GrammarInterpreter.run(source)` call) — no continuation-prompt buffering is added to the
REPL's input loop.

**No variable interpolation.** Text is always literal, exactly as written between the fences. If
interpolating a computed value is ever needed, that's a separate, later addition.

## Non-goals

- Blocks (multi-statement function bodies) — explicitly rejected; `let...in` covers the one real use
  case (named intermediate values) without them.
- Vectors — separate spec/plan.
- Variadic user functions, default parameter values, keyword arguments — fixed positional arity
  only, matching the existing built-in function calling convention.
- Exact-unit-match typing (vs. dimension-match) — same convertible-dimension rule as the rest of the
  engine.
- Tail-call optimization or any recursion performance work — the 100-call default depth is a safety
  net for formulas (factorial, Fibonacci, small recurrences), not deep algorithmic recursion.
- Variable interpolation inside display-text blocks.
- Multi-line display-text block entry directly in the interactive REPL (continuation-prompt
  buffering) — only the single-line inline form works there.
- Display-text blocks inside function bodies or `let...in` expressions.

## Testing

Extends `tests/ext/test_grammar.py` (no new test files), same TDD failing-test-first pattern as
prior phases:

- Basic definition + call, multi-param, arity-error case.
- Typed parameter: valid dimension (including auto-converting compatible units), incompatible
  dimension raises `DimensionError`, bare number to a typed parameter raises `DimensionError`,
  untyped parameter accepts anything unchecked.
- Variable/function namespace collision both directions (`f=5` then `f(x)=...`, and reverse), and
  redefinition of an existing user function is allowed.
- Attempting to shadow a built-in name with a user function definition still raises.
- `let` inside a function body (basic, nested for two values), and `let...in` as a bare top-level
  statement raises `GrammarError`.
- `in` still resolves as inches everywhere outside the `let...in` position (regression case).
- Recursion: factorial and/or Fibonacci computed correctly; a function with no base case hits the
  configured recursion limit and raises `GrammarError` (not `RecursionError`); a custom
  `mkml_recursion_limit` value in a test `UnitSystem` changes the threshold.
- Each touched function gains one runnable doctest example (pytest runs `--doctest-modules`).
- Display-text block: inline single-line form, multi-line form, a block containing `#` and
  backtick-adjacent characters (verifying no comment-stripping or tokenization touches it), a block
  interleaved with normal statements (verifying source-order is preserved in the results list), and
  a script with no blocks at all (regression case, unaffected by the extraction step).

## Risks / open questions

- None blocking. The `in`/inches shadow is the one judgment call, resolved the same way the prior
  phase resolved `min`/minute — position-gated recognition, documented inline, no global
  reservation.
