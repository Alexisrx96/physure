"""Sanitization logic for unit strings before parsing."""

from __future__ import annotations

import re
from typing import ClassVar


class UnitSanitizer:
    """Sanitizes raw unit strings for SymPy parsing."""

    # Unicode superscript map
    _SUPERSCRIPTS: ClassVar[dict[str, str]] = {
        "⁰": "0",
        "¹": "1",
        "²": "2",
        "³": "3",
        "⁴": "4",
        "⁵": "5",
        "⁶": "6",
        "⁷": "7",
        "⁸": "8",
        "⁹": "9",
        "⁻": "-",
        "⋅": "*",
        "·": "*",
        "×": "*",  # noqa: RUF001 — intentional unicode operator
    }

    @classmethod
    def sanitize(cls, expression: str) -> str:
        """Sanitizes a unit string for SymPy parsing.

        Args:
            expression: The raw unit string.

        Returns:
            A clean string ready for sympy.parse_expr.
        """
        if not expression:
            return ""

        expr = expression.strip()

        # 1. Normalize unicode operators (dots, cross)
        # We handle this manually or via loop, doing explicit replace is safer for distinct chars
        expr = expr.replace("⋅", "*")
        expr = expr.replace("·", "*")
        expr = expr.replace("×", "*")  # noqa: RUF001

        # 2. Normalize degrees
        # "°" -> "deg"
        expr = expr.replace("°", "deg")

        # 3. Handle unicode superscripts: ² -> **2
        def replace_sup(match: re.Match) -> str:
            sup_str = match.group(0)
            # Filter out non-superscript chars if any (though regex handles it)
            normal_chars = []
            for c in sup_str:
                # Operators live in the dict too, but this regex only
                # matches superscripts.
                if c in cls._SUPERSCRIPTS and c in "⁰¹²³⁴⁵⁶⁷⁸⁹⁻":
                    normal_chars.append(cls._SUPERSCRIPTS[c])

            normal_str = "".join(normal_chars)
            return f"**{normal_str}"

        # Pattern matches one or more superscript chars
        sup_pattern = r"[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+"
        expr = re.sub(sup_pattern, replace_sup, expr)

        # 4. Normalize caret to python power
        expr = expr.replace("^", "**")

        # 5. Implicit multiplication
        # Insert * between:
        # - alphanumeric/paren AND alphanumeric/paren
        # separated by whitespace.
        # "m s" -> "m*s"
        # "kg m/s" -> "kg*m/s"
        # "(m) s" -> "(m)*s"
        expr = re.sub(r"(?<=[a-zA-Z0-9)])\s+(?=[a-zA-Z0-9(])", "*", expr)

        # 6. Handle Currency symbol "$" (invalid in SymPy)
        # We assume $ is a unit symbol.
        expr = expr.replace("$", "__DOLLAR__")

        return expr
