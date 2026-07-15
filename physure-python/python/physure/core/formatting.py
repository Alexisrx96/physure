"""Formatting utilities for units and numbers."""

from __future__ import annotations

# Superscript logic
__SUPERSCRIPT_TABLE = "⁰¹²³⁴⁵⁶⁷⁸⁹⋅⁻"
_SUPERSCRIPT_MAP = str.maketrans("0123456789.-", __SUPERSCRIPT_TABLE)


def to_superscript(n: str | float) -> str:
    """Convert a number to its superscript representation.

    Used for string formatting of units (e.g. m/s²).
    """
    if isinstance(n, float) and n.is_integer():
        n = int(n)

    result = str(n).translate(_SUPERSCRIPT_MAP)
    # Filter to ensure only valid superscript chars are returned
    return "".join(c for c in result if c in "⁰¹²³⁴⁵⁶⁷⁸⁹⋅⁻")
