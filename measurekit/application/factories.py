# measurekit/application/factories.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, overload

from measurekit.application.context import get_active_system
from measurekit.domain.measurement.quantity import Quantity, UncType, ValueType
from measurekit.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# Regex to separate magnitude (int/float) from unit string
# Matches: start, optional sign, digits, optional dot, digits,
#   optional exponent, space, unit string
_STRING_PARSE_REGEX = re.compile(
    r"^\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*(.*)$"
)


class SpecializedQuantityFactory:
    """A callable factory for creating Quantities with a predefined unit."""

    __slots__ = ("_default_unit", "_system")

    def __init__(
        self, default_unit: CompoundUnit, system: UnitSystem | None = None
    ):
        """Initializes a SpecializedQuantityFactory.

        Args:
            default_unit: The unit to be applied to quantities created by
                this factory.
            system: The UnitSystem context to use.
        """
        self._default_unit = default_unit
        self._system = system

    @overload
    def __call__(
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
        """Creates a Quantity with the factory's default unit."""
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
        """Returns a string representation of the factory."""
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class QuantityFactory:
    """The main facade for creating quantities within a specific UnitSystem."""

    __slots__ = ("_system", "_cache")

    def __init__(self, system: UnitSystem | None = None):
        """Initializes a QuantityFactory.

        Args:
            system: The optional UnitSystem to associate with this factory.
        """
        self._system = system
        self._cache: dict[CompoundUnit, SpecializedQuantityFactory] = {}

    @overload
    def __call__(
        self,
        value: ValueType,
        unit: str | CompoundUnit,
        uncertainty: UncType = 0.0,
    ) -> Quantity[ValueType, UncType]: ...

    @overload
    def __call__(
        self,
        value: str,
    ) -> Quantity[float, float]: ...

    def __call__(
        self,
        value: ValueType | str = 1,
        unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity:
        """Creates a Quantity, parsing strings if necessary."""
        system = (
            self._system if self._system is not None else get_active_system()
        )

        # Handle string input like "10 m/s"
        if isinstance(value, str) and unit is None:
            value, unit = self._parse_string_value(value, system)

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

    def _parse_string_value(
        self, value_str: str, system: UnitSystem
    ) -> tuple[ValueType, CompoundUnit]:
        """Parses a string like '10 m/s' into a value and a unit."""
        match = _STRING_PARSE_REGEX.match(value_str)
        if not match:
            return value_str, system.get_unit("dimensionless")

        num_str, unit_str = match.groups()
        try:
            parsed_value = float(num_str)
            # If the number was an integer (e.g., "10"), convert back to int
            # for cleanness
            if (
                parsed_value.is_integer()
                and "." not in num_str
                and "e" not in num_str.lower()
            ):
                parsed_value = int(parsed_value)

            unit = (
                system.get_unit(unit_str.strip())
                if unit_str
                else system.get_unit("dimensionless")
            )
            return parsed_value, unit
        except ValueError:
            return value_str, system.get_unit("dimensionless")
