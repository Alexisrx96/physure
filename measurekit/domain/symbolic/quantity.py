"""Provides classes for atomic symbolic quantities and equations."""

from typing import Union

import sympy as sp

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
        unit: Union[str, CompoundUnit],
        system: UnitSystem = default_system,
    ):
        symbol = sp.Symbol(name, positive=True)
        if isinstance(unit, str):
            resolved_unit = system.get_unit(unit)
        else:
            resolved_unit = unit

        super().__init__(symbol, resolved_unit, system, variables=None)
        self.variables = {self}

    @property
    def symbol(self) -> sp.Symbol:
        return self.expr

    def __repr__(self) -> str:
        return f"Symbol({self.expr}) [{self.unit}]"

    @classmethod
    def from_expression(cls, *args, **kwargs):
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
        if lhs.dimension != rhs.dimension:
            raise IncompatibleUnitsError(lhs.unit, rhs.unit)

        self.equation = sp.Eq(lhs.expr, rhs.expr)
        self.lhs = lhs
        self.rhs = rhs
        self.system = lhs.system

        if variables:
            self.variable_map = {v.expr: v for v in variables}
        else:
            all_vars = lhs.variables | rhs.variables
            self.variable_map = {v.expr: v for v in all_vars}

    def __repr__(self) -> str:
        return str(self.equation)

    # --- NEW: Jupyter Pretty Printing ---
    def _repr_latex_(self):
        """Returns the LaTeX representation for Jupyter rendering."""
        from sympy import latex

        return f"${latex(self.equation)}$"

    def solve_for(
        self, target: str | SymbolicQuantity
    ) -> SymbolicExpression | None:
        target_symbol = None

        if isinstance(target, SymbolicQuantity):
            target_symbol = target.expr
        else:
            for sym in self.variable_map:
                if sym.name == target:
                    target_symbol = sym
                    break

        if target_symbol is None:
            for sym in self.equation.free_symbols:
                if str(sym) == str(target):
                    target_symbol = sym
                    break
            if target_symbol is None:
                raise ValueError(f"Symbol '{target}' not found in equation.")

        solutions = sp.solve(self.equation, target_symbol)
        if not solutions:
            return None

        chosen_sol = solutions[0]
        for sol in solutions:
            if hasattr(sol, "is_positive") and sol.is_positive:
                chosen_sol = sol
                break

        result_unit = CompoundUnit({})
        if target_symbol in self.variable_map:
            result_unit = self.variable_map[target_symbol].unit

        result_vars = set(self.variable_map.values())

        return SymbolicExpression(
            chosen_sol, result_unit, self.system, result_vars
        )
