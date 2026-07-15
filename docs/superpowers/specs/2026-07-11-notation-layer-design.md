# Notation layer: Unicode scientific notation (grammar improvements, Phase 1)

## Naming

The language implemented by `measurekit/ext/grammar.py` is now **MeasureKit Meta-Lang (MKML)**,
file extension `.mkml` — its own identity, not MeasureNote's MNML. MKML is inspired by MNML but is
deliberately a different (currently smaller, and now diverging) grammar with its own zero-dep,
fast-parse design goals. Docstrings and comments in `ext/grammar.py` that currently say "MNML"
should say "MKML" going forward; this spec and its phases use MKML terminology throughout.

## Context

`measurekit/ext/grammar.py` (MKML expression grammar) and `measurekit/ext/chemistry/{species,reaction}.py`
(chemical formula/reaction parsing) are hand-rolled, zero-dependency, regex-tokenized
recursive-descent parsers. MeasureNote's MNML grammar (`../MeasureNote/python/measurenote_core/grammar.lark`)
covers a much larger surface (functions, derivatives, integrals, vectors, blocks, `let`, `solve`,
typed function defs) via Lark, but pays for it with an external dependency and a `Lark(...)`
grammar rebuild on every `MNMLParser()` instantiation — incompatible with measurekit's zero-deps
and fast-startup invariants (CLAUDE.md).

This is Phase 1 of a 4-phase effort:
1. **Notation layer** (this doc) — Unicode scientific notation, ASCII fallback.
2. Structural grammar (functions, typed functions, `let`, vectors, blocks) — future spec.
3. Calculus (derivatives, integrals via `domain/symbolic/`) — future spec.
4. `solve` — future spec.

Each phase gets its own spec/plan/PR. This keeps the AST-vs-parse-and-eval architecture decision
(needed for Phase 2+) out of a phase that doesn't need it.

Driving motivation: the user is preparing to lean on the chemistry extension more heavily and
wants notation close to real scientific/mathematical writing (H₂O, ⇌, Greek letters), with a
keyboard-typable ASCII fallback for every Unicode symbol. A secondary constraint: token
definitions must stay declarative (flat regex tables) so a future editor extension
(VSCode/Neovim syntax highlighting or LSP) can derive its grammar mechanically — no tooling for
that is built in this phase, but design choices must not block it.

## Findings that shrink scope

Verified live against the current code before designing:

- **Greek letters as identifiers already work.** `_IDENT_PAT = r"[^\W\d]\w*"` in `grammar.py` uses
  Python's Unicode-aware `\w`, so `ΔH = 5 kJ` parses today with zero changes.
- **The `→` reaction arrow already works.** `reaction.py`'s `_ARROW_RE = re.compile(r"->|=|→")`
  already accepts it.
- Spelled-out Greek fallbacks (`Delta`, `mu`) for *units/constants* are not a grammar concern —
  that's the existing `.conf` alias mechanism (see `add-unit` skill and the "unit aliases collide
  silently" invariant in CLAUDE.md). Building a second aliasing path in the tokenizer would
  duplicate that mechanism. Out of scope here.
- `∑` needs an iterable to sum over, i.e. vectors — Phase 2. Not included here.

## Scope (Phase 1)

### 1. Unicode subscript digits in chemical formulas

`species.py`'s `_TOKEN_RE` and `reaction.py`'s `_TERM_RE` use ASCII `\d`. Rather than teach three
regexes to understand Unicode digits, add one normalization helper next to the existing
`to_subscript`/`parse_superscript` in `measurekit/domain/notation/lexer.py`:

```python
_SUBSCRIPT_REVERSE_MAP = str.maketrans("₀₁₂₃₄₅₆₇₈₉₋", "0123456789-")

def subscript_to_ascii(s: str) -> str:
    """Normalize Unicode subscript digits to ASCII (₂ -> 2)."""
    return s.translate(_SUBSCRIPT_REVERSE_MAP)
```

`species.py::parse_formula` and `reaction.py::_parse_species` call this once on their input before
running their existing (unchanged) regexes. `H₂O` normalizes to `H2O` and parses through the
current code path unmodified. This only affects *parsing* input — printing formulas back with
subscripts is not requested and stays out of scope.

### 2. Equilibrium arrow `⇌`, ASCII fallback `<=>`

`reaction.py`:
- `_ARROW_RE` becomes `re.compile(r"<=>|->|=|→|⇌")`.
- Switch from `_ARROW_RE.split(equation)` to `_ARROW_RE.search` first (to capture which arrow
  matched), then split on its span.
- Add `reversible: bool = False` to the `Reaction` dataclass, set `True` when the matched arrow is
  `⇌` or `<=>`.
- Existing inputs (`->`, `=`, `→`) are unaffected — new alternative only.

### 3. `×` `÷` as multiplication/division

`grammar.py::_TOKEN_SPEC`: widen the existing MUL/DIV patterns to `r"[*×]"` / `r"[/÷]"`. Token type
is unchanged, so `_ExprParser` needs no changes — this is purely a tokenizer-level alias.

### 4. `√` prefix operator

`grammar.py`:
- Add a `SQRT` token: `r"√"`.
- In `_ExprParser._atom()`: if the token is `SQRT`, or is `IDENT` with value `"sqrt"` followed by
  `(`, parse one operand (parenthesized expression, or a single atom for the bare `√x` form) and
  return `operand ** 0.5`.
- `sqrt` becomes a reserved word in this context: using it as an assignment target raises
  `GrammarError("'sqrt' is reserved")`. This is a self-contained special case, not general
  function-call support (that arrives in Phase 2) — deliberately narrow to avoid half-building
  Phase 2 here.

## Non-goals

- `∑`, user-defined functions, vectors, `let`, `solve` — later phases.
- Spelled-out ASCII aliases for Greek letters as units/constants — use `.conf` aliasing.
- Formula pretty-printing with Unicode subscripts.
- Any editor/LSP tooling — only keeping the token tables declarative enough to support it later.

## Testing

One additional case per change, added to the existing test files (no new test files):

- `tests/ext/test_chemistry_integration.py`: `H₂O` parses identically to `H2O`; a reaction with
  `⇌` and one with `<=>` both balance and set `reversible=True`; a `->` reaction still sets
  `reversible=False`.
- `tests/ext/test_grammar.py`: `2 × 3 = ?` and `6 ÷ 2 = ?`; `√(x^2 + y^2)` and bare `√x`; assigning
  to `sqrt` raises `GrammarError`.
- Each touched function keeps or gains one runnable doctest example (pytest runs
  `--doctest-modules`).

## Risks / open questions

- None blocking. The one judgment call (making `sqrt` a reserved word rather than a plain
  identifier) is narrow enough to revisit painlessly in Phase 2 if general function calls make the
  special case redundant.
