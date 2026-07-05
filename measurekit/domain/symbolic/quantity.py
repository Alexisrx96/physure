"""Provides classes for atomic symbolic quantities and equations."""

from typing import Any

import sympy as sp

try:
    import symengine as se

    HAVE_SYMENGINE = True
except ImportError:
    se = None  # type: ignore
    HAVE_SYMENGINE = False


from measurekit import default_system
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.symbolic.expression import SymbolicExpression


class SymbolicQuantity(SymbolicExpression):
    """Represents an atomic symbolic variable with a unit."""

    def __init__(
        self,
        name: str,
        unit: str | CompoundUnit,
        system: UnitSystem = default_system,
    ):
        """Initializes a SymbolicQuantity."""
        symbol = (
            se.Symbol(name)
            if HAVE_SYMENGINE
            else sp.Symbol(name, positive=True)  # type: ignore
        )
        resolved_unit = (
            system.get_unit(unit) if isinstance(unit, str) else unit
        )

        super().__init__(symbol, resolved_unit, system, variables=None)
        self.variables = {self}

    @property
    def symbol(self) -> sp.Symbol:
        """Returns the underlying SymPy symbol."""
        return self.expr

    def __repr__(self) -> str:
        """Returns a string representation."""
        return f"Symbol({self.expr}) [{self.unit}]"

    @classmethod
    def from_expression(cls, *args, **kwargs):
        """Quantities are atomic; this method is disabled."""
        raise NotImplementedError(
            "Quantities are atomic. Use SymbolicExpression for results."
        )


class Equation:
    """Represents an equation between two SymbolicExpressions."""

    def __init__(
        self,
        lhs: SymbolicExpression,
        rhs: SymbolicExpression,
        variables: list[SymbolicQuantity] | None = None,
    ):
        """Initializes an Equation between two symbolic expressions."""
        if lhs.dimension != rhs.dimension:
            raise IncompatibleUnitsError(lhs.unit, rhs.unit)

        self.sympy_eq = sp.Eq(lhs.expr, rhs.expr)
        self.lhs = lhs
        self.rhs = rhs
        self.system = lhs.system

        if variables:
            self.variable_map = {v.expr: v for v in variables}
        else:
            all_vars = lhs.variables | rhs.variables
            self.variable_map = {v.expr: v for v in all_vars}

    def __repr__(self) -> str:
        """Returns a string representation of the equation."""
        return str(self.sympy_eq)

    # --- NEW: Jupyter Pretty Printing ---
    def _repr_latex_(self):
        """Returns the LaTeX representation for Jupyter rendering."""
        from sympy import latex

        return f"${latex(self.sympy_eq)}$"

    def _find_target_symbol(self, target: str | SymbolicQuantity) -> Any:
        """Resolves a target name or SymbolicQuantity to a SymPy symbol."""
        if isinstance(target, SymbolicQuantity):
            return target.expr

        for sym in self.variable_map:
            if sym.name == target:
                return sym

        for sym in self.sympy_eq.free_symbols:
            if str(sym) == str(target):
                return sym

        raise ValueError(f"Symbol '{target}' not found in equation.")

    @staticmethod
    def _pick_positive_solution(solutions: list) -> Any:
        """Returns the first positive solution, or the first solution."""
        for sol in solutions:
            if hasattr(sol, "is_positive") and sol.is_positive:
                return sol
        return solutions[0]

    def solve_all(
        self, target: str | SymbolicQuantity
    ) -> list[SymbolicExpression]:
        """Returns every symbolic root for the target variable.

        Physical equations often have multiple valid roots (e.g. the two
        times a projectile crosses a given height); this returns all of
        them so the caller can pick the physically meaningful one.
        """
        target_symbol = self._find_target_symbol(target)
        solutions = sp.solve(self.sympy_eq, target_symbol)

        result_unit = CompoundUnit({})
        if target_symbol in self.variable_map:
            result_unit = self.variable_map[target_symbol].unit

        result_vars = set(self.variable_map.values())

        return [
            SymbolicExpression(sol, result_unit, self.system, result_vars)
            for sol in solutions
        ]

    def solve_for(
        self, target: str | SymbolicQuantity
    ) -> SymbolicExpression | None:
        """Solves the equation for a single root of the target variable.

        Selection policy: returns the first root SymPy can prove
        positive, else the first root found. Use :meth:`solve_all` when
        the equation may have several physically meaningful roots.
        """
        solutions = self.solve_all(target)
        if not solutions:
            return None

        chosen_expr = self._pick_positive_solution(
            [sol.expr for sol in solutions]
        )
        for sol in solutions:
            if sol.expr == chosen_expr:
                return sol
        return solutions[0]
