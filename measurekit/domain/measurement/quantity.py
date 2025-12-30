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
    TypeVar,
    cast,
    overload,
)

import numpy as np
import sympy as sp
from numpy.typing import NDArray
from typing_extensions import Self

from measurekit.application.context import get_active_system
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.uncertainty import Uncertainty
from measurekit.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType", float, int, NDArray[Any], sp.Symbol, sp.Expr)
UncType = TypeVar("UncType", float, NDArray[Any])
Numeric = int | float | NDArray[Any] | sp.Symbol | sp.Expr
ScalarValue = TypeVar("ScalarValue", int, float, sp.Symbol, sp.Expr)
ArrayValue = TypeVar("ArrayValue", bound=NDArray[Any])
ScalarUnc = TypeVar("ScalarUnc", bound=float)
ArrayUnc = TypeVar("ArrayUnc", bound=NDArray[Any])
ScalarValueSelf = TypeVar("ScalarValueSelf", int, float, sp.Symbol, sp.Expr)
ScalarValueOther = TypeVar("ScalarValueOther", int, float, sp.Symbol, sp.Expr)
OtherValueType = TypeVar(
    "OtherValueType", float, int, NDArray[Any], sp.Symbol, sp.Expr
)
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

        # Fast path for same unit
        if target_unit == self.unit:
            return self

        if self.dimension != target_unit.dimension(self.system):
            raise IncompatibleUnitsError(self.unit, target_unit)

        # --- Manejo de Conversiones con Offset (ej: Temperatura) ---
        if (
            len(self.unit.exponents) == 1
            and list(self.unit.exponents.values())[0] == 1
            and len(target_unit.exponents) == 1
            and list(target_unit.exponents.values())[0] == 1
        ):
            source_u = list(self.unit.exponents.keys())[0]
            target_u = list(target_unit.exponents.keys())[0]

            source_def = self.system.get_definition(source_u)
            target_def = self.system.get_definition(target_u)

            if source_def and target_def:
                base_mag = source_def.converter.to_base(self.magnitude)
                new_magnitude = target_def.converter.from_base(base_mag)

                scale_source = source_def.factor_to_base
                scale_target = target_def.factor_to_base
                scale_ratio = scale_source / scale_target
                new_uncertainty = cast(Numeric, self.uncertainty) * scale_ratio

                return cast(
                    Quantity[ValueType, UncType],
                    Quantity.from_input(
                        new_magnitude,
                        target_unit,
                        self.system,
                        uncertainty=new_uncertainty,
                    ),
                )
        # ----------------------------------------------------------

        conversion_factor = self.unit.conversion_factor_to(target_unit)
        new_value = cast(Numeric, self.magnitude) * conversion_factor
        new_uncertainty = cast(Numeric, self.uncertainty) * conversion_factor

        return cast(
            Quantity[ValueType, UncType],
            Quantity.from_input(
                new_value,
                target_unit,
                self.system,
                uncertainty=new_uncertainty,
            ),
        )

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity:
        """Handles cases like my_quantity + other."""
        if not isinstance(other, Quantity):
            return NotImplemented

        # --- FAST PATH ---
        if self.unit is other.unit:
            new_magnitude = self.magnitude + other.magnitude
            new_unc = self.uncertainty_obj + other.uncertainty_obj
            return Quantity(
                new_magnitude,
                self.unit,
                new_unc,
                system=self.system,
            )
        # -----------------

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

        # --- FAST PATH ---
        if self.unit is other.unit:
            new_magnitude = self.magnitude - other.magnitude
            new_unc = self.uncertainty_obj + other.uncertainty_obj
            return Quantity(
                new_magnitude,
                self.unit,
                new_unc,
                system=self.system,
            )
        # -----------------

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
            # Use cast for arithmetic safety
            new_magnitude = cast(Numeric, self.magnitude) * other
            new_uncertainty = cast(
                Numeric, self.uncertainty_obj.std_dev
            ) * np.abs(other)

            # Cast the return to the expected generic type
            return cast(
                Quantity[ValueType, UncType],
                Quantity.from_input(
                    new_magnitude,
                    self.unit,
                    self.system,
                    uncertainty=new_uncertainty,
                ),
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
            # Use cast for arithmetic safety
            new_magnitude = cast(Numeric, self.magnitude) / other
            new_uncertainty = cast(
                Numeric, self.uncertainty_obj.std_dev
            ) / np.abs(other)

            # Cast the return to the expected generic type
            return cast(
                Quantity[ValueType, UncType],
                Quantity.from_input(
                    new_magnitude,
                    self.unit,
                    self.system,
                    uncertainty=new_uncertainty,
                ),
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

            # --- Manejo Adimensional ---
            if new_unit.is_dimensionless:
                # Retornamos un Quantity adimensional
                return Quantity.from_input(
                    new_magnitude,
                    new_unit,
                    self.system,
                    uncertainty=new_uncertainty_obj,
                )
            # ---------------------------

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
        if method != "__call__":
            # For reduce, accumulate, etc.
            magnitudes = [
                i.magnitude if isinstance(i, Quantity) else i for i in inputs
            ]
            result = getattr(ufunc, method)(*magnitudes, **kwargs)
            if isinstance(result, (np.ndarray, float, int)):
                return Quantity.from_input(result, self.unit, self.system)
            return result

        # --- FAST PATH for Binary Operations ---
        if (
            len(inputs) == 2
            and isinstance(inputs[0], Quantity)
            and isinstance(inputs[1], Quantity)
        ):
            if inputs[0].unit is inputs[1].unit:
                if ufunc in (np.add, np.subtract):
                    res_mag = ufunc(
                        inputs[0].magnitude, inputs[1].magnitude, **kwargs
                    )
                    res_unc = (
                        inputs[0].uncertainty_obj + inputs[1].uncertainty_obj
                    )
                    return Quantity(
                        res_mag,
                        inputs[0].unit,
                        res_unc,
                        system=inputs[0].system,
                    )
                if ufunc == np.multiply:
                    res_mag = inputs[0].magnitude * inputs[1].magnitude
                    res_unit = inputs[0].unit * inputs[1].unit
                    res_unc = inputs[0].uncertainty_obj.propagate_mul_div(
                        inputs[1].uncertainty_obj,
                        inputs[0].magnitude,
                        inputs[1].magnitude,
                        res_mag,
                    )
                    return Quantity(
                        res_mag, res_unit, res_unc, system=inputs[0].system
                    )
                if ufunc == np.true_divide:
                    res_mag = inputs[0].magnitude / inputs[1].magnitude
                    res_unit = inputs[0].unit / inputs[1].unit
                    res_unc = inputs[0].uncertainty_obj.propagate_mul_div(
                        inputs[1].uncertainty_obj,
                        inputs[0].magnitude,
                        inputs[1].magnitude,
                        res_mag,
                    )
                    return Quantity(
                        res_mag, res_unit, res_unc, system=inputs[0].system
                    )

        # Delegate to dunder methods if available for standard arithmetic
        op_map = {
            np.add: operator.add,
            np.subtract: operator.sub,
            np.multiply: operator.mul,
            np.true_divide: operator.truediv,
            np.power: operator.pow,
        }
        if ufunc in op_map:
            return op_map[ufunc](*inputs, **kwargs)

        # Handle other ufuncs
        magnitudes = [
            i.magnitude if isinstance(i, Quantity) else i for i in inputs
        ]
        result_magnitude = ufunc(*magnitudes, **kwargs)

        if ufunc in (np.sin, np.cos, np.tan, np.exp, np.log, np.log10):
            # Must be dimensionless
            for i, inp in enumerate(inputs):
                if (
                    isinstance(inp, Quantity)
                    and not inp.unit.dimension(inp.system).is_dimensionless
                ):
                    raise IncompatibleUnitsError(inp.unit, CompoundUnit({}))

            # Uncertainty propagation (simplified)
            if isinstance(inputs[0], Quantity):
                q = inputs[0]
                if ufunc == np.sin:
                    deriv = np.abs(np.cos(q.magnitude))
                elif ufunc == np.cos:
                    deriv = np.abs(-np.sin(q.magnitude))
                elif ufunc == np.exp:
                    deriv = np.abs(np.exp(q.magnitude))
                else:
                    deriv = 1.0  # Fallback

                res_unc = Uncertainty(deriv * q.uncertainty)
                return Quantity.from_input(
                    result_magnitude,
                    CompoundUnit({}),
                    q.system,
                    uncertainty=res_unc,
                )
            return result_magnitude

        if ufunc == np.sqrt:
            q = inputs[0]
            res_unit = q.unit**0.5
            # Simplified uncertainty: rel_unc_res = 0.5 * rel_unc_q
            rel_unc = (
                (q.uncertainty / q.magnitude)
                if np.all(q.magnitude != 0)
                else 0
            )
            res_unc = np.abs(result_magnitude * 0.5) * rel_unc
            return Quantity.from_input(
                result_magnitude, res_unit, q.system, uncertainty=res_unc
            )

        if ufunc in (
            np.absolute,
            np.abs,
            np.fabs,
            np.floor,
            np.ceil,
            np.trunc,
            np.rint,
            np.around,
            np.round,
            np.negative,
            np.positive,
        ):
            q = next((i for i in inputs if isinstance(i, Quantity)), None)
            if q:
                return Quantity.from_input(
                    result_magnitude, q.unit, q.system, q.uncertainty_obj
                )

        if ufunc == np.square:
            q = inputs[0]
            return Quantity.from_input(result_magnitude, q.unit**2, q.system)

        if ufunc in (
            np.less,
            np.less_equal,
            np.greater,
            np.greater_equal,
            np.equal,
            np.not_equal,
        ):
            # Comparison: return plain bool/array
            return result_magnitude

        # Default: if result is numeric, wrap in dimensionless Quantity if input was Quantity
        if isinstance(result_magnitude, (np.ndarray, float, int)):
            q_input = next(
                (i for i in inputs if isinstance(i, Quantity)), None
            )
            if q_input:
                return Quantity.from_input(
                    result_magnitude, CompoundUnit({}), q_input.system
                )

        return result_magnitude

    def __array_function__(
        self, func: Any, types: Any, args: Any, kwargs: Any
    ) -> Any:
        """Supports NEP-18 high-level NumPy functions."""
        if func not in HANDLED_FUNCTIONS:
            # Fallback to default behavior: convert all to magnitude
            new_args = [
                a.magnitude if isinstance(a, Quantity) else a for a in args
            ]
            return func(*new_args, **kwargs)
        return HANDLED_FUNCTIONS[func](*args, **kwargs)

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
        if self.system != other.system or self.dimension != other.dimension:
            return False
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
        recognized_unit_formats = {"alias", "full", "latex"}
        if "|" in format_spec:
            numeric_format, unit_format = format_spec.split("|", 1)
        else:
            is_unit_format = (
                format_spec in recognized_unit_formats
                or format_spec.startswith("alias:")
                or format_spec.startswith("full:")
                or format_spec.startswith("latex:")
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
        if unit_format == "latex":
            unit_str = self.unit.to_latex()
        else:
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
            if unit_format == "latex":
                mag_str = sp.latex(self.magnitude)
                unc_str = sp.latex(self.uncertainty)
                return f"({mag_str} \\pm {unc_str}) \\; {unit_str}"
            else:
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


HANDLED_FUNCTIONS: dict[Any, Any] = {}


def implements(numpy_function):
    """Register an __array_function__ implementation for Quantity objects."""

    def decorator(func):
        HANDLED_FUNCTIONS[numpy_function] = func
        return func

    return decorator


@implements(np.concatenate)
def concatenate(items, *args, **kwargs):
    unit = items[0].unit
    system = items[0].system
    if not all(i.unit == unit for i in items):
        # Could convert all to first unit, but simpler to raise for now
        raise IncompatibleUnitsError(items[1].unit, unit)
    magnitudes = [i.magnitude for i in items]
    uncertainties = []
    for i in items:
        val = i.uncertainty
        if np.isscalar(val):
            # If magnitudes are arrays, uncertainties should be arrays for concatenate
            if isinstance(i.magnitude, np.ndarray):
                uncertainties.append(
                    np.full_like(i.magnitude, val, dtype=float)
                )
            else:
                uncertainties.append(val)
        else:
            uncertainties.append(val)
    res_mag = np.concatenate(magnitudes, *args, **kwargs)
    res_unc = np.concatenate(uncertainties, *args, **kwargs)
    return Quantity.from_input(res_mag, unit, system, uncertainty=res_unc)


@implements(np.mean)
def mean(a, *args, **kwargs):
    res_mag = np.mean(a.magnitude, *args, **kwargs)
    # Uncertainty of mean (simplified): sqrt(sum(u^2))/N
    res_unc = np.sqrt(np.sum(a.uncertainty**2)) / len(a)
    return Quantity.from_input(res_mag, a.unit, a.system, uncertainty=res_unc)


__all__ = ["Quantity"]
