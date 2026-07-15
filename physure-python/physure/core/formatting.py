"""Formatting utilities for units and numbers."""

from __future__ import annotations

# Superscript and Subscript logic
_SUPERSCRIPT_CHARS = "⁰¹²³⁴⁵⁶⁷⁸⁹⋅⁻"
_SUPERSCRIPT_MAP = str.maketrans("0123456789.-", _SUPERSCRIPT_CHARS)
_SUPERSCRIPT_REVERSE_MAP = str.maketrans(_SUPERSCRIPT_CHARS, "0123456789.-")
_SUBSCRIPT_MAP = str.maketrans("0123456789-", "₀₁₂₃₄₅₆₇₈₉₋")
_SUBSCRIPT_REVERSE_MAP = str.maketrans("₀₁₂₃₄₅₆₇₈₉₋", "0123456789-")


def to_superscript(n: str | float) -> str:
    """Convert a number to its superscript representation."""
    if isinstance(n, float) and n.is_integer():
        n = int(n)

    result = str(n).translate(_SUPERSCRIPT_MAP)
    return "".join(c for c in result if c in _SUPERSCRIPT_CHARS)


def to_subscript(n: str | float) -> str:
    """Convert an integer to its subscript representation."""
    result = str(n).translate(_SUBSCRIPT_MAP)
    return "".join(c for c in result if c in "₀₁₂₃₄₅₆₇₈₉₋")


def subscript_to_ascii(s: str) -> str:
    """Normalize Unicode subscript digits to ASCII."""
    return s.translate(_SUBSCRIPT_REVERSE_MAP)


def parse_superscript(sup: str) -> int | float:
    """Convert a superscript number to an integer or float."""
    try:
        return int(sup.translate(_SUPERSCRIPT_REVERSE_MAP))
    except ValueError:
        try:
            return float(sup.translate(_SUPERSCRIPT_REVERSE_MAP))
        except ValueError:
            return 0
