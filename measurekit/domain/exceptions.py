# measurekit/exceptions.py
"""This module defines custom exception types for the measurekit library.

It provides a clear hierarchy of exceptions, making it easier to catch
and handle specific errors that may occur during unit conversions,
calculations, or definitions.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from measurekit.domain.measurement.units import CompoundUnit


class MeasureKitError(Exception):
    """Base exception for all MeasureKit errors."""

    line: int | None = None
    column: int | None = None

    def __str__(self) -> str:
        msg = super().__str__()
        if self.line is not None and not msg.startswith("line "):
            if self.column is not None:
                return f"line {self.line}, column {self.column}: {msg}"
            return f"line {self.line}: {msg}"
        return msg


class IncompatibleUnitsError(MeasureKitError):
    """Raised when trying to perform an operation with incompatible units."""

    def __init__(self, unit1: CompoundUnit, unit2: CompoundUnit):
        """Initialize the exception with the incompatible units.

        :param unit1: The first unit involved in the operation.
        :param unit2: The second unit involved in the operation.
        """
        self.unit1 = unit1
        self.unit2 = unit2
        super().__init__(
            f"Cannot operate with incompatible units: '{unit1}' and '{unit2}'."
        )


class ConversionError(MeasureKitError):
    """Raised when a unit conversion is not possible."""

    def __init__(self, from_unit: CompoundUnit, to_unit: CompoundUnit):
        """Initialize the exception with the units involved in the conversion.

        :param from_unit: The unit from which we are trying to convert.
        :param to_unit: The unit to which we are trying to convert.
        """
        self.from_unit = from_unit
        self.to_unit = to_unit
        super().__init__(
            f"No conversion path from '{from_unit}' to '{to_unit}'."
        )


class UnitNotFoundError(MeasureKitError, KeyError):
    """Raised when a unit is not found in the registry."""

    def __init__(self, unit_name: str):
        """Initialize the exception with the missing unit name.

        :param unit_name: The name of the unit that was not found.
        """
        self.unit_name = unit_name
        super().__init__(f"Unit '{unit_name}' not found or registered.")


class UnknownUnitError(MeasureKitError, ValueError):
    """Raised when a unit string has no registered dimension."""

    def __init__(self, unit_name: str, suggestions: list[str] | None = None):
        """Initialize exception with unknown unit name and suggestions.

        :param unit_name: The name of the unit that was not recognized.
        :param suggestions: Optional list of suggested unit names.
        """
        self.unit_name = unit_name
        self.suggestions = suggestions
        hint = (
            f" Did you mean: {', '.join(suggestions)}?" if suggestions else ""
        )
        super().__init__(f"Unknown unit '{unit_name}'.{hint}")


class DimensionError(MeasureKitError):
    """Raised when an invalid operation is performed on dimensions."""

    pass
