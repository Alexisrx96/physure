# measurekit/application/parsing.py
"""Lexical analyzer for scientific notations."""

from __future__ import annotations

import functools
from typing import TypeVar

from measurekit.core.parsing.sympy_parser import SymPyUnitParser
from measurekit.domain.notation.protocols import ExponentEntityProtocol

T = TypeVar("T", bound=ExponentEntityProtocol)

# Singleton parser instance
_PARSER = SymPyUnitParser()


@functools.lru_cache(maxsize=2048)
def parse_unit_string(expression: str, entity_cls: type[T]) -> T:
    """Parses a unit or dimension string into the target entity class.

    This function uses simple memoization to avoid redundant processing.
    It delegates to the SymPy-based engine.

    Args:
        expression: The unit/dimension string to parse.
        entity_cls: The class to instantiate (e.g., CompoundUnit, Dimension).

    Returns:
        An instance of entity_cls.
    """
    # 1. Parse into CompoundUnit using SymPy engine
    try:
        # returns CompoundUnit (which has .exponents)
        compound_unit = _PARSER.parse(expression)
    except Exception as e:
        # Wrap as ValueError for API compatibility (legacy)
        raise ValueError(f"Parsing failed: {e}") from e

    # 2. Convert to the requested entity class if necessary
    # If the caller requested exactly CompoundUnit, we are done.
    if issubclass(entity_cls, type(compound_unit)):
        return compound_unit  # type: ignore

    # Otherwise, assume entity_cls accepts exponents dict (Protocol check)
    return entity_cls(compound_unit.exponents)
