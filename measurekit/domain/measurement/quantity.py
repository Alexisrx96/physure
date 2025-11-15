"""Defines the `Quantity` class, the representation of a physical quantity.

This module contains the `Quantity` class, which bundles a numerical value
(magnitude), a `CompoundUnit`, and an optional `Uncertainty`. It is the central
object that users interact with. The class overloads arithmetic, comparison,
and other operators to provide intuitive, unit-aware calculations, automatic
error propagation, and seamless integration with NumPy for handling array
values.
"""

from __future__ import annotations

import operator
from dataclasses import dataclass, field
from fractions import Fraction
from typing import (
    TYPE_CHECKING,
    Any,
    ClassVar,
    Generic,
    Literal,
    Self,
    TypeVar,
    cast,
    overload,
)

import numpy as np
import sympy as sp
from numpy.typing import NDArray

from measurekit.application.context import get_active_system
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.uncertainty import Uncertainty
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.exceptions import IncompatibleUnitsError

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType", float, int, NDArray[Any])
UncType = TypeVar("UncType", float, NDArray[Any])
Numeric = int | float | NDArray[Any]
ScalarValue = TypeVar("ScalarValue", int, float)
ArrayValue = TypeVar("ArrayValue", bound=NDArray[Any])
ScalarUnc = TypeVar("ScalarUnc", bound=float)
ArrayUnc = TypeVar("ArrayUnc", bound=NDArray[Any])
ScalarValueSelf = TypeVar("ScalarValueSelf", int, float)
ScalarValueOther = TypeVar("ScalarValueOther", int, float)
OtherValueType = TypeVar("OtherValueType", float, int, NDArray[Any])
OtherUncType = TypeVar("OtherUncType", float, NDArray[Any])


@dataclass(frozen=True, slots=True)
class Quantity(Generic[ValueType, UncType]):
    """Represents a physical quantity with magnitude, unit, and uncertainty."""

    magnitude: ValueType
    unit: CompoundUnit
    uncertainty_obj: Uncertainty[UncType] = field(
        default_factory=lambda: cast(Uncertainty[UncType], Uncertainty(0.0))
    )
    fraction: Fraction | None = None
    system: UnitSystem = field(default_factory=get_active_system)
    dimension: Dimension = field(init=False)

    _cache: ClassVar[dict[CompoundUnit, type]] = {}

    def __post_init__(self):
        """Calculates derived fields after the object is initialized."""
        calculated_dimension = self.unit.dimension(self.system)
        object.__setattr__(self, "dimension", calculated_dimension)

    @overload
    @classmethod
    def from_input(
        cls,
        value: ScalarValue,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: float = 0.0,
    ) -> Quantity[ScalarValue, float]: ...

    @overload
    @classmethod
    def from_input(
        cls,
        value: ArrayValue,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: ArrayUnc | float = 0.0,
    ) -> Quantity[ArrayValue, ArrayUnc]: ...

    @overload
    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
    ) -> Quantity[Any, Any]: ...

    @classmethod
    def from_input(  # type: ignore
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
    ) -> Self:
        """Creates a Quantity from raw input values."""
        from measurekit.application.context import get_active_system

        resolved_system = system if system is not None else get_active_system()

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
        return cls(
            magnitude=cast(ValueType, value),
            unit=unit,
            uncertainty_obj=cast(Uncertainty[UncType], uncertainty_obj),
            fraction=frac,
            system=resolved_system,
        )

    @property
    def uncertainty(self) -> UncType:
        """Returns the standard deviation of the uncertainty."""
        return self.uncertainty_obj.std_dev

    def to(
        self, target_unit: CompoundUnit | str
    ) -> Quantity[ValueType, UncType]:
        """Converts the quantity to a different unit."""
        if isinstance(target_unit, str):
            target_unit = self.system.get_unit(target_unit)
        if self.dimension != target_unit.dimension(self.system):
            raise IncompatibleUnitsError(self.unit, target_unit)
        conversion_factor = self.unit.conversion_factor_to(target_unit)
        new_value = self.magnitude * conversion_factor
        new_uncertainty = self.uncertainty * conversion_factor
        return Quantity.from_input(
            new_value, target_unit, self.system, uncertainty=new_uncertainty
        )

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity:
        """Handles cases like my_quantity + other."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
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
        """Handles cases like my_quantity - other."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
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
        """Handles cases like my_quantity * other."""
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
        """Handles cases like my_quantity / other."""
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
        """Handles cases like my_quantity ** power."""
        new_value = self.magnitude**exponent
        new_unit = self.unit**exponent
        calc_value = np.asarray(self.magnitude, dtype=float)
        new_uncertainty_obj = self.uncertainty_obj.power(
            exponent, cast(UncType, calc_value)
        )
        return Quantity.from_input(
            new_value, new_unit, self.system, uncertainty=new_uncertainty_obj
        )

    __radd__ = __add__
    __rmul__ = __mul__

    def __rtruediv__(self, other: Any) -> Quantity:
        """Handles right-side division, typically for creating a Quantity."""
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
        """Returns a new Quantity with negated magnitude."""
        return cast(
            Self,
            Quantity.from_input(
                -self.magnitude,
                self.unit,
                self.system,
                uncertainty=self.uncertainty_obj,
            ),
        )

    def __pos__(self) -> Self:
        """Returns the Quantity itself."""
        return self

    def __abs__(self) -> Self:
        """Returns the absolute value of the Quantity."""
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
        """Converts the Quantity to a float."""
        return float(self.magnitude)

    # --- NumPy Integration ---
    def __array_ufunc__(
        self,
        ufunc: np.ufunc,
        method: Literal[
            "__call__", "reduce", "reduceat", "accumulate", "outer", "at"
        ],
        *inputs: Any,
        **kwargs: Any,
    ) -> Any:
        """Handles NumPy ufuncs applied to Quantity instances."""
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
            if ufunc in {np.sin, np.cos, np.tan}:
                if not q_input.unit.dimension(
                    q_input.system
                ).is_dimensionless():
                    raise IncompatibleUnitsError(
                        q_input.unit, CompoundUnit({})
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

    # --- Vector and Other Methods ---
    def dot(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[float, float]:
        """Dot product."""
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
        """Cross product."""
        if not isinstance(other, Quantity):
            return NotImplemented
        result_value = np.cross(self.magnitude, other.magnitude)
        result_unit = self.unit * other.unit
        return Quantity.from_input(
            result_value, result_unit, self.system, uncertainty=0.0
        )

    def __len__(self):
        """Returns the length if the magnitude is an array."""
        if isinstance(self.magnitude, np.ndarray):
            return len(self.magnitude)
        raise TypeError(
            f"Object of type '{type(self).__name__}' has no len()."
        )

    def __getitem__(self, key):
        """Supports indexing and slicing if the magnitude is an array."""
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
        """Rounds the magnitude to a specified number of digits."""
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

    # --- Comparison Dunder Methods ---
    def __eq__(self, other: object) -> bool:
        """Equality comparison."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        return cast(
            bool, np.all(np.isclose(self.magnitude, other_converted.magnitude))
        )

    def __lt__(self, other: Any) -> bool:
        """Less than."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        return self.magnitude < other_converted.magnitude

    def __le__(self, other: Any) -> bool:
        """Less than or equal to."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        return self.magnitude <= other_converted.magnitude

    def __gt__(self, other: Any) -> bool:
        """Greater than."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        return self.magnitude > other_converted.magnitude

    def __ge__(self, other: Any) -> bool:
        """Greater than or equal to."""
        if not isinstance(other, Quantity):
            return NotImplemented
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        return self.magnitude >= other_converted.magnitude

    # --- Formatting Methods ---
    def __format__(self, format_spec: str) -> str:
        """Format the quantity as a string."""
        recognized_unit_formats = {"alias", "full"}
        if "|" in format_spec:
            numeric_format, unit_format = format_spec.split("|", 1)
        else:
            is_unit_format = (
                format_spec in recognized_unit_formats
                or format_spec.startswith("alias:")
                or format_spec.startswith("full:")
            )
            if is_unit_format:
                numeric_format, unit_format = "", format_spec
            else:
                numeric_format, unit_format = format_spec, "full"

        # --- Unit Formatting ---
        use_alias = unit_format.startswith("alias")
        alias_pref = (
            unit_format.split(":", 1)[1] if ":" in unit_format else None
        )
        unit_str = self.unit.to_string(
            system=self.system,
            use_alias=use_alias,
            alias_preference=alias_pref,
        )

        # --- Numeric and Uncertainty Formatting ---
        has_unc = np.any(np.asarray(self.uncertainty) > 0)

        if numeric_format == "frac" and self.fraction is not None:
            numeric_str = str(self.fraction)
            # Uncertainty not used with fractions
            return f"{numeric_str} {unit_str}"

        def format_val(val, fmt_spec):
            if isinstance(val, np.ndarray):
                return np.array2string(
                    val,
                    formatter={"float_kind": lambda x: format(x, fmt_spec)},
                )
            try:
                return format(float(val), fmt_spec)
            except (ValueError, TypeError):
                return str(val)

        if has_unc:
            mag_str = format_val(self.magnitude, numeric_format)
            unc_str = format_val(self.uncertainty, numeric_format)
            return f"({mag_str} ± {unc_str}) {unit_str}"
        else:
            numeric_str = format_val(self.magnitude, numeric_format)
            return f"{numeric_str} {unit_str}"

    def to_latex(self):
        """Return a LaTeX representation of the quantity."""
        value_latex = sp.latex(self.magnitude)
        unit_latex = self.unit.to_latex()
        if np.any(np.asarray(self.uncertainty) > 0):
            unc_latex = sp.latex(self.uncertainty)
            return f"({value_latex} \\pm {unc_latex}) \\; {unit_latex}"
        return f"{value_latex} \\; {unit_latex}"

    def __str__(self):
        """Return a string representation of the quantity."""
        is_array_unc = isinstance(self.uncertainty, np.ndarray)
        has_unc = is_array_unc or self.uncertainty > 0
        if not has_unc:
            return (
                f"{self.magnitude} "
                f"{self.unit.to_string(self.system, use_alias=True)}"
            )
        if is_array_unc:
            return (
                f"Quantity(value={self.magnitude}, "
                f"unit={self.unit.to_string(self.system, use_alias=True)}, "
                "uncertainty=[...])"
            )
        return (
            f"({self.magnitude} ± {self.uncertainty}) "
            f"{self.unit.to_string(self.system, use_alias=True)}"
        )

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
