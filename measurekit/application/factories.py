# measurekit/application/factories.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Any, overload

from measurekit.application.context import get_active_system
from measurekit.domain.measurement.quantity import Quantity, UncType, ValueType
from measurekit.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# Regex to separate magnitude (int/float) from unit string
# Matches: start, optional sign, digits, optional dot, digits,
#   optional exponent, space, unit string
_STRING_PARSE_REGEX = re.compile(
    r"^\s*([-+]?[0-9]*\.?[0-9]+(?:[eE][-+]?[0-9]+)?)\s*(.*)$"
)


class SpecializedQuantityFactory:
    """A callable factory for creating Quantities with a predefined unit."""

    __slots__ = ("_default_unit", "_system")

    def __init__(
        self, default_unit: CompoundUnit, system: UnitSystem | None = None
    ):
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
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class QuantityFactory:
    """The main facade for creating quantities within a specific UnitSystem."""

    __slots__ = ("_system", "_cache")

    def __init__(self, system: UnitSystem | None = None):
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
            match = _STRING_PARSE_REGEX.match(value)
            if match:
                num_str, unit_str = match.groups()
                try:
                    # Try parsing as float first
                    parsed_value = float(num_str)
                    # If the number was an integer (e.g., "10"), convert back to int for cleanness
                    if (
                        parsed_value.is_integer()
                        and "." not in num_str
                        and "e" not in num_str.lower()
                    ):
                        parsed_value = int(parsed_value)

                    value = parsed_value
                    unit = (
                        system.get_unit(unit_str.strip())
                        if unit_str
                        else system.get_unit("dimensionless")
                    )
                except ValueError:
                    # If parsing fails, fall through to normal handling (will likely fail later)
                    pass

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
