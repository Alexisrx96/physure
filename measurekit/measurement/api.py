# measurekit/api.py (Versión Definitiva y Simplificada)

from typing import overload

from measurekit.measurement.quantity import Quantity, UncType, ValueType
from measurekit.measurement.units import CompoundUnit, get_unit


class _SpecializedQuantityFactory:
    """Una fábrica callable diseñada para crear Quantities con una unidad
    predefinida.
    """

    __slots__ = ("_default_unit",)

    def __init__(self, default_unit: CompoundUnit):
        self._default_unit = default_unit

    @overload
    def __call__(  # type: ignore
        self,
        value: ValueType
        | Quantity = 1,  # <-- El valor por defecto se incluye aquí
        from_unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity[ValueType, UncType]: ...

    def __call__(
        self,
        value: ValueType | Quantity = 1,
        from_unit: str | CompoundUnit | None = None,
        uncertainty: UncType = 0.0,
    ) -> Quantity:
        if isinstance(value, Quantity):
            if from_unit is not None:
                raise ValueError(
                    "No se puede proporcionar una unidad cuando "
                    "el valor ya es una Quantity."
                )
            return value.to(self._default_unit)

        if from_unit is None:
            return Quantity.from_input(
                value=value, unit=self._default_unit, uncertainty=uncertainty
            )

        if isinstance(from_unit, str):
            provided_unit = get_unit(from_unit)
        elif isinstance(from_unit, CompoundUnit):
            provided_unit = from_unit
        else:
            raise TypeError(
                f"Se esperaba str o CompoundUnit, se obtuvo {type(from_unit)}"
            )

        temp_quantity = Quantity.from_input(
            value=value, unit=provided_unit, uncertainty=uncertainty
        )

        return temp_quantity.to(self._default_unit)

    def __repr__(self) -> str:
        return f"<Quantity Factory for unit='{self._default_unit}'>"


class _QuantityFactory:
    """La fachada principal de la librería."""

    _cache: dict[CompoundUnit, _SpecializedQuantityFactory] = {}

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
    ) -> Quantity[ValueType, UncType]:
        if unit is None:
            raise ValueError(
                "La unidad debe ser especificada para el constructor genérico."
            )
        if isinstance(unit, str):
            unit = get_unit(unit)
        return Quantity.from_input(
            value=value, unit=unit, uncertainty=uncertainty
        )

    def __getitem__(
        self, unit_expression: str | CompoundUnit
    ) -> _SpecializedQuantityFactory:
        if isinstance(unit_expression, str):
            default_unit = get_unit(unit_expression)
        elif isinstance(unit_expression, CompoundUnit):
            default_unit = unit_expression
        else:
            raise TypeError(
                f"Se esperaba str o CompoundUnit, se obtuvo {type(unit_expression)}"
            )
        if default_unit in self._cache:
            return self._cache[default_unit]
        factory = _SpecializedQuantityFactory(default_unit)
        self._cache[default_unit] = factory
        return factory


Q_ = _QuantityFactory()
__all__ = ["Q_"]
