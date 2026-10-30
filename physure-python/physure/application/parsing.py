# physure/application/parsing.py
"""Unit string parsing: the core's parser."""

from __future__ import annotations

import functools
import re
from typing import TypeVar

from physure._core import parse_unit_expression as _rust_parse_unit_expr
from physure.domain.measurement.base_entity import ExponentEntityProtocol

T = TypeVar("T", bound=ExponentEntityProtocol)

# Handles implicit multiplication: "m s" -> "m*s"
_IMPLICIT_MUL = re.compile(r"(?<=[a-zA-Z0-9)])\s+(?=[a-zA-Z0-9(])")


def _native_parse(expression: str, entity_cls: type[T]) -> T:
    """Parse using the native Rust core parser."""
    expr = expression.strip()
    expr = expr.replace("°", "deg")
    expr = _IMPLICIT_MUL.sub("*", expr)
    res = _rust_parse_unit_expr(expr)
    if hasattr(res, "dimensions"):
        exponents = {}
        for symbol, (num, den) in res.dimensions.items():
            exponents[symbol] = num if den == 1 else (num, den)
        return entity_cls(exponents)
    return res  # pyright: ignore[reportReturnType]


@functools.lru_cache(maxsize=2048)
def parse_unit_string(expression: str, entity_cls: type[T]) -> T:
    """Parse a unit or dimension string into the target entity class.

    The parser is the core's: ``m/s^2``, ``m/s**2``, ``kg m s^-2``, ``s**0.5``,
    ``J/(mol*K)`` and ``m²`` all read the same way in every binding.

    Raises:
        ValueError: The expression is not a unit.
    """
    try:
        return _native_parse(expression, entity_cls)
    except Exception as error:
        raise ValueError(f"Parsing failed: {error}") from error
