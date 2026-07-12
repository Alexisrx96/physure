# MKML built-in math functions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a general `name(args)` call-syntax mechanism to MKML (`measurekit/ext/grammar.py`) with a fixed dispatch table of built-in math functions (`abs`, `sqrt`, `round`, `floor`, `ceil`, `min`, `max`, `sin`, `cos`, `tan`, `exp`, `log`, `ln`), migrating the existing hardcoded `sqrt(...)` special case into it. Both `.mkml` file evaluation and the REPL use this automatically since both call `GrammarInterpreter` in this one file.

**Architecture:** Add a `,` token, a `_call_args()` parser method, and a module-level `_FUNCTIONS` dispatch table (`name -> (min_arity, max_arity, callable)`) to `measurekit/ext/grammar.py`. `_ExprParser._atom()` gets one new branch: an `IDENT` token whose value is a `_FUNCTIONS` key and is immediately followed by `(` is parsed as a call and dispatched. Every dispatched callable delegates to Python builtins (`abs`, `round`, `min`, `max`, `math.floor`, `math.ceil`) or to `Quantity`'s own bound methods (`.sin()`, `.cos()`, `.tan()`, `.exp()`, `.log()`, with a `math` module fallback for bare numbers) — no new unit-handling logic is written; `Quantity` already does it correctly.

**Tech Stack:** Pure Python, stdlib `math` only (already imported in the file). No new dependencies.

**Reference spec:** `docs/superpowers/specs/2026-07-11-mkml-math-functions-design.md`

---

## Task 1: Call-syntax mechanism (comma token, `_call_args`, `_FUNCTIONS` table, dispatch in `_atom`) — proven with `abs` and migrated `sqrt`

**Files:**
- Modify: `measurekit/ext/grammar.py:57` (`_OP_PAT`)
- Modify: `measurekit/ext/grammar.py:195-230` (`_ExprParser._atom`)
- Modify: `measurekit/ext/grammar.py:233-238` (insert new module-level helpers after `_to_number`)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing tests**

Add to `tests/ext/test_grammar.py` (append at the end of the file):

```python
def test_abs_function(mn):
    result = mn.eval("abs(-3 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function_on_bare_number(mn):
    assert mn.eval("abs(-5)") == 5


def test_function_call_wrong_arity_raises(mn):
    with pytest.raises(GrammarError, match="abs"):
        mn.eval("abs(1 m, 2 m)")


def test_sqrt_function_still_works_after_migration(mn):
    # Regression: sqrt(...) used to be a hardcoded special case in _atom();
    # it now goes through the generic _FUNCTIONS dispatch table instead.
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "abs_function or wrong_arity" -v`
Expected: FAIL — `abs` is not a registered unit, so `abs(-3 m)` currently raises `UnknownUnitError` (or similar) since `abs` resolves as a bare identifier, not a function call.

- [ ] **Step 3: Add the comma token**

In `measurekit/ext/grammar.py`, change line 57 from:

```python
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷]"  # noqa: RUF001
```

to:

```python
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷,]"  # noqa: RUF001
```

- [ ] **Step 4: Add `_transcendental`, `_check_arity`, and `_FUNCTIONS` module-level**

In `measurekit/ext/grammar.py`, insert immediately after the `_to_number` function (after line 237, before `class GrammarInterpreter:` on line 240):

```python
def _transcendental(x: GrammarValue, name: str) -> GrammarValue:
    """Calls `x.<name>()` (Quantity) or falls back to `math.<name>(x)`."""
    if hasattr(x, name):
        return getattr(x, name)()
    return getattr(math, name)(x)


def _check_arity(
    name: str, args: list[GrammarValue], lo: int, hi: float
) -> None:
    if lo <= len(args) <= hi:
        return
    if hi == math.inf:
        expected = f"at least {lo}"
    elif lo == hi:
        expected = str(lo)
    else:
        expected = f"{lo}-{int(hi)}"
    raise GrammarError(f"{name}() expects {expected} argument(s), got {len(args)}")


# name -> (min_arity, max_arity, implementation). Dispatched from
# _ExprParser._atom() whenever an IDENT token here is immediately followed
# by "(". Delegates to Quantity's own dunder/bound methods wherever
# possible; no unit-handling logic lives here.
# ponytail: "min" shadows the pre-existing "min" unit alias (minutes, see
# measurekit.conf:123). Only affects the narrow case of writing `min(` with
# no operator meaning "N minutes times (...)"; that now raises an arity
# error instead of silently misparsing. Accepted trade-off, confirmed with
# user rather than renaming the function.
_FUNCTIONS: dict[str, tuple[int, float, Callable[..., GrammarValue]]] = {
    "abs": (1, 1, lambda x: abs(x)),
    "sqrt": (1, 1, lambda x: x**0.5),
}
```

- [ ] **Step 5: Add `_call_args()` and the dispatch branch to `_atom()`**

In `measurekit/ext/grammar.py`, replace the `_atom` method (current lines 195-230):

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

with:

```python
    def _atom(self) -> GrammarValue:
        tok = self._peek()
        if tok is None:
            raise GrammarError("Unexpected end of expression")
        if tok.type == "SQRT":
            self._next()
            operand = self._atom()
            return operand**0.5
        if (
            tok.type == "IDENT"
            and tok.value in _FUNCTIONS
            and self._i + 1 < len(self._tokens)
            and self._tokens[self._i + 1].value == "("
        ):
            name = tok.value
            self._next()
            args = self._call_args()
            lo, hi, fn = _FUNCTIONS[name]
            _check_arity(name, args, lo, hi)
            return fn(*args)
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

    def _call_args(self) -> list[GrammarValue]:
        self._next()  # consume "("
        args: list[GrammarValue] = []
        tok = self._peek()
        if tok is not None and tok.value == ")":
            self._next()
            return args
        args.append(self._sum())
        while (tok := self._peek()) and tok.value == ",":
            self._next()
            args.append(self._sum())
        closing = self._peek()
        if closing is None or closing.value != ")":
            raise GrammarError("Missing closing parenthesis in function call")
        self._next()
        return args
```

Note: the bare `√` prefix branch (`tok.type == "SQRT"`) is unchanged — it is a distinct tokenizer rule from the word `"sqrt"` and is unaffected by this migration.

- [ ] **Step 6: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS — all tests including the new ones and the pre-existing `test_sqrt_unicode_prefix_parenthesized`, `test_sqrt_unicode_prefix_bare`, `test_sqrt_ascii_function_form`, `test_sqrt_is_reserved_assignment_target`.

- [ ] **Step 7: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add MKML function-call syntax, migrate sqrt into dispatch table"
```

---

## Task 2: `round`, `floor`, `ceil`

**Files:**
- Modify: `measurekit/ext/grammar.py` (`_FUNCTIONS` dict added in Task 1)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing tests**

```python
def test_round_function(mn):
    result = mn.eval("round(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 4)


def test_round_function_with_ndigits(mn):
    result = mn.eval("round(3.14159 m, 2)")
    assert math.isclose(result.to("m").magnitude, 3.14)


def test_floor_function(mn):
    result = mn.eval("floor(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_ceil_function(mn):
    result = mn.eval("ceil(3.2 m)")
    assert math.isclose(result.to("m").magnitude, 4)
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "round_function or floor_function or ceil_function" -v`
Expected: FAIL — `round`, `floor`, `ceil` are not yet in `_FUNCTIONS`, so they resolve as bare identifiers and raise (likely `UnknownUnitError`).

- [ ] **Step 3: Add the three entries to `_FUNCTIONS`**

In `measurekit/ext/grammar.py`, extend the `_FUNCTIONS` dict added in Task 1:

```python
_FUNCTIONS: dict[str, tuple[int, float, Callable[..., GrammarValue]]] = {
    "abs": (1, 1, lambda x: abs(x)),
    "sqrt": (1, 1, lambda x: x**0.5),
    "round": (1, 2, lambda *a: round(*a)),
    "floor": (1, 1, math.floor),
    "ceil": (1, 1, math.ceil),
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add round, floor, ceil to MKML functions"
```

---

## Task 3: `min`, `max`

**Files:**
- Modify: `measurekit/ext/grammar.py` (`_FUNCTIONS` dict)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing tests**

```python
def test_min_function_cross_unit(mn):
    # 200 cm == 2 m, so the smaller of (3 m, 200 cm) is 200 cm/2 m.
    result = mn.eval("min(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 2)


def test_max_function_cross_unit(mn):
    result = mn.eval("max(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_min_function_incompatible_units_raises(mn):
    with pytest.raises(IncompatibleUnitsError):
        mn.eval("min(3 m, 2 s)")


def test_min_function_variadic(mn):
    result = mn.eval("min(5 m, 1 m, 3 m)")
    assert math.isclose(result.to("m").magnitude, 1)
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "min_function or max_function" -v`
Expected: FAIL — `min`/`max` not yet in `_FUNCTIONS`; additionally, bare `min` currently resolves to the "minute" unit, so `min(3 m, 200 cm)` would fail while trying to implicitly-multiply the minute unit by a parenthesized comma expression.

- [ ] **Step 3: Add the two entries to `_FUNCTIONS`**

```python
    "min": (2, math.inf, lambda *a: min(*a)),
    "max": (2, math.inf, lambda *a: max(*a)),
```

(append inside the same `_FUNCTIONS` dict literal from Task 2)

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS. Also re-run the full unit-alias regression: `uv run pytest tests/ -k minute -v` should still pass unchanged (bare `min` as a unit, e.g. `5 min`, is untouched — only `min(` immediately followed by `(` is now a function call).

- [ ] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add min, max to MKML functions"
```

---

## Task 4: `sin`, `cos`, `tan`

**Files:**
- Modify: `measurekit/ext/grammar.py` (`_FUNCTIONS` dict)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing tests**

Add `DimensionError` to the existing import block at the top of `tests/ext/test_grammar.py`:

```python
from measurekit.domain.exceptions import (
    DimensionError,
    IncompatibleUnitsError,
    UnknownUnitError,
)
```

Then append:

```python
def test_sin_function_dimensionless(mn):
    assert math.isclose(mn.eval("sin(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_angle_unit(mn):
    result = mn.eval("sin(90 deg)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_cos_function_angle_unit(mn):
    result = mn.eval("cos(0 rad)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_tan_function_dimensionless(mn):
    assert math.isclose(mn.eval("tan(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("sin(3 kg)")
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "sin_function or cos_function or tan_function" -v`
Expected: FAIL — `sin`/`cos`/`tan` not yet in `_FUNCTIONS`.

- [ ] **Step 3: Add the three entries to `_FUNCTIONS`**

```python
    "sin": (1, 1, lambda x: _transcendental(x, "sin")),
    "cos": (1, 1, lambda x: _transcendental(x, "cos")),
    "tan": (1, 1, lambda x: _transcendental(x, "tan")),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add sin, cos, tan to MKML functions"
```

---

## Task 5: `exp`, `log`, `ln`

**Files:**
- Modify: `measurekit/ext/grammar.py` (`_FUNCTIONS` dict)
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing tests**

```python
def test_exp_function_dimensionless(mn):
    assert math.isclose(mn.eval("exp(0)"), 1.0)


def test_log_function_dimensionless(mn):
    assert math.isclose(mn.eval("log(1)"), 0.0, abs_tol=1e-12)


def test_ln_function_is_natural_log(mn):
    # ln and log are the same implementation (no log10 exists in the engine).
    assert mn.eval("ln(1)") == mn.eval("log(1)")


def test_log_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("log(3 kg)")
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "exp_function or log_function or ln_function" -v`
Expected: FAIL — `exp`/`log`/`ln` not yet in `_FUNCTIONS`.

- [ ] **Step 3: Add the three entries to `_FUNCTIONS`**

```python
    "exp": (1, 1, lambda x: _transcendental(x, "exp")),
    "log": (1, 1, lambda x: _transcendental(x, "log")),
    "ln": (1, 1, lambda x: _transcendental(x, "log")),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add exp, log, ln to MKML functions"
```

---

## Task 6: Generalize the reserved-word check in `_split_assignment`

**Files:**
- Modify: `measurekit/ext/grammar.py:333-334`
- Test: `tests/ext/test_grammar.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/ext/test_grammar.py`:

```python
@pytest.mark.parametrize("name", sorted(_FUNCTIONS))
def test_function_names_are_reserved_assignment_targets(mn, name):
    with pytest.raises(GrammarError):
        mn.eval(f"{name} = 5 m")
```

This requires importing `_FUNCTIONS` in the test file. Add to the existing import from `measurekit.ext.grammar`:

```python
from measurekit.ext.grammar import _FUNCTIONS, GrammarError, GrammarInterpreter, evaluate
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/ext/test_grammar.py -k test_function_names_are_reserved_assignment_targets -v`
Expected: FAIL for every name except `sqrt` (e.g. `abs = 5 m`, `min = 5 m` do not currently raise — only `sqrt` is special-cased).

- [ ] **Step 3: Generalize the check**

In `measurekit/ext/grammar.py`, replace lines 333-334:

```python
        if lhs_tokens[0].value == "sqrt":
            raise GrammarError(f"'sqrt' is reserved in: {stmt!r}")
```

with:

```python
        if lhs_tokens[0].value in _FUNCTIONS:
            raise GrammarError(f"{lhs_tokens[0].value!r} is reserved in: {stmt!r}")
```

- [ ] **Step 4: Remove the now-redundant single-purpose sqrt test**

`test_sqrt_is_reserved_assignment_target` in `tests/ext/test_grammar.py` is now a special case of the parametrized test added in Step 1. Delete it to avoid duplicate coverage:

```python
def test_sqrt_is_reserved_assignment_target(mn):
    with pytest.raises(GrammarError):
        mn.eval("sqrt = 5 m")
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS — all 13 parametrized cases plus the full existing suite.

- [ ] **Step 6: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "refactor: generalize MKML reserved-word check to all _FUNCTIONS names"
```

---

## Task 7: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite with coverage**

Run: `uv run pytest --cov=measurekit --cov-report=term-missing`
Expected: all tests pass; `measurekit/ext/grammar.py` coverage should be at or near 100% given every new branch has a dedicated test; total coverage stays ≥ 80% (`fail_under = 80` in `pyproject.toml`).

- [ ] **Step 2: Lint and format**

Run: `uv run ruff check .`
Expected: no errors.

Run: `uv run ruff format --check .`
Expected: no reformatting needed (if it reports files, run `uv run ruff format .` and re-commit).

- [ ] **Step 3: Type check (advisory)**

Run: `uv run ty check measurekit/ext/grammar.py`
Expected: no *new* errors introduced by this change (per CLAUDE.md, `ty` is advisory — don't add new errors to touched files, but the ~900 pre-existing repo-wide errors are not this task's concern).

- [ ] **Step 4: Doctest check**

Run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS — the module docstring's existing doctest example is unaffected by this change.

- [ ] **Step 5: Manual smoke test via REPL**

Run: `echo 'abs(-3 m) => m' | uv run python -m measurekit`
Expected output: `3.0 m` (or equivalent representation) confirming the REPL path (`measurekit/repl.py` → `GrammarInterpreter`) picks up the new functions with zero changes to `repl.py`.

- [ ] **Step 6: If SonarQube is configured locally, run the gate check**

Run: `make sonar` (requires `.env` with `SONAR_TOKEN` and a local server at `http://localhost:9000`; skip if not configured in this environment)
Expected: quality gate green on new code (coverage ≥ 80%, duplication ≤ 3%, zero new violations).
