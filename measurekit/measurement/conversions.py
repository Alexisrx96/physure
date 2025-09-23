# measurekit/measurement/conversions.py
"""This module defines the data structure for a unit's definition.

It contains the `UnitDefinition` class, which serves as a stateless
container for the properties of a single unit. This class is fundamental
to the unit system, providing the core information needed for conversions
and dimensional analysis.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from measurekit.measurement.dimensions import Dimension
    from measurekit.measurement.units import CompoundUnit


class UnitDefinition:
    """A stateless data class representing the definition of a single unit.

    An instance of this class holds the properties of a unit, such as its
    symbol, dimension, and conversion factor to the system's base unit.
    """

    _instances = {}
    symbol: str
    dimension: Dimension
    factor_to_base: float
    name: str | None
    recipe: CompoundUnit | None
    allow_prefixes: bool

    def __new__(
        cls,
        symbol: str,
        dimension: Dimension,
        factor_to_base: float,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
    ):
        """Ensures that each unit symbol corresponds to a single instance."""
        key = symbol
        if key in cls._instances:
            # Update properties if the unit is being redefined.
            instance = cls._instances[key]
            instance.dimension = dimension
            instance.factor_to_base = factor_to_base
            instance.name = name
            instance.recipe = recipe
            instance.allow_prefixes = allow_prefixes
            return instance

        instance = super().__new__(cls)
        cls._instances[key] = instance
        return instance

    def __init__(
        self,
        symbol: str,
        dimension: Dimension,
        factor_to_base: float,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
    ):
        """Initializes the attributes of the instance."""
        self.symbol = symbol
        self.dimension = dimension
        self.factor_to_base = factor_to_base
        self.name = name
        self.recipe = recipe
        self.allow_prefixes = allow_prefixes

    def __str__(self) -> str:
        """Provides a simple string representation of the unit definition."""
        return f"UnitDefinition({self.symbol}, {self.dimension}, {self.factor_to_base})"

    def __repr__(self) -> str:
        """Provides a detailed developer representation of the unit definition."""
        return (
            f"UnitDefinition({self.symbol}, {self.dimension}, "
            f"{self.factor_to_base}, {self.name})"
        )
