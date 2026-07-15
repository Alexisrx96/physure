# MKML User-Defined Functions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add user-defined functions (`f(x) = expr`), optional per-parameter dimension typing (`f(x: m) = expr`), `let...in` local bindings scoped to function bodies, recursion with a configurable depth limit, and display-text blocks (```` ```text``` ````) to MKML — plus a minimal ternary operator and comparison operators, added to the grammar so recursive functions (factorial, Fibonacci) can actually terminate.

**Architecture:** All work lands in the single existing file `measurekit/ext/grammar.py` (the "no-AST" recursive-descent interpreter shared by `.mkml` files and the REPL). Function bodies are stored as raw token lists (`UserFunction.body_tokens`) and re-parsed with a fresh `_ExprParser` on every call — the same "parse and eval in one pass" style already used everywhere else in this file. `let`/`in` and comparison/ternary syntax are recognized by adding new precedence levels to the existing recursive-descent expression grammar, not by introducing a separate reserved-word list.

**Tech Stack:** Pure Python (`measurekit/ext/grammar.py`), stdlib only (`operator`, `dataclasses`, `re`, `math`). No new dependencies.

---

## File Structure

- **Modify:** `measurekit/ext/grammar.py` — all 7 tasks land here: tokenizer regex, `_ExprParser` (ternary/comparison/let, user-function-call dispatch), `GrammarInterpreter` (function storage, definition parsing, recursion, typed-parameter binding, display-text extraction).
- **Modify:** `measurekit/infrastructure/config/measurekit.conf` — Task 3 adds the `mkml_recursion_limit` setting.
- **Modify:** `tests/ext/test_grammar.py` — new tests appended per task, following the file's existing `test_<feature>_<case>` naming and the `mn` fixture (bare `GrammarInterpreter()`).

No new files. This mirrors the structure of the prior MKML slice (math functions, PR #36/#37).

---

## Task 1: Ternary operator and comparison operators

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_comparison_operators(mn):
    assert mn.eval("3 < 5") is True
    assert mn.eval("3 > 5") is False
    assert mn.eval("3 <= 3") is True
    assert mn.eval("4 >= 5") is False
    assert mn.eval("3 != 4") is True


def test_ternary_true_branch(mn):
    assert mn.eval("1 < 2 ? 10 : 20") == 10


def test_ternary_false_branch(mn):
    assert mn.eval("1 > 2 ? 10 : 20") == 20


def test_ternary_nested_false_branch(mn):
    result = mn.eval("1 > 2 ? 1 : (2 > 3 ? 20 : 30)")
    assert result == 30


def test_ternary_with_quantities(mn):
    result = mn.eval("5 m > 3 m ? 5 m : 3 m")
    assert math.isclose(result.to("m").magnitude, 5)


def test_ternary_inside_function_call_args(mn):
    result = mn.eval("max(1 < 2 ? 5 : 1, 3)")
    assert result == 5
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "comparison or ternary" -v`
Expected: FAIL — `?`/`<`/`>` etc. are not recognized operators, or comparisons fall through to `GrammarError`.

- [x] **Step 3: Implement**

**3a. Add the `operator` import.** In `measurekit/ext/grammar.py`, change:

```python
import math
import re
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias
```

to:

```python
import math
import operator
import re
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias
```

**3b. Extend the tokenizer regex** to recognize the new operators. Change:

```python
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷,]"  # noqa: RUF001
```

to:

```python
_OP_PAT = r"\+/-|±|<=|>=|!=|==|=>|->|\*\*|[-+*/^()=?<>×÷,:]"  # noqa: RUF001
```

(The multi-char alternatives `<=`, `>=`, `!=` must come before the single-char class so the regex engine tries them first. The `:` is added now even though only Task 4/5 use it — it belongs to the same tokenizer edit.)

**3c. Add the comparison dispatch table.** Insert this immediately after `_top_level_index` (right before `class _ExprParser:`):

```python
_COMPARISONS: dict[str, Callable[[Any, Any], bool]] = {
    "<": operator.lt,
    ">": operator.gt,
    "<=": operator.le,
    ">=": operator.ge,
    "==": operator.eq,
    "!=": operator.ne,
}
```

**3d. Route `parse()` through the new top-level rule.** Change:

```python
    def parse(self) -> GrammarValue:
        result = self._sum()
        if self._i < len(self._tokens):
            tok = self._tokens[self._i]
            raise GrammarError(f"Unexpected token {tok.value!r} in expression")
        return result
```

to:

```python
    def parse(self) -> GrammarValue:
        result = self._expr()
        if self._i < len(self._tokens):
            tok = self._tokens[self._i]
            raise GrammarError(f"Unexpected token {tok.value!r} in expression")
        return result
```

**3e. Add the new precedence levels.** Insert these methods immediately after `parse()`, before `_peek()`:

```python
    def _expr(self) -> GrammarValue:
        return self._ternary()

    def _ternary(self) -> GrammarValue:
        cond = self._comparison()
        tok = self._peek()
        if not (tok and tok.value == "?"):
            return cond
        self._next()
        if cond:
            true_val = self._ternary()
            self._expect(":")
            self._discard_ternary()
            return true_val
        self._discard_ternary()
        self._expect(":")
        return self._ternary()

    def _comparison(self) -> GrammarValue:
        result = self._sum()
        tok = self._peek()
        if tok and tok.value in _COMPARISONS:
            self._next()
            rhs = self._sum()
            return _COMPARISONS[tok.value](result, rhs)
        return result

    def _expect(self, value: str) -> None:
        tok = self._peek()
        if tok is None or tok.value != value:
            raise GrammarError(f"Expected {value!r}")
        self._next()

    def _discard_ternary(self) -> None:
        self._ternary()
```

**3f. Route the parenthesized sub-expression through `_expr()`.** In `_atom()`, change:

```python
        if tok.value == "(":
            self._next()
            result = self._sum()
            closing = self._peek()
```

to:

```python
        if tok.value == "(":
            self._next()
            result = self._expr()
            closing = self._peek()
```

**3g. Route function-call arguments through `_expr()`.** In `_call_args()`, change:

```python
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

to:

```python
    def _call_args(self) -> list[GrammarValue]:
        self._next()  # consume "("
        args: list[GrammarValue] = []
        tok = self._peek()
        if tok is not None and tok.value == ")":
            self._next()
            return args
        args.append(self._expr())
        while (tok := self._peek()) and tok.value == ",":
            self._next()
            args.append(self._expr())
        closing = self._peek()
        if closing is None or closing.value != ")":
            raise GrammarError("Missing closing parenthesis in function call")
        self._next()
        return args
```

**3h. Extend the module docstring doctest.** Change:

```python
    g = 9.81 +/- 0.02 m/s^2  # uncertainty (also `±`)

Example:
    >>> from measurekit.ext.grammar import GrammarInterpreter
    >>> mn = GrammarInterpreter()
    >>> _ = mn.run('''
    ... force = 500 N
    ... area = 2 m^2
    ... stress = force / area
    ... ''')
    >>> mn.eval("stress == 250 Pa")
    True
"""
```

to:

```python
    g = 9.81 +/- 0.02 m/s^2  # uncertainty (also `±`)
    3 < 5                    # comparison -> bool
    1 < 2 ? 10 : 20          # ternary -> value

Example:
    >>> from measurekit.ext.grammar import GrammarInterpreter
    >>> mn = GrammarInterpreter()
    >>> _ = mn.run('''
    ... force = 500 N
    ... area = 2 m^2
    ... stress = force / area
    ... ''')
    >>> mn.eval("stress == 250 Pa")
    True
    >>> mn.eval("1 < 2 ? 10 : 20")
    10
"""
```

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v` (full file — Steps 3f/3g touch shared code paths, so run the whole file, not just the new tests)
Expected: PASS, all tests including pre-existing ones.

Also run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add ternary operator and comparison operators to MKML grammar"
```

---

## Task 2: User-defined function definitions and calls

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_user_function_basic_call(mn):
    mn.run("f(x) = x^2")
    assert mn.eval("f(3)") == 9


def test_user_function_multi_param(mn):
    mn.run("area(w, h) = w * h")
    result = mn.eval("area(3 m, 4 m)")
    assert math.isclose(result.to("m^2").magnitude, 12)


def test_user_function_wrong_arity_raises(mn):
    mn.run("f(x) = x^2")
    with pytest.raises(GrammarError, match="f"):
        mn.eval("f(1, 2)")


def test_user_function_shadowing_builtin_raises(mn):
    with pytest.raises(GrammarError):
        mn.eval("abs(x) = x")


def test_variable_then_function_namespace_collision(mn):
    mn.run("f = 5")
    with pytest.raises(GrammarError):
        mn.eval("f(x) = x^2")


def test_function_then_variable_namespace_collision(mn):
    mn.run("f(x) = x^2")
    with pytest.raises(GrammarError):
        mn.eval("f = 5")


def test_user_function_redefinition_allowed(mn):
    mn.run("f(x) = x^2")
    mn.run("f(x) = x^3")
    assert mn.eval("f(2)") == 8


def test_user_function_call_inside_larger_expression(mn):
    mn.run("f(x) = x + 1")
    assert mn.eval("f(2) * 3") == 9
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "user_function or namespace_collision" -v`
Expected: FAIL — `f(x) = x^2` is not recognized as a function definition, `f(3)` resolves `f` as an undefined variable/unit.

- [x] **Step 3: Implement**

**3a. Add the `dataclass` import.** Change:

```python
import math
import operator
import re
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias
```

to:

```python
import math
import operator
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias
```

**3b. Add the `UserFunction` dataclass.** Insert it between `Token` and `GrammarError`:

```python
class Token(NamedTuple):
    """A lexed token: kind, raw text, and source column."""

    type: str
    value: str
    pos: int


@dataclass
class UserFunction:
    params: list[tuple[str, str | None]]  # (name, unit_symbol_or_None)
    body_tokens: list[Token]


class GrammarError(ValueError):
    """Raised when a statement cannot be parsed."""
```

**3c. Add two module-level helpers**, inserted immediately after `_check_arity`, before the `_FUNCTIONS` table:

```python
def _find_matching_paren(tokens: list[Token], open_idx: int) -> int:
    """Index of the ')' matching the '(' at open_idx, or -1."""
    depth = 0
    for i in range(open_idx, len(tokens)):
        if tokens[i].value == "(":
            depth += 1
        elif tokens[i].value == ")":
            depth -= 1
            if depth == 0:
                return i
    return -1


def _split_on_commas(tokens: list[Token]) -> list[list[Token]]:
    """Splits a token list on top-level commas into sub-lists."""
    if not tokens:
        return []
    parts: list[list[Token]] = []
    current: list[Token] = []
    depth = 0
    for tok in tokens:
        if tok.value == "(":
            depth += 1
        elif tok.value == ")":
            depth -= 1
        if depth == 0 and tok.type == "OP" and tok.value == ",":
            parts.append(current)
            current = []
        else:
            current.append(tok)
    parts.append(current)
    return parts
```

**3d. Extend `_ExprParser.__init__`** to carry function lookup/call. Change:

```python
    def __init__(
        self,
        tokens: list[Token],
        resolve: Callable[[str], GrammarValue],
        make_quantity: Callable[..., GrammarValue],
    ) -> None:
        self._tokens = tokens
        self._i = 0
        self._resolve = resolve
        self._q = make_quantity
```

to:

```python
    def __init__(
        self,
        tokens: list[Token],
        resolve: Callable[[str], GrammarValue],
        make_quantity: Callable[..., GrammarValue],
        functions: dict[str, UserFunction],
        call_user_function: Callable[..., GrammarValue],
        depth: int = 0,
    ) -> None:
        self._tokens = tokens
        self._i = 0
        self._resolve = resolve
        self._q = make_quantity
        self._functions = functions
        self._call_user_function = call_user_function
        self._depth = depth
```

**3e. Add `_is_user_function_call`**, mirroring `_is_function_call`. Insert it immediately after `_is_function_call`, before `_atom`:

```python
    def _is_user_function_call(self, tok: Token) -> bool:
        return (
            tok.type == "IDENT"
            and tok.value in self._functions
            and self._i + 1 < len(self._tokens)
            and self._tokens[self._i + 1].value == "("
        )
```

**3f. Dispatch to it in `_atom()`.** Change:

```python
        if self._is_function_call(tok):
            name = tok.value
            self._next()
            args = self._call_args()
            lo, hi, fn = _FUNCTIONS[name]
            _check_arity(name, args, lo, hi)
            return fn(*args)
        if tok.value == "(":
```

to:

```python
        if self._is_function_call(tok):
            name = tok.value
            self._next()
            args = self._call_args()
            lo, hi, fn = _FUNCTIONS[name]
            _check_arity(name, args, lo, hi)
            return fn(*args)
        if self._is_user_function_call(tok):
            name = tok.value
            self._next()
            args = self._call_args()
            return self._call_user_function(name, args)
        if tok.value == "(":
```

**3g. Pass functions/call-hook through `_eval_expr`.** Change:

```python
    def _eval_expr(self, tokens: list[Token]) -> GrammarValue:
        if not tokens:
            raise GrammarError("Empty expression")
        return _ExprParser(tokens, self._resolve, self._q).parse()
```

to:

```python
    def _eval_expr(self, tokens: list[Token]) -> GrammarValue:
        if not tokens:
            raise GrammarError("Empty expression")
        return _ExprParser(
            tokens, self._resolve, self._q, self._functions, self._call_user_function
        ).parse()
```

**3h. Generalize `_split_assignment`'s reserved-word check** to also cover user functions, and drop `@staticmethod` (it now needs `self._functions`). Change:

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
        if lhs_tokens[0].value in _FUNCTIONS:
            raise GrammarError(
                f"{lhs_tokens[0].value!r} is reserved in: {stmt!r}"
            )
        return tokens[assign_idx + 1 :], lhs_tokens[0].value
```

to:

```python
    def _split_assignment(
        self, tokens: list[Token], stmt: str
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
        name = lhs_tokens[0].value
        if name in _FUNCTIONS or name in self._functions:
            raise GrammarError(f"{name!r} is reserved in: {stmt!r}")
        return tokens[assign_idx + 1 :], name
```

(The call site, `self._split_assignment(tokens, stmt)` inside `_eval_statement`, already uses `self.` — no change needed there for this step.)

**3i. Add `_try_define_function`**, `_param_list`, and `_call_user_function` to `GrammarInterpreter`. Insert `_try_define_function` and `_param_list` immediately after `_split_assignment`, before `_eval_statement`:

```python
    def _try_define_function(self, tokens: list[Token], stmt: str) -> bool:
        """Detects and stores `name(params) = body`; returns True if handled."""
        if len(tokens) < 4 or tokens[0].type != "IDENT" or tokens[1].value != "(":
            return False
        close_idx = _find_matching_paren(tokens, 1)
        if close_idx == -1:
            raise GrammarError(f"Missing closing parenthesis in: {stmt!r}")
        if close_idx + 1 >= len(tokens) or tokens[close_idx + 1].value != "=":
            return False
        name = tokens[0].value
        if name in _FUNCTIONS:
            raise GrammarError(f"{name!r} is reserved in: {stmt!r}")
        if name in self.env:
            raise GrammarError(f"{name!r} is already a variable in: {stmt!r}")
        param_tokens = tokens[2:close_idx]
        params = self._param_list(param_tokens, stmt)
        body_tokens = tokens[close_idx + 2 :]
        if not body_tokens:
            raise GrammarError(f"Empty function body in: {stmt!r}")
        self._functions[name] = UserFunction(params=params, body_tokens=body_tokens)
        return True

    def _param_list(
        self, tokens: list[Token], stmt: str
    ) -> list[tuple[str, str | None]]:
        if not tokens:
            return []
        params: list[tuple[str, str | None]] = []
        for part in _split_on_commas(tokens):
            if len(part) != 1 or part[0].type != "IDENT":
                raise GrammarError(f"Invalid parameter list in: {stmt!r}")
            params.append((part[0].value, None))
        return params
```

Insert `_call_user_function` immediately after `_eval_expr`, before `_resolve`:

```python
    def _call_user_function(
        self, name: str, args: list[GrammarValue], depth: int = 0
    ) -> GrammarValue:
        fn = self._functions[name]
        _check_arity(name, args, len(fn.params), len(fn.params))
        scope = dict(zip((p[0] for p in fn.params), args, strict=True))

        def resolve(ident: str) -> GrammarValue:
            if ident in scope:
                return scope[ident]
            return self._resolve(ident)

        return _ExprParser(
            fn.body_tokens, resolve, self._q, self._functions, self._call_user_function
        ).parse()
```

**3j. Call `_try_define_function` from `_eval_statement`.** Change:

```python
    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        eq_idx = _top_level_index(tokens, "==")
```

to:

```python
    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        if self._try_define_function(tokens, stmt):
            return None

        eq_idx = _top_level_index(tokens, "==")
```

**3k. Initialize `self._functions` in `GrammarInterpreter.__init__`.** Change:

```python
    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from measurekit.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, GrammarValue] = {}
```

to:

```python
    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from measurekit.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, GrammarValue] = {}
        self._functions: dict[str, UserFunction] = {}
```

**3l. Extend the module docstring.** Change:

```python
    3 < 5                    # comparison -> bool
    1 < 2 ? 10 : 20          # ternary -> value

Example:
```

to:

```python
    3 < 5                    # comparison -> bool
    1 < 2 ? 10 : 20          # ternary -> value
    f(x) = x^2               # user-defined function
    f(3)                     # -> 9

Example:
```

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS, all tests including pre-existing ones.

Also run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add user-defined function definitions and calls to MKML"
```

---

## Task 3: Recursion (depth limit, configurable setting)

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Modify: `measurekit/infrastructure/config/measurekit.conf`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_recursion_factorial(mn):
    mn.run("fact(n) = n <= 1 ? 1 : n * fact(n - 1)")
    assert mn.eval("fact(5)") == 120


def test_recursion_fibonacci(mn):
    mn.run("fib(n) = n <= 1 ? n : fib(n - 1) + fib(n - 2)")
    assert mn.eval("fib(10)") == 55


def test_recursion_without_base_case_hits_limit(mn):
    mn.run("loop(n) = loop(n + 1)")
    with pytest.raises(GrammarError, match="recursion limit"):
        mn.eval("loop(0)")


def test_recursion_custom_limit(mn):
    original = mn.system.settings.get("mkml_recursion_limit")
    mn.system.settings["mkml_recursion_limit"] = "5"
    try:
        mn.run("loop(n) = loop(n + 1)")
        with pytest.raises(GrammarError, match=r"recursion limit \(5\)"):
            mn.eval("loop(0)")
    finally:
        if original is None:
            mn.system.settings.pop("mkml_recursion_limit", None)
        else:
            mn.system.settings["mkml_recursion_limit"] = original
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k recursion -v`
Expected: FAIL — `fact(5)` currently either infinite-loops into a raw Python `RecursionError`, or (if `n <= 1 ? 1 : ...` parses fine from Task 1/2) recurses without any depth check, so `test_recursion_without_base_case_hits_limit` fails by raising `RecursionError` instead of `GrammarError`, and `mn.system` does not exist yet (`AttributeError`).

- [x] **Step 3: Implement**

**3a. Add the recursion-limit setting.** In `measurekit/infrastructure/config/measurekit.conf`, change:

```
[Settings]
default_output = plain
auto_simplify = true
default_system = SI
readable_representation = true
verbose = false
propagation_mode = correlated

[Dimensions]
```

to:

```
[Settings]
default_output = plain
auto_simplify = true
default_system = SI
readable_representation = true
verbose = false
propagation_mode = correlated
mkml_recursion_limit = 100

[Dimensions]
```

**3b. Store the active `UnitSystem` on the interpreter.** In `GrammarInterpreter.__init__`, change:

```python
    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from measurekit.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, GrammarValue] = {}
        self._functions: dict[str, UserFunction] = {}
```

to:

```python
    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from measurekit.application.context import get_active_system
        from measurekit.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, GrammarValue] = {}
        self._functions: dict[str, UserFunction] = {}
        self.system = system if system is not None else get_active_system()
```

**3c. Add the depth check to `_call_user_function`, and thread `depth + 1` into the nested parse.** Change:

```python
    def _call_user_function(
        self, name: str, args: list[GrammarValue], depth: int = 0
    ) -> GrammarValue:
        fn = self._functions[name]
        _check_arity(name, args, len(fn.params), len(fn.params))
        scope = dict(zip((p[0] for p in fn.params), args, strict=True))

        def resolve(ident: str) -> GrammarValue:
            if ident in scope:
                return scope[ident]
            return self._resolve(ident)

        return _ExprParser(
            fn.body_tokens, resolve, self._q, self._functions, self._call_user_function
        ).parse()
```

to:

```python
    def _call_user_function(
        self, name: str, args: list[GrammarValue], depth: int = 0
    ) -> GrammarValue:
        limit = int(self.system.get_setting("mkml_recursion_limit", "100"))
        if depth >= limit:
            raise GrammarError(
                f"recursion limit ({limit}) exceeded calling {name!r}"
            )
        fn = self._functions[name]
        _check_arity(name, args, len(fn.params), len(fn.params))
        scope = dict(zip((p[0] for p in fn.params), args, strict=True))

        def resolve(ident: str) -> GrammarValue:
            if ident in scope:
                return scope[ident]
            return self._resolve(ident)

        return _ExprParser(
            fn.body_tokens,
            resolve,
            self._q,
            self._functions,
            self._call_user_function,
            depth + 1,
        ).parse()
```

**3d. Pass the current depth from `_atom()`'s call site.** Change:

```python
        if self._is_user_function_call(tok):
            name = tok.value
            self._next()
            args = self._call_args()
            return self._call_user_function(name, args)
```

to:

```python
        if self._is_user_function_call(tok):
            name = tok.value
            self._next()
            args = self._call_args()
            return self._call_user_function(name, args, self._depth)
```

**3e. Neuter recursive calls inside the untaken ternary branch**, so parsing `n <= 1 ? 1 : n * fact(n - 1)` for its base case doesn't also recurse the `else` branch just to validate its syntax. Change:

```python
    def _discard_ternary(self) -> None:
        self._ternary()
```

to:

```python
    def _discard_ternary(self) -> None:
        # ponytail: neuters user-function calls in the untaken ternary branch
        # so e.g. `n <= 1 ? 1 : n * fact(n - 1)` doesn't recurse past its own
        # base case just to parse the branch it isn't taking. A GrammarError
        # from e.g. an unresolved variable in the untaken branch still
        # propagates -- only recursive calls are stubbed, not evaluation.
        real_call = self._call_user_function
        self._call_user_function = lambda name, args, depth=0: 0
        try:
            self._ternary()
        finally:
            self._call_user_function = real_call
```

**3f. Extend the module docstring.**

```python
    f(x) = x^2               # user-defined function
    f(3)                     # -> 9

Example:
```

becomes:

```python
    f(x) = x^2               # user-defined function
    f(3)                     # -> 9

Example:
    >>> from measurekit.ext.grammar import GrammarInterpreter
    >>> mn = GrammarInterpreter()
    >>> _ = mn.run('''
    ... force = 500 N
    ... area = 2 m^2
    ... stress = force / area
    ... ''')
    >>> mn.eval("stress == 250 Pa")
    True
    >>> mn.eval("1 < 2 ? 10 : 20")
    10
    >>> _ = mn.run("fact(n) = n <= 1 ? 1 : n * fact(n - 1)")
    >>> mn.eval("fact(5)")
    120
"""
```

(This replaces the whole existing `Example:` block, extending it with the recursion example rather than duplicating it.)

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS, all tests including pre-existing ones.

Also run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py measurekit/infrastructure/config/measurekit.conf tests/ext/test_grammar.py
git commit -m "feat: add recursion support with configurable depth limit to MKML functions"
```

---

## Task 4: Typed parameters (`f(x: m) = expr`)

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_typed_parameter_valid_dimension(mn):
    mn.run("double_len(x: m) = x * 2")
    result = mn.eval("double_len(3 m)")
    assert math.isclose(result.to("m").magnitude, 6)


def test_typed_parameter_auto_converts_compatible_unit(mn):
    mn.run("double_len(x: m) = x * 2")
    result = mn.eval("double_len(300 cm)")
    assert math.isclose(result.to("m").magnitude, 6)


def test_typed_parameter_incompatible_dimension_raises(mn):
    mn.run("double_len(x: m) = x * 2")
    with pytest.raises(DimensionError):
        mn.eval("double_len(3 kg)")


def test_typed_parameter_bare_number_raises(mn):
    mn.run("double_len(x: m) = x * 2")
    with pytest.raises(DimensionError):
        mn.eval("double_len(3)")


def test_untyped_parameter_accepts_anything(mn):
    mn.run("identity(x) = x")
    assert mn.eval("identity(5)") == 5
    result = mn.eval("identity(3 kg)")
    assert math.isclose(result.to("kg").magnitude, 3)


def test_typed_and_untyped_parameters_mixed(mn):
    mn.run("scale(x: m, k) = x * k")
    result = mn.eval("scale(3 m, 2)")
    assert math.isclose(result.to("m").magnitude, 6)
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k typed_parameter -v`
Expected: FAIL — `_param_list` raises `GrammarError("Invalid parameter list...")` on `x: m` since it only accepts a bare single `IDENT` per parameter.

- [x] **Step 3: Implement**

**3a. Import `DimensionError`.** Change:

```python
from measurekit.domain.notation.lexer import parse_superscript
```

to:

```python
from measurekit.domain.exceptions import DimensionError
from measurekit.domain.notation.lexer import parse_superscript
```

**3b. Extend `_param_list` to accept an optional `: unit` suffix.** Change:

```python
    def _param_list(
        self, tokens: list[Token], stmt: str
    ) -> list[tuple[str, str | None]]:
        if not tokens:
            return []
        params: list[tuple[str, str | None]] = []
        for part in _split_on_commas(tokens):
            if len(part) != 1 or part[0].type != "IDENT":
                raise GrammarError(f"Invalid parameter list in: {stmt!r}")
            params.append((part[0].value, None))
        return params
```

to:

```python
    def _param_list(
        self, tokens: list[Token], stmt: str
    ) -> list[tuple[str, str | None]]:
        if not tokens:
            return []
        params: list[tuple[str, str | None]] = []
        for part in _split_on_commas(tokens):
            if not part or part[0].type != "IDENT":
                raise GrammarError(f"Invalid parameter list in: {stmt!r}")
            if len(part) == 1:
                params.append((part[0].value, None))
                continue
            if len(part) == 3 and part[1].value == ":" and part[2].type == "IDENT":
                self.system.get_unit(part[2].value)  # validates the unit exists
                params.append((part[0].value, part[2].value))
                continue
            raise GrammarError(f"Invalid parameter list in: {stmt!r}")
        return params
```

**3c. Add `_bind_param`**, inserted immediately after `_call_user_function`, before `_resolve`:

```python
    def _bind_param(
        self, name: str, unit_symbol: str | None, arg: GrammarValue
    ) -> GrammarValue:
        if unit_symbol is None:
            return arg
        expected = self.system.get_unit(unit_symbol)
        actual_dim = getattr(arg, "unit", None)
        if actual_dim is None:
            raise DimensionError(
                f"Parameter {name!r} expects a quantity with dimension "
                f"{expected.dimension(self.system)!r}, got a bare number"
            )
        if actual_dim.dimension(self.system) != expected.dimension(self.system):
            raise DimensionError(
                f"Parameter {name!r} expects dimension "
                f"{expected.dimension(self.system)!r}, "
                f"got {actual_dim.dimension(self.system)!r}"
            )
        return arg
```

**3d. Bind each parameter through `_bind_param` in `_call_user_function`.** Change:

```python
        fn = self._functions[name]
        _check_arity(name, args, len(fn.params), len(fn.params))
        scope = dict(zip((p[0] for p in fn.params), args, strict=True))
```

to:

```python
        fn = self._functions[name]
        _check_arity(name, args, len(fn.params), len(fn.params))
        scope = {
            param_name: self._bind_param(param_name, unit_symbol, arg)
            for (param_name, unit_symbol), arg in zip(fn.params, args, strict=True)
        }
```

(This is the only part of `_call_user_function` that changes here — the recursion-limit check and `depth + 1` threading from Task 3 stay exactly as they are.)

**3e. Extend the module docstring.**

```python
    >>> _ = mn.run("fact(n) = n <= 1 ? 1 : n * fact(n - 1)")
    >>> mn.eval("fact(5)")
    120
"""
```

becomes:

```python
    >>> _ = mn.run("fact(n) = n <= 1 ? 1 : n * fact(n - 1)")
    >>> mn.eval("fact(5)")
    120
    >>> _ = mn.run("double_len(x: m) = x * 2")
    >>> mn.eval("double_len(3 m)")
    Quantity(6.0, m)
"""
```

Run the doctest for this exact line first (see Step 4) — if `Quantity`'s `repr` differs from `Quantity(6.0, m)`, use the actual observed repr instead; do not guess.

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS, all tests including pre-existing ones.

Run: `uv run python -c "from measurekit.ext.grammar import GrammarInterpreter; mn = GrammarInterpreter(); mn.run('double_len(x: m) = x * 2'); print(repr(mn.eval('double_len(3 m)')))"`
Use the printed repr to fix the doctest line from 3e if it doesn't match `Quantity(6.0, m)`.

Then run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add optional typed parameters to MKML function definitions"
```

---

## Task 5: `let...in` local bindings

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_let_binding_inside_function_body(mn):
    mn.run("f(x) = let y = x^2 in y + 1")
    assert mn.eval("f(3)") == 10


def test_let_binding_nested(mn):
    mn.run("f(x) = let a = x + 1 in let b = a * 2 in b")
    assert mn.eval("f(3)") == 8


def test_let_at_top_level_raises(mn):
    with pytest.raises(GrammarError, match="only valid inside a function body"):
        mn.eval("let y = 5 in y + 1")


def test_in_still_resolves_as_inches_outside_let(mn):
    result = mn.eval("5 in")
    assert math.isclose(result.to("in").magnitude, 5)
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k "let_" or "in_still_resolves" -v`
Expected: FAIL — `let` currently tokenizes as a plain `IDENT` and is resolved as an unknown variable/unit, so `f(x) = let y = x^2 in y + 1` raises during the call instead of binding `y`.

- [x] **Step 3: Implement**

**3a. Dispatch to `let` at the top of the expression grammar.** Change:

```python
    def _expr(self) -> GrammarValue:
        return self._ternary()
```

to:

```python
    def _expr(self) -> GrammarValue:
        tok = self._peek()
        if tok and tok.type == "IDENT" and tok.value == "let":
            return self._let_expr()
        return self._ternary()
```

**3b. Add `_let_expr`**, inserted immediately after `_expr`:

```python
    def _let_expr(self) -> GrammarValue:
        self._next()  # consume "let"
        name_tok = self._peek()
        if name_tok is None or name_tok.type != "IDENT":
            raise GrammarError("Expected a name after 'let'")
        self._next()
        self._expect("=")
        value = self._expr()
        in_tok = self._peek()
        if in_tok is None or in_tok.value != "in":
            raise GrammarError("Expected 'in' after let binding")
        self._next()
        outer_resolve = self._resolve

        def resolve(ident: str) -> GrammarValue:
            if ident == name_tok.value:
                return value
            return outer_resolve(ident)

        self._resolve = resolve
        try:
            return self._expr()
        finally:
            self._resolve = outer_resolve
```

(Note the peek-then-check-then-next order for both the name and `in` tokens: `_next()` has no bounds check and raises a raw `IndexError` if called with no tokens left, unlike `_peek()`. This matches the existing idiom used by `_expect`, `_call_args`, and `_atom`'s parenthesized branch.)

**3c. Reject a bare top-level `let...in`.** In `_eval_statement`, change:

```python
    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        if self._try_define_function(tokens, stmt):
            return None

        eq_idx = _top_level_index(tokens, "==")
```

to:

```python
    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        if self._try_define_function(tokens, stmt):
            return None
        if tokens and tokens[0].type == "IDENT" and tokens[0].value == "let":
            raise GrammarError("'let' is only valid inside a function body")

        eq_idx = _top_level_index(tokens, "==")
```

**3d. Extend the module docstring.**

```python
    >>> _ = mn.run("double_len(x: m) = x * 2")
    >>> mn.eval("double_len(3 m)")
    Quantity(6.0, m)
"""
```

becomes:

```python
    >>> _ = mn.run("double_len(x: m) = x * 2")
    >>> mn.eval("double_len(3 m)")
    Quantity(6.0, m)
    >>> _ = mn.run("g(x) = let y = x^2 in y + 1")
    >>> mn.eval("g(3)")
    10
"""
```

(Use whatever repr Task 4 settled on for the `double_len` line — the `g(x)` addition is independent of it.)

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS, all tests including pre-existing ones.

Also run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add let...in local bindings to MKML function bodies"
```

---

## Task 6: Display-text blocks

**Files:**
- Modify: `measurekit/ext/grammar.py`
- Test: `tests/ext/test_grammar.py`

- [x] **Step 1: Write the failing tests**

Append to `tests/ext/test_grammar.py`:

```python
def test_display_text_block_inline(mn):
    results = mn.run("```Hello world```")
    assert results == ["Hello world"]


def test_display_text_block_multiline(mn):
    results = mn.run("```\nLine one\nLine two\n```")
    assert results == ["Line one\nLine two"]


def test_display_text_block_with_hash_and_backtick_adjacent_chars(mn):
    results = mn.run("```price is #5 and uses ` backtick```")
    assert results == ["price is #5 and uses ` backtick"]


def test_display_text_block_interleaved_with_statements(mn):
    results = mn.run("x = 5 m\n```note```\nx => m")
    assert results[0] is None
    assert results[1] == "note"
    assert math.isclose(results[2].magnitude, 5)


def test_script_with_no_blocks_unaffected(mn):
    results = mn.run("a = 1 m\nb = 2 m\na + b = ?")
    assert math.isclose(results[-1].magnitude, 3)
```

- [x] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/ext/test_grammar.py -k display_text -v`
Expected: FAIL — `` ``` `` is not a recognized token, so `mn.run("```Hello world```")` raises `GrammarError` from the tokenizer's `BAD` fallback instead of returning `["Hello world"]`.

- [x] **Step 3: Implement**

**3a. Widen `GrammarValue` to include `str`.** Change:

```python
        # A statement evaluates to a bare number (unitless arithmetic) or to
        # a Quantity once a unit identifier enters the expression.
        GrammarValue: TypeAlias = Quantity[Any, Any, Any] | int | float
```

to:

```python
        # A statement evaluates to a bare number (unitless arithmetic), a
        # Quantity once a unit identifier enters the expression, or a str
        # for a display-text block.
        GrammarValue: TypeAlias = Quantity[Any, Any, Any] | int | float | str
```

**3b. Add the block-extraction regex**, inserted immediately after the `_TOKEN_RE = re.compile(...)` block, before `class Token`:

```python
_TEXT_BLOCK_RE = re.compile(r"```(.*?)```", re.DOTALL)
```

**3c. Restructure `run()` to extract blocks before per-line splitting**, and move the existing per-line logic into a new `_run_segment`. Change:

```python
    def run(self, source: str) -> list[GrammarValue | None]:
        """Evaluates every statement; returns one result per statement.

        Assignments yield None; queries, conversions, assertions and bare
        expressions yield their value.
        """
        results = []
        for raw in re.split(r"[\n;]", source):
            stmt = raw.split("#", 1)[0].strip()
            if stmt:
                results.append(self._eval_statement(stmt))
        return results
```

to:

```python
    def run(self, source: str) -> list[GrammarValue | None]:
        """Evaluates every statement; returns one result per statement.

        Assignments yield None; queries, conversions, assertions and bare
        expressions yield their value. A triple-backtick-delimited span is a
        display-text block: it yields its enclosed text verbatim as a str.
        """
        results: list[GrammarValue | None] = []
        pos = 0
        for match in _TEXT_BLOCK_RE.finditer(source):
            results.extend(self._run_segment(source[pos : match.start()]))
            text = match.group(1)
            if text.startswith("\n"):
                text = text[1:]
            if text.endswith("\n"):
                text = text[:-1]
            results.append(text)
            pos = match.end()
        results.extend(self._run_segment(source[pos:]))
        return results

    def _run_segment(self, segment: str) -> list[GrammarValue | None]:
        results: list[GrammarValue | None] = []
        for raw in re.split(r"[\n;]", segment):
            stmt = raw.split("#", 1)[0].strip()
            if stmt:
                results.append(self._eval_statement(stmt))
        return results
```

**3d. Extend the module docstring.**

```python
    >>> _ = mn.run("g(x) = let y = x^2 in y + 1")
    >>> mn.eval("g(3)")
    10
"""
```

becomes:

```python
    >>> _ = mn.run("g(x) = let y = x^2 in y + 1")
    >>> mn.eval("g(3)")
    10
    >>> mn.run("```This text is shown verbatim```")
    ['This text is shown verbatim']
"""
```

- [x] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/ext/test_grammar.py -v`
Expected: PASS, all tests including pre-existing ones.

Also run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add measurekit/ext/grammar.py tests/ext/test_grammar.py
git commit -m "feat: add display-text blocks to MKML scripts"
```

---

## Task 7: Final verification

**Files:** none (no code changes — verification only).

- [x] **Step 1: Full test suite with coverage**

Run: `uv run pytest --cov=measurekit --cov-report=term-missing`
Expected: All tests pass; total coverage ≥ 80% (per `fail_under = 80` in `pyproject.toml`).

- [x] **Step 2: Lint and format**

Run: `uv run ruff check .`
Expected: No errors.

Run: `uv run ruff format --check .`
Expected: No reformatting needed.

- [x] **Step 3: Type check the touched file**

Run: `uv run ty check measurekit/ext/grammar.py`
Expected: No new errors introduced by this plan (pre-existing repo-wide `ty` errors elsewhere are not this plan's concern — `ty` is advisory per `CLAUDE.md`).

- [x] **Step 4: Doctests**

Run: `uv run pytest --doctest-modules measurekit/ext/grammar.py -v`
Expected: PASS — all doctest examples added across Tasks 1–6 execute correctly.

- [x] **Step 5: REPL / CLI smoke test**

Run: `uv run python -m measurekit "fact(n) = n <= 1 ? 1 : n * fact(n - 1); fact(5) = ?"`
Expected: prints `120` (or the equivalent formatted bare-number result) with no traceback.

Run: `uv run python -m measurekit "double_len(x: m) = x * 2; double_len(3 m) => m"`
Expected: prints a `6 m`-equivalent result with no traceback.

- [x] **Step 6: SonarQube quality gate (if `.env` with `SONAR_TOKEN` is configured locally)**

Run: `make sonar`
Expected: quality gate green — coverage ≥ 80%, duplication ≤ 3%, zero new violations on the changed lines.

No commit for this task — it's verification only. If any step fails, fix the issue in the relevant earlier task's code (not by lowering thresholds or adding suppression comments) and re-run.

---

## Self-Review Notes

**Spec coverage** (`docs/superpowers/specs/2026-07-12-mkml-user-functions-design.md`):
- §1 User-defined function definitions → Task 2.
- §2 Typed parameters → Task 4.
- §3 `let...in`, restricted to function bodies → Task 5.
- §4 Recursion, configurable `mkml_recursion_limit` → Task 3.
- §5 Reserved-word / namespace rules (both directions, built-ins, redefinition) → Task 2, Step 1 tests (`test_variable_then_function_namespace_collision`, `test_function_then_variable_namespace_collision`, `test_user_function_shadowing_builtin_raises`, `test_user_function_redefinition_allowed`).
- §6 Display-text blocks → Task 6.
- Ternary/comparison addition (user-approved, needed for §"Testing" recursion requirement) → Task 1.
- Testing section's doctest requirement ("each touched function gains one runnable doctest example") → one doctest line added to the module's `Example:` block in each of Tasks 1, 3, 4, 5, 6 (Task 2's function-definition capability is covered by the doctest line added in Task 1's docstring `Supported statements::` list plus Task 3's `fact` example, which exercises function definition+call directly).
- `in`-as-inches regression → Task 5, `test_in_still_resolves_as_inches_outside_let`.

**Placeholder scan:** No "TBD"/"TODO"/"add appropriate error handling" anywhere above. The one deliberately open item is Task 4 Step 3e / Task 4 Step 4, which tells the engineer to run a one-line script to confirm `Quantity`'s exact `repr` before finalizing that specific doctest line — this is not a placeholder, it's a guard against hand-guessing a `repr` format not yet observed in this file (every other doctest line in this plan reuses reprs already confirmed against existing doctests in the file, e.g. bare numbers and `True`/`False`).

**Type/name consistency check** — confirmed identical spelling everywhere referenced across all 7 tasks: `UserFunction` (dataclass, `params`/`body_tokens` fields), `_functions`, `_call_user_function`, `_try_define_function`, `_param_list`, `_bind_param`, `_let_expr`, `_expr`, `_ternary`, `_comparison`, `_expect`, `_discard_ternary`, `_is_user_function_call`, `_find_matching_paren`, `_split_on_commas`, `_run_segment`, `_TEXT_BLOCK_RE`, `_COMPARISONS`, `self.system`, `self.env`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-12-mkml-user-functions.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
