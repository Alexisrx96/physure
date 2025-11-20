"""Defines the SymbolicExpression class for unit-aware symbolic math."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Callable

import sympy as sp

from measurekit import default_system
from measurekit.application.functions.functions import Function
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem


class SymbolicExpression:
    """Represents a dimensionally consistent symbolic mathematical expression."""

    def __init__(
        self,
        sympy_expr: sp.Expr,
        unit: CompoundUnit,
        system: UnitSystem = default_system,
        variables: set[Any] | None = None,
    ):
        """Initializes a symbolic expression."""
        self.expr = sympy_expr
        self.unit = unit
        self.system = system
        # Track the atomic variables that make up this expression
        self.variables = variables or set()

    @property
    def dimension(self):
        """Returns the physical dimension of the expression."""
        return self.unit.dimension(self.system)

    # --- NEW: Direct Evaluation ---
    def evaluate(
        self, output_unit: str | CompoundUnit | None = None, **kwargs: Any
    ) -> Quantity:
        """Evaluates the expression directly with Quantity arguments.

        Args:
            output_unit: The desired unit for the result. Defaults to the expression's native unit.
            **kwargs: The values for the symbolic variables (e.g., m=Q_(10, 'kg')).
        """
        # 1. Identify arguments from the internal variable tracking
        args_list = list(self.variables)
        # Sort by name for deterministic argument order
        args_list.sort(key=lambda v: v.expr.name)

        # 2. Compile to a temporary function
        func = self.to_function(*args_list)

        # 3. Determine target unit
        target = output_unit if output_unit else self.unit

        # 4. Execute
        return func(target, **kwargs)

    def __call__(
        self, output_unit: str | CompoundUnit | None = None, **kwargs: Any
    ) -> Quantity:
        """Alias for evaluate(), allowing the object to be called like a function."""
        return self.evaluate(output_unit, **kwargs)

    # --- NEW: Jupyter Pretty Printing ---
    def _repr_latex_(self):
        """Returns the LaTeX representation for Jupyter rendering."""
        # Format: Expression [Unit]
        from sympy import latex

        unit_latex = self.unit.to_latex()
        return (
            f"${latex(self.expr)} \\; [{unit_latex if unit_latex else '1'}]$"
        )

    # --- Existing Arithmetic Methods ---
    def _operate(
        self, other: Any, op: Callable, unit_op: Callable
    ) -> SymbolicExpression:
        """Helper to perform operations with unit propagation."""
        if isinstance(other, SymbolicExpression):
            if self.system is not other.system:
                raise ValueError(
                    "Cannot operate between different UnitSystems."
                )
            new_expr = op(self.expr, other.expr)
            new_unit = unit_op(self.unit, other.unit)
            new_vars = self.variables | other.variables
            return SymbolicExpression(
                new_expr, new_unit, self.system, new_vars
            )

        new_expr = op(self.expr, other)
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )

    def __add__(self, other: Any) -> SymbolicExpression:
        if not isinstance(other, SymbolicExpression):
            raise TypeError("Can only add/sub other SymbolicExpressions.")
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        return SymbolicExpression(
            self.expr + other.expr,
            self.unit,
            self.system,
            self.variables | other.variables,
        )

    def __sub__(self, other: Any) -> SymbolicExpression:
        if not isinstance(other, SymbolicExpression):
            raise TypeError("Can only add/sub other SymbolicExpressions.")
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        return SymbolicExpression(
            self.expr - other.expr,
            self.unit,
            self.system,
            self.variables | other.variables,
        )

    def __mul__(self, other: Any) -> SymbolicExpression:
        return self._operate(other, lambda x, y: x * y, lambda u1, u2: u1 * u2)

    def __rmul__(self, other: Any) -> SymbolicExpression:
        return self.__mul__(other)

    def __truediv__(self, other: Any) -> SymbolicExpression:
        return self._operate(other, lambda x, y: x / y, lambda u1, u2: u1 / u2)

    def __rtruediv__(self, other: Any) -> SymbolicExpression:
        if not isinstance(other, (int, float)):
            return NotImplemented
        new_expr = other / self.expr
        new_unit = 1 / self.unit
        return SymbolicExpression(
            new_expr, new_unit, self.system, self.variables
        )

    def __pow__(self, power: float) -> SymbolicExpression:
        new_expr = self.expr**power
        new_unit = self.unit**power
        return SymbolicExpression(
            new_expr, new_unit, self.system, self.variables
        )

    def __repr__(self) -> str:
        return f"Expression({self.expr}) [{self.unit}]"

    def to_function(self, *args: SymbolicExpression) -> Function:
        """Converts this expression into a callable Function object."""
        params = {str(arg.expr): arg.unit for arg in args}
        return Function(
            parameters=params,
            output_unit=self.unit,
            symbolic_func=self.expr,
            system=self.system,
        )

    def simplify(self) -> SymbolicExpression:
        """Simplifies the underlying symbolic expression."""
        new_expr = sp.simplify(self.expr)
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )

    def expand(self) -> SymbolicExpression:
        """Expands the underlying symbolic expression."""
        new_expr = sp.expand(self.expr)
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )
