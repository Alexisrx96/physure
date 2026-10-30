# physure/application/factories.py
"""This module provides the primary user-facing API for creating quantities.

It defines the `Q_` object, a versatile factory that allows for the easy
creation of `Quantity` instances in a variety of ways.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, TypeVar, overload

from physure.application.context import get_active_system
from physure.domain.measurement.quantity import (
    Quantity,
)
from physure.domain.measurement.quantity_text import parse_quantity_text

if TYPE_CHECKING:
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit

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
        # ponytail: _U is an unbound TypeVar; ty can't prove the literal
        # 0.0 default satisfies an arbitrary _U, but this overload body
        # never executes (only the untyped implementation below does).
        uncertainty: _U = 0.0,  # ty: ignore[invalid-parameter-default]
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
            temp_unit = system.resolve_unit(from_unit)
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
        # ponytail: same unbound-TypeVar-default limitation as above.
        uncertainty: _U = 0.0,  # ty: ignore[invalid-parameter-default]
        symbol: str | None = None,
    ) -> Quantity[_V, _U, Any]: ...

    @overload
    def __call__(
        self,
        value: _V,
        unit: _UT,
        # ponytail: same unbound-TypeVar-default limitation as above.
        uncertainty: _U = 0.0,  # ty: ignore[invalid-parameter-default]
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
                # ponytail: `unit` is Any here because this __call__ impl
                # backs generic overloads (unit: _UT) used for literal-string
                # unit narrowing by the mypy plugin; genuinely dynamic.
                target_unit = system.resolve_unit(unit)  # pyright: ignore[reportAny]
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
            value, unit = parse_quantity_text(value, system)

        # Handle string unit resolution
        # ponytail: `unit` is Any here because this __call__ impl backs
        # generic overloads (unit: _UT) used for literal-string unit
        # narrowing by the mypy plugin; genuinely dynamic.
        unit = (
            system.get_unit("dimensionless")
            if unit is None
            else system.resolve_unit(unit)  # pyright: ignore[reportAny]
        )

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
        default_unit = system.resolve_unit(unit_expression)
        if default_unit in self._cache:
            return self._cache[default_unit]
        factory = SpecializedQuantityFactory(default_unit, system)
        self._cache[default_unit] = factory
        return factory
