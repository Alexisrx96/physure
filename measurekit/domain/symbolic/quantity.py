"""Provides classes for symbolic representation of quantities and equations.

This module integrates the `sympy` library to allow for symbolic mathematics
with physical quantities. It defines `SymbolicQuantity` to represent a variable
with a unit (e.g., `x` in meters) and `Equation` to represent a dimensionally-
consistent relationship between these symbolic quantities. This enables solving
equations for a variable and automatically deducing the resulting unit of the
solution.
"""

from typing import Union

import sympy
from sympy.core.expr import Expr
from sympy.core.symbol import Symbol

from measurekit import default_system
from measurekit.application.functions.functions import Function
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.exceptions import IncompatibleUnitsError


class SymbolicQuantity:
    """Represents a symbolic quantity with an associated unit."""

    def __init__(
        self,
        symbol_name: str,
        unit: Union[str, CompoundUnit],
        system: UnitSystem = default_system,
    ):
        """Initializes a new symbolic quantity."""
        self.symbol: Expr = sympy.Symbol(symbol_name, positive=True)

        self.system = system
        if isinstance(unit, str):
            self.unit = self.system.get_unit(unit)
        else:
            self.unit = unit

    def __repr__(self) -> str:
        """Return a string representation of the quantity."""
        return f"({self.symbol}) [{self.unit}]"

    def _operate(self, other, op, unit_op):
        if not isinstance(other, SymbolicQuantity):
            new_symbol = op(self.symbol, other)
            return SymbolicQuantity.from_expression(
                new_symbol, self.unit, self.system
            )

        if self.system is not other.system:
            raise ValueError(
                "No se puede operar con SymbolicQuantities de "
                "diferentes sistemas."
            )

        new_symbol = op(self.symbol, other.symbol)
        new_unit = unit_op(self.unit, other.unit)
        return SymbolicQuantity.from_expression(
            new_symbol, new_unit, self.system
        )

    def __neg__(self):
        """Handles unary negation (e.g., -my_symbol)."""
        new_symbol = -self.symbol
        return SymbolicQuantity.from_expression(
            new_symbol, self.unit, self.system
        )

    def __mul__(self, other):
        """Handles cases like my_symbol * other_symbol."""
        return self._operate(other, lambda s, o: s * o, lambda u1, u2: u1 * u2)

    def __rmul__(self, other):
        """Handles cases like other_symbol * my_symbol."""
        return self.__mul__(other)

    def __truediv__(self, other):
        """Handles cases like my_symbol / other_symbol."""
        return self._operate(other, lambda s, o: s / o, lambda u1, u2: u1 / u2)

    def __rtruediv__(self, other):
        """Handles cases like 1 / my_symbol."""
        new_symbol = other / self.symbol
        new_unit = 1 / self.unit
        return SymbolicQuantity.from_expression(
            new_symbol, new_unit, self.system
        )

    def __pow__(self, power: float):
        """Handles cases like my_symbol ** power."""
        new_symbol = self.symbol**power
        new_unit = self.unit**power
        return SymbolicQuantity.from_expression(
            new_symbol, new_unit, self.system
        )

    def __add__(self, other: "SymbolicQuantity"):
        """Addition is only valid if the units are the same."""
        if not isinstance(other, SymbolicQuantity):
            raise TypeError("Solo se puede sumar otro SymbolicQuantity.")
        if self.unit.dimension(self.system) != other.unit.dimension(
            self.system
        ):
            raise IncompatibleUnitsError(self.unit, other.unit)
        new_symbol = self.symbol + other.symbol  # type: ignore
        return SymbolicQuantity.from_expression(
            new_symbol, self.unit, self.system
        )

    def __sub__(self, other: "SymbolicQuantity"):
        """Subtraction is only valid if the units are the same."""
        if not isinstance(other, SymbolicQuantity):
            raise TypeError("Solo se puede restar otro SymbolicQuantity.")
        if self.unit.dimension(self.system) != other.unit.dimension(
            self.system
        ):
            raise IncompatibleUnitsError(self.unit, other.unit)
        new_symbol = self.symbol - other.symbol  # type: ignore
        return SymbolicQuantity.from_expression(
            new_symbol, self.unit, self.system
        )

    @classmethod
    def from_expression(
        cls, symbol_expr: Expr, unit: CompoundUnit, system: UnitSystem
    ):
        """Creates a SymbolicQuantity from a sympy expression and a unit."""
        instance = cls.__new__(cls)
        instance.symbol = symbol_expr
        instance.unit = unit
        instance.system = system
        return instance

    def to_function(self, *args: "SymbolicQuantity") -> "Function":
        """Converts the symbolic expression into a unit-aware Function object.

        This method gathers the symbolic expression, parameter dimensions, and
        output dimension to construct a full Function object from the
        measurekit.functions module.

        Args:
            *args (SymbolicQuantity): The symbolic variables that will be the
                                     arguments to the final function, in order.

        Returns:
            A Function object that is callable, inspectable, and can be
            differentiated.
        """
        # 1. Gather the parameters and their dimensions for the Function
        # constructor
        params = {
            arg.symbol.name: arg.unit.dimension(self.system)  # type: ignore
            for arg in args
        }

        # 2. Get the output dimension from this symbolic quantity's unit
        output_dim = self.unit.dimension(self.system)

        # 3. Create and return the Function object
        return Function(
            parameters=params,
            output_dimension=output_dim,
            symbolic_func=self.symbol,
            system=self.system,
        )


class Equation:
    """Represents an equation between symbolic quantities."""

    def __init__(
        self,
        lhs: SymbolicQuantity,
        rhs: SymbolicQuantity,
        variables: list[SymbolicQuantity],
    ):
        """Initializes a new Equation instance."""
        if lhs.unit.dimension(lhs.system) != rhs.unit.dimension(rhs.system):
            raise IncompatibleUnitsError(lhs.unit, rhs.unit)
        self.equation = sympy.Eq(lhs.symbol, rhs.symbol)
        self.lhs = lhs
        self.rhs = rhs
        self.system = lhs.system
        self.variable_map: dict[Symbol, SymbolicQuantity] = {  # type: ignore
            var.symbol: var for var in variables
        }

    def __repr__(self) -> str:
        """Returns a string representation of the equation."""
        return str(self.equation)

    def solve_for(self, symbol_to_solve: Union[str, SymbolicQuantity]):
        """Solve the equation for the given symbol."""
        # Busca el objeto de símbolo correcto en lugar de crear uno nuevo.
        target_symbol_obj = None
        if isinstance(symbol_to_solve, SymbolicQuantity):
            target_symbol_obj = symbol_to_solve.symbol
        else:  # Si es un string como "a"
            for s in self.variable_map:
                if s.name == symbol_to_solve:
                    target_symbol_obj = s
                    break

        if target_symbol_obj is None:
            raise ValueError(
                f"Símbolo '{symbol_to_solve}' no encontrado en las variables "
                "de la ecuación."
            )

        solutions = sympy.solve(self.equation, target_symbol_obj)
        if not solutions:
            return None

        positive_solutions = [s for s in solutions if s.is_positive]
        solution_expr = (
            positive_solutions[0] if positive_solutions else solutions[0]
        )

        solution_unit = self._deduce_unit(solution_expr)
        simplified_unit = solution_unit.simplify(self.system)

        return SymbolicQuantity.from_expression(
            solution_expr, simplified_unit, self.system
        )

    def _deduce_unit(self, expr: sympy.Basic) -> CompoundUnit:
        if isinstance(expr, sympy.Symbol):
            return self.variable_map.get(expr, CompoundUnit({})).unit  # type: ignore
        if expr.is_Number:
            return CompoundUnit({})

        op = expr.func
        args = expr.args

        if issubclass(op, sympy.Mul):
            unit = CompoundUnit({})
            for arg in args:
                unit *= self._deduce_unit(arg)
            return unit
        if issubclass(op, sympy.Pow):
            base, exponent = args
            return self._deduce_unit(base) ** float(exponent)  # type: ignore
        if issubclass(op, sympy.Add):
            return self._deduce_unit(args[0])

        raise TypeError(
            "Operación simbólica no soportada para "
            f"deducción de unidades: {op}"
        )
