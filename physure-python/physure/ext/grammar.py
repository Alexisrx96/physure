"""MKML grammar extension: evaluate Physure-style engineering notes.

Implements the core of the Physure Meta-Lang (MKML) as a
zero-dependency interpreter on top of physure. Units are plain
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
    3 < 5                    # comparison -> bool
    1 < 2 ? 10 : 20          # ternary -> value

Example:
    >>> from physure.ext.grammar import GrammarInterpreter
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
    >>> _ = mn.run("double_len(x: m) = x * 2")
    >>> mn.eval("double_len(3 m)")
    Quantity(6.0, m)
    >>> _ = mn.run("g(x) = let y = x^2 in y + 1")
    >>> mn.eval("g(3)")
    10
    >>> mn.run("```This text is shown verbatim```")
    ['This text is shown verbatim']
"""

from __future__ import annotations

import math
import operator
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, NamedTuple, TypeAlias

from physure.core.formatting import parse_superscript
from physure.domain.exceptions import DimensionError, PhysureError

if TYPE_CHECKING:
    from collections.abc import Callable

    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem

    # A statement evaluates to a bare number (unitless arithmetic), a
    # Quantity once a unit identifier enters the expression, or a str
    # for a display-text block.
    GrammarValue: TypeAlias = Quantity[Any, Any, Any] | int | float | str
else:
    GrammarValue = Any

# The tokenizer regex, built from one small pattern per token kind.
_NUMBER_PAT = r"\d+\.?\d*(?:[eE]\s*[+-]?\s*\d+)?|\.\d+(?:[eE]\s*[+-]?\s*\d+)?"
_IDENT_PAT = r"[^\W\d]\w*"
_SUP_PAT = r"[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+"
_SQRT_PAT = r"√"
_OP_PAT = r"\+/-|±|<=|>=|!=|==|=>|->|\*\s*\*|\*\*|[-+*/^()=?<>×÷,:|]"  # noqa: RUF001

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
_TEXT_BLOCK_RE = re.compile(r"```(.*?)```", re.DOTALL)


class Token(NamedTuple):
    """A lexed token: kind, raw text, and source column."""

    type: str
    value: str
    pos: int


@dataclass
class UserFunction:
    """Parsed definition of a user-defined MKML function."""

    params: list[tuple[str, list[Token] | None]]  # (name, unit_tokens_or_None)
    body_tokens: list[Token] | None = None
    body_statements: list[list[Token]] | None = None
    body_lines: list[str] | None = None


class GrammarError(PhysureError, ValueError):
    """Raised when a statement cannot be parsed or evaluated."""

    def __init__(
        self,
        message: str,
        line: int | None = None,
        column: int | None = None,
    ) -> None:
        self.raw_message = message
        self.line = line
        self.column = column
        super().__init__(message)


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
            if value.replace(" ", "") == "**":
                value = "**"
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


_COMPARISONS: dict[str, Callable[[Any, Any], bool]] = {
    "<": operator.lt,
    ">": operator.gt,
    "<=": operator.le,
    ">=": operator.ge,
    "==": operator.eq,
    "!=": operator.ne,
}


class _ExprParser:
    """Recursive-descent expression parser mirroring MKML precedence.

    sum > product > implicit multiplication > power > atom, so
    ``500 N / 2 m^2`` parses as ``(500*N) / (2*m^2)``.
    """

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

    def parse(self) -> GrammarValue:
        result = self._expr()
        if self._i < len(self._tokens):
            tok = self._tokens[self._i]
            raise GrammarError(f"Unexpected token {tok.value!r} in expression")
        return result

    def _expr(self) -> GrammarValue:
        tok = self._peek()
        if tok and tok.type == "IDENT" and tok.value == "let":
            return self._let_expr()
        return self._ternary()

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
            if tok.value == "in" and self._i + 1 < len(self._tokens):
                nxt = self._tokens[self._i + 1]
                if nxt.type in ("NUMBER", "IDENT") or nxt.value in ("(", "-"):
                    break
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

    def _is_function_call(self, tok: Token) -> bool:
        return (
            tok.type == "IDENT"
            and tok.value in _FUNCTIONS
            and self._i + 1 < len(self._tokens)
            and self._tokens[self._i + 1].value == "("
        )

    def _is_user_function_call(self, tok: Token) -> bool:
        return (
            tok.type == "IDENT"
            and tok.value in self._functions
            and self._i + 1 < len(self._tokens)
            and self._tokens[self._i + 1].value == "("
        )

    def _atom(self) -> GrammarValue:
        tok = self._peek()
        if tok is None:
            raise GrammarError("Unexpected end of expression")
        if tok.type == "SQRT":
            return self._atom_sqrt()
        if self._is_function_call(tok):
            return self._atom_builtin_call(tok)
        if self._is_user_function_call(tok):
            return self._atom_user_call(tok)
        if tok.value == "(":
            return self._atom_group()
        if tok.type == "NUMBER":
            return self._atom_number(tok)
        if tok.type == "IDENT":
            self._next()
            return self._resolve(tok.value)
        raise GrammarError(f"Unexpected token {tok.value!r}")

    def _atom_sqrt(self) -> GrammarValue:
        self._next()
        operand = self._atom()
        return operand**0.5

    def _atom_builtin_call(self, tok: Token) -> GrammarValue:
        name = tok.value
        self._next()
        args = self._call_args()
        lo, hi, fn = _FUNCTIONS[name]
        _check_arity(name, args, lo, hi)
        return fn(*args)

    def _atom_user_call(self, tok: Token) -> GrammarValue:
        name = tok.value
        self._next()
        args = self._call_args()
        return self._call_user_function(name, args, self._depth)

    def _atom_group(self) -> GrammarValue:
        self._next()
        result = self._expr()
        closing = self._peek()
        if closing is None or closing.value != ")":
            raise GrammarError("Missing closing parenthesis")
        self._next()
        return result

    def _atom_number(self, tok: Token) -> GrammarValue:
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


def _to_number(text: str) -> int | float:
    clean_text = text.replace(" ", "")
    value = float(clean_text)
    if value.is_integer() and "." not in text and "e" not in text.lower():
        return int(value)
    return value


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
    raise GrammarError(
        f"{name}() expects {expected} argument(s), got {len(args)}"
    )


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


# name -> (min_arity, max_arity, implementation). Dispatched from
# _ExprParser._atom() whenever an IDENT token here is immediately followed
# by "(". Delegates to Quantity's own dunder/bound methods wherever
# possible; no unit-handling logic lives here.
# ponytail: "min" shadows the pre-existing "min" unit alias (minutes, see
# physure.conf:123). Only affects the narrow case of writing `min(` with
# no operator meaning "N minutes times (...)"; that now raises an arity
# error instead of silently misparsing. Accepted trade-off, confirmed with
# user rather than renaming the function.
_FUNCTIONS: dict[str, tuple[int, float, Callable[..., GrammarValue]]] = {
    "abs": (1, 1, lambda x: abs(x)),
    "sqrt": (1, 1, lambda x: x**0.5),
    "round": (1, 2, lambda *a: round(*a)),
    "floor": (1, 1, math.floor),
    "ceil": (1, 1, math.ceil),
    "min": (2, math.inf, lambda *a: min(*a)),
    "max": (2, math.inf, lambda *a: max(*a)),
    "sin": (1, 1, lambda x: _transcendental(x, "sin")),
    "cos": (1, 1, lambda x: _transcendental(x, "cos")),
    "tan": (1, 1, lambda x: _transcendental(x, "tan")),
    "exp": (1, 1, lambda x: _transcendental(x, "exp")),
    "log": (1, 1, lambda x: _transcendental(x, "log")),
    "ln": (1, 1, lambda x: _transcendental(x, "log")),
}


def _format_sig_figs(val: GrammarValue, sig_figs: int) -> GrammarValue:
    """Formats a scalar or Quantity magnitude to sig_figs significant figures."""
    mag = getattr(val, "magnitude", val)
    if mag == 0:
        rounded_mag = 0.0
    else:
        mag_abs = abs(mag)
        digits = sig_figs - math.floor(math.log10(mag_abs)) - 1
        rounded_mag = round(mag, max(0, digits))
        if digits < 0:
            factor = 10 ** (-digits)
            rounded_mag = round(mag / factor) * factor

    if hasattr(val, "unit"):
        from physure.domain.measurement.quantity import Quantity

        return Quantity.from_input(
            rounded_mag, val.unit, val.system, val.uncertainty
        )
    return rounded_mag


class GrammarInterpreter:
    """Stateful interpreter for MKML statements.

    Args:
        system: UnitSystem to resolve units against (default: active system).
        rel_tol: Relative tolerance for ``==`` assertions.
    """

    def __init__(
        self, system: UnitSystem | None = None, rel_tol: float = 1e-9
    ) -> None:
        from physure.application.context import get_active_system
        from physure.application.factories import QuantityFactory

        self._q = QuantityFactory(system)
        self.rel_tol = rel_tol
        self.env: dict[str, GrammarValue] = {}
        self._functions: dict[str, UserFunction] = {}
        self.system = system if system is not None else get_active_system()

    def __getitem__(self, name: str) -> GrammarValue:
        return self.env[name]

    def __setitem__(self, name: str, value: GrammarValue) -> None:
        self.env[name] = value

    def run(self, source: str) -> list[GrammarValue | None]:
        """Evaluates every statement; returns one result per statement.

        Assignments yield None; queries, conversions, assertions and bare
        expressions yield their value. A triple-backtick-delimited span is a
        display-text block: it yields its enclosed text verbatim as a str.
        """
        results: list[GrammarValue | None] = []
        pos = 0
        current_line = 1
        for match in _TEXT_BLOCK_RE.finditer(source):
            prefix = source[pos : match.start()]
            results.extend(
                self._run_segment(prefix, start_line_num=current_line)
            )
            current_line += prefix.count("\n")

            text = match.group(1)
            if text.startswith("\n"):
                text = text[1:]
            if text.endswith("\n"):
                text = text[:-1]
            results.append(text)
            current_line += match.group(0).count("\n")
            pos = match.end()

        tail = source[pos:]
        results.extend(self._run_segment(tail, start_line_num=current_line))
        return results

    def _run_segment(
        self, segment: str, start_line_num: int = 1
    ) -> list[GrammarValue | None]:
        results: list[GrammarValue | None] = []
        raw_lines = segment.split("\n")
        i = 0
        while i < len(raw_lines):
            line = raw_lines[i]
            current_line = start_line_num + i
            stmt = line.split("#", 1)[0].rstrip()
            stripped_stmt = stmt.strip()
            if not stripped_stmt:
                i += 1
                continue

            sub_stmts = [
                s.strip() for s in stripped_stmt.split(";") if s.strip()
            ]
            first_tokens = self._tokenize_located(
                sub_stmts[0] if sub_stmts else "", line, current_line
            )

            if len(sub_stmts) == 1 and self._is_multiline_func_header(
                first_tokens
            ):
                header_line_num = current_line
                header_col = line.find(stripped_stmt) + 1
                i, body_lines = self._collect_multiline_body(raw_lines, i + 1)
                self._define_multiline_from(
                    first_tokens,
                    body_lines,
                    stripped_stmt,
                    header_line_num,
                    header_col,
                )
                results.append(None)
                continue

            results.extend(
                self._run_substatements(sub_stmts, line, current_line)
            )
            i += 1

        return results

    def _tokenize_located(
        self, text: str, line: str, current_line: int
    ) -> list[Token]:
        try:
            return _tokenize(text) if text else []
        except GrammarError:
            raise
        except PhysureError as err:
            col = line.find(text) + 1 if text else 1
            if err.line is None:
                err.line = current_line
                err.column = col
            raise
        except Exception as err:
            col = line.find(text) + 1 if text else 1
            raise GrammarError(
                str(err), line=current_line, column=col
            ) from err

    def _collect_multiline_body(
        self, raw_lines: list[str], i: int
    ) -> tuple[int, list[str]]:
        body_lines: list[str] = []
        while i < len(raw_lines):
            sub_line = raw_lines[i]
            sub_comment_stripped = sub_line.split("#", 1)[0].rstrip()
            if not sub_comment_stripped.strip():
                i += 1
                continue
            indent = len(sub_line) - len(sub_line.lstrip())
            if indent > 0:
                body_lines.append(sub_comment_stripped.strip())
                i += 1
            else:
                break
        return i, body_lines

    def _define_multiline_from(
        self,
        first_tokens: list[Token],
        body_lines: list[str],
        stripped_stmt: str,
        header_line_num: int,
        header_col: int,
    ) -> None:
        try:
            self._define_multiline_function(
                first_tokens, body_lines, stripped_stmt
            )
        except GrammarError:
            raise
        except PhysureError as err:
            if err.line is None:
                err.line = header_line_num
                err.column = header_col
            raise
        except Exception as err:
            raise GrammarError(
                str(err), line=header_line_num, column=header_col
            ) from err

    def _run_substatements(
        self, sub_stmts: list[str], line: str, current_line: int
    ) -> list[GrammarValue | None]:
        results: list[GrammarValue | None] = []
        for part_stmt in sub_stmts:
            col = line.find(part_stmt) + 1
            try:
                results.append(self._eval_statement(part_stmt))
            except GrammarError:
                raise
            except PhysureError as err:
                if err.line is None:
                    err.line = current_line
                    err.column = col
                raise
            except Exception as err:
                msg = str(err)
                raise GrammarError(msg, line=current_line, column=col) from err
        return results

    def _is_multiline_func_header(self, tokens: list[Token]) -> bool:
        if len(tokens) < 3:
            return False
        if tokens[0].type != "IDENT" or tokens[1].value != "(":
            return False
        close_idx = _find_matching_paren(tokens, 1)
        if close_idx == -1:
            return False
        return (
            close_idx + 2 == len(tokens)
            and tokens[close_idx].value == ")"
            and tokens[-1].value == "="
        )

    def _define_multiline_function(
        self, header_tokens: list[Token], body_lines: list[str], stmt: str
    ) -> None:
        if not body_lines:
            raise GrammarError(
                f"Indented multi-line function {stmt!r} has no body statements"
            )
        close_idx = _find_matching_paren(header_tokens, 1)
        params = self._param_list(header_tokens[2:close_idx], stmt)
        name = header_tokens[0].value
        if name in _FUNCTIONS:
            raise GrammarError(f"{name!r} is reserved in: {stmt!r}")
        body_statements = [_tokenize(b_line) for b_line in body_lines]
        self._functions[name] = UserFunction(
            params=params,
            body_statements=body_statements,
            body_lines=body_lines,
        )

    def eval(self, source: str) -> GrammarValue | None:
        """Evaluates statements and returns the last result."""
        results = self.run(source)
        return results[-1] if results else None

    # --- statement handling -------------------------------------------

    @staticmethod
    def _split_format_spec(
        tokens: list[Token], stmt: str
    ) -> tuple[list[Token], str | int | None]:
        """Strips a trailing `: spec` (e.g. `: .2f`, `: .3e`, `: base`, `: .2f|base`, `: 3`)."""
        colon_idx = _top_level_index(tokens, ":")
        question_idx = _top_level_index(tokens, "?")
        if colon_idx == -1 or (
            question_idx != -1 and question_idx < colon_idx
        ):
            return tokens, None

        spec_start = (
            tokens[colon_idx + 1].pos if colon_idx + 1 < len(tokens) else None
        )
        if spec_start is None:
            return tokens, None

        end = tokens[-1].pos + len(tokens[-1].value)
        spec_str = stmt[spec_start:end].strip()

        try:
            return tokens[:colon_idx], int(spec_str)
        except ValueError:
            return tokens[:colon_idx], spec_str

    @staticmethod
    def _strip_value_query(
        tokens: list[Token],
    ) -> tuple[list[Token], bool | str]:
        """Strips a trailing `?` or `= ?`; returns tokens and the want_value flag."""
        want_value: bool | str = True
        if tokens and tokens[-1].value == "?":
            if len(tokens) >= 2 and tokens[-2].value == "=":
                tokens = tokens[:-2]
                if (
                    _top_level_index(tokens, "=") != -1
                    or _top_level_index(tokens, "->") != -1
                ):
                    want_value = "assign"
            else:
                tokens = tokens[:-1]
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

    def _try_define_function(self, tokens: list[Token], stmt: str) -> bool:
        """Detects and stores `name(params) = body`; returns True if handled."""
        if (
            len(tokens) < 4
            or tokens[0].type != "IDENT"
            or tokens[1].value != "("
        ):
            return False
        close_idx = _find_matching_paren(tokens, 1)
        if close_idx == -1:
            raise GrammarError(f"Missing closing parenthesis in: {stmt!r}")
        if close_idx + 1 >= len(tokens) or tokens[close_idx + 1].value != "=":
            return False
        if tokens[-1].value == "?":
            return False
        try:
            params = self._param_list(tokens[2:close_idx], stmt)
        except GrammarError:
            return False
        name = tokens[0].value
        if name in _FUNCTIONS:
            raise GrammarError(f"{name!r} is reserved in: {stmt!r}")
        if name in self.env:
            raise GrammarError(f"{name!r} is already a variable in: {stmt!r}")
        body_tokens = tokens[close_idx + 2 :]
        if not body_tokens:
            return False
        self._functions[name] = UserFunction(
            params=params, body_tokens=body_tokens
        )
        return True

    def _eval_unit_expr(self, tokens: list[Token]) -> GrammarValue:
        if not tokens:
            raise GrammarError("Empty unit specifier")
        return _ExprParser(
            tokens,
            self.system.get_unit,
            self._q,
            self._functions,
            self._call_user_function,
        ).parse()

    def _param_list(
        self, tokens: list[Token], stmt: str
    ) -> list[tuple[str, list[Token] | None]]:
        if not tokens:
            return []
        params: list[tuple[str, list[Token] | None]] = []
        for part in _split_on_commas(tokens):
            if not part or part[0].type != "IDENT":
                raise GrammarError(f"Invalid parameter list in: {stmt!r}")
            if len(part) == 1:
                params.append((part[0].value, None))
                continue
            if len(part) >= 3 and part[1].value == ":":
                unit_tokens = part[2:]
                expected = self._eval_unit_expr(unit_tokens)
                expected_unit = getattr(expected, "unit", expected)
                if not hasattr(expected_unit, "dimension"):
                    raise GrammarError(
                        f"Invalid unit specifier in parameter list: {stmt!r}"
                    )
                params.append((part[0].value, unit_tokens))
                continue
            raise GrammarError(f"Invalid parameter list in: {stmt!r}")
        return params

    def _bind_param(
        self, name: str, unit_tokens: list[Token] | None, arg: GrammarValue
    ) -> GrammarValue:
        if unit_tokens is None:
            return arg
        expected = self._eval_unit_expr(unit_tokens)
        expected_unit = getattr(expected, "unit", expected)
        actual_dim = getattr(arg, "unit", None)
        if actual_dim is None:
            raise DimensionError(
                f"Parameter {name!r} expects a quantity with dimension "
                f"{expected_unit.dimension(self.system)!r}, got a bare number"
            )
        if actual_dim.dimension(self.system) != expected_unit.dimension(
            self.system
        ):
            raise DimensionError(
                f"Parameter {name!r} expects dimension "
                f"{expected_unit.dimension(self.system)!r}, "
                f"got {actual_dim.dimension(self.system)!r}"
            )
        return arg

    def _eval_statement(self, stmt: str) -> GrammarValue | None:
        tokens = _tokenize(stmt)

        if self._try_define_function(tokens, stmt):
            return None
        if tokens and tokens[0].type == "IDENT" and tokens[0].value == "let":
            raise GrammarError("'let' is only valid inside a function body")

        eq_idx = _top_level_index(tokens, "==")
        if eq_idx != -1:
            lhs = self._eval_expr(tokens[:eq_idx])
            rhs = self._eval_expr(tokens[eq_idx + 1 :])
            return self._is_close(lhs, rhs)

        # Trailing `= ?` -> return the value even when assigning.
        tokens, want_value = self._strip_value_query(tokens)
        # Trailing `: format_spec` (e.g. `: .2f`, `: .3e`, `: base`, `: .2f|base`, `: 3`).
        tokens, format_spec = self._split_format_spec(tokens, stmt)
        # Trailing `=> unit` conversion (unit slice taken from source text).
        tokens, target_unit = self._split_conversion(tokens, stmt)
        tokens, name = self._split_assignment(tokens, stmt)

        value = self._eval_expr(tokens)
        if target_unit is not None:
            value = self._apply_target_unit(value, target_unit)

        if name is not None:
            self.env[name] = value

        display_val: Any = value
        if format_spec is not None:
            display_val = self._apply_format_spec(value, format_spec)

        if name is not None:
            return display_val if want_value == "assign" else None
        return display_val

    def _apply_target_unit(
        self, value: GrammarValue, target_unit: str
    ) -> GrammarValue:
        if hasattr(value, "to_base_units") and target_unit.lower() in (
            "base",
            "si",
            "raw",
            "expand",
        ):
            return value.to_base_units()
        return value.to(target_unit)

    def _apply_format_spec(
        self, value: GrammarValue, format_spec: int | str
    ) -> GrammarValue:
        if isinstance(format_spec, int) and format_spec > 0:
            return _format_sig_figs(value, format_spec)
        if isinstance(format_spec, str) and format_spec:
            return format(value, format_spec)
        return value

    def _eval_expr(self, tokens: list[Token]) -> GrammarValue:
        if not tokens:
            raise GrammarError("Empty expression")
        return _ExprParser(
            tokens,
            self._resolve,
            self._q,
            self._functions,
            self._call_user_function,
        ).parse()

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
        scope = {
            param_name: self._bind_param(param_name, unit_symbol, arg)
            for (param_name, unit_symbol), arg in zip(
                fn.params, args, strict=True
            )
        }

        def resolve(ident: str) -> GrammarValue:
            if ident in scope:
                return scope[ident]
            return self._resolve(ident)

        try:
            if fn.body_statements is not None:
                last_val: GrammarValue | None = None
                body_lines = fn.body_lines or [""] * len(fn.body_statements)
                for stmt_tokens, stmt_line in zip(
                    fn.body_statements, body_lines, strict=True
                ):
                    last_val = self._eval_tokens_with_scope(
                        stmt_tokens, stmt_line, resolve, depth + 1
                    )
                if last_val is None:
                    raise GrammarError(
                        f"Function {name!r} ended without returning a value"
                    )
                return last_val
            elif fn.body_tokens is not None:
                return _ExprParser(
                    fn.body_tokens,
                    resolve,
                    self._q,
                    self._functions,
                    self._call_user_function,
                    depth + 1,
                ).parse()
            else:
                raise GrammarError(f"Function {name!r} has no body")
        except RecursionError as err:
            raise GrammarError(
                f"recursion limit ({limit}) exceeded calling {name!r}"
            ) from err

    def _eval_tokens_with_scope(
        self,
        tokens: list[Token],
        stmt: str,
        resolve_fn: Callable[[str], GrammarValue],
        depth: int,
    ) -> GrammarValue | None:
        eq_idx = _top_level_index(tokens, "==")
        if eq_idx != -1:
            return self._eval_scoped_equality(
                tokens, eq_idx, resolve_fn, depth
            )

        tokens, want_value = self._strip_value_query(tokens)
        tokens, format_spec = self._split_format_spec(tokens, stmt)
        tokens, target_unit = self._split_conversion(tokens, stmt)
        tokens, name = self._split_assignment(tokens, stmt)

        value = _ExprParser(
            tokens,
            resolve_fn,
            self._q,
            self._functions,
            self._call_user_function,
            depth,
        ).parse()
        if target_unit is not None:
            value = self._apply_target_unit(value, target_unit)

        if name is not None:
            # Rebind in the local scope layer (closure cell of resolve_fn)
            resolve_fn.__closure__[0].cell_contents[name] = value

        display_val: Any = value
        if format_spec is not None:
            display_val = self._apply_format_spec(value, format_spec)

        if name is not None:
            return display_val if want_value == "assign" else None
        return display_val

    def _eval_scoped_equality(
        self,
        tokens: list[Token],
        eq_idx: int,
        resolve_fn: Callable[[str], GrammarValue],
        depth: int,
    ) -> bool:
        lhs = _ExprParser(
            tokens[:eq_idx],
            resolve_fn,
            self._q,
            self._functions,
            self._call_user_function,
            depth,
        ).parse()
        rhs = _ExprParser(
            tokens[eq_idx + 1 :],
            resolve_fn,
            self._q,
            self._functions,
            self._call_user_function,
            depth,
        ).parse()
        return self._is_close(lhs, rhs)

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
    """Evaluate `source` with a fresh interpreter, returning the last result."""
    return GrammarInterpreter(system=system, rel_tol=rel_tol).eval(source)
