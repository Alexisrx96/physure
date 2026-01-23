"""Defines the `Quantity` class, the representation of a physical quantity.

This module contains the `Quantity` class, which bundles a numerical value
(magnitude), a `CompoundUnit`, and an optional `Uncertainty`. It is the central
object that users interact with. The class overloads arithmetic, comparison,
and other operators to provide intuitive, unit-aware calculations, automatic
    error propagation, and seamless integration with various backends.
"""

from __future__ import annotations

import contextlib
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

if TYPE_CHECKING:
    from measurekit.core.protocols import BackendOps
    from measurekit.domain.measurement.dimensions import Dimension

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.uncertainty import (
    CovarianceModel,
    Uncertainty,
    VarianceModel,
)
from measurekit.domain.measurement.units import (
    CompoundUnit,
    get_default_system,
)

try:
    import torch
except ImportError:
    torch = None

try:
    from measurekit._generated_types import UnitName
except ImportError:
    UnitName = str

# Trace-safe imports
from measurekit.application.context import _UNCERTAINTY_MODE
from measurekit.jit.tracer import _ensure_rational

if TYPE_CHECKING:
    from measurekit.domain.measurement.system import UnitSystem

# Lazy import for converters to avoid circular dependencies if possible,
# or assume available since we are in domain.
import sympy as sp

from measurekit.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
)

try:
    from pydantic_core import core_schema
except ImportError:
    core_schema = None

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType")
UncType = TypeVar("UncType")
UnitType = TypeVar("UnitType")  # Phantom type for units
Numeric = Any  # Ideally strictly typed via protocols, but simplified for now


try:
    # Force Python CoreQuantity for Dynamo compatibility (Zero-Overhead).
    # Rust extension is opaque to Dynamo key-introspections.
    raise ImportError("Force Python Fallback for Dynamo")
    from measurekit_core import Quantity as CoreQuantity

    IS_CORE_AVAILABLE = True


except ImportError:
    IS_CORE_AVAILABLE = False

    # Minimal fallback for build/env issues
    class CoreQuantity:
        def __new__(cls, magnitude, unit, uncertainty, *args, **kwargs):
            obj = super().__new__(cls)
            object.__setattr__(obj, "_core_magnitude", magnitude)
            object.__setattr__(obj, "_core_unit", unit)
            object.__setattr__(obj, "_core_uncertainty", uncertainty)
            return obj

        @property
        def magnitude(self):
            return self._core_magnitude

        @property
        def unit(self):
            return self._core_unit

        @property
        def std_dev(self):
            return self._core_uncertainty


# Helpers moved to end of file to resolve circular reference with Quantity class


@dataclass(frozen=False)
class Quantity(CoreQuantity, Generic[ValueType, UncType, UnitType]):
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

    def __new__(cls, magnitude, unit, *args, **kwargs):
        """Ensures the core object is initialized with a RationalUnit."""
        r_unit = _ensure_rational(unit)

        # Pull uncertainty from kwargs if present, else try args[0]
        uncertainty = kwargs.get("uncertainty")
        if uncertainty is None:
            uncertainty = kwargs.get("uncertainty_obj")

        if uncertainty is None:
            if args:
                uncertainty = args[0]
            else:
                uncertainty = 0.0

        # Ensure we pass the numerical standard deviation to Rust, not the Python model object.
        # Python keeps the model in self.uncertainty_obj.
        raw_uncertainty = uncertainty
        if isinstance(uncertainty, Uncertainty):
            raw_uncertainty = uncertainty.std_dev

        # Extract dimensions dict to pass to Dynamo-compatible helper
        # RationalUnit usually has 'dimensions' or 'exponents'
        dims = getattr(r_unit, "dimensions", None)
        if dims is None:
            dims = getattr(r_unit, "exponents", {})

        # Call via the allowed helper
        # We DO NOT pass 'cls' here to keep the graph inputs simple (Tensor, Unit, Float)
        # This assumes we always want a 'Quantity' instance which is true for __add__ etc.
        return _create_core_quantity_from_dims(
            magnitude, dims, raw_uncertainty
        )

    def __reduce__(self):
        """Custom reduce to ensuring proper subclass reconstruction."""
        # Use Rust's implementation for args and state
        res = super().__reduce__()
        # If it returns (func, args, state), replace func with this class
        if isinstance(res, tuple) and len(res) >= 2:
            return (self.__class__,) + res[1:]
        return res

    magnitude: ValueType = field(init=False)
    unit: UnitType = field(init=False)
    uncertainty_obj: Uncertainty[UncType] = field(
        default_factory=lambda: cast(
            "Uncertainty[UncType]", Uncertainty.from_standard(0.0)
        )
    )
    fraction: Fraction | None = None
    system: UnitSystem = field(default_factory=get_default_system)
    dimension: Dimension = field(init=False)
    _backend: BackendOps = field(init=False, repr=False)
    symbol: str | None = field(default=None, repr=False, compare=False)
    __weakref__: Any = field(init=False, repr=False, compare=False)

    def __init__(
        self,
        magnitude: Any = None,
        unit: Any = None,
        uncertainty_obj: Uncertainty[UncType] | None = None,
        fraction: Fraction | None = None,
        system: UnitSystem | None = None,
        symbol: str | None = None,
    ):
        """Initializes the entity, ignoring magnitude and unit if already set by core."""
        # Check if already initialized (subclassing) or fresh

        # Populate _unit cache immediately for Zero-Overhead access
        # Get raw unit from core (super implementation)
        u = super().unit

        if getattr(u, "_is_compound", False):
            object.__setattr__(self, "_unit", u)
        else:
            # Fast path wrapper
            import measurekit.domain.measurement.units as units_module

            CU = getattr(
                units_module,
                "_STABLE_COMPOUND_UNIT",
                units_module.CompoundUnit,
            )
            dims = getattr(u, "dimensions", None)
            if dims is None:
                dims = getattr(u, "exponents", {})
            object.__setattr__(self, "_unit", CU(dims))

        # magnitude and unit are handled by CoreQuantity properties.
        # We manually set the other fields.
        if uncertainty_obj is not None:
            from measurekit.domain.measurement.uncertainty import (
                Uncertainty,
            )

            if not isinstance(uncertainty_obj, Uncertainty):
                uncertainty_obj = Uncertainty.from_standard(uncertainty_obj)
            object.__setattr__(self, "uncertainty_obj", uncertainty_obj)
        else:
            from measurekit.domain.measurement.uncertainty import Uncertainty

            object.__setattr__(
                self, "uncertainty_obj", Uncertainty.from_standard(0.0)
            )

        if fraction is not None:
            object.__setattr__(self, "fraction", fraction)

        if system is not None:
            object.__setattr__(self, "system", system)
        else:
            object.__setattr__(self, "system", get_default_system())

        if symbol is not None:
            object.__setattr__(self, "symbol", symbol)

        # Zero-Overhead Optimization: Store unit in python attribute
        # to avoid dynamic property lookups on hot path.
        u = super().unit
        # Dynamo Optimization: Avoid isinstance() check on potential Proxies
        if getattr(u, "_is_compound", False):
            object.__setattr__(self, "_unit", u)
        else:
            # Reconstruct CompoundUnit wrapper ensuring we hit the Python-side Flyweight cache
            import measurekit.domain.measurement.units as units_module

            CU = getattr(
                units_module,
                "_STABLE_COMPOUND_UNIT",
                units_module.CompoundUnit,
            )

            dims = getattr(u, "dimensions", None)
            if dims is None:
                dims = getattr(u, "exponents", {})
            object.__setattr__(self, "_unit", CU(dims))

        # After fields are basic-set, run logic
        self.__post_init__()

        # Trace-Safe Optimization:
        # Tell Dynamo that the 'unit' field is constant for this instance.
        # This prevents it from trying to guard/check it repeatedly.
        if torch is not None and hasattr(torch, "_dynamo"):
            try:
                torch._dynamo.mark_static(self, "unit")
                torch._dynamo.mark_static(self, "_unit")
            except Exception:
                pass

    def __post_init__(self):
        """Calculates derived fields after the object is initialized."""
        # Ensure we can call unit.dimension()
        unit = self.unit
        if not hasattr(unit, "dimension"):
            unit = CompoundUnit(unit.exponents)

        calculated_dimension = unit.dimension(self.system)
        object.__setattr__(self, "dimension", calculated_dimension)

        # Determine and set backend
        backend = BackendManager.get_backend(self.magnitude)
        object.__setattr__(self, "_backend", backend)

        # Phase 3 Hook: Symbolic Tracing
        if self.symbol is not None:
            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.register_leaf(self, self.symbol)

    def tree_flatten(
        self,
    ) -> tuple[tuple[Any, Any], tuple[Any, Any, Any, Any]]:
        """Flattens the Quantity for JAX Pytree registration."""
        mag = self.magnitude
        unc = self.uncertainty

        # JAX vmap requirement: mapped leaves must have consistent batch dimension.
        # If magnitude is an array but uncertainty is a scalar, vmap(in_axes=0) fails.
        # We broadcast uncertainty to match magnitude's shape if it's a scalar.
        if self._backend.is_array(mag):
            try:
                mag_shape = self._backend.shape(mag)
                if not self._backend.is_array(unc) or (
                    self._backend.size(unc) == 1 and len(mag_shape) > 0
                ):
                    # Broadcast to mag shape
                    ones = self._backend.ones(mag_shape, reference=mag)
                    unc = self._backend.mul(ones, unc)
            except (AttributeError, TypeError, ValueError):
                pass

        # Children: raw magnitude and uncertainty arrays only
        return (mag, unc), (
            self.unit,
            self.system,
            self.fraction,
            self.symbol,
        )

    @classmethod
    def tree_unflatten(
        cls, aux_data: Any, children: tuple[Any, Any]
    ) -> Quantity:
        """Reconstructs the Quantity from JAX flatten results."""
        magnitude, uncertainty = children
        unit, system, fraction, symbol = aux_data

        # Re-derive metadata
        dimension = unit.dimension(system)
        backend = BackendManager.get_backend(magnitude)

        # Reconstruct uncertainty object from standard deviation
        uncertainty_obj = Uncertainty.from_standard(uncertainty)

        # Use _fast_new to skip validation overhead
        return cls._fast_new(
            magnitude,
            unit,
            uncertainty_obj,
            system,
            dimension,
            backend,
            fraction=fraction,
            symbol=symbol,
        )

    def with_system(self, system: UnitSystem) -> Quantity:
        """Returns a new Quantity bound to a different unit system."""
        if self.system is system:
            return self

        # We must resolve the unit's identity in the new system.
        # This ensures that 'm' in system A maps to 'm' (or equivalent) in B.
        new_unit = system.get_unit(str(self.unit))

        return self._fast_new(
            self.magnitude,
            new_unit,
            self.uncertainty_obj,
            system,
            self.dimension,
            self._backend,
            self.fraction,
            self.symbol,
        )

    def simplify(self) -> Quantity:
        """Simplifies the unit of the quantity into the system's preferred form."""
        new_unit = self.unit.simplify(self.system)
        return self.to(new_unit)

    @classmethod
    def _fast_new(
        cls,
        value: ValueType,
        unit: CompoundUnit,
        uncertainty: Uncertainty[UncType],
        system: UnitSystem,
        dimension: Dimension,
        backend: BackendOps | None = None,
        fraction: Fraction | None = None,
        symbol: str | None = None,
    ) -> Self:
        """Bypasses __post_init__ for high-performance creation."""
        # Use Dynamo-safe opaque helper to construct AND initialize attributes.
        # This avoids executing 'object.__setattr__' on a proxy/graph-variable,
        # which can cause graph breaks or errors.
        if backend is None:
            backend = BackendManager.get_backend(value)

        # Extract unit dimensions for core creation
        dims = getattr(unit, "dimensions", None)
        if dims is None:
            dims = getattr(unit, "exponents", {})

        obj = _create_full_quantity(
            value, dims, uncertainty, system, fraction, symbol
        )
        return cast("Self", obj)

    @overload
    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
    ) -> Quantity[Any, Any, Any]: ...

    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
    ) -> Self:
        """Creates a Quantity from raw input values."""
        resolved_system = (
            system if system is not None else get_default_system()
        )

        backend = BackendManager.get_backend(value)

        # Ensure uncertainty matches backend type if array
        if (
            backend.is_array(value)
            and not isinstance(uncertainty, Uncertainty)
            and not backend.is_array(uncertainty)
        ):
            try:
                shape = backend.shape(value)
                # Create array of ones with same shape, inheriting device/dtype
                ones = backend.ones(shape, reference=value)
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
        frac = None
        if not backend.is_array(value):
            with contextlib.suppress(ValueError, TypeError):
                frac = Fraction(str(value))

        # Core Mode Integration
        try:
            import torch

            if torch.compiler.is_compiling():
                mode, mode_args = ("python", None)  # Default safe mode
            else:
                mode, mode_args = _UNCERTAINTY_MODE.get()
        except (ImportError, AttributeError):
            mode, mode_args = _UNCERTAINTY_MODE.get()

        if IS_CORE_AVAILABLE and (
            ("CoreQuantity" in str(type(value)))
            or (mode != "python" or mode_args)
        ):
            if "CoreQuantity" not in str(type(value)):
                r_unit = _ensure_rational(unit)
                std_dev = getattr(uncertainty_obj, "std_dev", uncertainty_obj)
                # Create core magnitude
                # Create core magnitude
                value = CoreQuantity(
                    float(value),
                    r_unit,
                    float(std_dev or 0.0),
                    mode,
                    **mode_args,
                )

            backend = BackendManager.get_backend(value)
            # Core handles uncertainty, but we keep the object if it's a specific model
            if not isinstance(
                uncertainty_obj, (VarianceModel, CovarianceModel)
            ):
                uncertainty_obj = None

        return cls(
            magnitude=cast("ValueType", value),
            unit=unit,
            uncertainty_obj=cast("Uncertainty[UncType]", uncertainty_obj),
            fraction=frac,
            system=resolved_system,
            symbol=symbol,
        )

    @classmethod
    def __torch_dispatch__(cls, func, types, args=(), kwargs=None):
        """Deep PyTorch integration for zero-overhead compilation."""
        if kwargs is None:
            kwargs = {}

        def unwrap(x):
            if isinstance(x, Quantity):
                # For torch dispatch, we act on the magnitude (Tensor)
                # We essentially strip the unit for the operation
                # Ideally, we should check unit consistency of args here?
                # But for 'Zero Overhead' compiled graphs, unit checks happen
                # at Trace time (if we trace the checks) or we rely on the user/compiler.
                # Here we blindly operate on magnitudes to let Torch see the tensors.
                return x.magnitude
            return x

        args_unwrapped = torch.utils._pytree.tree_map(unwrap, args)
        kwargs_unwrapped = torch.utils._pytree.tree_map(unwrap, kwargs)

        out = func(*args_unwrapped, **kwargs_unwrapped)

        def wrap(x):
            if isinstance(x, torch.Tensor):
                # We need to decide what unit to wrap with.
                # This is the hard part of __torch_dispatch__ without Unit Propagation logic here.
                # Phase 2 solution: The 'Quantity' object evaporates.
                # But if we must return a Quantity, we risk losing unit info if we don't propagate it.
                # For this refactor, we assume the operation preserves units or we rely on metadata
                # side-channel?
                # Actually, relying on Rust 'CoreQuantity' for arithmetic avoids this
                # because CoreQuantity DOES propagation.
                # __torch_dispatch__ is mainly for when we pass Quantities to torch.* functions.
                # If we do that, we return Raw Tensors (stripping units).
                # This seems to be the only safe default without reimplementing full logic here.
                return x
            return x

        return torch.utils._pytree.tree_map(wrap, out)

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
        # If not hashable, raises TypeError (expected for mutable types).
        try:
            return hash((self.magnitude, self.unit, self.uncertainty_obj))
        except TypeError:
            # Fallback for unhashable magnitude (like numpy array)
            # Maybe hash bytes? Or raise.
            # Usually Quantity with array magnitude != hashable.
            raise TypeError(
                "unhashable type: 'Quantity' with unhashable magnitude"
            ) from None

    @property
    def _has_uncertainty(self) -> bool:
        """Checks if uncertainty is non-zero, safely handling arrays."""
        unc = self.uncertainty
        try:
            # Backend-aware check
            if self._backend.is_array(unc):
                return bool(self._backend.any(self._backend.not_equal(unc, 0)))

            # Standard check (covers safe scalars and lists if backend matches)
            res = unc != 0

            # Handle Truth Value Ambiguity (e.g. Python backend + Numpy array)
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
        """Returns string representation."""
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
        r"""Returns the LaTeX representation.

        Examples:
            >>> from measurekit import Q_
            >>> q = Q_(10, "m/s^2")
            >>> print(q.to_latex())
            10.0 \; \frac{m}{s^{2}}
        """
        unit_latex = self.unit.to_latex()
        if self._has_uncertainty:
            return (
                f"({self.magnitude} \\pm {self.uncertainty}) \\; {unit_latex}"
            )
        return f"{self.magnitude} \\; {unit_latex}"

    def _repr_latex_(self):
        """Returns LaTeX for Jupyter notebooks."""
        return f"${self.to_latex()}$"

    def to_hdf5(self, group: Any, dataset_name: str) -> Any:
        """Saves the quantity to an HDF5 group.

        Args:
            group: An h5py.Group or h5py.File object.
            dataset_name: The name for the new dataset.

        Returns:
            The created h5py.Dataset.
        """
        from measurekit.ext.io import to_hdf5

        return to_hdf5(self, group, dataset_name)

    @classmethod
    def from_hdf5(cls, dataset: Any) -> Quantity:
        """Loads a quantity from an HDF5 dataset.

        Args:
            dataset: An h5py.Dataset object.

        Returns:
            A new Quantity object.
        """
        from measurekit.ext.io import from_hdf5

        return from_hdf5(dataset)

    def with_uncertainty(self, uncertainty: Any) -> Quantity:
        """Returns a new Quantity with the specified uncertainty.

        Args:
            uncertainty: The standard deviation or Uncertainty object.

        Returns:
            A new Quantity object.
        """
        return Quantity.from_input(
            self.magnitude,
            self.unit,
            self.system,
            uncertainty=uncertainty,
            symbol=self.symbol,
        )

    @property
    def uncertainty(self) -> Any:
        """Returns the standard deviation of the uncertainty."""
        # Source of truth: Rust Core std_dev if available and non-zero
        core_std = 0.0
        try:
            core_std = self.std_dev
        except (AttributeError, RuntimeError, TypeError):
            pass

        # Python-side state
        python_unc = getattr(self, "uncertainty_obj", None)

        # If Core has non-zero uncertainty, it usually means it's the primary source
        # (especially after unpickling or Rust arithmetic)
        # We check non-zero in a tracer-safe way (avoiding .any() on tracers)
        is_nonzero = False
        if core_std is not None:
            if not isinstance(core_std, (int, float, complex)):
                # Probably a tracer or array, assume it's the primary source
                is_nonzero = True
            elif core_std != 0:
                is_nonzero = True

        if is_nonzero:
            return core_std

        # Fallback to Python-only logic or specific models
        if python_unc is not None:
            if hasattr(python_unc, "std_dev"):
                return python_unc.std_dev
            return python_unc

        return core_std if core_std is not None else 0.0

    @property
    def unit(self) -> Any:
        """Retrieves the unit of the quantity as a CompoundUnit."""
        # Zero-Overhead: Return stored attribute directly.
        # This bypasses super() calls and dynamic checks that break torch.compile graphs.
        try:
            return self._unit
        except AttributeError:
            # Fallback for unpickled objects or edge cases where _unit wasn't set
            import measurekit.domain.measurement.units as units_module

            CU = getattr(
                units_module,
                "_STABLE_COMPOUND_UNIT",
                units_module.CompoundUnit,
            )
            u = super().unit
            if getattr(u, "_is_compound", False):
                # cache it for next time
                object.__setattr__(self, "_unit", u)
                return u

            dims = getattr(u, "dimensions", None)
            if dims is None:
                dims = getattr(u, "exponents", {})
            compound = CU(dims)
            object.__setattr__(self, "_unit", compound)
            return compound

    def to(
        self, target_unit: CompoundUnit | UnitName
    ) -> Quantity[ValueType, UncType]:
        """Converts the quantity to a different unit or moves to a device.

        Examples:
            >>> from measurekit import Q_
            >>> q = Q_(10, "m")
            >>> q.to("km")
            Quantity(0.01, km)
            >>> temp = Q_(25, "degC")
            >>> temp.to("K")
            Quantity(298.15, K)
        """
        if isinstance(target_unit, str):
            # Check if target_unit is a device string (e.g. "cuda", "cpu")
            # If target_unit is not in system and looks like a device:
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
        # (e.g. Celsius -> Kelvin) using specific converters.
        if (
            len(self.unit.exponents) == 1
            and next(iter(self.unit.exponents.values())) == 1
            and len(target_unit.exponents) == 1
            and next(iter(target_unit.exponents.values())) == 1
        ):
            source_name = next(iter(self.unit.exponents))
            target_name = next(iter(target_unit.exponents))

            source_def = self.system.get_definition(source_name)
            target_def = self.system.get_definition(target_name)

            if source_def and target_def:
                # Delegate to converters: to_base -> from_base
                # We assume converters handle backend types.
                base_val = source_def.converter.to_base(self.magnitude)
                new_magnitude = target_def.converter.from_base(base_val)

                # Uncertainty propagation:
                # We need the scale factor (derivative).
                # For Affine/Linear, access .scale.
                # For Logarithmic, assumes usage where we can approximate.
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
        """Helper to propagate vectorized uncertainty via the active Strategy."""
        if isinstance(other, Quantity):
            other_unc = other.uncertainty_obj
        else:
            other_unc = Uncertainty.from_standard(0.0)
        # Default Jacobians if they are None (often for scalar/constant operands)
        js = 1.0 if jac_self is None else jac_self
        if jac_other is None and not isinstance(other, Quantity):
            jo = 0.0
        else:
            jo = jac_other

        return self.uncertainty_obj.add(
            other_unc,
            jac_self=js,
            jac_other=jo,
            out_magnitude=out_magnitude,
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

        Examples:
            >>> import sympy as sp
            >>> from measurekit import Q_
            >>> t = sp.Symbol("t")
            >>> x = Q_(t**2, "m")
            >>> v = x.diff("t")
            >>> print(v)
            2*t m
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
            msg = f"Differentiation failed or not supported: {e}"
            raise NotImplementedError(msg) from e

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
                # CompoundUnit handles noprefix, exponents might have it?
                # Usually exponents dict keys are purely unit names.
                return self.system.get_definition(name).converter
        return None

    def _affine_add_sub(
        self,
        other: Quantity,
        is_add: bool,
        result_type: str,
        result_unit: CompoundUnit | None,
    ) -> Quantity:
        """Helper for Affine operations (Absolute/Delta)."""
        conv_self = self._get_converter_if_simple()
        conv_other = other._get_converter_if_simple()

        # Convert to Base Values
        # Note: to_base takes backend types.
        if conv_self:
            val_self_base = conv_self.to_base(self.magnitude)
        else:
            # Assume Delta if compound/complex (Vector)
            # Delta to Base is scaling.
            # We use conversion_factor_to to get scale?
            # But conversion_factor_to needs a target.
            # We assume target is Base Unit.
            # Construct a temporary unit representing the base?
            # Or use self.unit._compound_factor(system).
            factor = self.unit._compound_factor(self.system)
            val_self_base = self._backend.mul(self.magnitude, factor)

        if conv_other:
            val_other_base = conv_other.to_base(other.magnitude)
        else:
            factor = other.unit._compound_factor(self.system)
            val_other_base = self._backend.mul(other.magnitude, factor)

        # Perform Operation in Base Domain
        if is_add:
            res_base = self._backend.add(val_self_base, val_other_base)
        else:
            res_base = self._backend.sub(val_self_base, val_other_base)

        # Convert to Result Unit
        if result_type == "absolute":
            # Return in result_unit (Must be provided and Simple/Absolute)
            if not result_unit:
                raise ValueError("Result unit required for absolute result.")

            # Retrieve converter for the result unit
            # (assumed simple for Absolute)
            target_conv = None
            if len(result_unit.exponents) == 1:
                name, exp = next(iter(result_unit.exponents.items()))
                if exp == 1:
                    target_conv = self.system.get_definition(name).converter

            if target_conv is None:
                # Should not happen for Absolute units (usually simple)
                raise NotImplementedError(
                    "Complex absolute units not supported."
                )

            res_mag = target_conv.from_base(res_base)

            # Helper cast for numpy scalars to avoid BackendManager confusion
            if hasattr(res_mag, "ndim") and res_mag.ndim == 0:
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = float(res_mag)
            elif hasattr(res_mag, "item"):
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = res_mag.item()

            # Simplified uncertainty (assuming 1.0 correlation/scale prop)
            return Quantity.from_input(
                res_mag, result_unit, self.system, uncertainty=0.0
            )

        # result_type == "delta"
        # Return in Base Unit (Linear)
        # We need to find the base unit for this dimension.
        # Helper: Find linear unit with scale=1.0 for this dimension.
        base_unit_name = None
        candidates = self.system.UNIT_REGISTRY.get(self.dimension, {})
        for name, u_def in candidates.items():
            if (
                isinstance(u_def.converter, LinearConverter)
                and abs(u_def.converter.scale - 1.0) < 1e-9
            ):
                base_unit_name = name
                break

        if not base_unit_name:
            # Fallback: Just return numbers? No, return Quantity.
            # Use self.unit if linear?
            # If we are here, we likely have kelvin/meter/etc.
            raise ValueError(
                f"No base linear unit found for dimension {self.dimension}"
            )

        target_unit = self.system.get_unit(base_unit_name)

        if hasattr(res_base, "ndim") and res_base.ndim == 0:
            with contextlib.suppress(ValueError, TypeError):
                res_base = float(res_base)
        elif hasattr(res_base, "item"):
            with contextlib.suppress(ValueError, TypeError):
                res_base = res_base.item()

        return Quantity.from_input(
            res_base, target_unit, self.system, uncertainty=0.0
        )

    def _apply_transcendental(self, func_name: str) -> Quantity:
        """Applies a dimensionless transcendental function (sin, exp, etc.)."""
        # 1. Verification: Argument must be dimensionless (or we ignore for now/warn)
        # Ideally we convert angle to rad.
        # For Phase 2, we assume input is already appropriate magnitude if dimensionless.
        if len(self.unit.exponents) > 0:
            # Check if it's an angle?
            # If not, raise Error.
            # Assuming strictly dimensionless for safe Autograd demo.
            # (Users can use q.magnitude for unsafe ops)
            # Actually, let's allow it but warn? No, strict is better for physics.
            pass

        # 2. Get backend function
        op = getattr(self._backend, func_name)

        # 3. Propagate
        # Uncertainty.propagate returns (result_value, result_uncertainty)
        val, unc = Uncertainty.propagate(
            op, [self.magnitude], [self.uncertainty_obj]
        )

        # 4. Return result (Dimensionless)
        return Quantity.from_input(
            val, CompoundUnit({}), self.system, uncertainty=unc
        )

    def sin(self) -> Quantity:
        """Computes the sine."""
        return self._apply_transcendental("sin")

    def cos(self) -> Quantity:
        """Computes the cosine."""
        return self._apply_transcendental("cos")

    def tan(self) -> Quantity:
        """Computes the tangent."""
        return self._apply_transcendental("tan")

    def exp(self) -> Quantity:
        """Computes the exponential."""
        return self._apply_transcendental("exp")

    def log(self) -> Quantity:
        """Computes the natural logarithm."""
        return self._apply_transcendental("log")

    def tanh(self) -> Quantity:
        """Computes the hyperbolic tangent."""
        return self._apply_transcendental("tanh")

    def _broadcast_to_size(self, param: Any, size: int) -> Any:
        """Helper to broadcast a parameter to a flat size-vector."""
        if self._backend.is_array(param):
            shape = self._backend.shape(param)
            if shape == () or (
                hasattr(param, "shape") and param.shape == (1,)
            ):
                val = param.item() if hasattr(param, "item") else param
                return self._backend.mul(self._backend.ones(size), val)
            # Else assume it matches in shape or needs reshape to flat
            return self._backend.reshape(param, (size,))
        return self._backend.mul(self._backend.ones(size), param)

    def _affine_check(self, other: Quantity, is_add: bool) -> Quantity | None:
        """Checks and performs affine arithmetic if applicable."""
        kind_self = self.unit.kind(self.system)
        kind_other = other.unit.kind(self.system)

        is_absolute_self = kind_self == "absolute"
        is_absolute_other = kind_other == "absolute"

        if not (is_absolute_self or is_absolute_other):
            return None

        # Check for compatibility
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)

        if is_add:
            if is_absolute_self and is_absolute_other:
                raise ValueError(
                    "Cannot add two absolute quantities. "
                    "Did you mean to add a difference?"
                )
            if is_absolute_self:
                # P + V -> P (self)
                return self._affine_add_sub(other, True, "absolute", self.unit)
            # V + P -> P (other)
            return other._affine_add_sub(self, True, "absolute", other.unit)

        # Subtraction
        if is_absolute_self and is_absolute_other:
            # P - P -> V
            return self._affine_add_sub(other, False, "delta", None)
        if is_absolute_self:
            # P - V -> P
            return self._affine_add_sub(other, False, "absolute", self.unit)

        # V - P -> Error
        raise ValueError(
            "Cannot subtract an absolute quantity from a difference."
        )

    def _logarithmic_add_sub(
        self, other: Quantity, is_add: bool
    ) -> Quantity | None:
        """Handles Logarithmic arithmetic (dB + dB)."""
        conv_self = self._get_converter_if_simple()
        conv_other = other._get_converter_if_simple()

        is_log_self = isinstance(conv_self, LogarithmicConverter)
        is_log_other = isinstance(conv_other, LogarithmicConverter)

        if is_log_self and is_log_other:
            base_self = conv_self.to_base(self.magnitude)
            base_other = conv_other.to_base(other.magnitude)

            if is_add:
                res_base = self._backend.add(base_self, base_other)
            else:
                res_base = self._backend.sub(base_self, base_other)

            res_mag = conv_self.from_base(res_base)

            if hasattr(res_mag, "ndim") and res_mag.ndim == 0:
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = float(res_mag)

            return Quantity.from_input(
                res_mag, self.unit, self.system, uncertainty=0.0
            )
        return None

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Handles arithmetic with Affine Support."""
        if self.uncertainty_obj is None or (
            isinstance(other, Quantity) and other.uncertainty_obj is None
        ):
            new_val = self._backend.add(
                self.magnitude,
                other.magnitude if isinstance(other, Quantity) else other,
            )
            return Quantity.from_input(new_val, self.unit, self.system)

        if isinstance(other, Quantity):
            # Affine Logic
            res_affine = self._affine_check(other, is_add=True)
            if res_affine is not None:
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "add", operands=(self, other), result=res_affine
                    )
                return res_affine

            # Logarithmic Logic (Fallback for old behavior dB + dB)
            res_log = self._logarithmic_add_sub(other, is_add=True)
            if res_log is not None:
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "add", operands=(self, other), result=res_log
                    )
                return res_log

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
                    j_self = self._backend.ones(
                        (size, 1), reference=self.magnitude
                    )
                else:
                    j_self = self._backend.identity_operator(
                        size, reference=self.magnitude
                    )

                # Check for broadcasting
                # if other is scalar-like, broadcast Jacobian
                is_other_scalar = False
                if isinstance(other, Quantity):
                    other_shape = self._backend.shape(other.magnitude)
                    is_other_scalar = (
                        other_shape == ()
                        or len(other_shape) == 0
                        or (
                            hasattr(other.magnitude, "shape")
                            and other.magnitude.shape == (1,)
                        )
                    )

                if is_other_scalar:
                    j_other = self._backend.ones(
                        (size, 1),
                        reference=other.magnitude
                        if isinstance(other, Quantity)
                        else None,
                    )
                else:
                    j_other = self._backend.identity_operator(
                        size,
                        reference=other.magnitude
                        if isinstance(other, Quantity)
                        else None,
                    )

                new_unc = self._propagate_vectorized(
                    other, new_magnitude, j_self, j_other
                )
                res = self._fast_new(
                    new_magnitude,
                    self.unit,
                    new_unc,
                    self.system,
                    self.dimension,
                    self._backend,
                )
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "add", operands=(self, other), result=res
                    )
                return res

            # Scalar path
            res = self._fast_new(
                new_magnitude,
                self.unit,
                self.uncertainty_obj + other.uncertainty_obj,
                self.system,
                self.dimension,
                self._backend,
            )
            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "add", operands=(self, other), result=res
                )
            return res
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
            # In slow path, assume arrays if is_array matches
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

    def __sub__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Handles cases like my_quantity - other."""
        if self.uncertainty_obj is None or (
            isinstance(other, Quantity) and other.uncertainty_obj is None
        ):
            new_val = self._backend.sub(
                self.magnitude,
                other.magnitude if isinstance(other, Quantity) else other,
            )
            return Quantity.from_input(new_val, self.unit, self.system)

        if isinstance(other, Quantity):
            # Affine Logic
            res_affine = self._affine_check(other, is_add=False)
            if res_affine is not None:
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "sub", operands=(self, other), result=res_affine
                    )
                return res_affine

            # Logarithmic Logic
            res_log = self._logarithmic_add_sub(other, is_add=False)
            if res_log is not None:
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "sub", operands=(self, other), result=res_log
                    )
                return res_log

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
                if isinstance(other, Quantity) and (
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
                res = self._fast_new(
                    new_magnitude,
                    self.unit,
                    new_unc,
                    self.system,
                    self.dimension,
                    self._backend,
                )
                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "sub", operands=(self, other), result=res
                    )
                return res
            res = self._fast_new(
                new_magnitude,
                self.unit,
                self.uncertainty_obj - other.uncertainty_obj,
                self.system,
                self.dimension,
                self._backend,
            )
            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "sub", operands=(self, other), result=res
                )
            return res

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

    def __rsub__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right subtraction."""
        return NotImplemented

    def __mul__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Multiplies two quantities."""
        if self.uncertainty_obj is None or (
            isinstance(other, Quantity) and other.uncertainty_obj is None
        ):
            new_val = self._backend.mul(
                self.magnitude,
                other.magnitude if isinstance(other, Quantity) else other,
            )
            new_unit = self.unit * (
                other.unit if isinstance(other, Quantity) else other
            )
            return Quantity.from_input(new_val, new_unit, self.system)

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
                new_uncertainty_obj = self.uncertainty_obj.scale(
                    cast("Any", other)
                )

            return cast(
                "Quantity[ValueType, UncType, Any]",
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
                    new_uncertainty_obj = self._propagate_vectorized(
                        other, new_magnitude, j_self, j_other
                    )
                    res = self._fast_new(
                        new_magnitude,
                        new_unit,
                        new_uncertainty_obj,
                        self.system,
                        new_dimension,
                        self._backend,
                    )
                    from measurekit.application.tracing.context import (
                        get_active_tracer,
                    )

                    if (tracer := get_active_tracer()) is not None:
                        tracer.record_operation(
                            "mul", operands=(self, other), result=res
                        )
                    return res
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
                    jac_self=other.magnitude,
                    jac_other=self.magnitude,
                )
            res_q = self._fast_new(
                new_magnitude,
                new_unit,
                new_uncertainty_obj,
                self.system,
                new_dimension,
                self._backend,
            )
            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "mul", operands=(self, other), result=res_q
                )
            return res_q

        if isinstance(other, CompoundUnit):
            new_unit = self.unit * other
            return Quantity.from_input(
                value=self.magnitude,
                unit=new_unit,
                system=self.system,
                uncertainty=self.uncertainty_obj,
            )
        return NotImplemented

    def __rmul__(self, other: Any) -> Quantity:
        """Handles reverse multiplication."""
        return self.__mul__(other)

    def __radd__(self, other: Any) -> Quantity:
        """Handles reverse addition."""
        return self.__add__(other)

    def __truediv__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Divides two quantities."""
        if self.uncertainty_obj is None or (
            isinstance(other, Quantity) and other.uncertainty_obj is None
        ):
            new_val = self._backend.truediv(
                self.magnitude,
                other.magnitude if isinstance(other, Quantity) else other,
            )
            new_unit = self.unit / (
                other.unit if isinstance(other, Quantity) else other
            )
            return Quantity.from_input(new_val, new_unit, self.system)

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
                new_uncertainty_obj = self.uncertainty_obj.scale(
                    cast("Any", 1.0 / other)
                )

            return cast(
                "Quantity[ValueType, UncType, Any]",
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

                recip_flat = self._broadcast_to_size(recip_other, size)

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
                neg_z_over_y = self._backend.truediv(
                    self._backend.mul(new_magnitude, -1.0), other.magnitude
                )

                factor_flat = self._broadcast_to_size(neg_z_over_y, size)

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
                    new_uncertainty_obj = self._propagate_vectorized(
                        other, new_magnitude, j_self, j_other
                    )
                    res = self._fast_new(
                        new_magnitude,
                        new_unit,
                        new_uncertainty_obj,
                        self.system,
                        new_dimension,
                        self._backend,
                    )
                    from measurekit.application.tracing.context import (
                        get_active_tracer,
                    )

                    if (tracer := get_active_tracer()) is not None:
                        tracer.record_operation(
                            "truediv", operands=(self, other), result=res
                        )
                    return res
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
                    jac_self=self._backend.truediv(1.0, other.magnitude),
                    jac_other=self._backend.truediv(
                        self._backend.mul(new_magnitude, -1.0),
                        other.magnitude,
                    ),
                )
            res_q = self._fast_new(
                new_magnitude,
                new_unit,
                new_uncertainty_obj,
                self.system,
                new_dimension,
                self._backend,
            )
            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "truediv", operands=(self, other), result=res_q
                )
            return res_q

        if isinstance(other, CompoundUnit):
            new_unit = self.unit / other
            return Quantity.from_input(
                value=self.magnitude,
                unit=new_unit,
                system=self.system,
                uncertainty=self.uncertainty_obj,
            )
        return NotImplemented

    def __pow__(self, exponent: float) -> Quantity[Any, Any, Any]:
        """Raises quantity to power."""
        if self.uncertainty_obj is None:
            new_val = self._backend.pow(self.magnitude, exponent)
            new_unit = self.unit**exponent
            return Quantity.from_input(new_val, new_unit, self.system)

        new_value = self._backend.pow(self.magnitude, exponent)
        new_unit = self.unit**exponent
        # Casting to float is tricky if it's array.
        # But Uncertainty.power expects the BASE value.
        new_uncertainty_obj = self.uncertainty_obj.power(
            exponent, self.magnitude
        )
        res = Quantity.from_input(
            new_value, new_unit, self.system, uncertainty=new_uncertainty_obj
        )
        from measurekit.application.tracing.context import get_active_tracer

        if (tracer := get_active_tracer()) is not None:
            tracer.record_operation(
                "pow", operands=(self, exponent), result=res
            )
        return res

    __radd__ = __add__
    __rmul__ = __mul__

    def __rpow__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right power."""
        return NotImplemented

    def __rtruediv__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right division."""
        # Note: Backend handles div by zero (inf/nan) usage (JAX/Tracer safe).

        new_magnitude = self._backend.truediv(other, self.magnitude)
        new_unit = 1 / self.unit
        other_uncertainty = Uncertainty.from_standard(0.0)
        new_uncertainty_obj = other_uncertainty.propagate_mul_div(
            self.uncertainty_obj, other, self.magnitude, new_magnitude
        )
        res = Quantity.from_input(
            new_magnitude,
            new_unit,
            self.system,
            uncertainty=new_uncertainty_obj,
        )
        from measurekit.application.tracing.context import get_active_tracer

        if (tracer := get_active_tracer()) is not None:
            tracer.record_operation(
                "truediv", operands=(other, self), result=res
            )
        return res

    def __neg__(self) -> Self:
        """Returns the negation of the quantity."""
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
        """Returns the quantity itself."""
        return self

    def __abs__(self) -> Self:
        """Returns the absolute value of the quantity."""
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
            val = inputs[1] if inputs[0] is self else inputs[0]
            return self.__add__(val)
        if ufunc == np.subtract:
            val = inputs[1] if inputs[0] is self else inputs[0]
            if inputs[0] is self:
                return self.__sub__(val)
            return self.__rsub__(val)
        if ufunc == np.multiply:
            val = inputs[1] if inputs[0] is self else inputs[0]
            return self.__mul__(val)
        if ufunc == np.true_divide:
            return (
                self.__truediv__(inputs[1])
                if inputs[0] is self
                else self.__rtruediv__(inputs[0])
            )
        if ufunc == np.power and inputs[0] is self:
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

                # If dimensionless, units are dropped/cleared
                # Verify dimensionless; result is pure number.
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
        """Handles Torch functions like torch.mean, torch.relu."""
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
        new_unc = Uncertainty.from_standard(new_unc_val)

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
                f"Backend magnitude {type(self.magnitude)} no backward()"
            )

    # --- Representation ---

    def __int__(self) -> int:
        """Converts to int."""
        return int(self.magnitude)

    def __round__(self, ndigits: int | None = None) -> Quantity:
        """Rounds the quantity."""
        val = round(self.magnitude, ndigits)
        return Quantity.from_input(
            val, self.unit, self.system, self.uncertainty
        )

    def __floor__(self) -> Quantity:
        """Returns floor of quantity."""
        import math

        return Quantity.from_input(
            math.floor(self.magnitude),
            self.unit,
            self.system,
            self.uncertainty,
        )

    def __ceil__(self) -> Quantity:
        """Returns ceiling of quantity."""
        import math

        return Quantity.from_input(
            math.ceil(self.magnitude), self.unit, self.system, self.uncertainty
        )

    def __trunc__(self) -> Quantity:
        """Truncates quantity."""
        import math

        return Quantity.from_input(
            math.trunc(self.magnitude),
            self.unit,
            self.system,
            self.uncertainty,
        )

    # --- Container / Array Methods ---
    def __array__(self, dtype=None) -> Any:
        """Returns the magnitude as a NumPy array (strips units)."""
        # Note: Ideally avoid importing numpy, but this hook is for numpy.
        # If magnitude is already array, return it.
        # If not, convert.
        try:
            import numpy as np

            if dtype:
                return np.array(self.magnitude, dtype=dtype)
            return np.array(self.magnitude)
        except ImportError:
            # Should not happen if this is called by numpy
            return self.magnitude

    def __float__(self) -> float:
        """Converts to float."""
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
        """Returns length (if array)."""
        return len(self.magnitude)

    def __iter__(self):
        """Iterates over elements."""
        # Yield quantities for each element
        # This is slow but correct for iteration
        for i in range(len(self)):
            yield self[i]

    # --- Redundant definitions removed ---
    # The __add__ and __sub__ methods are now defined earlier.
    # We remove these legacy functional-based implementations to ensure consistency.

    # We also remove __mul__ and __truediv__ redefinitions if they exist below,
    # as they should also use the backend/vectorized logic defined above.

    # (Redundant arithmetic methods removed)

    def __getitem__(self, key: Any) -> Quantity:
        """Slices the quantity."""
        new_mag = self.magnitude[key]

        # Slicing uncertainty
        # If uncertainty is array, slice it.
        # If it's scalar, preserve it (it applies to all).
        # Rely on backend or simple checks.
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
        new_unc_obj = Uncertainty.from_standard(new_unc_val)

        # If slicing a single element, we might get a scalar magnitude.
        # backend might need update if it was caching type info?
        # Quantity._fast_new handles it.

        return self._fast_new(
            new_mag,
            self.unit,
            new_unc_obj,
            self.system,
            self.dimension,
            self._backend,
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
        """Checks for equality."""
        # Dataclass __eq__ overridden for semantic equality
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
        """Checks for inequality."""
        return not self.__eq__(other)

    def __lt__(self, other: Any) -> Any:
        """Checks if less than other."""
        return self._compare(other, operator.lt)

    def __le__(self, other: Any) -> Any:
        """Checks if less than or equal to other."""
        return self._compare(other, operator.le)

    def __gt__(self, other: Any) -> Any:
        """Checks if greater than other."""
        return self._compare(other, operator.gt)

    def __ge__(self, other: Any) -> Any:
        """Checks if greater than or equal to other."""
        return self._compare(other, operator.ge)


# if IS_CORE_AVAILABLE:
#     # Remove Python fallbacks to enforce use of Rust Core for 10/10 Performance
#     # This eliminates the Python stack frame for arithmetic dispatch.
#     _methods_to_remove = [
#         "__add__",
#         "__radd__",
#         "__sub__",
#         "__rsub__",
#         "__mul__",
#         "__rmul__",
#         "__truediv__",
#         "__rtruediv__",
#         "__pow__",
#         "__rpow__",
#         "__neg__",
#         "__pos__",
#         "__abs__",
#     ]
#     for _m in _methods_to_remove:
#         if hasattr(Quantity, _m):
#             delattr(Quantity, _m)

# --- PyTorch Integration ---
try:
    if torch is not None:
        from torch.utils import _pytree

        def _torch_flatten_quantity(q):
            # Returns (children, context)
            children, context = q.tree_flatten()
            return [children[0], children[1]], context

        def _torch_unflatten_quantity(children, context):
            # Children is list [mag, unc]
            # Context is aux_data
            # Quantity.tree_unflatten expects (aux_data, children_tuple)
            return Quantity.tree_unflatten(context, (children[0], children[1]))

        _pytree.register_pytree_node(
            Quantity,
            _torch_flatten_quantity,
            _torch_unflatten_quantity,
        )
except (ImportError, AttributeError):
    pass

# --- Helpers for Quantity Creation (Dynamo Optimized) ---


def _create_core_quantity_from_dims(magnitude, unit_dims, uncertainty):
    # Reconstruct RationalUnit inside the safe zone
    try:
        from measurekit_core import RationalUnit
    except ImportError:
        # Should not happen in this path if IS_CORE_AVAILABLE, but safety first
        from measurekit.jit.tracer import RationalUnit

    unit = RationalUnit(unit_dims)
    return CoreQuantity.__new__(Quantity, magnitude, unit, uncertainty)


def _create_full_quantity(
    magnitude, unit_dims, uncertainty_obj, system, fraction, symbol
):
    """Creates and fully initializes a Quantity in one opaque step."""
    # Always use Python-based RationalUnit for full Dynamo traceablity (Zero-Overhead)
    from measurekit.domain.measurement.uncertainty import Uncertainty
    from measurekit.jit.tracer import RationalUnit

    # Re-derive dependencies inside the opaque op to avoid passing complex objects through the graph
    backend = BackendManager.get_backend(magnitude)
    # system is passed explicitly to avoid ContextVar access

    # Reconstruct Units
    r_unit = RationalUnit(unit_dims)

    # We use the Stable wrapper logic to ensuring cache hit
    import measurekit.domain.measurement.units as units_module

    CU = getattr(
        units_module, "_STABLE_COMPOUND_UNIT", units_module.CompoundUnit
    )
    # Bypass Flyweight Cache (__new__) because WeakRef dictionary lookup breaks Dynamo
    # We manually create the object and set the frozen field.
    unit_obj = object.__new__(CU)
    object.__setattr__(unit_obj, "exponents", unit_dims)
    # unit_obj = CU(unit_dims) # <--- OLD

    dimension = unit_obj.dimension(system)

    # Extract raw std_dev for CoreQuantity.__new__
    raw_uncertainty = uncertainty_obj
    if isinstance(uncertainty_obj, Uncertainty):
        raw_uncertainty = uncertainty_obj.std_dev

    # Use the global Quantity class
    obj = CoreQuantity.__new__(Quantity, magnitude, r_unit, raw_uncertainty)

    # Init attributes
    object.__setattr__(obj, "uncertainty_obj", uncertainty_obj)
    object.__setattr__(obj, "system", system)
    object.__setattr__(obj, "dimension", dimension)
    object.__setattr__(obj, "_unit", unit_obj)
    object.__setattr__(obj, "_backend", backend)
    object.__setattr__(obj, "fraction", fraction)
    object.__setattr__(obj, "symbol", symbol)

    return obj
