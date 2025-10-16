"""Defines the `CompoundUnit` class for representing complex physical units.

This module provides the `CompoundUnit` class, which represents any physical
unit as a combination of base units raised to various powers (e.g., meters per
second squared as `m¹·s⁻²`). It supports arithmetic operations (multiplication,
division, exponentiation) and is responsible for calculating conversion factors
and determining the physical dimension of a unit within a given `UnitSystem`.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, ClassVar, cast, overload

import numpy as np
import sympy as sp

from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.notation.base_entity import BaseExponentEntity
from measurekit.domain.notation.typing import ExponentsDict
from measurekit.exceptions import IncompatibleUnitsError

if TYPE_CHECKING:
    from numpy.typing import NDArray

    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.system import UnitSystem


@dataclass(frozen=True)
class CompoundUnit(BaseExponentEntity):
    """Represents a unit composed of base units raised to various powers.

    This class is immutable and uses a caching mechanism to ensure that
    identical units are represented by the same object instance. It provides
    methods for arithmetic operations, unit conversions, and dimensional
    analysis within a given unit system.

    Attributes:
    exponents (ExponentsDict): A dictionary mapping base unit symbols to their
    floating-point exponents.
    """

    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}

    def __new__(cls, exponents: ExponentsDict):
        """Create or retrieve a cached CompoundUnit instance."""
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0.0))
        if key in cls._cache:
            return cls._cache[key]
        instance = super().__new__(cls, exponents)
        cls._cache[key] = cast(CompoundUnit, instance)
        return cast(CompoundUnit, instance)

    def __init__(self, exponents: ExponentsDict) -> None:
        """Initializes the compound unit with a dictionary of exponents."""
        pass

    def __hash__(self) -> int:
        """Returns a hash value for the compound unit."""
        return super().__hash__()

    # --- System-Dependent Methods ---
    def conversion_factor_to(self, target: CompoundUnit) -> float:
        """Calculate the conversion factor to a target unit within a system.

        Args:
        system (UnitSystem): The unit system providing conversion definitions.
        target (CompoundUnit): The unit to convert to.

        Returns:
        float: The numerical factor to multiply by to convert to the target
        unit.

        Raises:
        IncompatibleUnitsError: If the units have incompatible dimensions.
        """
        from measurekit.application.context import get_active_system

        system = get_active_system()
        if self.dimension(system) != target.dimension(system):
            raise IncompatibleUnitsError(self, target)
        source_factor = self._compound_factor(system)
        target_factor = target._compound_factor(system)
        return source_factor / target_factor

    def _compound_factor(self, system: UnitSystem) -> float:
        """Calculate the unit's total conversion factor relative to SI units.

        This is a helper method used for conversions.

        Args:
        system (UnitSystem): The unit system providing conversion definitions.

        Returns:
        float: The unit's conversion factor.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
        factor = 1.0
        for unit, exp in self.exponents.items():
            _unit = system.get_unit(unit)
            dim = _unit.dimension(system)

            if dim is None:
                raise ValueError(
                    f"Unit '{unit}' not found in system for conversion."
                )
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if unit_def is None:
                raise ValueError(f"Unit definition for '{unit}' not found.")
            factor *= unit_def.factor_to_base**exp
        return factor

    def dimension(self, system: UnitSystem) -> Dimension:
        """Determine the physical dimension of the unit within a system.

        Args:
        system (UnitSystem): The unit system that defines the dimensions of
        base units.

        Returns:
        Dimension: The resulting physical dimension of the compound unit.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
        overall = Dimension({})
        for unit, exp in self.exponents.items():
            if unit in system.UNIT_DIMENSIONS:
                overall *= system.UNIT_DIMENSIONS[unit] ** exp
            else:
                raise ValueError(
                    f"Unknown dimension for unit '{unit}'"
                    " in the provided system."
                )
        return overall

    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...

    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    def __rmul__(self, other: Any) -> Any:
        """Handle right-side multiplication, typically for creating a Quantity.

        This allows for intuitive syntax like 5 * meter.

        Args:
        other (Any): The scalar or array to be multiplied with the unit.

        Returns:
        Any: A new Quantity instance, or NotImplemented if the operation is
        not supported.
        """
        from measurekit.application.context import get_active_system
        from measurekit.domain.measurement.quantity import Quantity

        if isinstance(other, (float, int, np.ndarray)):
            return Quantity.from_input(
                value=other, unit=self, system=get_active_system()
            )
        return NotImplemented

    def to_string(
        self,
        system: UnitSystem | None = None,
        use_alias: bool = False,
        alias_preference: str | None = None,
    ) -> str:
        """Generate a human-readable string representation of the unit.

        Args:
        system (UnitSystem | None, optional): The system to check for aliases.
        use_alias (bool, optional): If True, uses a registered alias if one
        exists. Defaults to False.
        alias_preference (str | None, optional): A preferred alias to use if
        multiple exist. Defaults to None.

        Returns:
        str: The string representation of the unit.
        """
        if use_alias and system:
            key = tuple(
                sorted((k, v) for k, v in self.exponents.items() if v != 0)
            )
            aliases = system.ALIASES.get(key, [])
            if aliases:
                if alias_preference and alias_preference in aliases:
                    return alias_preference
                return aliases[0]

        return super().__str__()

    def __format__(self, format_spec: str) -> str:
        """Format the CompoundUnit using a format specification.

        This method is now primarily for internal use by Quantity.__format__.
        """
        # This method is simple now; the complex logic is in to_string
        return self.to_string(use_alias=format_spec.startswith("alias"))

    def to_latex(self) -> str:
        r"""Generate a LaTeX representation of the unit for display.

        This method uses SymPy to produce a properly formatted LaTeX string,
        handling fractions and exponents correctly.

        Examples:
        - m/s becomes \frac{m}{s}
        - kg*m/s^2 becomes \frac{kg \cdot m}{s^{2}}

        Returns:
        str: The LaTeX formatted string.
        """
        if not self.exponents:
            return ""

        symbols = {name: sp.Symbol(name) for name in self.exponents}

        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
            expr *= symbols[unit_name] ** exponent

        return sp.latex(expr, mul_symbol="dot")

    def _repr_latex_(self):
        """Provide a LaTeX representation for automatic rendering in Jupyter.

        Returns:
        str: The LaTeX string wrapped in '$' for display.
        """
        return f"${self.to_latex()}$"

    def is_dimensionless(self) -> bool:
        """Check if the unit is dimensionless (i.e., has no components).

        Returns:
        bool: True if the unit is dimensionless, False otherwise.
        """
        return not self.exponents

    def simplify(self, system: UnitSystem) -> CompoundUnit:
        """Simplifies the unit by expanding derived units into base components.

        This method uses the unit "recipes" defined in the given system to
        recursively substitute derived units (like 'N' or 'J') until only
        base units remain. The exponents are then consolidated.

        Args:
            system (UnitSystem): The system containing the unit recipes.

        Returns:
            A new, simplified CompoundUnit instance.
        """
        new_exponents: dict[str, float] = defaultdict(float)

        for unit_symbol, exponent in self.exponents.items():
            if unit_symbol in system._UNIT_RECIPES:
                recipe_unit = system._UNIT_RECIPES[unit_symbol]
                simplified_recipe = recipe_unit.simplify(system)

                for base_unit, base_exp in simplified_recipe.exponents.items():
                    new_exponents[base_unit] += base_exp * exponent
            else:
                new_exponents[unit_symbol] += exponent

        return CompoundUnit(new_exponents)
