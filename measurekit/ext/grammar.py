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
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias

from measurekit.domain.notation.lexer import parse_superscript

if TYPE_CHECKING:
    from collections.abc import Callable

    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.system import UnitSystem

    # A statement evaluates to a bare number (unitless arithmetic) or to
    # a Quantity once a unit identifier enters the expression.
    GrammarValue: TypeAlias = Quantity[Any, Any, Any] | int | float
else:
    GrammarValue = Any

# The tokenizer regex, built from one small pattern per token kind.
_NUMBER_PAT = r"\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?"
_IDENT_PAT = r"[^\W\d]\w*"
_SUP_PAT = r"[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+"
_SQRT_PAT = r"√"
_OP_PAT = r"\+/-|±|==|=>|->|\*\*|[-+*/^()=?×÷]"  # noqa: RUF001
_OP_ALIASES = {"×": "*", "÷": "/"}  # noqa: RUF001
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
        value = m.group()
        if kind == "OP":
            value = _OP_ALIASES.get(value, value)
        tokens.append(Token(kind, value, m.start()))
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
        resolve: Callable[[str], GrammarValue],
        make_quantity: Callable[..., GrammarValue],
    ) -> None:
        self._tokens = tokens
        self._i = 0
        self._resolve = resolve
        self._q = make_quantity

    def parse(self) -> GrammarValue:
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

    def _sum(self) -> GrammarValue:
        result = self._product()
        while (tok := self._peek()) and tok.value in ("+", "-"):
            self._next()
            rhs = self._product()
            result = result + rhs if tok.value == "+" else result - rhs
        return result

    def _product(self) -> GrammarValue:
        result = self._implicit()
        while (tok := self._peek()) and tok.value in ("*", "/"):
            self._next()
            rhs = self._implicit()
            result = result * rhs if tok.value == "*" else result / rhs
        return result

    def _implicit(self) -> GrammarValue:
        result = self._unary()
        # Adjacency is multiplication: `500 N`, `2 m^2`, `3 (x + y)`.
        while (tok := self._peek()) and (
            tok.type in ("NUMBER", "IDENT") or tok.value == "("
        ):
            result = result * self._power()
        return result

    def _unary(self) -> GrammarValue:
        if (tok := self._peek()) and tok.value == "-":
            self._next()
            return -self._unary()
        return self._power()

    def _power(self) -> GrammarValue:
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
        self.env: dict[str, GrammarValue] = {}

    def __getitem__(self, name: str) -> GrammarValue:
        return self.env[name]

    def __setitem__(self, name: str, value: GrammarValue) -> None:
        self.env[name] = value

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

    def eval(self, source: str) -> GrammarValue | None:
        """Evaluates statements and returns the last result."""
        results = self.run(source)
        return results[-1] if results else None

    # --- statement handling -------------------------------------------

    @staticmethod
    def _strip_value_query(
        tokens: list[Token],
    ) -> tuple[list[Token], bool | str]:
        """Strips a trailing `= ?`; returns tokens and the want_value flag."""
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
        return tokens, want_value

    @staticmethod
    def _split_conversion(
        tokens: list[Token], stmt: str
    ) -> tuple[list[Token], str | None]:
        """Strips a trailing `=> unit`; returns tokens and the target unit."""
        conv_idx = _top_level_index(tokens, "=>")
        if conv_idx == -1:
            return tokens, None
        unit_start = (
            tokens[conv_idx + 1].pos if conv_idx + 1 < len(tokens) else None
        )
        if unit_start is None:
            raise GrammarError(f"Missing unit after '=>' in: {stmt!r}")
        end = tokens[-1].pos + len(tokens[-1].value)
        return tokens[:conv_idx], stmt[unit_start:end].strip()

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

    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        eq_idx = _top_level_index(tokens, "==")
        if eq_idx != -1:
            lhs = self._eval_expr(tokens[:eq_idx])
            rhs = self._eval_expr(tokens[eq_idx + 1 :])
            return self._is_close(lhs, rhs)

        # Trailing `= ?` -> return the value even when assigning.
        tokens, want_value = self._strip_value_query(tokens)
        # Trailing `=> unit` conversion (unit slice taken from source text).
        tokens, target_unit = self._split_conversion(tokens, stmt)
        tokens, name = self._split_assignment(tokens, stmt)

        value = self._eval_expr(tokens)
        if target_unit is not None:
            value = value.to(target_unit)
        if name is not None:
            self.env[name] = value
            return value if want_value == "assign" else None
        return value

    def _eval_expr(self, tokens: list[Token]) -> GrammarValue:
        if not tokens:
            raise GrammarError("Empty expression")
        return _ExprParser(tokens, self._resolve, self._q).parse()

    def _resolve(self, name: str) -> GrammarValue:
        if name in self.env:
            return self.env[name]
        # Unknown names are tried as units; UnknownUnitError (with
        # suggestions) propagates if the unit system doesn't know them.
        return self._q(1, name)

    def _is_close(self, lhs: GrammarValue, rhs: GrammarValue) -> bool:
        # ponytail: scalar isclose; no uncertainty-overlap test yet — add a
        # sigma-based comparison if assertions on uncertain values need it.
        if hasattr(lhs, "to") and hasattr(rhs, "unit"):
            lhs = lhs.to(rhs.unit)
        a = getattr(lhs, "magnitude", lhs)
        b = getattr(rhs, "magnitude", rhs)
        return math.isclose(a, b, rel_tol=self.rel_tol)


def evaluate(
    source: str, system: UnitSystem | None = None, rel_tol: float = 1e-9
) -> GrammarValue | None:
    """One-shot evaluation: fresh interpreter, returns the last result.

    Example:
        >>> from measurekit.ext.grammar import evaluate
        >>> str(evaluate("KE = 0.5 * 2 kg * (3 m/s)^2 = ?"))
        '9.0 kg·m²/s²'
    """
    return GrammarInterpreter(system=system, rel_tol=rel_tol).eval(source)
