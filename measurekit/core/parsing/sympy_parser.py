"""Main entry point for SymPy-based unit parsing."""

from __future__ import annotations

# sympy is imported lazily inside parse() to improve startup performance.
_SAFE_GLOBALS = None


def _get_safe_globals():
    global _SAFE_GLOBALS
    if _SAFE_GLOBALS is None:
        import sympy as sp

        # ponytail: lazy singleton reassigns this "constant" once,
        # from None to a real dict.
        _SAFE_GLOBALS = {  # pyright: ignore[reportConstantRedefinition]
            "Mul": sp.Mul,
            "Add": sp.Add,
            "Pow": sp.Pow,
            "Integer": sp.Integer,
            "Float": sp.Float,
            "Rational": sp.Rational,
            "Symbol": sp.Symbol,
        }
    return _SAFE_GLOBALS


from measurekit.core.parsing.sanitizer import UnitSanitizer  # noqa: E402
from measurekit.core.parsing.transformer import SymPyTransformer  # noqa: E402
from measurekit.domain.measurement.units import CompoundUnit  # noqa: E402


class SymPyUnitParser:
    """Parses unit strings using SymPy and transforms them into CompoundUnits."""

    def __init__(self) -> None:
        self._transformer = SymPyTransformer()

    def parse(self, expression: str | None) -> CompoundUnit:
        """Parses a unit string into a CompoundUnit.

        Args:
            expression: The string representation of the unit (e.g., "kg * m / s^2").

        Returns:
            A CompoundUnit representing the parsed unit. Returns a dimensionless
            unit if the expression is empty or None.
        """
        if not expression or not expression.strip():
            return CompoundUnit({})

        # 1. Sanitize the input string
        clean_expr = UnitSanitizer.sanitize(expression)

        # 2. Parse into SymPy AST
        # evaluate=False ensures we control the structure.
        # global_dict=_get_safe_globals() ensures that common identifiers are parsed as Symbols.
        import sympy as sp

        try:
            sympy_ast = sp.parse_expr(
                clean_expr, evaluate=False, global_dict=_get_safe_globals()
            )
        except Exception as e:
            raise ValueError(
                f"SymPy failed to parse expression '{expression}' (sanitized: '{clean_expr}'): {e}"
            ) from e

        # 3. Transform AST into CompoundUnit
        return self._transformer.transform(sympy_ast)
