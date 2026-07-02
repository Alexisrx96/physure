"""MNML grammar extension: evaluate MeasureNote-style engineering notes.

Implements the core of the MeasureNote Meta-Language (MNML) as a
zero-dependency interpreter on top of measurekit. Units are plain
identifiers resolved against the active :class:`UnitSystem`, so
``500 N`` is simply implicit multiplication ``500 * N``.

Supported statements::

    force = 500 N            # assignment (`->` also accepted)
    area = 2 m^2
    stress = force / area
    stress = ?               # query -> Quantity
    stress => kPa            # conversion -> Quantity
    stress = force / area = ?    # assign and return
    x = 500 N => kN          # assign converted
    stress == 250 Pa         # assertion -> bool
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

from __future__ import annotations

import math
import re
from typing import TYPE_CHECKING, Any, NamedTuple

from measurekit.domain.notation.lexer import parse_superscript

if TYPE_CHECKING:
    from collections.abc import Callable

    from measurekit.domain.measurement.system import UnitSystem

_TOKEN_RE = re.compile(
    r"""
      (?P<NUMBER>\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?)
    | (?P<IDENT>[^\W\d]\w*)
    | (?P<SUP>[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+)
    | (?P<OP>\+/-|±|==|=>|->|\*\*|[-+*/^()=?])
    | (?P<WS>[ \t]+)
    | (?P<BAD>.)
    """,
    re.VERBOSE,
)


class Token(NamedTuple):
    """A lexed token: kind, raw text, and source column."""

    type: str
    value: str
    pos: int


class GrammarError(ValueError):
    """Raised when a statement cannot be parsed."""


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
        tokens.append(Token(kind, m.group(), m.start()))
    return tokens


def _top_level_index(tokens: list[Token], op: str) -> int:
    """Index of the first paren-depth-0 occurrence of ``op``, or -1."""
    depth = 0
    for i, tok in enumerate(tokens):
        if tok.value == "(":
            depth += 1
        elif tok.value == ")":
            depth -= 1
        elif depth == 0 and tok.type == "OP" and tok.value == op:
            return i
    return -1


class _ExprParser:
    """Recursive-descent expression parser mirroring MNML precedence.

    sum > product > implicit multiplication > power > atom, so
    ``500 N / 2 m^2`` parses as ``(500*N) / (2*m^2)``.
    """

    def __init__(
        self,
        tokens: list[Token],
        resolve: Callable[[str], Any],
        make_quantity: Callable[..., Any],
    ) -> None:
        self._tokens = tokens
        self._i = 0
        self._resolve = resolve
        self._q = make_quantity

    def parse(self) -> Any:
        result = self._sum()
        if self._i < len(self._tokens):
            tok = self._tokens[self._i]
            raise GrammarError(f"Unexpected token {tok.value!r} in expression")
        return result

    def _peek(self) -> Token | None:
        return self._tokens[self._i] if self._i < len(self._tokens) else None

    def _next(self) -> Token:
        tok = self._tokens[self._i]
        self._i += 1
        return tok

    def _sum(self) -> Any:
        result = self._product()
        while (tok := self._peek()) and tok.value in ("+", "-"):
            self._next()
            rhs = self._product()
            result = result + rhs if tok.value == "+" else result - rhs
        return result

    def _product(self) -> Any:
        result = self._implicit()
        while (tok := self._peek()) and tok.value in ("*", "/"):
            self._next()
            rhs = self._implicit()
            result = result * rhs if tok.value == "*" else result / rhs
        return result

    def _implicit(self) -> Any:
        result = self._unary()
        # Adjacency is multiplication: `500 N`, `2 m^2`, `3 (x + y)`.
        while (tok := self._peek()) and (
            tok.type in ("NUMBER", "IDENT") or tok.value == "("
        ):
            result = result * self._power()
        return result

    def _unary(self) -> Any:
        if (tok := self._peek()) and tok.value == "-":
            self._next()
            return -self._unary()
        return self._power()

    def _power(self) -> Any:
        base = self._atom()
        tok = self._peek()
        if tok and tok.value in ("^", "**"):
            self._next()
            exp = self._unary()
            if hasattr(exp, "magnitude"):
                exp = exp.magnitude
            return base**exp
        if tok and tok.type == "SUP":
            self._next()
            return base ** parse_superscript(tok.value)
        return base

    def _atom(self) -> Any:
        tok = self._peek()
        if tok is None:
            raise GrammarError("Unexpected end of expression")
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


def _to_number(text: str) -> int | float:
    value = float(text)
    if value.is_integer() and "." not in text and "e" not in text.lower():
        return int(value)
    return value


class GrammarInterpreter:
    """Stateful interpreter for MNML statements.

    Args:
        system: UnitSystem to resolve units against (default: active system).
        rel_tol: Relative tolerance for ``==`` assertions.
    """

    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from measurekit.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, Any] = {}

    def __getitem__(self, name: str) -> Any:
        return self.env[name]

    def __setitem__(self, name: str, value: Any) -> None:
        self.env[name] = value

    def run(self, source: str) -> list[Any]:
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

    def eval(self, source: str) -> Any:
        """Evaluates statements and returns the last result."""
        results = self.run(source)
        return results[-1] if results else None

    # --- statement handling -------------------------------------------

    def _eval_statement(self, stmt: str) -> Any:
        tokens = _tokenize(stmt)

        eq_idx = _top_level_index(tokens, "==")
        if eq_idx != -1:
            lhs = self._eval_expr(tokens[:eq_idx])
            rhs = self._eval_expr(tokens[eq_idx + 1 :])
            return self._is_close(lhs, rhs)

        # Trailing `= ?` -> return the value even when assigning.
        want_value: bool | str = True
        if (
            len(tokens) >= 2
            and tokens[-1].value == "?"
            and tokens[-2].value == "="
        ):
            tokens = tokens[:-2]
            if (
                _top_level_index(tokens, "=") != -1
                or _top_level_index(tokens, "->") != -1
            ):
                want_value = "assign"

        # Trailing `=> unit` conversion (unit slice taken from source text).
        target_unit = None
        conv_idx = _top_level_index(tokens, "=>")
        if conv_idx != -1:
            unit_start = (
                tokens[conv_idx + 1].pos
                if conv_idx + 1 < len(tokens)
                else None
            )
            if unit_start is None:
                raise GrammarError(f"Missing unit after '=>' in: {stmt!r}")
            end = tokens[-1].pos + len(tokens[-1].value)
            target_unit = stmt[unit_start:end].strip()
            tokens = tokens[:conv_idx]

        assign_idx = _top_level_index(tokens, "=")
        if assign_idx == -1:
            assign_idx = _top_level_index(tokens, "->")
        name = None
        if assign_idx != -1:
            lhs_tokens = tokens[:assign_idx]
            if len(lhs_tokens) != 1 or lhs_tokens[0].type != "IDENT":
                raise GrammarError(
                    f"Assignment target must be a single name in: {stmt!r}"
                )
            name = lhs_tokens[0].value
            tokens = tokens[assign_idx + 1 :]

        value = self._eval_expr(tokens)
        if target_unit is not None:
            value = value.to(target_unit)
        if name is not None:
            self.env[name] = value
            return value if want_value == "assign" else None
        return value

    def _eval_expr(self, tokens: list[Token]) -> Any:
        if not tokens:
            raise GrammarError("Empty expression")
        return _ExprParser(tokens, self._resolve, self._q).parse()

    def _resolve(self, name: str) -> Any:
        if name in self.env:
            return self.env[name]
        # Unknown names are tried as units; UnknownUnitError (with
        # suggestions) propagates if the unit system doesn't know them.
        return self._q(1, name)

    def _is_close(self, lhs: Any, rhs: Any) -> bool:
        # ponytail: scalar isclose; no uncertainty-overlap test yet — add a
        # sigma-based comparison if assertions on uncertain values need it.
        if hasattr(lhs, "to") and hasattr(rhs, "unit"):
            lhs = lhs.to(rhs.unit)
        a = getattr(lhs, "magnitude", lhs)
        b = getattr(rhs, "magnitude", rhs)
        return math.isclose(a, b, rel_tol=self.rel_tol)


def evaluate(source: str, **kwargs: Any) -> Any:
    """One-shot evaluation: fresh interpreter, returns the last result.

    Example:
        >>> from measurekit.ext.grammar import evaluate
        >>> str(evaluate("KE = 0.5 * 2 kg * (3 m/s)^2 = ?"))
        '9.0 kg·m²/s²'
    """
    return GrammarInterpreter(**kwargs).eval(source)
