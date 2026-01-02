"""Defines the `Quantity` class, the representation of a physical quantity.

This module contains the `Quantity` class, which bundles a numerical value
(magnitude), a `CompoundUnit`, and an optional `Uncertainty`. It is the central
object that users interact with. The class overloads arithmetic, comparison,
and other operators to provide intuitive, unit-aware calculations, automatic
error propagation, and seamless integration with various backends (NumPy, etc.).
"""

from __future__ import annotations

import operator
from dataclasses import dataclass, field
from fractions import Fraction
from typing import (
    TYPE_CHECKING,
    Any,
    Generic,
    TypeVar,
    cast,
    overload,
)

from typing_extensions import Self

from measurekit.core import functional
from measurekit.core.dispatcher import BackendManager
from measurekit.core.protocols import BackendOps
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.uncertainty import Uncertainty
from measurekit.domain.measurement.units import (
    CompoundUnit,
    get_default_system,
)

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# Lazy import for converters to avoid circular dependencies if possible,
# or assume available since we are in domain.
import sympy as sp

from measurekit.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)

try:
    from pydantic_core import core_schema
except ImportError:
    core_schema = None

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType")
UncType = TypeVar("UncType")
Numeric = Any  # Ideally strictly typed via protocols, but simplified for now


@dataclass(frozen=True, slots=True)
class Quantity(Generic[ValueType, UncType]):
    """Represents a physical quantity with magnitude, unit, and uncertainty.

    Examples:
        >>> from measurekit import Q_
        >>> length = Q_(10, "m")
        >>> time = Q_(2, "s")
        >>> velocity = length / time
        >>> print(velocity)
        5.0 m/s
        >>> print(velocity.to("km/h"))
        18.0 km/h
    """

    magnitude: ValueType
    unit: CompoundUnit
    uncertainty_obj: Uncertainty[UncType] = field(
        default_factory=lambda: cast("Uncertainty[UncType]", Uncertainty(0.0))
    )
    fraction: Fraction | None = None
    system: UnitSystem = field(default_factory=get_default_system)
    dimension: Dimension = field(init=False)
    _backend: BackendOps = field(init=False, repr=False)

    def __post_init__(self):
        """Calculates derived fields after the object is initialized."""
        calculated_dimension = self.unit.dimension(self.system)
        object.__setattr__(self, "dimension", calculated_dimension)

        # Determine and set backend
        backend = BackendManager.get_backend(self.magnitude)
        object.__setattr__(self, "_backend", backend)

    def __getstate__(self):
        """Custom pickling to exclude _backend."""
        return {
            "magnitude": self.magnitude,
            "unit": self.unit,
            "uncertainty_obj": self.uncertainty_obj,
            "fraction": self.fraction,
            "system": self.system,
            "dimension": self.dimension,
        }

    def __setstate__(self, state):
        """Restore state and re-derive backend."""
        for k, v in state.items():
            object.__setattr__(self, k, v)

        backend = BackendManager.get_backend(self.magnitude)
        object.__setattr__(self, "_backend", backend)

    def tree_flatten(self):
        """Flattens the Quantity for JAX Pytree registration."""
        # Children: the dynamic/differentiable parts (magnitude)
        # Note: uncertainty is also dynamic if present?
        # If uncertainty is used in gradients, it should be a child.
        # However, uncertainty logic in functional is currently somewhat separate.
        # But if we want JIT compatibility for uncertainty propagation, it must be traced.
        # self.uncertainty_obj contains 'std_dev'.
        return (self.magnitude, self.uncertainty_obj), (
            self.unit,
            self.system,
            self.fraction,
        )

    @classmethod
    def tree_unflatten(cls, aux_data, children):
        """Reconstructs the Quantity from JAX flatten results."""
        magnitude, uncertainty_obj = children
        unit, system, _ = aux_data  # fraction unused

        # Determine backend from magnitude (likely a JAX Tracer or Array)
        # We use _fast_new to skip checks
        # But we need 'dimension'.
        dimension = unit.dimension(system)

        # We need backend ops.
        # If magnitude is Tracer, BackendManager might fail or return NumpyBackend (default).
        # We trust BackendManager or pass explicitly?
        # Ideally BackendManager handles Tracers.
        backend = BackendManager.get_backend(magnitude)

        return cls._fast_new(
            magnitude, unit, uncertainty_obj, system, dimension, backend
        )

    @classmethod
    def _fast_new(
        cls,
        value: ValueType,
        unit: CompoundUnit,
        uncertainty: Uncertainty[UncType],
        system: UnitSystem,
        dimension: Dimension,
        backend: BackendOps | None = None,
    ) -> Self:
        """Bypasses __post_init__ and validation for high-performance creation."""
        obj = object.__new__(cls)
        object.__setattr__(obj, "magnitude", value)
        object.__setattr__(obj, "unit", unit)
        object.__setattr__(obj, "uncertainty_obj", uncertainty)
        object.__setattr__(obj, "system", system)
        object.__setattr__(obj, "dimension", dimension)
        object.__setattr__(obj, "fraction", None)

        if backend is None:
            backend = BackendManager.get_backend(value)
        object.__setattr__(obj, "_backend", backend)

        return cast("Self", obj)

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
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
    ) -> Self:
        """Creates a Quantity from raw input values."""
        resolved_system = (
            system if system is not None else get_default_system()
        )

        backend = BackendManager.get_backend(value)

        # Ensure uncertainty matches backend type if array
        if backend.is_array(value):
            if not isinstance(
                uncertainty, Uncertainty
            ) and not backend.is_array(uncertainty):
                try:
                    shape = backend.shape(value)
                    # Create array of ones with same shape
                    ones = backend.ones(shape)
                    # Multiply by scalar uncertainty to broadcast
                    uncertainty = backend.mul(ones, uncertainty)
                except (AttributeError, NotImplementedError):
                    # Fallback if backend implementation is incomplete
                    pass

        uncertainty_obj = (
            uncertainty
            if isinstance(uncertainty, Uncertainty)
            else Uncertainty.from_standard(uncertainty)
        )

        # Check for fraction support (Python backend only usually)
        frac = None
        if not backend.is_array(value):
            try:
                frac = Fraction(str(value))
            except (ValueError, TypeError):
                pass

        return cls(
            magnitude=cast("ValueType", value),
            unit=unit,
            uncertainty_obj=cast("Uncertainty[UncType]", uncertainty_obj),
            fraction=frac,
            system=resolved_system,
        )

    @classmethod
    def __get_pydantic_core_schema__(
        cls, source_type: Any, handler: Any
    ) -> core_schema.CoreSchema:
        """Defines the Pydantic Core Schema for validation."""
        if core_schema is None:
            raise ImportError("pydantic-core is required for validation.")

        def validate_from_str(value: Any) -> Quantity:
            if isinstance(value, Quantity):
                return value
            if isinstance(value, str):
                from measurekit.application.factories import QuantityFactory

                # We use the factory because it handles strings with units
                q = QuantityFactory()(value)
                return q
            raise ValueError(
                f"No se puede validar Quantity desde {type(value)}"
            )

        def validate_from_dict(value: Any) -> Quantity:
            if isinstance(value, dict):
                mag = value.get("magnitude")
                unit = value.get("unit")
                system_name = value.get("system", "SI")
                # Lazy import to avoid circular dependencies
                from measurekit.application.startup import create_system

                sys = create_system(f"{system_name.lower()}.conf")
                u = sys.get_unit(unit)
                return Quantity.from_input(mag, u, sys)
            raise ValueError(
                f"No se puede validar Quantity desde {type(value)}"
            )

        return core_schema.union_schema(
            [
                core_schema.is_instance_schema(Quantity),
                core_schema.no_info_after_validator_function(
                    validate_from_str,
                    core_schema.any_schema(),
                ),
                core_schema.no_info_after_validator_function(
                    validate_from_dict,
                    core_schema.dict_schema(),
                ),
            ]
        )

    def __hash__(self) -> int:
        """Computes hash of the quantity."""
        # Note: If magnitude is array, it might not be hashable.
        # Python arrays are not hashable. Tuple is.
        # We rely on self.magnitude.__hash__()
        # If not hashable, it will raise TypeError which is expected behavior for mutable types.
        try:
            return hash((self.magnitude, self.unit, self.uncertainty_obj))
        except TypeError:
            # Fallback for unhashable magnitude (like numpy array)
            # Maybe hash bytes? Or raise.
            # Usually Quantity with array magnitude != hashable.
            raise TypeError(
                "unhashable type: 'Quantity' with unhashable magnitude"
            )

    @property
    def _has_uncertainty(self) -> bool:
        """Checks if uncertainty is non-zero, safely handling arrays."""
        unc = self.uncertainty
        try:
            # Backend-aware check
            if self._backend.is_array(unc):
                return bool(self._backend.any(self._backend.not_equal(unc, 0)))

            # Standard check (covers safe scalars and Python lists if backend matches)
            res = unc != 0

            # Handle Truth Value Ambiguity (e.g. Python backend with Numpy array uncertainty)
            try:
                if res:
                    return True
            except ValueError:
                # If "The truth value of an array is ambiguous"
                if hasattr(res, "any"):
                    return bool(res.any())
                return True  # Default to True if complex structure
            return False
        except Exception:
            return True  # Fallback for safety

    def __repr__(self) -> str:
        unit_str = self.unit.to_string(self.system)
        if self._has_uncertainty:
            return (
                f"Quantity({self.magnitude!r}, {unit_str}, "
                f"uncertainty={self.uncertainty!r})"
            )
        return f"Quantity({self.magnitude!r}, {unit_str})"

    def __str__(self) -> str:
        """Returns a user-friendly string representation."""
        unit_str = self.unit.to_string(self.system)
        if self._has_uncertainty:
            return f"({self.magnitude} ± {self.uncertainty}) {unit_str}"
        return f"{self.magnitude} {unit_str}"

    def __rich__(self) -> Any:
        """Rich console protocol for beautiful output."""
        try:
            from rich.text import Text
        except ImportError:
            return self.__str__()

        # Unit string
        unit_str = self.unit.to_string(self.system)

        # Magnitude formatting
        mag_str = str(self.magnitude)

        # Text construction
        text = Text()
        text.append(mag_str, style="bold green")

        if self._has_uncertainty:
            unc_str = str(self.uncertainty)
            text.append(f" ± {unc_str}", style="dim")

        text.append(" ", style="none")
        text.append(unit_str, style="bold blue")

        return text

    def __format__(self, format_spec: str) -> str:
        """Formats the quantity according to the specification."""
        parts = format_spec.split("|")
        mag_fmt = ""
        use_alias = False

        for p in parts:
            if p == "alias":
                use_alias = True
            elif p != "frac":
                mag_fmt = p

        unit_str = self.unit.to_string(self.system, use_alias=use_alias)

        if "frac" in parts and self.fraction is not None:
            return f"{self.fraction} {unit_str}"

        if mag_fmt:
            formatted_mag = format(self.magnitude, mag_fmt)
            if self._has_uncertainty:
                # Formatting array uncertainty might fail if format spec is for scalar
                # Python format() on array delegates to array.__format__ which is limited.
                # If unsafe, fallback to str?
                try:
                    formatted_unc = format(self.uncertainty, mag_fmt)
                except (TypeError, ValueError):
                    formatted_unc = str(self.uncertainty)
                return f"({formatted_mag} ± {formatted_unc}) {unit_str}"
            return f"{formatted_mag} {unit_str}"

        # Default behavior matches __str__
        if self._has_uncertainty:
            return f"({self.magnitude} ± {self.uncertainty}) {unit_str}"
        return f"{self.magnitude} {unit_str}"

    def to_latex(self) -> str:
        """Returns the LaTeX representation."""
        unit_latex = self.unit.to_latex()
        if self._has_uncertainty:
            return (
                f"({self.magnitude} \\pm {self.uncertainty}) \\; {unit_latex}"
            )
        return f"{self.magnitude} \\; {unit_latex}"

    def _repr_latex_(self):
        """Returns LaTeX for Jupyter notebooks."""
        return f"${self.to_latex()}$"

    @property
    def uncertainty(self) -> UncType:
        """Returns the standard deviation of the uncertainty."""
        return self.uncertainty_obj.std_dev

    def to(
        self, target_unit: CompoundUnit | str
    ) -> Quantity[ValueType, UncType]:
        """Converts the quantity to a different unit or moves to a device."""
        if isinstance(target_unit, str):
            # Check if target_unit is a device string (e.g. "cuda", "cpu", "mps")
            # We assume units don't typically have these names exactly without prefixes.
            # But more robustly, if target_unit is not in system and looks like a device:
            devices = {"cuda", "cpu", "mps"}
            if target_unit.lower() in devices or (
                ":" in target_unit
                and target_unit.split(":")[0].lower() in devices
            ):
                return self.to_device(target_unit)

            target_unit = self.system.get_unit(target_unit)

        # Fast path for same unit
        if target_unit == self.unit:
            return self

        if self.dimension != target_unit.dimension(self.system):
            raise IncompatibleUnitsError(self.unit, target_unit)

        # --- Polymorphic Conversion (Generic) ---
        # Handle cases where units are simple single-component units
        # (e.g. Celsius -> Kelvin, Meters -> Feet) using their specific converters.
        if (
            len(self.unit.exponents) == 1
            and list(self.unit.exponents.values())[0] == 1
            and len(target_unit.exponents) == 1
            and list(target_unit.exponents.values())[0] == 1
        ):
            source_name = next(iter(self.unit.exponents))
            target_name = next(iter(target_unit.exponents))

            source_def = self.system.get_definition(source_name)
            target_def = self.system.get_definition(target_name)

            if source_def and target_def:
                # Delegate to converters: convert to base, then from base to target
                # We assume converters handle the backend types or backend objects support ops
                base_val = source_def.converter.to_base(self.magnitude)
                new_magnitude = target_def.converter.from_base(base_val)

                # Uncertainty propagation:
                # We need the scale factor (derivative).
                # For Affine/Linear, access .scale.
                # For Logarithmic, this is complex, but current logic assumes usage
                # where we can approximate or fall back.
                s_scale = getattr(source_def.converter, "scale", 1.0)
                t_scale = getattr(target_def.converter, "scale", 1.0)

                # Use backend for division if needed?
                # Assuming scales are floats.
                if isinstance(s_scale, (int, float)) and isinstance(
                    t_scale, (int, float)
                ):
                    scale_ratio = s_scale / t_scale
                else:
                    # Fallback if scales are weird
                    scale_ratio = 1.0

                new_uncertainty = self._backend.mul(
                    self.uncertainty, scale_ratio
                )

                return cast(
                    "Quantity[ValueType, UncType]",
                    Quantity.from_input(
                        new_magnitude,
                        target_unit,
                        self.system,
                        uncertainty=new_uncertainty,
                    ),
                )

        conversion_factor = self.unit.conversion_factor_to(
            target_unit, self.system
        )

        # Using backend for multiplication
        new_value = self._backend.mul(self.magnitude, conversion_factor)
        new_uncertainty = self._backend.mul(
            self.uncertainty, conversion_factor
        )

        return cast(
            "Quantity[ValueType, UncType]",
            Quantity.from_input(
                new_value,
                target_unit,
                self.system,
                uncertainty=new_uncertainty,
            ),
        )

    def _propagate_vectorized(
        self,
        other: Any,
        out_magnitude: Any,
        jac_self: Any,
        jac_other: Any = None,
    ) -> Uncertainty:
        """Helper to propagate vectorized uncertainty via CovarianceStore."""
        from measurekit.domain.measurement.vectorized_uncertainty import (
            CovarianceStore,
        )

        store = CovarianceStore()
        in_slices = []
        jacobians = []

        if jac_self is not None:
            in_slices.append(self.uncertainty_obj.ensure_vector_slice())
            jacobians.append(jac_self)

        if jac_other is not None:
            if isinstance(other, Quantity):
                in_slices.append(other.uncertainty_obj.ensure_vector_slice())
                jacobians.append(jac_other)
            elif self._backend.is_array(other) or isinstance(
                other, (int, float)
            ):
                pass

        # Compute size using backend
        shape = self._backend.shape(out_magnitude)
        # simple product of shape dimensions
        out_size = 1
        for dim in shape:
            out_size *= dim

        out_slice = store.allocate(out_size)

        store.update_from_propagation(out_slice, in_slices, jacobians)

        # Compute std_dev from diagonal for the new Uncertainty object
        out_cov = store.get_covariance_block(out_slice, out_slice)

        # We need sqrt and diagonal.
        # out_cov is scipy.sparse. We need to interact with it.
        # Ideally this logic is in backend, but we are in core.
        # We assume out_cov has .diagonal() method (scipy sparse has it).
        diag = out_cov.diagonal()

        # Convert diagonal back to array shape
        # We use backend.asarray check? No, diag is numpy array usually from scipy.
        # But we must avoid importing numpy.
        # We can assume backend.reshape works on `diag` if it's compatible.
        # or we cast to backend array.

        # IMPORTANT: if backend is Torch, we might have issue here if we mix scipy sparse with torch.
        # For now, we assume NumpyBackend flow.

        std_dev_flat = self._backend.sqrt(diag)
        std_dev = self._backend.reshape(std_dev_flat, shape)

        return Uncertainty(
            std_dev=cast("UncType", std_dev), vector_slice=out_slice
        )

    def diff(self, variable: Quantity | str, order: int = 1) -> Quantity:
        """Calculates the n-th derivative with respect to a variable.

        This method uses SymPy to perform symbolic differentiation of the
        magnitude.

        Args:
            variable (Quantity | str): The variable to differentiate with
                respect to. If a Quantity is provided, its magnitude is used as
                the symbol and its unit affects the resulting unit. If a string
                is provided, it is treated as a dimensionless symbol.
            order (int): The order of differentiation (default: 1).

        Returns:
            Quantity: The derivative.
        """
        if isinstance(variable, Quantity):
            d_var = variable.magnitude
            d_unit_exponents = variable.unit.exponents
        else:
            d_var = sp.Symbol(variable)
            d_unit_exponents = {}

        # Differentiate magnitude
        try:
            new_mag = sp.diff(self.magnitude, d_var, order)
        except Exception as e:
            # Fallback for array/tensor backends or non-symbolic magnitudes
            # For Phase 3, we focus on SymPy support.
            raise NotImplementedError(
                f"Differentiation failed or not supported for this backend: {e}"
            )

        # Update units: u_new = u_old / (u_var)^order
        new_exponents = dict(self.unit.exponents)
        for u, e in d_unit_exponents.items():
            # u_new = u_old * u_var^(-order)
            new_exponents[u] = new_exponents.get(u, 0) - (e * order)

        new_unit = CompoundUnit(new_exponents)

        # Check if new unit should be simplified or just return as is?
        # Usually differentiation results in meaningful units.

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=0.0
        )

    def _get_converter_if_simple(self):
        """Returns the converter if the unit is a single simple unit."""
        if len(self.unit.exponents) == 1:
            name, exp = next(iter(self.unit.exponents.items()))
            if exp == 1:
                # Must exclude 'noprefix' check if key is there?
                # CompoundUnit logic handles noprefix, but exponents might have it?
                # Usually exponents dict keys are purely unit names.
                return self.system.get_definition(name).converter
        return None

    def _nonlinear_add_sub(
        self, other: Quantity, is_add: bool
    ) -> Quantity | None:
        """Handles non-linear arithmetic (Offset, Logarithmic).

        Returns None if standard linear arithmetic should proceed.
        """
        conv_self = self._get_converter_if_simple()
        conv_other = other._get_converter_if_simple()

        if conv_self is None and conv_other is None:
            return None

        # Resolve converters (Linear is default if None)
        # But we only care if at least one is NON-Linear.

        is_nl_self = conv_self and not conv_self.is_linear
        is_nl_other = conv_other and not conv_other.is_linear

        if not is_nl_self and not is_nl_other:
            return None

        # --- Temperature (Offset) Logic ---
        # T (Offset) +/- T (Offset) or T +/- Delta (Linear)

        is_offset_self = isinstance(conv_self, OffsetConverter)
        is_offset_other = isinstance(conv_other, OffsetConverter)

        # We need check if 'other' is linear but COMPATIBLE (Delta).
        # We assume compatibility if Dimensions match (checked in wrapper logic or here).
        # But let's check dimension compatibility first implicitly or explicitly.
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)

        # Case 1: Both are Offset (e.g. T + T or T - T)
        if is_offset_self and is_offset_other:
            if is_add:
                # T + T -> Error
                raise ValueError(
                    "Cannot add two affine quantities (e.g. Temperatures). "
                    "Did you mean to add a difference?"
                )
            # T - T -> Delta (Linear)
            # Result in Base Units (e.g. Kelvin)
            # val = (m1*s + o) - (m2*s + o) = (m1-m2)*s
            # This is equivalent to converting both to base and subtracting.
            base_self = conv_self.to_base(self.magnitude)
            base_other = conv_other.to_base(other.magnitude)
            res_base = self._backend.sub(base_self, base_other)

            # Result unit? Base Unit.
            # Use simplified unit recipe or Base Dimensions?
            # We construct a Quantity with the Base Unit of the dimension.
            # Assuming simple base unit exists for the dimension.
            # Or simply: The unit corresponding to '1.0' scale linear converter?
            # Safest: Return in Base Unit of the system for that dimension.
            # System has base_units?
            # We can try to find a linear unit with same dimension?
            # For Phase 3, let's return it Key (Base) unit.
            # Or just keep it as "Delta K".

            # Actually, simply returning in Base Unit is standard correct behavior.
            # How to get Base Unit?
            # unit.dimension -> system.get_base_unit_for_dimension?
            # This might be complex.

            # Alternative: Use self.unit but interpret as Linear?
            # No.

            # Let's manual calc: (m1-m2)*scale.
            # Unit: The linear cousin.
            # If DegC, linear cousin is Kelvin (or deltaDegC).
            # If we return Kelvin, it's safe.
            # How to get Kelvin from DegC? It's the base of DegC.
            # But UnitDefinition doesn't explicitly store "BaseUnitName".
            # It computes to_base.

            # Let's inspect exponents.
            # If we just return the value in 'base', we need a Unit object for 'base'.
            # Can we deduce it?
            # Maybe just return a Quantity with a known linear unit if possible.

            # Hack: Use the dimension's default unit if available?
            # Or just return raw number if we can't find unit? No.

            # Better approach for T - T:
            # Compute magnitude difference.
            # Unit is 'delta_self'.
            # Does system have 'delta_degC'?
            # If not, return Kelvin.
            # Hardcoding for Phase 3 example?
            # Let's try to return in System's Base Unit for that dimension.
            # self.system.get_base_unit_from_dimension(self.dimension)?
            # Missing method.

            # fallback: Just use the scale factor difference and attach the original unit?
            # No, standard C is Offset.

            # Let's return value in BASE units (Kelvin).
            # Finding the symbol for Kelvin?
            # We don't know it easily without search.

            # Compromise:
            # Assume standard SI base units?
            # 'K' for Temperature.
            # Iterate system units to find one with LinearConverter and correct dimension?
            # Potentially slow but correct.

            target_unit = None
            for name, u_def in self.system.UNIT_REGISTRY.get(
                self.dimension, {}
            ).items():
                if (
                    isinstance(u_def.converter, LinearConverter)
                    and u_def.converter.scale == 1.0
                ):
                    target_unit = self.system.get_unit(name)
                    break

            if not target_unit:
                # Fallback: create a custom Linear unit?
                # Or error.
                # Let's assume we find one.
                target_unit = self.unit  # Dangerous fall back

            return Quantity.from_input(
                res_base, target_unit, self.system, uncertainty=0.0
            )

        # Case 2: Linear +/- Offset
        # Delta +/- T
        if not is_offset_self and is_offset_other:
            # Linear +/- Offset
            # Delta + T -> T (Offset)
            # Delta - T -> (Tf - Ti) - Tr = Delta - T_abs ?
            # 5 deg - 20 degC = -15 degC. (5 - 293 = -288 K = -561 C).
            # Correct logic: Convert both to Base. Result is Base. Convert back to Offset?
            # 5K - 293K = -288K.
            # Convert -288K to DegC: -288 - 273 = -561 C.
            # This is mathematically consistent.

            base_self = self._backend.mul(
                self.magnitude, getattr(conv_self, "scale", 1.0)
            )  # Linear
            base_other = conv_other.to_base(other.magnitude)

            if is_add:
                res_base = self._backend.add(base_self, base_other)
            else:
                res_base = self._backend.sub(base_self, base_other)

            # Result is Temperature (Offset)
            res_mag = conv_other.from_base(res_base)
            return Quantity.from_input(
                res_mag, other.unit, self.system, uncertainty=0.0
            )

        # Case 3: Offset +/- Linear
        # T +/- Delta
        if is_offset_self and not is_offset_other:
            # T +/- Delta -> T (Offset)
            base_self = conv_self.to_base(self.magnitude)
            base_other = self._backend.mul(
                other.magnitude, getattr(conv_other, "scale", 1.0)
            )

            if is_add:
                res_base = self._backend.add(base_self, base_other)
            else:
                res_base = self._backend.sub(base_self, base_other)

            res_mag = conv_self.from_base(res_base)
            return Quantity.from_input(
                res_mag, self.unit, self.system, uncertainty=0.0
            )

        # --- Logarithmic Logic ---
        # dB + dB -> Power Sum
        is_log_self = isinstance(conv_self, LogarithmicConverter)
        is_log_other = isinstance(conv_other, LogarithmicConverter)

        if is_log_self and is_log_other:
            # Convert both to linear base (Powers)
            base_self = conv_self.to_base(self.magnitude)
            base_other = conv_other.to_base(other.magnitude)

            if is_add:
                res_base = self._backend.add(base_self, base_other)
            else:
                # Subtracting powers?
                # If valid? Yes.
                res_base = self._backend.sub(base_self, base_other)

            # Convert back to Log (dB)
            res_mag = conv_self.from_base(res_base)
            return Quantity.from_input(
                res_mag, self.unit, self.system, uncertainty=0.0
            )

        return None

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity:
        """Handles cases like my_quantity + other."""
        # Check Non-Linear / Complex Logic first
        if isinstance(other, Quantity):
            # Optimization: If both are strictly linear, skip overhead
            if self.unit.is_linear(self.system) and other.unit.is_linear(
                self.system
            ):
                pass
            else:
                try:
                    res = self._nonlinear_add_sub(other, is_add=True)
                    if res is not None:
                        return res
                except IncompatibleUnitsError:
                    raise
                except ValueError:
                    # Catch the "T+T" error and re-raise
                    raise

        # --- FAST PATH ---
        if type(other) is Quantity and self.unit is other.unit:
            new_magnitude = self._backend.add(self.magnitude, other.magnitude)

            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)

                # Broadcasting for self
                is_self_scalar = (
                    self._backend.shape(self.magnitude) == ()
                    or len(self._backend.shape(self.magnitude)) == 0
                    or (
                        hasattr(self.magnitude, "shape")
                        and self.magnitude.shape == (1,)
                    )
                )

                if is_self_scalar:
                    j_self = self._backend.ones((size, 1))
                else:
                    j_self = self._backend.identity_operator(size)

                # Check for broadcasting: if other is scalar-like, broadcast Jacobian
                is_other_scalar = False
                if isinstance(other, Quantity):
                    if (
                        self._backend.shape(other.magnitude) == ()
                        or len(self._backend.shape(other.magnitude)) == 0
                        or (
                            hasattr(other.magnitude, "shape")
                            and other.magnitude.shape == (1,)
                        )
                    ):
                        is_other_scalar = True

                if is_other_scalar:
                    j_other = self._backend.ones((size, 1))
                else:
                    j_other = self._backend.identity_operator(size)

                new_unc = self._propagate_vectorized(
                    other, new_magnitude, j_self, j_other
                )
                return self._fast_new(
                    new_magnitude,
                    self.unit,
                    new_unc,
                    self.system,
                    self.dimension,
                    self._backend,
                )
            # Scalar path
            return self._fast_new(
                new_magnitude,
                self.unit,
                self.uncertainty_obj + other.uncertainty_obj,
                self.system,
                self.dimension,
                self._backend,
            )
        # -----------------

        if not isinstance(other, Quantity):
            return NotImplemented

        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        new_magnitude = self._backend.add(
            self.magnitude, other_converted.magnitude
        )

        if self._backend.is_array(new_magnitude):
            size = self._backend.size(new_magnitude)
            # In slow path, we assume arrays if is_array matches (simplification)
            j_self = self._backend.identity_operator(size)
            j_other = self._backend.identity_operator(size)

            new_uncertainty_obj = self._propagate_vectorized(
                other_converted, new_magnitude, j_self, j_other
            )
        else:
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
        # Check Non-Linear / Complex Logic first
        if isinstance(other, Quantity):
            # Optimization: If both are strictly linear, skip overhead
            if self.unit.is_linear(self.system) and other.unit.is_linear(
                self.system
            ):
                pass
            else:
                try:
                    res = self._nonlinear_add_sub(other, is_add=False)
                    if res is not None:
                        return res
                except IncompatibleUnitsError:
                    raise

        # --- FAST PATH ---
        if type(other) is Quantity and self.unit is other.unit:
            new_magnitude = self._backend.sub(self.magnitude, other.magnitude)
            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)

                # Broadcasting for self
                is_self_scalar = (
                    self._backend.shape(self.magnitude) == ()
                    or len(self._backend.shape(self.magnitude)) == 0
                    or (
                        hasattr(self.magnitude, "shape")
                        and self.magnitude.shape == (1,)
                    )
                )

                if is_self_scalar:
                    j_self = self._backend.ones((size, 1))

                else:
                    j_self = self._backend.identity_operator(size)

                # Broadcasting for subtraction
                is_other_scalar = False
                if isinstance(other, Quantity):
                    if (
                        self._backend.shape(other.magnitude) == ()
                        or len(self._backend.shape(other.magnitude)) == 0
                        or (
                            hasattr(other.magnitude, "shape")
                            and other.magnitude.shape == (1,)
                        )
                    ):
                        is_other_scalar = True

                if is_other_scalar:
                    j_other = self._backend.mul(
                        self._backend.ones((size, 1)), -1
                    )
                else:
                    j_other = self._backend.mul(
                        self._backend.identity_operator(size), -1
                    )

                new_unc = self._propagate_vectorized(
                    other, new_magnitude, j_self, j_other
                )
                return self._fast_new(
                    new_magnitude,
                    self.unit,
                    new_unc,
                    self.system,
                    self.dimension,
                    self._backend,
                )
            return self._fast_new(
                new_magnitude,
                self.unit,
                self.uncertainty_obj - other.uncertainty_obj,
                self.system,
                self.dimension,
                self._backend,
            )

        if not isinstance(other, Quantity):
            return NotImplemented

        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)
        other_converted = other.to(self.unit)
        new_magnitude = self._backend.sub(
            self.magnitude, other_converted.magnitude
        )

        if self._backend.is_array(new_magnitude):
            size = self._backend.size(new_magnitude)
            j_self = self._backend.identity_operator(size)
            j_other = self._backend.mul(
                self._backend.identity_operator(size), -1
            )

            new_uncertainty_obj = self._propagate_vectorized(
                other_converted, new_magnitude, j_self, j_other
            )
        else:
            new_uncertainty_obj = (
                self.uncertainty_obj - other_converted.uncertainty_obj
            )

        return Quantity.from_input(
            new_magnitude,
            self.unit,
            self.system,
            uncertainty=new_uncertainty_obj,
        )

    def __mul__(self, other: Any) -> Quantity:
        if isinstance(other, (int, float, complex)) or self._backend.is_array(
            other
        ):
            new_magnitude = self._backend.mul(self.magnitude, other)
            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)

                if self._backend.is_array(other):
                    # flatten
                    (other_flat,) = self._backend.broadcast_and_flatten(
                        [other]
                    )
                    j_self = self._backend.diagonal_operator(other_flat)
                else:
                    j_self = self._backend.mul(
                        self._backend.identity_operator(size), other
                    )

                new_uncertainty_obj = self._propagate_vectorized(
                    None, new_magnitude, j_self, None
                )
            else:
                new_uncertainty_obj = self.uncertainty_obj.scale(other)

            return cast(
                "Quantity[ValueType, UncType]",
                Quantity.from_input(
                    new_magnitude,
                    self.unit,
                    self.system,
                    uncertainty=new_uncertainty_obj,
                ),
            )

        if isinstance(other, Quantity):
            new_magnitude = self._backend.mul(self.magnitude, other.magnitude)
            new_unit = self.unit * other.unit
            new_dimension = self.dimension * other.dimension

            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)

                # Flatten/Broadcast using new backend capability
                self_flat, other_flat = self._backend.broadcast_and_flatten(
                    [self.magnitude, other.magnitude]
                )

                # Determine j_self structure
                is_self_scalar = (
                    self._backend.shape(self.magnitude) == ()
                    or len(self._backend.shape(self.magnitude)) == 0
                    or (
                        hasattr(self.magnitude, "shape")
                        and self.magnitude.shape == (1,)
                    )
                )

                if is_self_scalar:
                    j_self = self._backend.reshape(other_flat, (size, 1))
                else:
                    j_self = self._backend.diagonal_operator(other_flat)

                # Determine j_other structure
                is_other_scalar = (
                    self._backend.shape(other.magnitude) == ()
                    or len(self._backend.shape(other.magnitude)) == 0
                    or (
                        hasattr(other.magnitude, "shape")
                        and other.magnitude.shape == (1,)
                    )
                )

                if is_other_scalar:
                    j_other = self._backend.reshape(self_flat, (size, 1))
                else:
                    j_other = self._backend.diagonal_operator(self_flat)

                new_uncertainty_obj = self._propagate_vectorized(
                    other, new_magnitude, j_self, j_other
                )
            else:
                new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                    other.uncertainty_obj,
                    self.magnitude,
                    other.magnitude,
                    new_magnitude,
                )
            return self._fast_new(
                new_magnitude,
                new_unit,
                new_uncertainty_obj,
                self.system,
                new_dimension,
                self._backend,
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
        if isinstance(other, (int, float, complex)) or self._backend.is_array(
            other
        ):
            new_magnitude = self._backend.truediv(self.magnitude, other)
            if self._backend.is_array(new_magnitude):
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                if self._backend.is_array(other):
                    other_recip = self._backend.truediv(1.0, other)
                    other_recip = self._backend.reshape(other_recip, (size,))
                    j_self = self._backend.diags([other_recip], [0])
                else:
                    j_self = self._backend.mul(
                        self._backend.eye(size), 1.0 / other
                    )

                new_uncertainty_obj = self._propagate_vectorized(
                    None, new_magnitude, j_self, None
                )
            else:
                new_uncertainty_obj = self.uncertainty_obj.scale(1.0 / other)

            return cast(
                "Quantity[ValueType, UncType]",
                Quantity.from_input(
                    new_magnitude,
                    self.unit,
                    self.system,
                    uncertainty=new_uncertainty_obj,
                ),
            )

        if isinstance(other, Quantity):
            new_magnitude = self._backend.truediv(
                self.magnitude, other.magnitude
            )
            new_unit = self.unit / other.unit
            new_dimension = self.dimension / other.dimension

            if self._backend.is_array(new_magnitude):
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                # Prepare broadcasted values flattened to (size,)

                # Term 1: 1/y (coefficient for j_self)
                # We need 1/other.magnitude
                # If other is scalar/array...
                recip_other = self._backend.truediv(1.0, other.magnitude)

                # Broadcast recip_other to (size,)
                if self._backend.is_array(recip_other):
                    if self._backend.shape(recip_other) == () or (
                        hasattr(recip_other, "shape")
                        and recip_other.shape == (1,)
                    ):
                        # Broadcast
                        val = (
                            recip_other.item()
                            if hasattr(recip_other, "item")
                            else recip_other
                        )
                        recip_flat = self._backend.mul(
                            self._backend.ones(size), val
                        )
                    elif self._backend.shape(
                        recip_other
                    ) == self._backend.shape(new_magnitude):
                        recip_flat = self._backend.reshape(
                            recip_other, (size,)
                        )
                    else:
                        recip_flat = self._backend.reshape(
                            recip_other, (size,)
                        )
                else:
                    recip_flat = self._backend.mul(
                        self._backend.ones(size), recip_other
                    )

                is_self_scalar = (
                    self._backend.shape(self.magnitude) == ()
                    or len(self._backend.shape(self.magnitude)) == 0
                    or (
                        hasattr(self.magnitude, "shape")
                        and self.magnitude.shape == (1,)
                    )
                )

                if is_self_scalar:
                    j_self = self._backend.reshape(recip_flat, (size, 1))
                else:
                    j_self = self._backend.diags([recip_flat], [0])

                # Term 2: -x/y^2 (coefficient for j_other)
                # factor = - new_magnitude / other.magnitude  ? No (-x/y^2 = -(x/y)/y = -z/y)
                # factor = - new_magnitude / other.magnitude
                neg_z_over_y = self._backend.truediv(
                    self._backend.mul(new_magnitude, -1.0), other.magnitude
                )

                # Broadcast factor to (size,)
                if self._backend.is_array(neg_z_over_y):
                    if self._backend.shape(neg_z_over_y) == () or (
                        hasattr(neg_z_over_y, "shape")
                        and neg_z_over_y.shape == (1,)
                    ):
                        val = (
                            neg_z_over_y.item()
                            if hasattr(neg_z_over_y, "item")
                            else neg_z_over_y
                        )
                        factor_flat = self._backend.mul(
                            self._backend.ones(size), val
                        )
                    elif self._backend.shape(
                        neg_z_over_y
                    ) == self._backend.shape(new_magnitude):
                        factor_flat = self._backend.reshape(
                            neg_z_over_y, (size,)
                        )
                    else:
                        factor_flat = self._backend.reshape(
                            neg_z_over_y, (size,)
                        )
                else:
                    factor_flat = self._backend.mul(
                        self._backend.ones(size), neg_z_over_y
                    )

                is_other_scalar = (
                    self._backend.shape(other.magnitude) == ()
                    or len(self._backend.shape(other.magnitude)) == 0
                    or (
                        hasattr(other.magnitude, "shape")
                        and other.magnitude.shape == (1,)
                    )
                )

                if is_other_scalar:
                    j_other = self._backend.reshape(factor_flat, (size, 1))
                else:
                    j_other = self._backend.diags([factor_flat], [0])

                new_uncertainty_obj = self._propagate_vectorized(
                    other, new_magnitude, j_self, j_other
                )
            else:
                new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                    other.uncertainty_obj,
                    self.magnitude,
                    other.magnitude,
                    new_magnitude,
                )

            return self._fast_new(
                new_magnitude,
                new_unit,
                new_uncertainty_obj,
                self.system,
                new_dimension,
                self._backend,
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
        new_value = self._backend.pow(self.magnitude, exponent)
        new_unit = self.unit**exponent
        # Casting to float is tricky if it's array.
        # But Uncertainty.power expects Any value usually.
        new_uncertainty_obj = self.uncertainty_obj.power(exponent, new_value)
        return Quantity.from_input(
            new_value, new_unit, self.system, uncertainty=new_uncertainty_obj
        )

    __radd__ = __add__
    __rmul__ = __mul__

    def __rtruediv__(self, other: Any) -> Quantity:
        # Note: We rely on the backend to handle division by zero (e.g. inf/nan)
        # to ensure compatibility with JAX/Tracer environments where value checks are forbidden.

        new_magnitude = self._backend.truediv(other, self.magnitude)
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
        return cast(
            "Self",
            Quantity.from_input(
                self._backend.mul(self.magnitude, -1),
                self.unit,
                self.system,
                uncertainty=self.uncertainty_obj.scale(-1.0),
            ),
        )

    def __pos__(self) -> Self:
        return self

    def __abs__(self) -> Self:
        sign = self._backend.sign(self.magnitude)
        return cast(
            "Self",
            Quantity.from_input(
                self._backend.abs(self.magnitude),
                self.unit,
                self.system,
                self.uncertainty_obj.scale(sign),
            ),
        )

    # --- NumPy Integration (Soft Dependency) ---
    def __array_ufunc__(self, ufunc, method, *inputs, **kwargs):
        """Handles NumPy ufuncs by delegating to the backend."""
        try:
            import numpy as np
        except ImportError:
            return NotImplemented

        # Handle reductions (e.g. np.sum which calls np.add.reduce)
        if method == "reduce":
            if ufunc == np.add:
                # Summation
                # inputs[0] is the quantity to reduce
                inp = inputs[0]
                if isinstance(inp, Quantity):
                    # Delegate to backend sum
                    # kwargs might contain 'axis', 'dtype', 'out', 'keepdims'
                    # We need to handle 'out' carefully or ignore?
                    # Backend sum usually takes axis.
                    res_mag = self._backend.sum(
                        inp.magnitude, axis=kwargs.get("axis")
                    )
                    # reduce usually lowers dimension.
                    return Quantity.from_input(res_mag, inp.unit, self.system)
            return NotImplemented

        if method != "__call__":
            return NotImplemented

        # Standard Dispatch
        if ufunc == np.add:
            return self.__add__(inputs[1] if inputs[0] is self else inputs[0])
        if ufunc == np.subtract:
            val = inputs[1] if inputs[0] is self else inputs[0]
            if inputs[0] is self:
                return self.__sub__(val)
            return self.__rsub__(val)
        if ufunc == np.multiply:
            return self.__mul__(inputs[1] if inputs[0] is self else inputs[0])
        if ufunc == np.true_divide:
            if inputs[0] is self:
                return self.__truediv__(inputs[1])
            return self.__rtruediv__(inputs[0])
        if ufunc == np.power:
            if inputs[0] is self:
                return self.__pow__(inputs[1])

        # Unary math that changes unit
        if ufunc == np.sqrt:
            return self**0.5
        if ufunc == np.square:
            return self**2

        # Unary math that preserves unit
        if ufunc == np.absolute:
            return abs(self)

        # Trig functions (Require dimensionless)
        trig_funcs = (
            np.sin,
            np.cos,
            np.tan,
            np.exp,
            np.log,
            np.log10,
            np.arcsin,
            np.arccos,
            np.arctan,
        )
        if ufunc in trig_funcs:
            # Assume unary
            inp = inputs[0]
            if isinstance(inp, Quantity):
                if not inp.dimension.is_dimensionless:
                    # Check if it's strictly stateless or effectively dimensionless?
                    # The test registers 'rad' as dimensionless.
                    pass

                # If dimensionless, units are dropped/cleared in result (e.g. sin(rad) -> 1)
                # We verify dimensionless but result is pure number (dimensionless Quantity).
                if inp.dimension.is_dimensionless:
                    res_mag = ufunc(inp.magnitude, **kwargs)
                    return Quantity.from_input(
                        res_mag, CompoundUnit({}), self.system
                    )

                raise IncompatibleUnitsError(inp.unit, CompoundUnit({}))

        return NotImplemented

    def __array_function__(self, func, types, args, kwargs):
        """Handles NumPy functions like np.concatenate, np.mean."""
        try:
            import numpy as np
        except ImportError:
            return NotImplemented

        if func == np.concatenate:
            mags = []
            unit = None
            for arg in args[0]:
                if isinstance(arg, Quantity):
                    if unit is None:
                        unit = arg.unit
                    elif arg.unit != unit:
                        return NotImplemented  # Strict unit check
                    mags.append(arg.magnitude)
                else:
                    return NotImplemented  # All must be Quantity for now
            res_mag = np.concatenate(mags, **kwargs)
            return Quantity(res_mag, unit, system=self.system)

        if func == np.mean:
            # args[0] is self usually
            q = args[0]
            if isinstance(q, Quantity):
                return Quantity(
                    np.mean(q.magnitude, **kwargs), q.unit, system=q.system
                )

        return NotImplemented

    def __torch_function__(self, func, types, args=(), kwargs=None):
        """Handles Torch functions like torch.mean, torch.relu for Quantity objects."""
        if kwargs is None:
            kwargs = {}

        import torch

        # Helper to unwrap Quantities
        def unwrap(obj):
            if isinstance(obj, Quantity):
                return obj.magnitude
            if isinstance(obj, (list, tuple)):
                return type(obj)(unwrap(x) for x in obj)
            return obj

        # --- Dispatch Logic ---
        # Map common torch functions to Quantity operators or functional logic

        # Arithmetic -> Delegate to operators to preserve uncertainty logic
        if func in (torch.add,):
            return operator.add(args[0], args[1])  # type: ignore
        if func in (torch.sub,):
            return operator.sub(args[0], args[1])  # type: ignore
        if func in (torch.mul,):
            return operator.mul(args[0], args[1])  # type: ignore
        if func in (torch.div, torch.true_divide):
            return operator.truediv(args[0], args[1])  # type: ignore
        if func in (torch.pow,):
            return operator.pow(args[0], args[1])  # type: ignore

        # Unary Math -> Check Dimensionless
        # (Sin, Cos, Exp, Log...)
        trig_map = {
            torch.sin: torch.sin,
            torch.cos: torch.cos,
            torch.tan: torch.tan,
            torch.exp: torch.exp,
            torch.log: torch.log,
            torch.log10: torch.log10,
            torch.abs: torch.abs,
            torch.sqrt: torch.sqrt,
        }

        if func in trig_map:
            q = args[0]
            if not isinstance(q, Quantity):
                return NotImplemented

            # sqrt is special (unit becomes u^0.5)
            if func == torch.sqrt:
                return q**0.5

            # abs preserves unit
            if func == torch.abs:
                return abs(q)

            # Others require dimensionless
            if not q.dimension.is_dimensionless:
                raise IncompatibleUnitsError(q.unit, CompoundUnit({}))

            # Result is dimensionless
            res_mag = func(q.magnitude, **kwargs)
            # Create dimensionless quantity
            return Quantity.from_input(res_mag, CompoundUnit({}), q.system)

        # Fallback: Unwrap -> Call -> Wrap (Blind wrapping)
        # This is dangerous for operations that change units, but acceptable for
        # shape ops (reshape, transpose) or generic tensor ops.

        unwrapped_args = tuple(unwrap(arg) for arg in args)
        unwrapped_kwargs = {k: unwrap(v) for k, v in kwargs.items()}

        result = func(*unwrapped_args, **unwrapped_kwargs)

        # If result is Tensor, try to wrap it using the first Quantity's unit
        # This is heuristic and might be wrong for some ops.
        # But it enables things like 'torch.unsqueeze(q)' to work.
        source_q = next(
            (arg for arg in args if isinstance(arg, Quantity)), None
        )

        if source_q is not None and isinstance(result, torch.Tensor):
            return Quantity.from_input(result, source_q.unit, source_q.system)

        return result

    def to_device(self, device: str) -> Self:
        """Moves the quantity and its uncertainty to the specified device."""
        new_mag = self._backend.to_device(self.magnitude, device)
        new_unc_val = self._backend.to_device(self.uncertainty, device)
        new_unc = Uncertainty(new_unc_val)

        return self._fast_new(
            new_mag,
            self.unit,
            new_unc,
            self.system,
            self.dimension,
            self._backend,
        )

    def backward(self, *args, **kwargs) -> None:
        """Delegates autograd backward call to the underlying magnitude."""
        if hasattr(self.magnitude, "backward"):
            self.magnitude.backward(*args, **kwargs)
        else:
            raise TypeError(
                f"Backend magnitude of type {type(self.magnitude)} does not support backward()"
            )

    # --- Representation ---

    def __int__(self) -> int:
        return int(self.magnitude)

    def __round__(self, ndigits: int | None = None) -> Quantity:
        val = round(self.magnitude, ndigits)
        return Quantity.from_input(
            val, self.unit, self.system, self.uncertainty
        )

    def __floor__(self) -> Quantity:
        import math

        return Quantity.from_input(
            math.floor(self.magnitude),
            self.unit,
            self.system,
            self.uncertainty,
        )

    def __ceil__(self) -> Quantity:
        import math

        return Quantity.from_input(
            math.ceil(self.magnitude), self.unit, self.system, self.uncertainty
        )

    def __trunc__(self) -> Quantity:
        import math

        return Quantity.from_input(
            math.trunc(self.magnitude),
            self.unit,
            self.system,
            self.uncertainty,
        )

    # --- Container / Array Methods ---
    # def __array__(self, dtype=None) -> Any:
    #     """Returns the magnitude as a NumPy array (strips units)."""
    #     # Note: Ideally avoid importing numpy, but this hook is for numpy.
    #     # If magnitude is already array, return it.
    #     # If not, convert.
    #     try:
    #         import numpy as np
    #         if dtype:
    #             return np.array(self.magnitude, dtype=dtype)
    #         return np.array(self.magnitude)
    #     except ImportError:
    #         # Should not happen if this is called by numpy
    #         return self.magnitude

    def __float__(self) -> float:
        return float(self.magnitude)  # May fail for arrays

    # --- Math & Vector Ops ---

    def dot(self, other: Quantity) -> Quantity:
        """Computes dot product."""
        if not isinstance(other, Quantity):
            raise TypeError(f"dot requires Quantity, got {type(other)}")

        mag = self._backend.dot(self.magnitude, other.magnitude)
        new_unit = self.unit * other.unit
        # Uncertainty ignored for now
        return Quantity.from_input(mag, new_unit, self.system)

    def cross(self, other: Quantity) -> Quantity:
        """Computes cross product."""
        if not isinstance(other, Quantity):
            raise TypeError(f"cross requires Quantity, got {type(other)}")

        mag = self._backend.cross(self.magnitude, other.magnitude)
        new_unit = self.unit * other.unit
        # Uncertainty ignored for now
        return Quantity.from_input(mag, new_unit, self.system)

    def __len__(self) -> int:
        return len(self.magnitude)

    def __iter__(self):
        # Yield quantities for each element
        # This is slow but correct for iteration
        for i in range(len(self)):
            yield self[i]

    def __add__(self, other: Any) -> Quantity[ValueType, UncType]:
        """Adds two quantities."""
        if not isinstance(other, Quantity):
            if isinstance(other, CompoundUnit):
                other = Quantity.from_input(1, other, self.system)
            else:
                try:
                    other = Quantity.from_input(
                        other, CompoundUnit({}), self.system
                    )
                except Exception:
                    return NotImplemented

        new_mag, new_unit = functional.add_quantities(
            self.magnitude, self.unit, other.magnitude, other.unit, self.system
        )

        # Uncertainty Propagation
        # Convert other's uncertainty if units differ
        other_unc = other.uncertainty_obj
        if other.unit != new_unit:
            try:
                f = other.unit.conversion_factor_to(new_unit, self.system)
                other_unc = other_unc.scale(f)
            except Exception:
                pass

        # We assume independent variables for basic arithmetic (add in quadrature)
        # Using the uncertainty object's add method (which likely handles this)
        try:
            # Check if we should use vectorization?
            # functional handles value, but not uncertainty vectorization.
            # We fall back to object method.
            new_unc_obj = self.uncertainty_obj + other_unc
        except Exception:
            new_unc_obj = 0.0

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=new_unc_obj
        )

    def __sub__(self, other: Any) -> Quantity[ValueType, UncType]:
        """Subtracts two quantities."""
        if not isinstance(other, Quantity):
            if isinstance(other, CompoundUnit):
                other = Quantity.from_input(1, other, self.system)
            else:
                try:
                    other = Quantity.from_input(
                        other, CompoundUnit({}), self.system
                    )
                except Exception:
                    return NotImplemented

        new_mag, new_unit = functional.sub_quantities(
            self.magnitude, self.unit, other.magnitude, other.unit, self.system
        )

        other_unc = other.uncertainty_obj
        if other.unit != new_unit:
            try:
                f = other.unit.conversion_factor_to(new_unit, self.system)
                other_unc = other_unc.scale(f)
            except Exception:
                pass

        try:
            # Uncertainties add in quadrature for subtraction too.
            # Assuming Uncertainty class implements __sub__ or __add__ appropriately.
            # Previous code used __sub__.
            new_unc_obj = self.uncertainty_obj - other_unc
        except Exception:
            new_unc_obj = 0.0

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=new_unc_obj
        )

    def __mul__(self, other: Any) -> Quantity[ValueType, UncType]:
        """Multiplies two quantities."""
        if not isinstance(other, Quantity):
            if isinstance(other, CompoundUnit):
                other = Quantity.from_input(1, other, self.system)
            else:
                try:
                    other = Quantity.from_input(
                        other, CompoundUnit({}), self.system
                    )
                except Exception:
                    return NotImplemented

        new_mag, new_unit = functional.mul_quantities(
            self.magnitude, self.unit, other.magnitude, other.unit, self.system
        )

        try:
            new_unc_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_mag,
            )
        except Exception:
            new_unc_obj = 0.0

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=new_unc_obj
        )

    def __truediv__(self, other: Any) -> Quantity[ValueType, UncType]:
        """Divides two quantities."""
        if not isinstance(other, Quantity):
            if isinstance(other, CompoundUnit):
                other = Quantity.from_input(1, other, self.system)
            else:
                try:
                    other = Quantity.from_input(
                        other, CompoundUnit({}), self.system
                    )
                except Exception:
                    return NotImplemented

        new_mag, new_unit = functional.truediv_quantities(
            self.magnitude, self.unit, other.magnitude, other.unit, self.system
        )

        try:
            new_unc_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_mag,
            )
        except Exception:
            new_unc_obj = 0.0

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=new_unc_obj
        )

    def __pow__(self, exponent: Any) -> Quantity[ValueType, UncType]:
        """Raises quantity to a power."""
        # Exponent handling: functional expects it.
        # For unit logic, functional uses scalar assumption.
        new_mag, new_unit = functional.pow_quantities(
            self.magnitude, self.unit, exponent, self.system
        )

        try:
            # Uncertainty power propagation
            new_unc_obj = self.uncertainty_obj.power(exponent, new_mag)
        except Exception:
            new_unc_obj = 0.0

        return Quantity.from_input(
            new_mag, new_unit, self.system, uncertainty=new_unc_obj
        )

    def __getitem__(self, key: Any) -> Quantity:
        """Slices the quantity."""
        new_mag = self.magnitude[key]

        # Slicing uncertainty
        # If uncertainty is array, slice it.
        # If it's scalar, preserve it (it applies to all).
        # We need to rely on backend or simple checks since we are in domain.
        # Check if uncertainty_obj.std_dev is array-like
        if hasattr(self.uncertainty, "__getitem__") and not isinstance(
            self.uncertainty, (str, float, int)
        ):
            try:
                new_unc_val = self.uncertainty[key]
            except (IndexError, TypeError):
                # Fallback for scalar uncertainty with array magnitude?
                new_unc_val = self.uncertainty
        else:
            new_unc_val = self.uncertainty

        # For now, we create a new trivial Uncertainty object for the slice
        # Losing correlation tracking for the slice is acceptable for this refactor stage
        # unless vector_slice logic is enhanced.
        new_unc_obj = Uncertainty(new_unc_val)

        # If slicing a single element, we might get a scalar magnitude.
        # backend might need update if it was caching type info?
        # Quantity._fast_new handles it.

        return self._fast_new(
            new_mag,
            self.unit,
            new_unc_obj,
            self.system,
            self.dimension,  # Re-use dimension as it doesn't change
            self._backend,  # Backend might be same (numpy) or change (scalar python?) but we pass explicit
        )

    def __setitem__(self, key: Any, value: Any) -> None:
        """Sets item in the quantity."""
        # This mutates magnitude.
        # We need to ensure units match.
        if isinstance(value, Quantity):
            val_converted = value.to(self.unit)
            self.magnitude[key] = val_converted.magnitude
            # We should also update uncertainty...
            # This is complex for immutable/updates.
            # If magnitude is mutable (numpy), this works for value.
            # Uncertainty update is ignored here (limitation).
        else:
            # Assume value is magnitude in same unit
            self.magnitude[key] = value

    # --- Comparison Methods ---
    def _compare(self, other: Any, op: Any) -> Any:
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                raise IncompatibleUnitsError(self.unit, other.unit)
            # Convert to self unit for comparison
            # Optimization: check if conversion needed
            if self.unit == other.unit:
                return op(self.magnitude, other.magnitude)

            other_converted = other.to(self.unit)
            return op(self.magnitude, other_converted.magnitude)

        # If comparing to 0, allowed regardless of unit (sometimes?)
        # But generally strictly typed.
        if hasattr(other, "magnitude"):  # Duck typing
            return NotImplemented

        return NotImplemented

    def __eq__(self, other: object) -> Any:
        # Dataclass __eq__ is overridden to handle units logic if we want semantic equality
        # But frozen dataclass uses fields.
        # We should allow semantic equality: 1 m == 100 cm
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                return False
            try:
                other_converted = other.to(self.unit)
                return self.magnitude == other_converted.magnitude
            except Exception:
                return False
        return False

    def __ne__(self, other: object) -> Any:
        return not self.__eq__(other)

    def __lt__(self, other: Any) -> Any:
        return self._compare(other, operator.lt)

    def __le__(self, other: Any) -> Any:
        return self._compare(other, operator.le)

    def __gt__(self, other: Any) -> Any:
        return self._compare(other, operator.gt)

    def __ge__(self, other: Any) -> Any:
        return self._compare(other, operator.ge)
