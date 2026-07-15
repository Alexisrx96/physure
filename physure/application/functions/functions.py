"""Unit-aware mathematical functions."""

from __future__ import annotations

from dataclasses import dataclass, field
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

if TYPE_CHECKING:
    from collections.abc import Callable

    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit


@dataclass(frozen=True)
class Function:
    """Represents a unit-aware mathematical function."""

    parameters: dict[str, CompoundUnit]  # FIX: Store Units, not Dimensions
    output_unit: CompoundUnit  # FIX: Store Output Unit
    symbolic_func: sp.Expr
    system: UnitSystem = field(default=default_system, repr=False)
    backend: str = field(default="numpy", repr=False)
    arg_names: tuple[str, ...] = field(init=False, repr=False)
    numeric_func: Callable[..., Any] = field(init=False, repr=False)

    def __post_init__(self):
        """Initializes the numeric version of the function."""
        # Sort symbols by name to ensure consistent argument order
        sorted_symbols = sorted(
            self.symbolic_func.free_symbols, key=lambda s: str(s.name)
        )

        # Store argument names
        object.__setattr__(
            self,
            "arg_names",
            tuple(str(s.name) for s in sorted_symbols),
        )

        # Compile numeric function
        # Sympy supports 'numpy', 'tensorflow', 'jax', 'math', etc.
        # We need to ensure we map our backend names to sympy's if they differ
        # 'torch' requires explicit module usually for older sympy, but modern sympy supports it?
        # Sympy 1.12+ supports 'torch' often via 'numpy' or mapped modules.
        # Let's pass it through.
        callable_func = self._compile_symengine(sorted_symbols)

        if callable_func is None:
            callable_func = sp.lambdify(
                sorted_symbols, self.symbolic_func, self.backend
            )
        object.__setattr__(self, "numeric_func", callable_func)

    def _compile_symengine(self, sorted_symbols) -> Callable[..., Any] | None:
        """Helper to compile numeric function using SymEngine if available and supported."""
        if not (
            HAVE_SYMENGINE
            and self.backend in ("numpy", "math")
            and sorted_symbols
        ):
            return None
        try:
            import numpy as np

            se_symbols = [se.Symbol(s.name) for s in sorted_symbols]
            se_expr = se.sympify(self.symbolic_func)
            se_func = se.lambdify(se_symbols, [se_expr])

            def symengine_wrapped_func(*args):
                is_any_array = any(isinstance(a, np.ndarray) for a in args)
                if is_any_array:
                    broadcasted = np.broadcast_arrays(*args)
                    stacked = np.stack(broadcasted, axis=-1)
                    return se_func(stacked)
                else:
                    res = se_func(args)
                    return res.item() if res.ndim == 0 else res

            return symengine_wrapped_func
        except Exception:
            return None

    def __call__(
        self, output_unit: CompoundUnit | str, **kwargs: Quantity
    ) -> Quantity:
        """Evaluates the function with the given quantity arguments."""
        output_unit = self.system.resolve_unit(output_unit)

        # Verify output dimension consistency
        if output_unit.dimension(self.system) != self.output_unit.dimension(
            self.system
        ):
            raise ValueError(
                f"Output unit '{output_unit}' has incorrect dimension. "
                f"Expected: {self.output_unit.dimension(self.system)}"
            )

        # Check for missing arguments
        required_args = set(self.arg_names)
        provided_args = set(kwargs.keys())
        if not required_args.issubset(provided_args):
            raise TypeError(
                f"Missing required arguments: {required_args - provided_args}"
            )

        # --- FIX: Convert inputs to expected units before calculation ---
        numeric_args = []
        for name in self.arg_names:
            quantity = kwargs[name]
            target_unit = self.parameters[name]

            # This handles unit conversion (e.g. cm -> m) automatically
            # and raises IncompatibleUnitsError if dimensions don't match
            converted_val = quantity.to(target_unit).magnitude
            numeric_args.append(converted_val)

        # Calculate result (magnitude in terms of self.output_unit)
        result_value = self.numeric_func(*numeric_args)

        # Wrap result in the derivation's output unit,
        # then convert to user's requested unit
        return self.system.Q_(result_value, self.output_unit).to(output_unit)

    def derivative(self, respect_to: str) -> Function:
        """Computes the symbolic derivative of the function."""
        if respect_to not in self.parameters:
            raise ValueError(f"Unknown parameter '{respect_to}'")

        # --- FIX: Find the specific symbol instance in the expression ---
        respect_to_sym = None
        for sym in self.symbolic_func.free_symbols:
            if sym.name == respect_to:
                respect_to_sym = sym
                break

        # If symbol isn't found (variable cancelled out), we use a dummy
        if respect_to_sym is None:
            respect_to_sym = sp.Symbol(respect_to)

        derivative_expr = sp.diff(self.symbolic_func, respect_to_sym)

        # Calculate new output unit: Output / Parameter
        new_output_unit = self.output_unit / self.parameters[respect_to]

        return Function(
            parameters=self.parameters,
            output_unit=new_output_unit,
            symbolic_func=derivative_expr,
            system=self.system,
        )

    def __repr__(self) -> str:
        """Returns a string representation of the function."""
        return f"Function({self.symbolic_func}) -> [{self.output_unit}]"

    def __str__(self) -> str:
        """Returns the string form of the symbolic expression."""
        return f"'{self.symbolic_func}'"
