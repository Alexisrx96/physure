# measurekit/measurement/api.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways, including direct
instantiation and subscripting for specialized, unit-specific factories.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, overload

from measurekit.context import get_active_system
from measurekit.measurement.quantity import Quantity, UncType, ValueType
from measurekit.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.system import UnitSystem


class SpecializedQuantityFactory:
    """A callable factory for creating Quantities with a predefined unit."""

    __slots__ = ("_default_unit", "_system")

    def __init__(
        self, default_unit: CompoundUnit, system: UnitSystem | None = None
    ):
        """Initializes the factory with a default unit and optional system.

        Args:
            default_unit (CompoundUnit): The unit that will be used by default
                when creating quantities.
            system (UnitSystem | None): The unit system to use. If None, the
                active system from context will be used.
        """
        self._default_unit = default_unit
        self._system = system

    @overload
    def __call__(  # type: ignore
        self,
        value: ValueType | Quantity = 1,
        from_unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity[ValueType, UncType]: ...

    def __call__(
        self,
        value: ValueType | Quantity = 1,
        from_unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity:
        """Creates a Quantity in the factory's default unit."""
        system = (
            self._system if self._system is not None else get_active_system()
        )
        if from_unit:
            temp_unit = (
                system.get_unit(from_unit)
                if isinstance(from_unit, str)
                else from_unit
            )
            temp_q = Quantity.from_input(value, temp_unit, system, uncertainty)
            return temp_q.to(self._default_unit)

        return Quantity.from_input(
            value=value,
            unit=self._default_unit,
            system=system,
            uncertainty=uncertainty,
        )

    def __repr__(self) -> str:
        """Detailed representation for debugging."""
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class QuantityFactory:
    """The main facade for creating quantities within a specific UnitSystem."""

    __slots__ = ("_system", "_cache")

    def __init__(self, system: UnitSystem | None = None):
        """Initializes a new QuantityFactory instance."""
        self._system = system
        self._cache: dict[CompoundUnit, SpecializedQuantityFactory] = {}

    @overload
    def __call__(  # type: ignore
        self,
        value: ValueType,
        unit: str | CompoundUnit,
        uncertainty: UncType = 0.0,
    ) -> Quantity[ValueType, UncType]: ...

    def __call__(
        self,
        value: ValueType = 1,
        unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity:
        """Creates a Quantity with the specified value and unit."""
        system = (
            self._system if self._system is not None else get_active_system()
        )
        if unit is None:
            unit = system.get_unit("dimensionless")
        elif isinstance(unit, str):
            unit = system.get_unit(unit)

        return Quantity.from_input(
            value=value,
            unit=unit,
            system=system,
            uncertainty=uncertainty,
        )

    def __getitem__(
        self, unit_expression: str | CompoundUnit
    ) -> SpecializedQuantityFactory:
        """Returns a specialized Quantity factory for a specific unit."""
        system = (
            self._system if self._system is not None else get_active_system()
        )
        default_unit = (
            system.get_unit(unit_expression)
            if isinstance(unit_expression, str)
            else unit_expression
        )
        if default_unit in self._cache:
            return self._cache[default_unit]
        factory = SpecializedQuantityFactory(default_unit, system)
        self._cache[default_unit] = factory
        return factory
