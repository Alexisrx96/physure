# measurekit/application/parsing.py
"""Unit string parsing — native parser first, SymPy fallback."""

from __future__ import annotations

import functools
import re
from typing import TypeVar

from measurekit.domain.notation.lexer import generate_tokens
from measurekit.domain.notation.parsers import NotationParser
from measurekit.domain.notation.protocols import ExponentEntityProtocol

T = TypeVar("T", bound=ExponentEntityProtocol)

# Handles implicit multiplication: "m s" -> "m*s"
_IMPLICIT_MUL = re.compile(r"(?<=[a-zA-Z0-9)])\s+(?=[a-zA-Z0-9(])")

# Singleton SymPy parser, loaded lazily only when native parser fails
_SYMPY_PARSER = None


def _get_sympy_parser():
    global _SYMPY_PARSER
    if _SYMPY_PARSER is None:
        try:
            from measurekit.core.parsing.sympy_parser import SymPyUnitParser

            # ponytail: lazy singleton reassigns this "constant" once,
            # from None to a real parser instance.
            _SYMPY_PARSER = (  # pyright: ignore[reportConstantRedefinition]
                SymPyUnitParser()
            )
        except ImportError as e:
            raise ImportError(
                "sympy is required to parse this unit expression. "
                "Install it with: pip install measurekit[symbolic]"
            ) from e
    return _SYMPY_PARSER


def _native_parse(expression: str, entity_cls: type[T]) -> T:
    """Parse using the pure-Python NotationParser (no sympy)."""
    expr = expression.strip()
    expr = expr.replace("°", "deg")
    expr = _IMPLICIT_MUL.sub("*", expr)
    tokens = generate_tokens(expr)
    parser = NotationParser(tokens, entity_cls)
    # ponytail: NotationParser is typed against the ExponentEntityProtocol,
    # not generic over T; it always builds an instance of entity_cls though.
    return parser.parse()  # pyright: ignore[reportReturnType]


@functools.lru_cache(maxsize=2048)
def parse_unit_string(expression: str, entity_cls: type[T]) -> T:
    """Parse a unit or dimension string into the target entity class.

    Tries the native recursive-descent parser first (no dependencies).
    Falls back to the SymPy-based parser for complex expressions.
    """
    # Fast path: native parser, zero deps
    try:
        return _native_parse(expression, entity_cls)
    except Exception:
        pass

    # Slow path: SymPy parser (requires sympy installed)
    try:
        compound_unit = _get_sympy_parser().parse(expression)
    except ImportError:
        raise
    except Exception as e:
        raise ValueError(f"Parsing failed: {e}") from e

    if issubclass(entity_cls, type(compound_unit)):
        # ponytail: verified at runtime by the issubclass check above;
        # pyright can't narrow CompoundUnit to the generic T.
        return compound_unit  # pyright: ignore[reportReturnType]
    return entity_cls(compound_unit.exponents)
