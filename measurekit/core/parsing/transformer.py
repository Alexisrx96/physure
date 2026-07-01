"""Transforms SymPy AST into MeasureKit CompoundUnit objects."""

from __future__ import annotations

from typing import Any

# We need the concrete class to instantiate it
from measurekit.domain.measurement.units import CompoundUnit


class UnitParsingError(ValueError):
    """Raised when the unit expression contains unsupported operations."""

    pass


class SymPyTransformer:
    """Transforms a SymPy expression into a CompoundUnit."""

    def transform(self, expr: Any) -> CompoundUnit:
        """Recursively transforms a SymPy AST into a CompoundUnit.

        Args:
            expr: The SymPy expression to transform.

        Returns:
            The equivalent CompoundUnit.

        Raises:
            UnitParsingError: If the expression contains unsupported operations.
        """
        import sympy as sp

        # 1. Base Case: Symbol -> Unit
        if isinstance(expr, sp.Symbol):
            # sp.Symbol name is the unit string
            name = expr.name
            if name == "__DOLLAR__":
                name = "$"
            return CompoundUnit({name: 1})

        # 2. Integer/Number -> Dimensionless
        # Treat all numbers as dimensionless unity for unit composition purposes.
        if isinstance(expr, (sp.Integer, sp.Float, sp.Rational, sp.Number)):
            return CompoundUnit({})

        # 3. Power: unit**exp
        if isinstance(expr, sp.Pow):
            base = expr.base
            exp = expr.exp

            base_unit = self.transform(base)

            # Extract numeric exponent
            if not exp.is_number:
                raise UnitParsingError(f"Exponent must be a number, got {exp}")

            try:
                # Convert to simple python type
                val = float(exp)
                if val.is_integer():
                    val = int(val)
            except (TypeError, ValueError) as e:
                raise UnitParsingError(
                    f"Could not convert exponent {exp} to number."
                ) from e

            return base_unit**val

        # 4. Multiplication/Division: unit * unit
        if isinstance(expr, sp.Mul):
            # SymPy represents division as multiplication by power -1,
            # so Mul covers both.
            result = CompoundUnit({})
            for term in expr.args:
                result = result * self.transform(term)
            return result

        # 5. Unsupported operations
        if isinstance(expr, sp.Add):
            raise UnitParsingError(
                "Resulting expression contains addition, which is invalid for unit definitions."
            )

        # generic catch-all
        raise UnitParsingError(
            f"Unsupported expression type: {type(expr)} ({expr})"
        )
