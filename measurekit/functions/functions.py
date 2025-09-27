"""Unit-aware mathematical functions.

It leverages the sympy library for symbolic mathematics, allowing for
the creation of functions that understand physical dimensions. This enables
operations like differentiation while ensuring dimensional consistency.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Callable

import numpy as np
import sympy as sp

from measurekit import default_system
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.quantity import Quantity
from measurekit.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.system import UnitSystem


@dataclass(frozen=True)
class Function:
    """Represents a unit-aware mathematical function.

    This class binds a symbolic expression from sympy with the physical
    dimensions of its parameters and output, allowing for safe and
    dimensionally-aware calculations.
    """

    parameters: dict[str, Dimension]
    output_dimension: Dimension
    symbolic_func: sp.Expr
    system: UnitSystem = field(default=default_system, repr=False)
    arg_names: tuple[str, ...] = field(init=False, repr=False)
    numeric_func: Callable[..., np.ndarray] = field(init=False, repr=False)

    def __post_init__(self):
        """Initializes the numeric version of the function after the dataclass.

        This method is called after the dataclass __init__ method has finished
        executing. It populates the numeric_func attribute with a callable
        that can be used to evaluate the function with Quantity arguments.
        """
        arg_symbols = tuple(self.symbolic_func.free_symbols)
        sorted_symbols = sorted(arg_symbols, key=lambda s: str(s.name))  # type: ignore
        object.__setattr__(
            self,
            "arg_names",
            tuple(str(s.name) for s in sorted_symbols),  # type: ignore
        )
        callable_func = sp.lambdify(
            sorted_symbols, self.symbolic_func, "numpy"
        )
        object.__setattr__(self, "numeric_func", callable_func)

    def __call__(
        self, output_unit: CompoundUnit, **kwargs: Quantity
    ) -> Quantity:
        """Evaluates the function with the given quantity arguments."""
        if output_unit.dimension(self.system) != self.output_dimension:
            raise ValueError(
                f"The output unit '{output_unit}' has an incorrect dimension. "
                f"Expected: {self.output_dimension}, "
                f"Received: {output_unit.dimension(self.system)}"
            )

        # --- FIX: Stricter argument checking ---
        required_args = set(self.arg_names)
        provided_args = set(kwargs.keys())

        if required_args != provided_args:
            missing = required_args - provided_args
            extra = provided_args - required_args
            msg = ""
            if missing:
                msg += f"Missing required arguments: {missing}. "
            if extra:
                msg += f"Got unexpected arguments: {extra}."
            raise TypeError(msg)

        # Build the list of numeric arguments in the correct, sorted order
        numeric_args = []
        for name in self.arg_names:
            quantity = kwargs[name]
            expected_dim = self.parameters[name]
            if quantity.dimension != expected_dim:
                raise ValueError(
                    f"Argument '{name}' has an incorrect dimension. "
                    f"Expected: {expected_dim}, Received: {quantity.dimension}"
                )
            numeric_args.append(quantity.magnitude)

        result_value = self.numeric_func(*numeric_args)
        return self.system.Q_(result_value, output_unit)

    def derivative(self, respect_to: str) -> Function:
        """Computes the symbolic derivative of the function.

        :param respect_to: The name of the parameter to differentiate against.
        :return: A new Function object representing the derivative.
        """
        respect_to_sym = sp.Symbol(respect_to)

        if respect_to not in self.parameters:
            raise ValueError(
                f"Cannot differentiate with respect to '{respect_to}' "
                "because its dimension is unknown."
            )

        derivative_expr = sp.diff(self.symbolic_func, respect_to_sym)
        new_output_dim = self.output_dimension / self.parameters[respect_to]

        return Function(
            parameters=self.parameters,
            output_dimension=new_output_dim,
            symbolic_func=derivative_expr,
            system=self.system,
        )

    def __repr__(self) -> str:
        """Provides a detailed developer representation of the function."""
        param_str = ", ".join(
            f"{name}: {dim.analytical_representation}"
            for name, dim in self.parameters.items()
        )
        return (
            f"Function({self.symbolic_func}, "
            f"params={{ {param_str} }}, "
            f"output_dim={self.output_dimension.analytical_representation})"
        )

    def __str__(self) -> str:
        """Human-readable representation of the function.

        Returns a string showing the function's symbolic expression and
        the names and dimensions of its parameters.
        """
        param_str = ", ".join(
            f"{name}: {dim.analytical_representation}"
            for name, dim in self.parameters.items()
        )
        return f"'{self.symbolic_func}' with parameters {{{param_str}}}"
