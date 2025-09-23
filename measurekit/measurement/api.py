# measurekit/measurement/api.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways, including direct
instantiation and subscripting for specialized, unit-specific factories.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, overload

from measurekit.measurement.quantity import Quantity, UncType, ValueType
from measurekit.measurement.units import CompoundUnit, get_unit

if TYPE_CHECKING:
    from measurekit.system import UnitSystem


class _SpecializedQuantityFactory:
    """A callable factory for creating Quantities with a predefined default unit."""

    __slots__ = ("_default_unit", "_system")

    def __init__(self, default_unit: CompoundUnit, system: UnitSystem):
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
        from measurekit.measurement.quantity import Quantity

        # Create a temporary quantity if a `from_unit` is provided for conversion
        if from_unit:
            temp_unit = (
                get_unit(from_unit)
                if isinstance(from_unit, str)
                else from_unit
            )
            temp_q = Quantity.from_input(
                value, temp_unit, self._system, uncertainty
            )
            # Convert it to the factory's default unit
            return temp_q.to(self._default_unit)

        return Quantity.from_input(
            value=value,
            unit=self._default_unit,
            system=self._system,
            uncertainty=uncertainty,
        )

    def __repr__(self) -> str:
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class _QuantityFactory:
    """The main facade for the library."""

    _cache: dict[CompoundUnit, _SpecializedQuantityFactory] = {}

    @overload
    def __call__(  # type: ignore
        self,
        value: ValueType,
        unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity[ValueType, UncType]: ...

    def __call__(
        self,
        value: ValueType = 1,
        unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity:
        # Dynamic import to avoid cycles
        from measurekit import default_system
        from measurekit.measurement.quantity import Quantity

        if unit is None:
            unit = default_system.get_unit("dimensionless")

        if isinstance(unit, str):
            # This now correctly uses the default system's get_unit method
            unit = get_unit(unit)

        return Quantity.from_input(
            value=value,
            unit=unit,
            system=default_system,
            uncertainty=uncertainty,
        )

    def __getitem__(
        self, unit_expression: str | CompoundUnit
    ) -> _SpecializedQuantityFactory:
        from measurekit import default_system

        if isinstance(unit_expression, str):
            default_unit = get_unit(unit_expression)
        else:
            default_unit = unit_expression

        if default_unit in self._cache:
            return self._cache[default_unit]

        factory = _SpecializedQuantityFactory(default_unit, default_system)
        self._cache[default_unit] = factory
        return factory


Q_ = _QuantityFactory()
__all__ = ["Q_"]
