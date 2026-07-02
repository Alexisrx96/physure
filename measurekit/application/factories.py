# measurekit/application/factories.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Any, TypeVar, overload

from measurekit.application.context import get_active_system
from measurekit.domain.measurement.quantity import (
    Quantity,
)

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem
    from measurekit.domain.measurement.units import CompoundUnit

# Matches only the leading numeric literal (optional sign, int/float, optional
# exponent); the unit is whatever text follows and is sliced off in Python.
# No trailing `.*$` wildcard and an unambiguous digit pattern
# (`\d+\.?\d*|\.\d+`, not `\d*\.?\d+`), so there is nothing to force the engine
# to re-partition a long digit run — immune to ReDoS backtracking.
_NUMBER_PREFIX_REGEX = re.compile(
    r"\s*([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)"
)


_V = TypeVar("_V")
_U = TypeVar("_U")
_UT = TypeVar("_UT")


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
        value: Quantity[_V, _U, Any],
        from_unit: str | CompoundUnit | None = None,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
    ) -> Quantity[_V, _U, Any]: ...

    @overload
    def __call__(
        self,
        value: _V,
        from_unit: str | CompoundUnit | None = None,
        uncertainty: _U = 0.0,
        symbol: str | None = None,
    ) -> Quantity[_V, _U, Any]: ...

    def __call__(
        self,
        value: Any = 1,
        from_unit: str | CompoundUnit | None = None,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
    ) -> Quantity[Any, Any, Any]:
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
            temp_q = Quantity.from_input(
                value, temp_unit, system, uncertainty, symbol
            )
            return temp_q.to(self._default_unit)

        return Quantity.from_input(
            value=value,
            unit=self._default_unit,
            system=system,
            uncertainty=uncertainty,
            symbol=symbol,
        )

    def __repr__(self) -> str:
        """Returns a string representation of the factory."""
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class QuantityFactory:
    """The main facade for creating quantities within a specific UnitSystem."""

    __slots__ = ("_cache", "_system")

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
        value: _V,
        unit: CompoundUnit,
        uncertainty: _U = 0.0,
        symbol: str | None = None,
    ) -> Quantity[_V, _U, Any]: ...

    @overload
    def __call__(
        self,
        value: _V,
        unit: _UT,
        uncertainty: _U = 0.0,
        symbol: str | None = None,
    ) -> Quantity[_V, _U, _UT]: ...

    @overload
    def __call__(
        self,
        value: str,
    ) -> Quantity[float, float, Any]: ...

    def __call__(
        self,
        value: Any = 1,
        unit: Any = None,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
    ) -> Quantity[Any, Any, Any]:
        """Creates a Quantity, parsing strings if necessary."""
        system = (
            self._system if self._system is not None else get_active_system()
        )

        if isinstance(value, Quantity):
            # If a unit is provided, convert to that unit first (in its own
            # system)
            if unit is not None:
                # Resolve unit in the target system preferably
                target_unit = (
                    system.get_unit(unit) if isinstance(unit, str) else unit
                )
                value = value.to(target_unit)

            # Move to the target system if different
            if value.system != system:
                # We need to ensure the unit is valid in the new system
                # and the conversion factor is preserved.
                # For now, we assume dimension equality is enough and we keep
                # magnitude.
                # A more robust way: converted_val = value.to(unit_in_new_system)

                # Check if unit exists in target system and has same scale
                # Actually, simplified: just transfer magnitude and uncertainty
                # if units are compatible.
                return value.with_system(system)
            return value

        # Handle string parsing if value is a string and no unit is provided
        if isinstance(value, str) and unit is None:
            value, unit = self._parse_string_value(value, system)

        # Handle string unit resolution
        if unit is None:
            unit = system.get_unit("dimensionless")
        elif isinstance(unit, str):
            unit = system.get_unit(unit)

        return Quantity.from_input(
            value=value,
            unit=unit,
            system=system,
            uncertainty=uncertainty,
            symbol=symbol,
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
    ) -> tuple[Any, CompoundUnit]:
        """Parses a string like '10 m/s' into a value and a unit."""
        match = _NUMBER_PREFIX_REGEX.match(value_str)
        if not match:
            return value_str, system.get_unit("dimensionless")

        num_str = match.group(1)
        unit_str = value_str[match.end() :].strip()
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
        except ValueError:
            return value_str, system.get_unit("dimensionless")

        unit = (
            system.get_unit(unit_str)
            if unit_str
            else system.get_unit("dimensionless")
        )
        return parsed_value, unit
