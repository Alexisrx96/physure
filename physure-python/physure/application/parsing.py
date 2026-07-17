# physure/application/parsing.py
"""Unit string parsing — native parser first, SymPy fallback."""

from __future__ import annotations

import functools
import re
from typing import TypeVar

from physure.domain.measurement.base_entity import ExponentEntityProtocol

T = TypeVar("T", bound=ExponentEntityProtocol)

# Handles implicit multiplication: "m s" -> "m*s"
_IMPLICIT_MUL = re.compile(r"(?<=[a-zA-Z0-9)])\s+(?=[a-zA-Z0-9(])")

# Singleton SymPy parser, loaded lazily only when native parser fails
_SYMPY_PARSER = None


def _get_sympy_parser():
    global _SYMPY_PARSER
    if _SYMPY_PARSER is None:
        try:
            from physure.core.parsing.sympy_parser import SymPyUnitParser

            # ponytail: lazy singleton reassigns this "constant" once,
            # from None to a real parser instance.
            _SYMPY_PARSER = (  # pyright: ignore[reportConstantRedefinition]
                SymPyUnitParser()
            )
        except ImportError as e:
            raise ImportError(
                "sympy is required to parse this unit expression. "
                "Install it with: pip install physure[symbolic]"
            ) from e
    return _SYMPY_PARSER


from physure._core import parse_unit_expression as _rust_parse_unit_expr


def _native_parse(expression: str, entity_cls: type[T]) -> T:
    """Parse using the native Rust core parser."""
    expr = expression.strip()
    expr = expr.replace("°", "deg")
    expr = _IMPLICIT_MUL.sub("*", expr)
    res = _rust_parse_unit_expr(expr)
    if hasattr(res, "dimensions"):
        exponents = {}
        for symbol, (num, den) in res.dimensions.items():
            if den != 1:
                raise ValueError(
                    f"Non-integer dimension exponent {num}/{den} for "
                    f"{symbol!r} in {expression!r} is not supported."
                )
            exponents[symbol] = num
        return entity_cls(exponents)
    return res  # pyright: ignore[reportReturnType]


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
