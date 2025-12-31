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

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType")
UncType = TypeVar("UncType")
Numeric = Any  # Ideally strictly typed via protocols, but simplified for now


@dataclass(frozen=True, slots=True)
class Quantity(Generic[ValueType, UncType]):
    """Represents a physical quantity with magnitude, unit, and uncertainty."""

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

        return cast(Self, obj)

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
            magnitude=cast(ValueType, value),
            unit=unit,
            uncertainty_obj=cast("Uncertainty[UncType]", uncertainty_obj),
            fraction=frac,
            system=resolved_system,
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
        """Returns a concise string representation."""
        return (
            f"Quantity({self.magnitude!r}, {self.unit!r}, "
            f"uncertainty={self.uncertainty!r})"
        )

    def __str__(self) -> str:
        """Returns a user-friendly string representation."""
        unit_str = self.unit.to_string(self.system)
        if self._has_uncertainty:
            return f"({self.magnitude} ± {self.uncertainty}) {unit_str}"
        return f"{self.magnitude} {unit_str}"

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
        """Converts the quantity to a different unit."""
        if isinstance(target_unit, str):
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
            std_dev=cast(UncType, std_dev), vector_slice=out_slice
        )

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity:
        """Handles cases like my_quantity + other."""
        # --- FAST PATH ---
        if type(other) is Quantity and self.unit is other.unit:
            new_magnitude = self._backend.add(self.magnitude, other.magnitude)

            if self._backend.is_array(new_magnitude):
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                # Broadcasting for self
                is_self_scalar = False
                if (
                    (
                        self._backend.shape(self.magnitude) == ()
                        or len(self._backend.shape(self.magnitude)) == 0
                    )
                    or hasattr(self.magnitude, "shape")
                    and self.magnitude.shape == (1,)
                ):
                    is_self_scalar = True

                if is_self_scalar:
                    j_self = self._backend.ones((size, 1))
                else:
                    j_self = self._backend.eye(size, format="csr")

                # Check for broadcasting: if other is scalar-like, broadcast Jacobian
                is_other_scalar = False
                if isinstance(other, Quantity):
                    # If backend supports shape on magnitude, checking shape is robust
                    if (
                        self._backend.shape(other.magnitude) == ()
                        or len(self._backend.shape(other.magnitude)) == 0
                    ):
                        is_other_scalar = True
                    elif hasattr(
                        other.magnitude, "shape"
                    ) and other.magnitude.shape == (1,):
                        # Numpy specific fallback safe for now if backend is numpy
                        is_other_scalar = True

                if is_other_scalar:
                    j_other = self._backend.ones((size, 1))
                else:
                    j_other = self._backend.eye(size, format="csr")

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
            size = 1
            for d in self._backend.shape(new_magnitude):
                size *= d

            # For sparse matrix construction we need ones/eye.
            # Delegating to backend.
            if self._backend.is_array(self.magnitude):
                j_self = self._backend.eye(size)
            else:
                j_self = self._backend.eye(size)

            if self._backend.is_array(other_converted.magnitude):
                j_other = self._backend.eye(size)
            else:
                j_other = self._backend.eye(size)

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
        # --- FAST PATH ---
        if type(other) is Quantity and self.unit is other.unit:
            new_magnitude = self._backend.sub(self.magnitude, other.magnitude)
            if self._backend.is_array(new_magnitude):
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                # Broadcasting for self
                is_self_scalar = False
                if (
                    (
                        self._backend.shape(self.magnitude) == ()
                        or len(self._backend.shape(self.magnitude)) == 0
                    )
                    or hasattr(self.magnitude, "shape")
                    and self.magnitude.shape == (1,)
                ):
                    is_self_scalar = True

                if is_self_scalar:
                    j_self = self._backend.ones((size, 1))
                else:
                    j_self = self._backend.eye(size)

                # Broadcasting for subtraction
                is_other_scalar = False
                if isinstance(other, Quantity):
                    if (
                        self._backend.shape(other.magnitude) == ()
                        or len(self._backend.shape(other.magnitude)) == 0
                        or hasattr(other.magnitude, "shape")
                        and other.magnitude.shape == (1,)
                    ):
                        is_other_scalar = True

                if is_other_scalar:
                    j_other = self._backend.mul(
                        self._backend.ones((size, 1)), -1
                    )
                else:
                    j_other = self._backend.mul(self._backend.eye(size), -1)

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
            size = 1
            for d in self._backend.shape(new_magnitude):
                size *= d
            j_self = self._backend.eye(size)
            j_other = self._backend.mul(self._backend.eye(size), -1)

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
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                if self._backend.is_array(other):
                    # flatten
                    other_flat = self._backend.reshape(other, (size,))
                    j_self = self._backend.diags([other_flat], [0])
                else:
                    j_self = self._backend.mul(self._backend.eye(size), other)

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
                size = 1
                for d in self._backend.shape(new_magnitude):
                    size *= d

                # Prepare broadcasted values flattened to (size,)
                # Note: This relies on backend to handle broadcasting in existing mul op,
                # here we need explicit flat arrays for Jacobian construction.

                # Flatten/Broadcast other (coefficient for j_self)
                if self._backend.is_array(other.magnitude):
                    if self._backend.shape(other.magnitude) == () or (
                        hasattr(other.magnitude, "shape")
                        and other.magnitude.shape == (1,)
                    ):
                        other_val = (
                            other.magnitude.item()
                            if hasattr(other.magnitude, "item")
                            else other.magnitude
                        )
                        other_flat = self._backend.mul(
                            self._backend.ones(size), other_val
                        )
                    elif self._backend.shape(
                        other.magnitude
                    ) == self._backend.shape(new_magnitude):
                        other_flat = self._backend.reshape(
                            other.magnitude, (size,)
                        )
                    else:
                        # Complex broadcast? Fallback to ones*val if it implicitly broadcasted?
                        # Or assume correct shape.
                        other_flat = self._backend.reshape(
                            other.magnitude, (size,)
                        )  # Simplistic
                else:
                    other_flat = self._backend.mul(
                        self._backend.ones(size), other.magnitude
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
                    # If backend returns dense, it's fine. If sparse required, we might need conversion.
                    # But CovarianceStore handles dense too.
                else:
                    j_self = self._backend.diags([other_flat], [0])

                # Flatten/Broadcast self (coefficient for j_other)
                if self._backend.is_array(self.magnitude):
                    if self._backend.shape(self.magnitude) == () or (
                        hasattr(self.magnitude, "shape")
                        and self.magnitude.shape == (1,)
                    ):
                        self_val = (
                            self.magnitude.item()
                            if hasattr(self.magnitude, "item")
                            else self.magnitude
                        )
                        self_flat = self._backend.mul(
                            self._backend.ones(size), self_val
                        )
                    elif self._backend.shape(
                        self.magnitude
                    ) == self._backend.shape(new_magnitude):
                        self_flat = self._backend.reshape(
                            self.magnitude, (size,)
                        )
                    else:
                        self_flat = self._backend.reshape(
                            self.magnitude, (size,)
                        )
                else:
                    self_flat = self._backend.mul(
                        self._backend.ones(size), self.magnitude
                    )

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
                    j_other = self._backend.diags([self_flat], [0])

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
        # Check for zero.
        if self._backend.any(self._backend.allclose(self.magnitude, 0)):
            raise ZeroDivisionError("Division by zero magnitude Quantity")

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
            Self,
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
            Self,
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
        elif ufunc == np.subtract:
            val = inputs[1] if inputs[0] is self else inputs[0]
            if inputs[0] is self:
                return self.__sub__(val)
            else:
                return self.__rsub__(val)
        elif ufunc == np.multiply:
            return self.__mul__(inputs[1] if inputs[0] is self else inputs[0])
        elif ufunc == np.true_divide:
            if inputs[0] is self:
                return self.__truediv__(inputs[1])
            else:
                return self.__rtruediv__(inputs[0])
        elif ufunc == np.power:
            if inputs[0] is self:
                return self.__pow__(inputs[1])

        # Unary math that changes unit
        if ufunc == np.sqrt:
            return self**0.5
        elif ufunc == np.square:
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
