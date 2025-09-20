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
    parameters: dict[str, Dimension]
    output_dimension: Dimension
    symbolic_func: sp.Expr
    system: UnitSystem = field(default=default_system, repr=False)
    arg_names: tuple[str, ...] = field(init=False, repr=False)
    numeric_func: Callable[..., np.ndarray] = field(init=False, repr=False)

    def __post_init__(self):
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
        if output_unit.dimension(self.system) != self.output_dimension:
            raise ValueError(
                f"The output unit '{output_unit}' has an incorrect dimension. "
                f"Expected: {self.output_dimension}, "
                f"Received: {output_unit.dimension(self.system)}"
            )

        numeric_args = []
        # Ensure kwargs are processed in the correct order for the numeric function
        for name in self.arg_names:
            if name not in kwargs:
                # This handles the case where a derivative results in a constant (e.g., `v`)
                # but the user still provides the original arguments (x0, v, t).
                # We simply ignore the ones that are no longer in the symbolic expression.
                continue
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
        respect_to_sym = sp.Symbol(respect_to)

        # THIS IS THE FIX: The check must be on the original parameters, not the symbols.
        if respect_to not in self.parameters:
            raise ValueError(
                f"Cannot differentiate with respect to '{respect_to}' because its dimension is unknown."
            )

        derivative_expr = sp.diff(self.symbolic_func, respect_to_sym)

        # The new dimension is always the original output dimension divided by the
        # dimension of the variable we are differentiating with respect to.
        new_output_dim = self.output_dimension / self.parameters[respect_to]

        # The new function still understands all original parameters, even if
        # some have been eliminated from the symbolic expression.
        return Function(
            parameters=self.parameters,  # Keep all original parameter dimensions
            output_dimension=new_output_dim,
            symbolic_func=derivative_expr,
            system=self.system,
        )

    def __repr__(self) -> str:
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

        Examples:
        >>> f = Function(sp.sin(x), params={"x": Dimension("length")})
        >>> str(f)
        "sin(x) with parameters {x: length}"
        """
        param_str = ", ".join(
            f"{name}: {dim.analytical_representation}"
            for name, dim in self.parameters.items()
        )
        return f"'{self.symbolic_func}' with parameters {{{param_str}}}"
