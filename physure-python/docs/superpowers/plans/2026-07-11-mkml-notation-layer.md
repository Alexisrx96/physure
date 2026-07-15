# MKML Notation Layer (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Unicode scientific notation to physure's grammar/chemistry parsers — subscript digits in chemical formulas, the `⇌` equilibrium arrow, `×`/`÷` operators, and a `√` prefix operator — each with a keyboard-typable ASCII fallback, per the approved spec at `docs/superpowers/specs/2026-07-11-notation-layer-design.md`.

**Architecture:** Four small, independent additions to existing hand-rolled regex tokenizers (no new files, no new dependencies). A single new helper, `subscript_to_ascii()`, normalizes Unicode subscript digits to ASCII before existing formula/reaction-term regexes run. `×`/`÷` are normalized to `*`/`/` at MKML tokenize time. `√`/`sqrt(...)` is handled as a narrow special case in `_ExprParser._atom()` returning `operand ** 0.5`, which already works correctly on unit-bearing `Quantity` values (verified live: `Q_(9, "m^2") ** 0.5 == 3.0 m`) with zero changes to `CompoundUnit`/`Quantity`.

**Tech Stack:** Pure Python (stdlib `re` only), pytest with `--doctest-modules`.

**Note on the spec vs. actual code:** Two details in the committed spec don't match current code, corrected here:
- Spec says `grammar.py::_TOKEN_SPEC` has separate MUL/DIV patterns — that structure exists in `lexer.py`, not `grammar.py`. `grammar.py` has one `OP` token kind; `×`/`÷` are added to `_OP_PAT`'s character class and normalized to `*`/`/` at tokenize time.
- Spec says `reversible: bool = False` goes on the `Reaction` *dataclass* — `Reaction` is a plain class using `__slots__`, not a dataclass. `reversible` is threaded through `__slots__`, `__init__`, and `from_string()` manually.
- Spec's Testing section names `tests/ext/test_chemistry_integration.py` — that file is the Hess's-law cross-module suite. New chemistry test cases go in `tests/ext/test_species.py` and `tests/ext/test_reaction.py` instead.

---

### Task 1: `subscript_to_ascii()` helper

**Files:**
- Modify: `physure/domain/notation/lexer.py:14-17`
- Test: `tests/notation_tests/test_lexer.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/notation_tests/test_lexer.py`, extend the import at the top and add a new test function:

```python
from physure.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
    parse_superscript,
    subscript_to_ascii,
    to_subscript,
    to_superscript,
)
```

```python
def test_subscript_to_ascii():
    """Test normalizing Unicode subscript digits back to ASCII."""
    assert subscript_to_ascii("H₂O") == "H2O"
    assert subscript_to_ascii("C₆H₁₂O₆") == "C6H12O6"
    assert subscript_to_ascii("CO₋₁") == "CO-1"
    assert subscript_to_ascii("H2O") == "H2O"  # already ASCII: no-op
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/notation_tests/test_lexer.py::test_subscript_to_ascii -xvs`
Expected: FAIL with `ImportError: cannot import name 'subscript_to_ascii'`

- [ ] **Step 3: Write minimal implementation**

In `physure/domain/notation/lexer.py`, immediately after the existing `_SUBSCRIPT_MAP` line (line 17: `_SUBSCRIPT_MAP = str.maketrans("0123456789-", "₀₁₂₃₄₅₆₇₈₉₋")`), add:

```python
_SUBSCRIPT_REVERSE_MAP = str.maketrans("₀₁₂₃₄₅₆₇₈₉₋", "0123456789-")


def subscript_to_ascii(s: str) -> str:
    """Normalize Unicode subscript digits to ASCII.

    Example:
        >>> subscript_to_ascii("H₂O")
        'H2O'
    """
    return s.translate(_SUBSCRIPT_REVERSE_MAP)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/notation_tests/test_lexer.py::test_subscript_to_ascii -xvs`
Expected: PASS

Also run the doctest: `uv run pytest --doctest-modules physure/domain/notation/lexer.py -v`
Expected: PASS (new `subscript_to_ascii` doctest included)

- [ ] **Step 5: Commit**

```bash
git add physure/domain/notation/lexer.py tests/notation_tests/test_lexer.py
git commit -m "feat: add subscript_to_ascii normalization helper"
```

---

### Task 2: Wire `subscript_to_ascii()` into `species.py::parse_formula`

**Files:**
- Modify: `physure/ext/chemistry/species.py:1-11` (imports), `:163-188` (`parse_formula`)
- Test: `tests/ext/test_species.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/ext/test_species.py`, after `test_parse_simple_formula`:

```python
def test_parse_unicode_subscript_formula():
    assert parse_formula("H₂O") == {"H": 2, "O": 1}
    assert parse_formula("C₆H₁₂O₆") == {"C": 6, "H": 12, "O": 6}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_species.py::test_parse_unicode_subscript_formula -xvs`
Expected: FAIL — `parse_formula("H₂O")` raises `ValueError: Invalid formula: 'H₂O'` (the `₂` isn't ASCII `\d`, so `_TOKEN_RE` can't consume it and the position check at the end of `parse_formula` fails).

- [ ] **Step 3: Write minimal implementation**

In `physure/ext/chemistry/species.py`, add the import (after the existing `import re` at line 10):

```python
import re
from typing import TYPE_CHECKING

from physure.domain.notation.lexer import subscript_to_ascii
```

In `parse_formula` (currently at line 163), normalize the input as the first line of the function body, and add a doctest example:

```python
def parse_formula(formula: str) -> dict[str, int]:
    """Parses a chemical formula into element -> atom-count.

    Examples:
        >>> parse_formula("H2O")
        {'H': 2, 'O': 1}
        >>> parse_formula("Ca(NO3)2")
        {'Ca': 1, 'N': 2, 'O': 6}
        >>> parse_formula("H₂O")
        {'H': 2, 'O': 1}
    """
    formula = subscript_to_ascii(formula)
    stack: list[dict[str, int]] = [{}]
    pos = 0
    for match in _TOKEN_RE.finditer(formula):
        if match.start() != pos:
            raise ValueError(f"Invalid formula: {formula!r}")
        pos = match.end()
        element, count, open_paren, close_paren, group_count = match.groups()
        if open_paren:
            stack.append({})
        elif close_paren:
            _close_group(stack, group_count, formula)
        elif element:
            _add_element(stack, element, count, formula)

    if pos != len(formula) or len(stack) != 1 or not stack[0]:
        raise ValueError(f"Invalid formula: {formula!r}")
    return stack[0]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/ext/test_species.py::test_parse_unicode_subscript_formula -xvs`
Expected: PASS

Also run: `uv run pytest tests/ext/test_species.py -v` and `uv run pytest --doctest-modules physure/ext/chemistry/species.py -v`
Expected: all PASS (no regressions, new doctest line included)

- [ ] **Step 5: Commit**

```bash
git add physure/ext/chemistry/species.py tests/ext/test_species.py
git commit -m "feat: accept Unicode subscript digits in chemical formulas"
```

---

### Task 3: Wire `subscript_to_ascii()` into `reaction.py::_parse_species`

**Files:**
- Modify: `physure/ext/chemistry/reaction.py:13-28` (imports + `_parse_species`)
- Test: `tests/ext/test_reaction.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/ext/test_reaction.py`, after `test_balance_matches_textbook_stoichiometry`:

```python
def test_unicode_subscript_formula_in_reaction():
    rxn = Reaction.from_string("H₂ + O₂ -> H₂O")
    assert rxn.reactant_coeffs == [2, 1]
    assert rxn.product_coeffs == [2]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_reaction.py::test_unicode_subscript_formula_in_reaction -xvs`
Expected: FAIL — `_parse_species("H₂")` raises `ValueError: Invalid reaction term: 'H₂'` because `Species("H₂")` fails inside `parse_formula` the same way as Task 2 before its fix (note: `species.py` is already fixed by Task 3's point in the plan, but `reaction.py`'s own `_TERM_RE` still needs the subscript stripped first — `_TERM_RE = re.compile(r"^\s*\d*\s*([A-Za-z0-9()]+)\s*$")` doesn't match `₂` either, so the term itself fails to match before `Species()` is ever called).

- [ ] **Step 3: Write minimal implementation**

In `physure/ext/chemistry/reaction.py`, add the import. Current imports (lines 13-25):

```python
from __future__ import annotations

import math
import re
from dataclasses import dataclass
from fractions import Fraction
from typing import TYPE_CHECKING

from physure.ext.chemistry.equivalency import mass_to_moles, moles_to_mass
from physure.ext.chemistry.species import Species

if TYPE_CHECKING:
    from physure.domain.measurement.quantity import Quantity
```

Add `subscript_to_ascii` to the imports:

```python
from physure.domain.notation.lexer import subscript_to_ascii
from physure.ext.chemistry.equivalency import mass_to_moles, moles_to_mass
from physure.ext.chemistry.species import Species
```

Update `_parse_species` (currently lines 35-39):

```python
def _parse_species(term: str) -> Species:
    match = _TERM_RE.match(subscript_to_ascii(term))
    if not match:
        raise ValueError(f"Invalid reaction term: {term!r}")
    return Species(match.group(1))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/ext/test_reaction.py::test_unicode_subscript_formula_in_reaction -xvs`
Expected: PASS

Also run: `uv run pytest tests/ext/test_reaction.py -v`
Expected: all PASS (no regressions)

- [ ] **Step 5: Commit**

```bash
git add physure/ext/chemistry/reaction.py tests/ext/test_reaction.py
git commit -m "feat: accept Unicode subscript digits in reaction terms"
```

---

### Task 4: Equilibrium arrow `⇌`, ASCII fallback `<=>`

**Files:**
- Modify: `physure/ext/chemistry/reaction.py:27` (`_ARROW_RE`), `:131-160` (`Reaction` class)
- Test: `tests/ext/test_reaction.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/ext/test_reaction.py`, after `test_unicode_subscript_formula_in_reaction` (added in Task 3):

```python
def test_equilibrium_arrow_sets_reversible():
    rxn = Reaction.from_string("N2 + 3 H2 ⇌ 2 NH3")
    assert rxn.reversible is True
    assert rxn.reactant_coeffs == [1, 3]
    assert rxn.product_coeffs == [2]


def test_ascii_equilibrium_arrow_sets_reversible():
    rxn = Reaction.from_string("N2 + 3 H2 <=> 2 NH3")
    assert rxn.reversible is True


def test_irreversible_arrow_leaves_reversible_false():
    rxn = Reaction.from_string("H2 + O2 -> H2O")
    assert rxn.reversible is False
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_reaction.py::test_equilibrium_arrow_sets_reversible -xvs`
Expected: FAIL — `⇌` isn't in `_ARROW_RE`, so `_ARROW_RE.split(equation)` returns a list of length 1 (no split occurred), and `from_string` raises `ValueError: Invalid reaction equation`.

- [ ] **Step 3: Write minimal implementation**

Widen `_ARROW_RE` (currently line 27):

```python
_ARROW_RE = re.compile(r"<=>|->|=|→|⇌")
```

Update the `__slots__` tuple (currently line 140):

```python
__slots__ = (
    "product_coeffs",
    "products",
    "reactant_coeffs",
    "reactants",
    "reversible",
)
```

Update `__init__` (currently lines 142-150):

```python
def __init__(
    self,
    reactants: list[Species],
    products: list[Species],
    reversible: bool = False,
) -> None:
    self.reactants = reactants
    self.products = products
    self.reversible = reversible
    self.reactant_coeffs, self.product_coeffs = _balance(
        reactants, products
    )
```

Update the class docstring and `from_string` (currently lines 131-160) to switch from `.split()` to `.search()` + span slicing, and set `reversible` from the matched arrow:

```python
class Reaction:
    """A balanced chemical reaction.

    Examples:
        >>> rxn = Reaction.from_string("H2 + O2 -> H2O")
        >>> rxn.reactant_coeffs, rxn.product_coeffs
        ([2, 1], [2])
        >>> Reaction.from_string("N2 + 3 H2 ⇌ 2 NH3").reversible
        True
    """

    __slots__ = (
        "product_coeffs",
        "products",
        "reactant_coeffs",
        "reactants",
        "reversible",
    )

    def __init__(
        self,
        reactants: list[Species],
        products: list[Species],
        reversible: bool = False,
    ) -> None:
        self.reactants = reactants
        self.products = products
        self.reversible = reversible
        self.reactant_coeffs, self.product_coeffs = _balance(
            reactants, products
        )

    @classmethod
    def from_string(cls, equation: str) -> Reaction:
        """Parses e.g. "2 H2 + O2 -> 2 H2O" (coefficients are re-derived).

        A `->`, `=`, or `→` arrow yields an irreversible reaction; `<=>` or
        `⇌` sets `reversible=True`.
        """
        match = _ARROW_RE.search(equation)
        if not match:
            raise ValueError(f"Invalid reaction equation: {equation!r}")
        lhs, rhs = equation[: match.start()], equation[match.end() :]
        reactants = [_parse_species(t) for t in _split_terms(lhs)]
        products = [_parse_species(t) for t in _split_terms(rhs)]
        reversible = match.group() in ("<=>", "⇌")
        return cls(reactants, products, reversible=reversible)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/ext/test_reaction.py -v`
Expected: all PASS, including the three new tests and every pre-existing test (`test_malformed_equation_raises` and `test_empty_term_raises` must still pass — `.search()` + span slicing produces identical `lhs`/`rhs` to the old `.split()` for all existing inputs).

Also run: `uv run pytest --doctest-modules physure/ext/chemistry/reaction.py -v`
Expected: PASS (new doctest line included)

- [ ] **Step 5: Commit**

```bash
git add physure/ext/chemistry/reaction.py tests/ext/test_reaction.py
git commit -m "feat: add equilibrium arrow (⇌, <=>) with reversible flag"
```

---

### Task 5: `×` `÷` as multiplication/division in MKML

**Files:**
- Modify: `physure/ext/grammar.py:53-68` (tokenizer), `:83-95` (`_tokenize`)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/ext/test_grammar.py`, after `test_bare_expression`:

```python
def test_unicode_multiplication_and_division_operators(mn):
    assert mn.eval("2 × 3") == 6
    assert mn.eval("6 ÷ 2") == 3
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_grammar.py::test_unicode_multiplication_and_division_operators -xvs`
Expected: FAIL — `×` isn't matched by any token pattern, so `_tokenize` falls through to the `BAD` group and raises `GrammarError: Unexpected character '×' at column 2 in: '2 × 3'`.

- [ ] **Step 3: Write minimal implementation**

Widen `_OP_PAT` (currently line 56) to accept `×`/`÷` in the operator character class:

```python
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷]"
```

Add an alias table right after the `_TOKEN_RE` definition (after line 68), and use it in `_tokenize` (currently lines 83-95) to normalize the matched value before constructing the `Token` — this keeps `_ExprParser` completely unchanged, since it only ever sees `*`/`/`:

```python
_OP_ALIASES = {"×": "*", "÷": "/"}


class Token(NamedTuple):
```

```python
def _tokenize(stmt: str) -> list[Token]:
    tokens = []
    for m in _TOKEN_RE.finditer(stmt):
        kind = m.lastgroup or "BAD"
        if kind == "WS":
            continue
        if kind == "BAD":
            raise GrammarError(
                f"Unexpected character {m.group()!r} at column {m.start()} "
                f"in: {stmt!r}"
            )
        value = _OP_ALIASES.get(m.group(), m.group())
        tokens.append(Token(kind, value, m.start()))
    return tokens
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/ext/test_grammar.py::test_unicode_multiplication_and_division_operators -xvs`
Expected: PASS

Also run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: all PASS (no regressions)

- [ ] **Step 5: Commit**

```bash
git add physure/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: accept × and ÷ as multiplication/division in MKML"
```

---

### Task 6: `√` prefix operator and `sqrt(...)` ASCII fallback

**Files:**
- Modify: `physure/ext/grammar.py:53-68` (tokenizer), `:189-215` (`_atom`), `:303-318` (`_split_assignment`)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing test**

Add to `tests/ext/test_grammar.py`, after `test_unicode_multiplication_and_division_operators` (added in Task 5):

```python
def test_sqrt_unicode_prefix_parenthesized(mn):
    result = mn.eval("√(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_sqrt_unicode_prefix_bare(mn):
    mn.run("x = 16")
    assert math.isclose(mn.eval("√x"), 4)


def test_sqrt_ascii_function_form(mn):
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_sqrt_is_reserved_assignment_target(mn):
    with pytest.raises(GrammarError):
        mn.eval("sqrt = 5 m")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_grammar.py::test_sqrt_unicode_prefix_parenthesized -xvs`
Expected: FAIL — `√` isn't matched by any token pattern, so `_tokenize` raises `GrammarError: Unexpected character '√' at column 0 in: '√(9 m^2)'`.

- [ ] **Step 3: Write minimal implementation**

Add a `SQRT` token pattern next to `_SUP_PAT` (currently line 55), and add it to `_TOKEN_RE`'s alternation (currently lines 57-68):

```python
_SUP_PAT = r"[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+"
_SQRT_PAT = r"√"
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷]"
_TOKEN_RE = re.compile(
    "|".join(
        (
            f"(?P<NUMBER>{_NUMBER_PAT})",
            f"(?P<IDENT>{_IDENT_PAT})",
            f"(?P<SUP>{_SUP_PAT})",
            f"(?P<SQRT>{_SQRT_PAT})",
            f"(?P<OP>{_OP_PAT})",
            r"(?P<WS>[ \t]+)",
            r"(?P<BAD>.)",
        )
    )
)
```

In `_ExprParser._atom()` (currently lines 189-215), add the `√`/`sqrt(...)` handling as the *first* check, before the `(` check:

```python
def _atom(self) -> GrammarValue:
    tok = self._peek()
    if tok is None:
        raise GrammarError("Unexpected end of expression")
    if tok.type == "SQRT" or (
        tok.type == "IDENT"
        and tok.value == "sqrt"
        and self._i + 1 < len(self._tokens)
        and self._tokens[self._i + 1].value == "("
    ):
        self._next()
        operand = self._atom()
        return operand**0.5
    if tok.value == "(":
        self._next()
        result = self._sum()
        closing = self._peek()
        if closing is None or closing.value != ")":
            raise GrammarError("Missing closing parenthesis")
        self._next()
        return result
    if tok.type == "NUMBER":
        self._next()
        value = _to_number(tok.value)
        nxt = self._peek()
        if nxt and nxt.value in ("+/-", "±"):
            self._next()
            err = self._next()
            if err.type != "NUMBER":
                raise GrammarError("Expected a number after '+/-'")
            return self._q(value, None, uncertainty=_to_number(err.value))
        return value
    if tok.type == "IDENT":
        self._next()
        return self._resolve(tok.value)
    raise GrammarError(f"Unexpected token {tok.value!r}")
```

In `GrammarInterpreter._split_assignment()` (currently lines 303-318), reject `sqrt` as an assignment target:

```python
@staticmethod
def _split_assignment(
    tokens: list[Token], stmt: str
) -> tuple[list[Token], str | None]:
    """Strips a leading `name =` / `name ->`; returns tokens and name."""
    assign_idx = _top_level_index(tokens, "=")
    if assign_idx == -1:
        assign_idx = _top_level_index(tokens, "->")
    if assign_idx == -1:
        return tokens, None
    lhs_tokens = tokens[:assign_idx]
    if len(lhs_tokens) != 1 or lhs_tokens[0].type != "IDENT":
        raise GrammarError(
            f"Assignment target must be a single name in: {stmt!r}"
        )
    if lhs_tokens[0].value == "sqrt":
        raise GrammarError(f"'sqrt' is reserved in: {stmt!r}")
    return tokens[assign_idx + 1 :], lhs_tokens[0].value
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: all PASS, including the four new tests and every pre-existing test.

- [ ] **Step 5: Commit**

```bash
git add physure/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add sqrt prefix operator (√, sqrt(...)) to MKML"
```

---

### Task 7: Rename MNML → MKML in `grammar.py` docs/comments

**Files:**
- Modify: `physure/ext/grammar.py:1-30` (module docstring), `:112-116` (`_ExprParser` docstring), `:226-231` (`GrammarInterpreter` docstring)

This is a pure documentation/naming change — no behavior changes, no new tests. Existing doctests must still pass unmodified since only prose changes, not code or examples.

- [ ] **Step 1: Update the module docstring**

Lines 1-3, from:

```python
"""MNML grammar extension: evaluate MeasureNote-style engineering notes.

Implements the core of the MeasureNote Meta-Language (MNML) as a
```

to:

```python
"""MKML grammar extension: evaluate Physure-style engineering notes.

Implements the core of the Physure Meta-Lang (MKML) as a
```

- [ ] **Step 2: Update the `_ExprParser` class docstring**

Line 112, from:

```python
    """Recursive-descent expression parser mirroring MNML precedence.
```

to:

```python
    """Recursive-descent expression parser mirroring MKML precedence.
```

- [ ] **Step 3: Update the `GrammarInterpreter` class docstring**

Line 226, from:

```python
    """Stateful interpreter for MNML statements.
```

to:

```python
    """Stateful interpreter for MKML statements.
```

- [ ] **Step 4: Run the full test suite and doctests to confirm no regressions**

Run: `uv run pytest tests/ext/test_grammar.py --doctest-modules physure/ext/grammar.py -v`
Expected: all PASS (docstring-only change; no example text was altered)

- [ ] **Step 5: Commit**

```bash
git add physure/ext/grammar.py
git commit -m "docs: rename MNML to MKML in grammar.py docstrings"
```

---

### Task 8: Full verification pass

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `uv run pytest`
Expected: all tests PASS, including doctests (pytest runs `--doctest-modules` by default per `pyproject.toml`).

- [ ] **Step 2: Lint and format check**

Run: `uv run ruff check .` and `uv run ruff format --check .`
Expected: both clean (no errors).

- [ ] **Step 3: Confirm no coverage regression**

Run: `uv run pytest --cov=physure --cov-report=term-missing`
Expected: total coverage stays ≥ 80% (per CLAUDE.md code quality policy). All new code (the `subscript_to_ascii` helper, the widened `_ARROW_RE`/`reversible` path, the `×`/`÷` alias, and the `√`/`sqrt` branch) is exercised by the tests added in Tasks 1-6, so this should hold without extra tests.

No commit for this task — it's a verification checkpoint before considering Phase 1 complete.
