"""Provides the core Quantity class for the MeasureKit library.

This module defines the Quantity class, which is the fundamental object for
representing physical quantities. It encapsulates a numerical value
(magnitude), a unit of measurement, and an associated uncertainty, enabling
dimensionally-aware arithmetic, unit conversions, and uncertainty propagation.
"""

from __future__ import annotations

import operator
from dataclasses import dataclass
from fractions import Fraction
from typing import (
    TYPE_CHECKING,
    Any,
    ClassVar,
    Generic,
    Self,
    TypeVar,
    cast,
)

import numpy as np
import sympy as sp
from numpy.typing import NDArray

from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.uncertainty import Uncertainty
from measurekit.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.system import UnitSystem

# Define generic types for values and uncertainties
ValueType = TypeVar("ValueType", float, int, NDArray[Any])
UncType = TypeVar("UncType", float, NDArray[Any])
Numeric = int | float | NDArray[Any]


@dataclass(frozen=True)
class Quantity(Generic[ValueType, UncType]):
    """Represents a physical quantity with magnitude, unit, and uncertainty.

    This class is the cornerstone of the MeasureKit library, providing a robust
    and flexible way to handle physical quantities. It supports arithmetic
    operations, unit conversions, and uncertainty propagation.

    Attributes:
    magnitude (ValueType): The numerical value of the quantity.
    unit (CompoundUnit): The unit of measurement for the quantity.
    uncertainty_obj (Uncertainty[UncType]): The uncertainty associated
    with the magnitude.
    fraction (Fraction | None): The fractional representation of the
    magnitude, if applicable.
    dimension (Dimension): The physical dimension of the quantity.
    system (UnitSystem): The unit system to which the quantity belongs.
    """

    magnitude: ValueType
    unit: CompoundUnit
    uncertainty_obj: Uncertainty[UncType]
    fraction: Fraction | None
    dimension: Dimension
    system: UnitSystem

    __slots__ = (
        "magnitude",
        "unit",
        "uncertainty_obj",
        "fraction",
        "dimension",
        "system",
    )

    _cache: ClassVar[dict[CompoundUnit, type]] = {}

    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
    ) -> Self:
        """Create a new Quantity instance from raw input values.

        Args:
        value (Any): The numerical value of the quantity.
        unit (CompoundUnit): The unit of measurement.
        system (UnitSystem): The unit system for the quantity.
        uncertainty (Any, optional): The uncertainty of the measurement.
        Defaults to 0.0.

        Returns:
        Self: A new instance of the Quantity class.
        """
        if isinstance(value, np.ndarray):
            value.flags.writeable = False

        uncertainty_obj = (
            uncertainty
            if isinstance(uncertainty, Uncertainty)
            else Uncertainty(uncertainty)
        )

        if isinstance(uncertainty_obj.std_dev, np.ndarray):
            uncertainty_obj.std_dev.flags.writeable = False

        frac = Fraction(str(value)) if np.isscalar(value) else None
        dim = unit.dimension(system)

        return cls(
            magnitude=cast(ValueType, value),
            unit=unit,
            uncertainty_obj=cast(Uncertainty[UncType], uncertainty_obj),
            fraction=frac,
            dimension=dim,
            system=system,
        )

    # ... The rest of the file remains the same as the complete version
    # you had before the error. All logic for arithmetic, comparisons,
    # and representation will now work correctly.
    @property
    def uncertainty(self) -> UncType:
        """The uncertainty of the quantity as a standard deviation.

        Returns:
        UncType: The standard deviation of the uncertainty.
        """
        return self.uncertainty_obj.std_dev

    def to(
        self, target_unit: CompoundUnit | str
    ) -> Quantity[ValueType, UncType]:
        """Convert the quantity to a different unit.

        Args:
        target_unit (CompoundUnit | str): The target unit to convert to.

        Returns:
        Quantity[ValueType, UncType]: A new Quantity instance with the
        converted value and unit.
        """
        if isinstance(target_unit, str):
            target_unit = self.system.get_unit(target_unit)

        if self.dimension != target_unit.dimension(self.system):
            raise ValueError("Cannot convert between incompatible dimensions.")

        conversion_factor = self.unit.conversion_factor_to(
            self.system, target_unit
        )
        new_value = self.magnitude * conversion_factor
        new_uncertainty = self.uncertainty * conversion_factor

        return Quantity.from_input(
            new_value, target_unit, self.system, uncertainty=new_uncertainty
        )

    # --- Full Arithmetic Implementations ---

    def __add__(self, other: Any) -> Quantity:
        """Add two quantities.

        Args:
        other (Any): The quantity to add.

        Returns:
        Quantity: The result of the addition.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError("Cannot add quantities with different dimensions")
        other_converted = other.to(self.unit)
        new_magnitude = self.magnitude + other_converted.magnitude
        new_uncertainty_obj = self.uncertainty_obj.add(
            other_converted.uncertainty_obj
        )
        return Quantity.from_input(
            new_magnitude,
            self.unit,
            self.system,
            uncertainty=new_uncertainty_obj,
        )

    def __sub__(self, other: Any) -> Quantity:
        """Subtract two quantities.

        Args:
        other (Any): The quantity to subtract.

        Returns:
        Quantity: The result of the subtraction.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot subtract quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        new_magnitude = self.magnitude - other_converted.magnitude
        new_uncertainty_obj = self.uncertainty_obj.add(
            other_converted.uncertainty_obj
        )
        return Quantity.from_input(
            new_magnitude,
            self.unit,
            self.system,
            uncertainty=new_uncertainty_obj,
        )

    def __mul__(self, other: Any) -> Quantity:
        """Multiply two quantities.

        Args:
        other (Any): The quantity to multiply by.

        Returns:
        Quantity: The result of the multiplication.
        """
        if isinstance(other, (int, float, np.ndarray)):
            new_magnitude = self.magnitude * other
            new_uncertainty = self.uncertainty_obj.std_dev * np.abs(other)
            return Quantity.from_input(
                new_magnitude,
                self.unit,
                self.system,
                uncertainty=new_uncertainty,
            )

        if isinstance(other, Quantity):
            new_magnitude = self.magnitude * other.magnitude
            new_unit = self.unit * other.unit
            new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_magnitude,
            )
            return Quantity.from_input(
                new_magnitude,
                new_unit,
                self.system,
                uncertainty=new_uncertainty_obj,
            )

        # Add support for multiplying a Quantity by a CompoundUnit
        if isinstance(other, CompoundUnit):
            new_unit = self.unit * other
            return Quantity.from_input(
                value=self.magnitude,
                unit=new_unit,
                system=self.system,
                uncertainty=self.uncertainty_obj,
            )

        return NotImplemented

    def __truediv__(self, other: Any) -> Quantity:
        """Divide two quantities.

        Args:
        other (Any): The quantity to divide by.

        Returns:
        Quantity: The result of the division.
        """
        if isinstance(other, (int, float, np.ndarray)):
            new_magnitude = self.magnitude / other
            new_uncertainty = self.uncertainty_obj.std_dev / np.abs(other)
            return Quantity.from_input(
                new_magnitude,
                self.unit,
                self.system,
                uncertainty=new_uncertainty,
            )

        if isinstance(other, Quantity):
            new_magnitude = self.magnitude / other.magnitude
            new_unit = self.unit / other.unit
            new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_magnitude,
            )
            return Quantity.from_input(
                new_magnitude,
                new_unit,
                self.system,
                uncertainty=new_uncertainty_obj,
            )

        # Add support for dividing a Quantity by a CompoundUnit
        if isinstance(other, CompoundUnit):
            new_unit = self.unit / other
            return Quantity.from_input(
                value=self.magnitude,
                unit=new_unit,
                system=self.system,
                uncertainty=self.uncertainty_obj,
            )

        return NotImplemented

    def __pow__(self, exponent: float) -> Quantity:
        """Raise the quantity to a power.

        Args:
        exponent (float): The exponent to raise the quantity to.

        Returns:
        Quantity: The result of the exponentiation.
        """
        new_value = self.magnitude**exponent
        new_unit = self.unit**exponent
        calc_value = np.asarray(self.magnitude, dtype=float)
        new_uncertainty_obj = self.uncertainty_obj.power(
            exponent, cast(UncType, calc_value)
        )
        return Quantity.from_input(
            new_value, new_unit, self.system, uncertainty=new_uncertainty_obj
        )

    # --- Reverse and Other Dunder Methods ---

    __radd__ = __add__
    __rmul__ = __mul__

    def __rsub__(self, other: Any) -> Quantity:
        """Reverse subtraction.

        Args:
        other (Any): The quantity to be subtracted from.

        Returns:
        Quantity: The result of the reverse subtraction.
        """
        return self.__neg__().__add__(other)

    def __rtruediv__(self, other: Any) -> Quantity:
        """Reverse division.

        Args:
        other (Any): The quantity to be divided.

        Returns:
        Quantity: The result of the reverse division.
        """
        if np.any(np.asarray(self.magnitude) == 0):
            raise ZeroDivisionError(
                "Division by a Quantity with zero magnitude."
            )
        new_magnitude = other / self.magnitude
        new_unit = 1 / self.unit
        other_uncertainty = Uncertainty(0.0)
        new_uncertainty_obj = other_uncertainty.propagate_mul_div(
            self.uncertainty_obj, other, self.magnitude, new_magnitude
        )
        return Quantity.from_input(
            new_magnitude,
            new_unit,
            self.system,
            uncertainty=new_uncertainty_obj,
        )

    def __neg__(self) -> Self:
        """Negate the quantity.

        Returns:
        Self: The negated quantity.
        """
        return cast(Self, -1 * self)

    def __pos__(self) -> Self:
        """Return the quantity as is.

        Returns:
        Self: The quantity.
        """
        return cast(Self, +1 * self)

    def __abs__(self) -> Self:
        """Return the absolute value of the quantity.

        Returns:
        Self: The absolute value of the quantity.
        """
        return cast(
            Self,
            Quantity.from_input(
                abs(self.magnitude),
                self.unit,
                self.system,
                self.uncertainty_obj,
            ),
        )

    def __float__(self) -> float:
        """Convert the quantity to a float.

        Returns:
        float: The magnitude of the quantity as a float.
        """
        return float(self.magnitude)

    # --- NumPy Integration ---

    def __array_ufunc__(self, ufunc, method, *inputs, **kwargs):
        """Handle NumPy universal functions."""
        q_input = next(
            (inp for inp in inputs if isinstance(inp, Quantity)), None
        )
        if q_input is None:
            return NotImplemented

        if method == "reduce":
            result_magnitude = ufunc.reduce(q_input.magnitude, **kwargs)
            return Quantity.from_input(
                result_magnitude, q_input.unit, q_input.system
            )

        if method == "__call__":
            numeric_inputs = [
                inp.magnitude if isinstance(inp, Quantity) else inp
                for inp in inputs
            ]
            result_magnitude = ufunc(*numeric_inputs, **kwargs)

            # Special cases for units and uncertainty
            if ufunc == np.absolute:
                return Quantity.from_input(
                    result_magnitude,
                    q_input.unit,
                    q_input.system,
                    uncertainty=q_input.uncertainty,
                )

            if ufunc == np.sqrt:
                result_unit = q_input.unit**0.5
                rel_unc = (
                    (q_input.uncertainty / q_input.magnitude)
                    if np.all(q_input.magnitude != 0)
                    else 0
                )
                result_uncertainty = np.abs(result_magnitude * 0.5) * rel_unc
                return Quantity.from_input(
                    result_magnitude,
                    result_unit,
                    q_input.system,
                    uncertainty=result_uncertainty,
                )

            if ufunc == np.square:
                return Quantity.from_input(
                    result_magnitude, q_input.unit**2, q_input.system
                )

            elif ufunc in {np.sin, np.cos, np.tan}:
                # The dimension check now correctly uses the system
                if not q_input.unit.dimension(
                    q_input.system
                ).is_dimensionless():
                    raise ValueError(
                        f"{ufunc.__name__} requires a dimensionless quantity."
                    )

                result_unit = CompoundUnit({})
                if ufunc == np.sin:
                    derivative = np.abs(np.cos(q_input.magnitude))
                elif ufunc == np.cos:
                    derivative = np.abs(-np.sin(q_input.magnitude))
                else:
                    derivative = np.abs(1 / np.cos(q_input.magnitude) ** 2)
                result_uncertainty = derivative * q_input.uncertainty
                return Quantity.from_input(
                    result_magnitude,
                    result_unit,
                    q_input.system,
                    uncertainty=result_uncertainty,
                )

            op_map = {
                np.add: operator.add,
                np.subtract: operator.sub,
                np.multiply: operator.mul,
                np.true_divide: operator.truediv,
            }
            if ufunc in op_map and len(inputs) == 2:
                return op_map[ufunc](inputs[0], inputs[1])

            if q_input.unit.dimension(q_input.system).is_dimensionless():
                return Quantity.from_input(
                    result_magnitude, q_input.unit, q_input.system
                )

        return NotImplemented

    # --- Vector and other methods from your original, now refactored ---
    def dot(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[float, float]:
        """Calculate the dot product of two quantities.

        Args:
        other (Quantity[NDArray[Any], Any]): The other quantity.

        Returns:
        Quantity[float, float]: The dot product of the two quantities.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        result_value = np.dot(self.magnitude, other.magnitude)
        result_unit = self.unit * other.unit
        return cast(
            "Quantity[float, float]",
            Quantity.from_input(
                result_value, result_unit, self.system, uncertainty=0.0
            ),
        )

    def cross(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]:
        """Calculate the cross product of two quantities.

        Args:
        other (Quantity[NDArray[Any], Any]): The other quantity.

        Returns:
        Quantity[NDArray[Any], NDArray[Any]]: The cross product.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        result_value = np.cross(self.magnitude, other.magnitude)
        result_unit = self.unit * other.unit
        return Quantity.from_input(
            result_value, result_unit, self.system, uncertainty=0.0
        )

    def __len__(self):
        """Return the length of the quantity's magnitude."""
        if isinstance(self.magnitude, np.ndarray):
            return len(self.magnitude)
        raise TypeError(
            f"Object of type '{type(self).__name__}' has no len()."
        )

    def __getitem__(self, key):
        """Get an item from the quantity's magnitude."""
        if not isinstance(self.magnitude, np.ndarray):
            raise TypeError(
                f"'{type(self).__name__}' object is not subscriptable."
            )

        sliced_value = self.magnitude[key]
        sliced_uncertainty = (
            self.uncertainty[key]
            if isinstance(self.uncertainty, np.ndarray)
            else self.uncertainty
        )

        return Quantity.from_input(
            sliced_value,
            self.unit,
            self.system,
            uncertainty=sliced_uncertainty,
        )

    def __round__(self, ndigits: int | None = None) -> Self:
        """Round the quantity's magnitude.

        Args:
        ndigits (int | None, optional): The number of digits to round to.
        Defaults to None.

        Returns:
        Self: The rounded quantity.
        """
        new_value = (
            np.round(self.magnitude, ndigits)
            if ndigits is not None
            else np.round(self.magnitude)
        )
        return cast(
            Self,
            Quantity.from_input(
                new_value, self.unit, self.system, self.uncertainty_obj
            ),
        )

    def __eq__(self, other: object) -> bool:
        """Check if two quantities are equal.

        Args:
        other (object): The other quantity.

        Returns:
        bool: True if the quantities are equal, False otherwise.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        return cast(
            bool, np.all(np.isclose(self.magnitude, other_converted.magnitude))
        )

    def __lt__(self, other: Any) -> bool:
        """Check if this quantity is less than another.

        Args:
        other (Any): The other quantity.

        Returns:
        bool: True if this quantity is less than the other, False otherwise.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        return self.magnitude < other_converted.magnitude

    def __le__(self, other: Any) -> bool:
        """Check if this quantity is less than or equal to another.

        Args:
        other (Any): The other quantity.

        Returns:
        bool: True if this quantity is less than or equal to the other,
        False otherwise.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        return self.magnitude <= other_converted.magnitude

    def __gt__(self, other: Any) -> bool:
        """Check if this quantity is greater than another.

        Args:
        other (Any): The other quantity.

        Returns:
        bool: True if this quantity is greater than the other, False
        otherwise.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        return self.magnitude > other_converted.magnitude

    def __ge__(self, other: Any) -> bool:
        """Check if this quantity is greater than or equal to another.

        Args:
        other (Any): The other quantity.

        Returns:
        bool: True if this quantity is greater than or equal to the other,
        False otherwise.
        """
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
            )
        other_converted = other.to(self.unit)
        return self.magnitude >= other_converted.magnitude

    # --- Formateo (__format__) ---
    def __format__(self, format_spec: str) -> str:
        """Format the quantity as a string.

        Args:
        format_spec (str): The format specification.

        Returns:
        str: The formatted string.
        """
        recognized_unit_formats = {"alias", "full"}

        # Check for a composite spec using the delimiter '|'.
        if "|" in format_spec:
            numeric_format, unit_format = format_spec.split("|", 1)
        else:
            # If the provided format spec is one of the recognized unit
            # formats, treat it as a unit spec and default the numeric part.
            if (
                format_spec in recognized_unit_formats
                or format_spec.startswith("alias:")
                or format_spec.startswith("full:")
            ):
                numeric_format = ""
                unit_format = format_spec
            else:
                numeric_format = format_spec
                unit_format = "full"  # Default unit format.

        # Format numeric part.
        if numeric_format == "frac":
            numeric_str = str(self.fraction)
        elif numeric_format:
            # Check if self.magnitude is an array
            if isinstance(self.magnitude, np.ndarray):
                numeric_str = np.array2string(
                    self.magnitude,
                    formatter={
                        "float_kind": lambda x: format(x, numeric_format)
                    },
                )
            else:
                try:
                    numeric_str = format(float(self.magnitude), numeric_format)
                except (ValueError, TypeError):
                    numeric_str = str(self.magnitude)
        else:
            numeric_str = str(self.magnitude)

        # Format unit part.
        unit_str = format(self.unit, unit_format)
        return f"{numeric_str} {unit_str}"

    def to_latex(self):
        """Return a LaTeX representation of the quantity.

        Returns:
        str: The LaTeX representation of the quantity.
        """
        # sympy tiene excelentes capacidades de impresión LaTeX
        value_latex = sp.latex(self.magnitude)
        unit_latex = self.unit.to_latex()

        if self.uncertainty > 0:
            unc_latex = sp.latex(self.uncertainty)
            return f"({value_latex} \\pm {unc_latex}) \\; {unit_latex}"

        return f"{value_latex} \\; {unit_latex}"

    def __str__(self):
        """Return a string representation of the quantity."""
        is_array_unc = isinstance(self.uncertainty, np.ndarray)
        if not is_array_unc and self.uncertainty == 0:
            return f"{self.magnitude} {self.unit:full}"

        if is_array_unc:
            return (
                f"Quantity(value={self.magnitude}, unit={self.unit:full}"
                ", uncertainty=[...])"
            )

        return f"({self.magnitude} ± {self.uncertainty}) {self.unit:full}"

    def _repr_latex_(self):
        """Return a LaTeX representation of the quantity for use in Jupyter."""
        return f"${self.to_latex()}$"

    def __repr__(self) -> str:
        """Return a string representation of the quantity."""
        return (
            f"Quantity({self.magnitude!r}, {self.unit!r}, "
            f"uncertainty={self.uncertainty!r})"
        )


__all__ = ["Quantity"]
