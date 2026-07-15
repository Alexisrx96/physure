# physure/measurement/conversions.py
"""This module defines the data structure for a unit's definition.

It contains the `UnitDefinition` class, which serves as a stateless
container for the properties of a single unit. This class is fundamental
to the unit system, providing the core information needed for conversions
and dimensional analysis.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar

if TYPE_CHECKING:
    from physure.domain.measurement.converters import UnitConverter
    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.units import CompoundUnit


class UnitDefinition:
    """A stateless data class representing the definition of a single unit.

    An instance of this class holds the properties of a unit, such as its
    symbol, dimension, and conversion factor to the system's base unit.
    """

    _instances: ClassVar[dict[str, UnitDefinition]] = {}
    symbol: str
    dimension: Dimension
    converter: UnitConverter
    name: str | None
    recipe: CompoundUnit | None
    allow_prefixes: bool

    def __new__(
        cls,
        symbol: str,
        dimension: Dimension,
        converter: UnitConverter,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
        kind: str = "delta",
    ):
        """Ensures that each unit symbol corresponds to a single instance."""
        key = symbol
        if key in cls._instances:
            # Update properties if the unit is being redefined.
            instance = cls._instances[key]
            instance.dimension = dimension
            instance.converter = converter
            instance.name = name
            instance.recipe = recipe
            instance.allow_prefixes = allow_prefixes
            instance.kind = kind
            return instance

        instance = super().__new__(cls)
        cls._instances[key] = instance
        return instance

    def __init__(
        self,
        symbol: str,
        dimension: Dimension,
        converter: UnitConverter,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
        kind: str = "delta",
    ):
        """Initializes the attributes of the instance."""
        self.symbol = symbol
        self.dimension = dimension
        self.converter = converter
        self.name = name
        self.recipe = recipe
        self.allow_prefixes = allow_prefixes
        self.kind = kind

    def __getnewargs__(self):
        """Arguments for __new__ during unpickling."""
        return (
            self.symbol,
            self.dimension,
            self.converter,
            self.name,
            self.recipe,
            self.allow_prefixes,
            self.kind,
        )

    @property
    def factor_to_base(self) -> float:
        """Backward compatibility helper returning linear scale."""
        from physure.domain.measurement.converters import (
            LinearConverter,
            OffsetConverter,
        )

        if isinstance(self.converter, (LinearConverter, OffsetConverter)):
            return self.converter.scale
        return 1.0

    def __str__(self) -> str:
        """Provides a simple string representation of the unit definition."""
        return (
            f"UnitDefinition({self.symbol}, {self.dimension}, "
            f"{self.converter})"
        )

    def __repr__(self) -> str:
        """Provides a detailed representation of the unit definition."""
        return (
            f"UnitDefinition({self.symbol}, {self.dimension}, "
            f"{self.converter}, {self.name})"
        )
