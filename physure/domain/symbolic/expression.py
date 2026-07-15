"""Defines the SymbolicExpression class for unit-aware symbolic math."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import sympy as sp

try:
    import symengine as se

    HAVE_SYMENGINE = True
except ImportError:
    se = None  # type: ignore
    # ponytail: HAVE_SYMENGINE toggles between True/False across the
    # try/except branches by design; not a real constant-redefinition bug.
    HAVE_SYMENGINE = False  # pyright: ignore[reportConstantRedefinition]


from physure import default_system
from physure.application.functions.functions import Function
from physure.domain.exceptions import IncompatibleUnitsError

if TYPE_CHECKING:
    from collections.abc import Callable

    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit
    from physure.domain.symbolic.quantity import SymbolicQuantity


class SymbolicExpression:
    """A dimensionally consistent symbolic mathematical expression."""

    def __init__(
        self,
        sympy_expr: sp.Expr,
        unit: CompoundUnit,
        system: UnitSystem = default_system,
        variables: set[SymbolicQuantity] | None = None,
    ):
        """Initializes a symbolic expression."""
        if HAVE_SYMENGINE:
            self._expr = se.sympify(sympy_expr)  # type: ignore
        else:
            self._expr = sympy_expr
        self.unit = unit
        self.system = system
        # Track the atomic variables that make up this expression
        self.variables = variables or set()
        self._sympy_expr_cached: sp.Expr | None = None

    @property
    def expr(self) -> sp.Expr:
        """Returns the expression converted to a SymPy Expr for external compatibility."""
        if HAVE_SYMENGINE:
            cached = self._sympy_expr_cached
            if cached is None:
                sp_expr = sp.sympify(self._expr)
                symbols = {
                    s: sp.Symbol(s.name, positive=True)
                    for s in sp_expr.free_symbols
                    if isinstance(s, sp.Symbol)
                }
                cached = sp_expr.xreplace(symbols) if symbols else sp_expr
                self._sympy_expr_cached = cached
            return cached
        return self._expr

    @property
    def dimension(self) -> Dimension:
        """Returns the physical dimension of the expression."""
        return self.unit.dimension(self.system)

    # --- NEW: Direct Evaluation ---
    def evaluate(
        self, output_unit: str | CompoundUnit | None = None, **kwargs: Quantity
    ) -> Quantity:
        """Evaluates the expression directly with Quantity arguments.

        Args:
            output_unit: The desired unit for the result. Defaults to the
                expression's native unit.
            **kwargs: The values for the symbolic variables
                (e.g., m=Q_(10, 'kg')).
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
        self, output_unit: str | CompoundUnit | None = None, **kwargs: Quantity
    ) -> Quantity:
        """Alias for evaluate(), allowing the object to be called."""
        return self.evaluate(output_unit, **kwargs)

    # --- NEW: Jupyter Pretty Printing ---
    def _repr_latex_(self) -> str:
        """Returns the LaTeX representation for Jupyter rendering."""
        # Format: Expression [Unit]
        from sympy import latex

        unit_latex = self.unit.to_latex()
        return (
            f"${latex(self.expr)} \\; [{unit_latex if unit_latex else '1'}]$"
        )

    # --- Existing Arithmetic Methods ---
    def _operate(
        self,
        other: SymbolicExpression | float,
        op: Callable,
        unit_op: Callable,
    ) -> SymbolicExpression:
        """Helper to perform operations with unit propagation."""
        if isinstance(other, SymbolicExpression):
            if self.system is not other.system:
                raise ValueError(
                    "Cannot operate between different UnitSystems."
                )
            new_expr = op(self._expr, other._expr)
            new_unit = unit_op(self.unit, other.unit)
            new_vars = self.variables | other.variables
            return SymbolicExpression(
                new_expr, new_unit, self.system, new_vars
            )

        new_expr = op(self._expr, other)
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )

    def __add__(self, other: SymbolicExpression) -> SymbolicExpression:
        """Adds two symbolic expressions."""
        if not isinstance(other, SymbolicExpression):
            raise TypeError("Can only add/sub other SymbolicExpressions.")
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        return SymbolicExpression(
            self._expr + other._expr,
            self.unit,
            self.system,
            self.variables | other.variables,
        )

    def __sub__(self, other: SymbolicExpression) -> SymbolicExpression:
        """Subtracts two symbolic expressions."""
        if not isinstance(other, SymbolicExpression):
            raise TypeError("Can only add/sub other SymbolicExpressions.")
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        return SymbolicExpression(
            self._expr - other._expr,
            self.unit,
            self.system,
            self.variables | other.variables,
        )

    def __mul__(self, other: SymbolicExpression | float) -> SymbolicExpression:
        """Multiplies two symbolic expressions."""
        return self._operate(other, lambda x, y: x * y, lambda u1, u2: u1 * u2)

    def __rmul__(
        self, other: SymbolicExpression | float
    ) -> SymbolicExpression:
        """Multiplies two symbolic expressions (reflected)."""
        return self.__mul__(other)

    def __truediv__(
        self, other: SymbolicExpression | float
    ) -> SymbolicExpression:
        """Divides two symbolic expressions."""
        return self._operate(other, lambda x, y: x / y, lambda u1, u2: u1 / u2)

    def __rtruediv__(self, other: float) -> SymbolicExpression:
        """Divides two symbolic expressions (reflected)."""
        if not isinstance(other, (int, float)):
            return NotImplemented
        new_expr = other / self._expr
        new_unit = 1 / self.unit
        return SymbolicExpression(
            new_expr, new_unit, self.system, self.variables
        )

    def __pow__(self, power: float) -> SymbolicExpression:
        """Raises the expression to a power."""
        new_expr = self._expr**power
        new_unit = self.unit**power
        return SymbolicExpression(
            new_expr, new_unit, self.system, self.variables
        )

    def __repr__(self) -> str:
        """Returns a string representation for debugging."""
        return f"Expression({self._expr}) [{self.unit}]"

    def to_function(
        self, *args: SymbolicExpression, backend: str = "numpy"
    ) -> Function:
        """Converts this expression into a callable Function object."""
        params = {str(arg._expr): arg.unit for arg in args}
        return Function(
            parameters=params,
            output_unit=self.unit,
            symbolic_func=self.expr,
            system=self.system,
            backend=backend,
        )

    def compile(self, backend: str = "numpy") -> Function:
        """Compiles the expression into a backend-optimized function.

        Args:
           backend: 'numpy', 'torch', 'jax', etc.

        Returns:
           A Function object that accepts arguments matching the variables.
        """
        # Automatically determine arguments from tracked variables
        args_list = list(self.variables)
        args_list.sort(key=lambda v: v.expr.name)
        return self.to_function(*args_list, backend=backend)

    def simplify(self) -> SymbolicExpression:
        """Simplifies the underlying symbolic expression."""
        new_expr = sp.simplify(self.expr)
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )

    def expand(self) -> SymbolicExpression:
        """Expands the underlying symbolic expression."""
        new_expr = (
            se.expand(self._expr) if HAVE_SYMENGINE else sp.expand(self._expr)  # type: ignore
        )
        return SymbolicExpression(
            new_expr, self.unit, self.system, self.variables
        )

    def diff(
        self, variable: SymbolicQuantity, n: int = 1
    ) -> SymbolicExpression:
        """Differentiates the expression with respect to a variable.

        Updates units: unit(df/dx) = unit(f) / unit(x).
        """
        if not hasattr(variable, "unit"):
            raise TypeError(
                "Differentiation variable must be a SymbolicQuantity"
            )

        # 1. SymPy / SymEngine Operation
        new_expr = (
            se.diff(self._expr, variable._expr, n)  # type: ignore
            if HAVE_SYMENGINE
            else sp.diff(self._expr, variable._expr, n)
        )

        # 2. Unit Operation (Derivative rule: unit / var_unit^n)
        variable_unit = variable.unit
        new_unit = self.unit / (variable_unit**n)

        # 3. Variable Tracking
        variable_vars = getattr(variable, "variables", set())
        new_vars = self.variables | variable_vars

        return SymbolicExpression(new_expr, new_unit, self.system, new_vars)

    def integrate(
        self,
        variable: SymbolicQuantity,
        # ponytail: sympy's integrate() kwargs (meijerg, conds, risch, ...)
        # are genuinely dynamic third-party flags, not a typing gap here.
        **kwargs: Any,
    ) -> SymbolicExpression:
        """Integrates the expression with respect to a variable.

        Updates units: unit(int f dx) = unit(f) * unit(x).
        """
        if not hasattr(variable, "unit"):
            raise TypeError(
                "Integration variable must be a SymbolicQuantity with units."
            )

        # 1. SymPy Operation
        new_expr = sp.integrate(self.expr, variable.expr, **kwargs)

        # 2. Unit Operation (Integral rule: unit * var_unit)
        variable_unit = variable.unit
        new_unit = self.unit * variable_unit

        # 3. Variable Tracking
        variable_vars = getattr(variable, "variables", set())
        new_vars = self.variables | variable_vars

        return SymbolicExpression(new_expr, new_unit, self.system, new_vars)
