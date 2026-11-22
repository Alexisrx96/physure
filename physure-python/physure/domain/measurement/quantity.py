"""Defines the `Quantity` class, the representation of a physical quantity.

This module contains the `Quantity` class, which bundles a numerical value
(magnitude), a `CompoundUnit`, and an optional `Uncertainty`. It is the central
object that users interact with. The class overloads arithmetic, comparison,
and other operators to provide intuitive, unit-aware calculations, automatic
    error propagation, and seamless integration with various backends.
"""

from __future__ import annotations

import contextlib
import math
import operator
import sys
from dataclasses import dataclass, field
from fractions import Fraction
from typing import (
    TYPE_CHECKING,
    Any,
    Generic,
    TypeVar,
    cast,
)

from physure.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator
    from typing import Self

    from physure.core.protocols import BackendOps, Numeric
    from physure.domain.measurement.dimensions import Dimension

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.conversion_plan import apply_conversion
from physure.domain.measurement.uncertainty import (
    LineageModel,
    Uncertainty,
)
from physure.domain.measurement.unit_text import parse_unit_string
from physure.domain.measurement.units import (
    _CORE_ERRORS,
    CompoundUnit,
    _python_error,
    get_default_system,
)

# torch and sympy are imported lazily when needed to improve startup time.
torch = None
sp = None

_CORE_QUANTITY_TYPE = "physure._core.Quantity"

try:
    from physure._generated_types import UnitName
except ImportError:
    UnitName = str

# Trace-safe imports
from physure._jit.tracer import _ensure_rational  # noqa: E402
from physure.application.context import _UNCERTAINTY_MODE  # noqa: E402

if TYPE_CHECKING:
    from physure.domain.measurement.system import UnitSystem

# sympy (sp) moved to lazy loading helper if needed.


# pydantic_core is imported lazily inside __get_pydantic_core_schema__

# --- Generic Type Variables ---
ValueType = TypeVar("ValueType")
UncType = TypeVar("UncType")
UnitType = TypeVar("UnitType")  # Phantom type for units


class CoreQuantity:
    """Container base for `Quantity`: holds magnitude/unit/uncertainty as plain attributes.

    Not a fallback for a missing Rust extension — physure._core.Quantity is a real,
    always-available PyO3 type (see physure/__init__.py's unconditional import of it).
    Quantity doesn't inherit that Rust type directly because Quantity needs multiple
    inheritance (with ArithmeticMixin/BackendMixin) and must stay traceable by PyTorch
    Dynamo, and a compiled PyO3 base class doesn't reliably support either. High-speed
    operations still delegate directly to physure._core functions/methods; this class only
    stores the magnitude/unit/uncertainty triple and exposes it as properties.
    """

    _core_magnitude: Numeric
    _core_unit: CompoundUnit
    _core_uncertainty: Any  # pyright: ignore[reportExplicitAny]

    def __new__(
        cls,
        magnitude: Numeric,
        unit: CompoundUnit,
        uncertainty: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        *args: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        **kwargs: Any,  # pyright: ignore[reportAny, reportExplicitAny]
    ) -> Self:
        """Stores magnitude/unit/uncertainty on the instance."""
        obj = super().__new__(cls)
        object.__setattr__(obj, "_core_magnitude", magnitude)
        object.__setattr__(obj, "_core_unit", unit)
        object.__setattr__(obj, "_core_uncertainty", uncertainty)
        return obj

    @property
    def magnitude(self) -> Numeric:
        """Returns the stored magnitude."""
        val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
        if _CORE_QUANTITY_TYPE in str(type(val)):
            return val.magnitude
        return val

    @property
    def unit(self) -> CompoundUnit:
        """Returns the stored unit."""
        return self._core_unit

    @property
    def std_dev(self) -> Any:  # pyright: ignore[reportExplicitAny]
        """Returns the stored uncertainty."""
        val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
        if _CORE_QUANTITY_TYPE in str(type(val)):
            return val.std_dev
        return self._core_uncertainty


from physure._core import RationalUnit as _CoreRationalUnit  # noqa: E402
from physure._core import (  # noqa: E402
    TemperatureKindError as _TemperatureKindError,
)
from physure._core import (  # noqa: E402
    point_against_interval_message as _point_against_interval_message,
)
from physure._core import (  # noqa: E402
    point_interval_conversion_message as _point_interval_conversion_message,
)

# the dimension of a temperature, the only one with points and intervals
_TEMPERATURE = {"O": 1}


def _exact_number(value: Any) -> Fraction | None:
    """A real scalar as an exact rational, `None` for anything else (an array, a tracer, a complex).

    A double is read as the decimal it prints as, the way the core reads one
    (`ExactScale::from_f64`): `2.54` is `127/50`, so `2.54 cm` is exactly `1 inch`.
    A non-finite double has no rational.
    """
    if isinstance(value, Fraction):
        return value
    if isinstance(value, int):
        return Fraction(value)
    numpy = sys.modules.get("numpy")
    if numpy is not None and isinstance(value, numpy.integer):
        return Fraction(int(value))
    if isinstance(value, float) or (
        numpy is not None and isinstance(value, numpy.floating)
    ):
        number = float(value)
        if math.isfinite(number):
            return Fraction(repr(number))
    return None


def _as_float(value: Any) -> float:
    """`float(value)`, an infinity of its sign beyond the range of a double."""
    try:
        return float(value)
    except OverflowError:
        return math.inf if value > 0 else -math.inf


def _python_scalar(value: Any) -> Any:
    """A NumPy scalar as the Python number it holds; anything else (an array) as it is."""
    numpy = sys.modules.get("numpy")
    if numpy is not None and isinstance(value, numpy.generic):
        return value.item()
    return value


def _is_plain_real(value: Any) -> bool:
    """A plain real number a dimensionless quantity may equal: a Python or NumPy int or float."""
    if isinstance(value, (int, float)):
        return True
    numpy = sys.modules.get("numpy")
    return numpy is not None and isinstance(
        value, (numpy.integer, numpy.floating)
    )


# Helpers moved to end of file to resolve circular reference with Quantity class
from physure.domain.measurement._arithmetic_mixin import (  # noqa: E402
    ArithmeticMixin,
)
from physure.domain.measurement._backend_mixin import (  # noqa: E402
    BackendMixin,
)
from physure.domain.measurement.degrees_of_freedom import (  # noqa: E402
    DegreesOfFreedomMixin,
)


@dataclass(frozen=False)
class Quantity(
    ArithmeticMixin,
    BackendMixin,
    DegreesOfFreedomMixin,
    CoreQuantity,
    Generic[ValueType, UncType, UnitType],
):
    """Represents a physical quantity with magnitude, unit, and uncertainty.

    Examples:
        >>> from physure import Q_
        >>> length = Q_(10, "m")
        >>> time = Q_(2, "s")
        >>> velocity = length / time
        >>> print(velocity)
        5.0 m/s
        >>> print(velocity.to("km/h"))
        18.0 km/h
    """

    __array_priority__ = 1000.0

    # ponytail: unit/args/kwargs mirror the Rust extension constructor's
    # flexible signature (unit may arrive as str/CompoundUnit/RationalUnit;
    # args/kwargs carry optional uncertainty) — same rationale as
    # CoreQuantity.__new__ above.
    def __new__(
        cls,
        magnitude: Numeric,
        unit: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        *args: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        **kwargs: Any,  # pyright: ignore[reportAny, reportExplicitAny]
    ) -> Self:
        """Ensures the core object is initialized with a RationalUnit."""
        r_unit = _ensure_rational(unit)

        # Pull uncertainty from kwargs if present, else try args[0]
        uncertainty = kwargs.get("uncertainty")

        if uncertainty is None:
            uncertainty = args[0] if args else 0.0

        # Ensure we pass the numerical standard deviation to Rust ONLY if it's a simple type.
        # If it's a rich Uncertainty model, pass it as-is so Rust can store it in TensorBackend.
        raw_uncertainty = uncertainty
        if hasattr(uncertainty, "std_dev") and not isinstance(
            uncertainty, (int, float, complex)
        ):
            # We want to keep the rich model if it's a CovarianceModel etc.
            # Actually, if we pass it as-is, Rust's extract::<f64>() will fail,
            # falling back to TensorBackend which is what we want.
            pass
        elif hasattr(uncertainty, "std_dev"):
            raw_uncertainty = uncertainty.std_dev

        dims = getattr(r_unit, "dimensions", None) or getattr(
            r_unit, "exponents", {}
        )
        obj = CoreQuantity.__new__(
            cls,
            magnitude,
            cast("CompoundUnit", _CoreRationalUnit(dims)),
            raw_uncertainty,
        )
        object.__setattr__(obj, "_core_magnitude", magnitude)
        object.__setattr__(obj, "_core_unit", r_unit)
        object.__setattr__(obj, "_core_uncertainty", uncertainty)
        return obj

    def __reduce__(self) -> str | tuple[Any, ...]:
        """Custom reduce to ensuring proper subclass reconstruction."""
        # Use Rust's implementation for args and state
        res = super().__reduce__()
        # If it returns (func, args, state), replace func with this class
        if isinstance(res, tuple) and len(res) >= 2:
            return (self.__class__, *res[1:])
        return res

    # ponytail: these dataclass field annotations exist for
    # dataclasses.fields()/repr/eq introspection only. __init__ is
    # hand-written below and never assigns them directly — reads resolve
    # through the @property redefinitions further down (or, for
    # `magnitude`, the CoreQuantity property). basedpyright's override/
    # redeclaration checks assume plain-attribute semantics, which is a
    # false positive for this dataclass-field-as-property-facade pattern.
    magnitude: ValueType  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    unit: CompoundUnit  # pyright: ignore[reportRedeclaration]
    uncertainty: Any = 0.0  # pyright: ignore[reportRedeclaration, reportAssignmentType]
    system: UnitSystem = field(default_factory=get_default_system)
    symbol: str | None = None
    _uncertainty_obj: Any = field(default=None, repr=False, compare=False)
    dimension: Dimension = field(init=False)
    _backend: BackendOps = field(init=False, repr=False)
    __weakref__: Any = field(init=False, repr=False, compare=False)

    # ponytail: __new__ takes *args/**kwargs to interoperate with the Rust
    # CoreQuantity constructor; __init__ has the explicit dataclass-style
    # signature. Both are intentional, but basedpyright expects the two to
    # match parameter-for-parameter.
    def __init__(  # pyright: ignore[reportInconsistentConstructor]
        self,
        magnitude: Numeric | None = None,
        unit: Any = None,
        uncertainty: Any = None,
        system: UnitSystem | None = None,
        symbol: str | None = None,
        **kwargs: Any,  # pyright: ignore[reportAny, reportExplicitAny]
    ) -> None:
        """Initializes the entity, ignoring magnitude and unit if already set by core."""
        # Reconstruct CompoundUnit wrapper ensuring we hit the Python-side Flyweight cache
        import physure.domain.measurement.units as units_module

        CU = getattr(
            units_module,
            "_STABLE_COMPOUND_UNIT",
            units_module.CompoundUnit,
        )
        u = super().unit
        dims = getattr(u, "dimensions", None)
        if dims is None:
            dims = getattr(u, "exponents", {})

        # Always call CU(dims) to ensure we get the cached singleton instance;
        # a unit made an interval (`1 K + 5 delta_degC`) stays one
        unit = CU(dims)
        if getattr(self._core_unit, "interval", False):
            unit = unit.as_interval()
        elif getattr(self._core_unit, "point", False):
            unit = unit.as_point()
        object.__setattr__(self, "_unit", unit)

        if system is not None:
            object.__setattr__(self, "system", system)
        else:
            from physure.domain.measurement.units import get_default_system

            object.__setattr__(self, "system", get_default_system())

        if symbol is not None:
            object.__setattr__(self, "symbol", symbol)

        # Store rich uncertainty model if provided (Phase 5 Fix)
        if "_uncertainty_obj" in kwargs:
            object.__setattr__(
                self, "_uncertainty_obj", kwargs["_uncertainty_obj"]
            )
        elif isinstance(uncertainty, Uncertainty):
            object.__setattr__(self, "_uncertainty_obj", uncertainty)

        # After fields are basic-set, run logic
        self.__post_init__()

        # Trace-Safe Optimization:
        # Tell Dynamo that the 'unit' field is constant for this instance.
        # This prevents it from trying to guard/check it repeatedly.
        if "torch" in sys.modules:
            try:
                import torch as _torch

                # Check is_compiling() first: it is cheap and never imports
                # torch._dynamo, while hasattr(_torch, "_dynamo") triggers a
                # ~1s lazy import. If we are compiling, _dynamo is loaded.
                if _torch.compiler.is_compiling():
                    # ponytail: mark_static's attribute-name overload (for
                    # marking a non-tensor object attribute static) isn't
                    # reflected in the bundled torch type stubs, which only
                    # type `index` as int/list/tuple/None.
                    _torch._dynamo.mark_static(
                        self,
                        "unit",  # pyright: ignore[reportArgumentType]
                    )
                    _torch._dynamo.mark_static(
                        self,
                        "_unit",  # pyright: ignore[reportArgumentType]
                    )
            except Exception:
                pass

    def __post_init__(self) -> None:
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
            from physure.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.register_leaf(self, self.symbol)

    def tree_flatten(
        self,
    ) -> tuple[tuple[Any, Any], tuple[Any, Any, Any]]:
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
            self.symbol,
        )

    @classmethod
    def tree_unflatten(
        cls, aux_data: Any, children: tuple[Any, Any]
    ) -> Quantity:
        """Reconstructs the Quantity from JAX flatten results."""
        magnitude, uncertainty = children
        unit, system, symbol = aux_data

        # Re-derive metadata
        dimension = unit.dimension(system)
        backend = BackendManager.get_backend(magnitude)

        # Use _fast_new to skip validation overhead
        return cls._fast_new(
            magnitude,
            unit,
            uncertainty,
            system,
            dimension,
            backend,
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
            cast("Numeric", self.magnitude),
            new_unit,
            self.std_dev,
            system,
            self.dimension,
            self._backend,
            self.symbol,
        )

    def simplify(self) -> Quantity:
        """Simplifies the unit of the quantity into the system's preferred form."""
        new_unit = self.unit.simplify(self.system)
        return self.to(new_unit)

    def approx_eq(
        self, other: Any, rel_tol: float = 1e-9, abs_tol: float = 1e-12
    ) -> bool:
        """Checks if self and other agree within tolerance."""
        if not hasattr(other, "dimension") or not hasattr(other, "to"):
            return False
        if self.dimension != getattr(other, "dimension", None):
            return False
        try:
            converted = other.to(self.unit)
            a_mag = float(self.magnitude)
            b_mag = float(converted.magnitude)
            diff = abs(a_mag - b_mag)
            return bool(
                diff <= max(rel_tol * max(abs(a_mag), abs(b_mag)), abs_tol)
            )
        except Exception:
            return False

    def exact_eq(self, other: Any) -> bool:
        """Passes when self and other match unit and magnitude."""
        if (
            not hasattr(other, "dimension")
            or not hasattr(other, "to")
            or not hasattr(other, "magnitude")
        ):
            return False
        if self.dimension != getattr(other, "dimension", None):
            return False
        if self.magnitude != other.magnitude:
            return False
        try:
            one_other = self._fast_new(
                1.0, other.unit, 0.0, self.system, self.dimension
            )
            converted_one = one_other.to(self.unit)
            # exact on purpose: `approx_eq` is the comparison with a tolerance
            return float(converted_one.magnitude) == 1.0  # NOSONAR
        except Exception:
            return False

    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        system: UnitSystem,
        uncertainty: Any = 0.0,
        symbol: str | None = None,
        dof: float | None = None,
    ) -> Self:
        """Creates a Quantity from raw input values.

        `dof` declares the degrees of freedom of the standard uncertainty (`with_dof`).
        """
        resolved_system = system

        backend = BackendManager.get_backend(value)

        # Lazy imports to avoid circularity
        from physure.domain.measurement.uncertainty import Uncertainty

        cls._reject_asymmetric_pair(backend, value, uncertainty)

        # Ensure uncertainty matches backend type if array
        needs_broadcast = (
            backend.is_array(value)
            and not isinstance(uncertainty, Uncertainty)
            and not backend.is_array(uncertainty)
        )
        if needs_broadcast:
            uncertainty = cls._broadcast_uncertainty_to_array(
                backend, value, uncertainty
            )

        # Cast scalar int to float for consistency with test expectations
        if isinstance(value, int):
            value = float(value)

        # Check for fraction support (Python backend only usually)
        if not backend.is_array(value):
            with contextlib.suppress(ValueError, TypeError):
                Fraction(str(value))

        # Core Mode Integration
        value = cls._maybe_wrap_in_rust_core(value, unit, uncertainty)

        # Extract raw standard deviation if it's a rich model
        raw_uncertainty = getattr(uncertainty, "std_dev", uncertainty)
        if _CORE_QUANTITY_TYPE in str(type(value)):
            raw_uncertainty = value.std_dev

        u_obj = None
        if isinstance(uncertainty, Uncertainty):
            u_obj = uncertainty
        else:
            # Upgrade to rich model (handles active store + scalar/array logic)
            u_obj = Uncertainty.from_standard(uncertainty)

        quantity = cls(
            magnitude=cast("Numeric", value),
            unit=unit,
            uncertainty=raw_uncertainty,
            system=resolved_system,
            symbol=symbol,
            _uncertainty_obj=u_obj,
        )
        if dof is not None:
            quantity.with_dof(dof)
        return quantity

    @staticmethod
    def _reject_asymmetric_pair(
        backend: Any, value: Any, uncertainty: Any
    ) -> None:
        """Refuses `Q_(12.3, "pb", (0.5, 0.4))`, the way PHS refuses `12.3 +/- (0.5, 0.4)`.

        A scalar has no second element for a second uncertainty to belong to, so a pair on
        one can only mean an asymmetric measurement. Nothing propagates a third moment yet,
        and the pair used to reach `float()` and come back as
        `TypeError: float() argument must be a string or a real number` — a message about
        the plumbing rather than about the measurement. An array magnitude keeps taking a
        tuple, which is an ordinary per-element uncertainty and has always worked.
        """
        if not isinstance(uncertainty, tuple) or len(uncertainty) != 2:
            return
        if backend.is_array(value):
            return
        raise NotImplementedError(
            f"An asymmetric uncertainty {uncertainty} cannot be propagated yet: "
            "physure_core::uncertainty::moments converts a (sigma-, sigma+) pair to "
            "moments and back, but nothing propagates the third moment they carry, and "
            "using one half alone would report the measurement as symmetric. Pass a "
            "single standard deviation if that is what you meant."
        )

    @classmethod
    def _maybe_wrap_in_rust_core(
        cls, value: Any, unit: Any, uncertainty: Any
    ) -> Any:
        """Wraps a scalar in a physure._core.Quantity for sampling modes."""
        mode, mode_args = cls._resolve_uncertainty_mode()

        try:
            from physure._core import Quantity as RustCoreQuantity
        except ImportError:
            return value

        if "Quantity" in str(type(value)) or mode not in (
            "monte_carlo",
            "unscented",
            "gaussian",
        ):
            return value

        r_unit = _ensure_rational(unit)
        std_dev = getattr(uncertainty, "std_dev", uncertainty)
        if hasattr(std_dev, "std_dev"):
            std_dev = std_dev.std_dev
        # None: the core's `[Settings] monte_carlo_samples` decides (default 1000)
        samples = mode_args.get("samples") if mode_args else None
        return RustCoreQuantity(
            float(value), r_unit, float(std_dev or 0.0), mode, samples
        )

    @classmethod
    def _broadcast_uncertainty_to_array(
        cls,
        backend: Any,
        value: Any,
        uncertainty: Any,
    ) -> Any:
        """Broadcasts a scalar uncertainty to match the shape of an array value."""
        try:
            shape = backend.shape(value)
            ones = backend.ones(shape, reference=value)
            return backend.mul(ones, uncertainty)
        except (AttributeError, NotImplementedError):
            return uncertainty

    @classmethod
    def _resolve_uncertainty_mode(cls) -> tuple:
        """Returns (mode, mode_args) from torch compiler state or context var."""
        # sys.modules guard: a bare `import torch` here would load all of
        # torch (~2s) for numpy/python users the first time they propagate.
        if "torch" in sys.modules:
            try:
                import torch as _torch

                if _torch.compiler.is_compiling():
                    return ("python", {})
            except (ImportError, AttributeError):
                pass

        from physure.application.context import get_propagation_mode

        prop_mode = get_propagation_mode()
        if prop_mode in ("monte_carlo", "montecarlo", "unscented"):
            mode = "monte_carlo" if prop_mode == "montecarlo" else prop_mode
            _, mode_args = _UNCERTAINTY_MODE.get()
            return (mode, mode_args or {})

        mode, mode_args = _UNCERTAINTY_MODE.get()
        return mode, mode_args or {}

    @classmethod
    def _fast_new(
        cls,
        value: Numeric,
        unit: CompoundUnit,
        uncertainty: Any,
        system: UnitSystem,
        dimension: Dimension,
        backend: BackendOps | None = None,
        symbol: str | None = None,
    ) -> Self:
        """Bypasses some validation for high-performance creation."""
        # If value is a Rust Quantity, use its std_dev as the uncertainty
        if _CORE_QUANTITY_TYPE in str(type(value)):
            uncertainty = value.std_dev
        return cls(
            magnitude=value,
            unit=unit,
            uncertainty=uncertainty,
            system=system,
            symbol=symbol,
        )

    # ponytail: torch's __torch_dispatch__ protocol is Any-typed upstream
    # (OpOverload/overload-packet dispatch args); nothing here can recover
    # concrete types torch itself doesn't expose.
    @classmethod
    def __torch_dispatch__(
        cls,
        func: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        types: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        args: tuple[Any, ...] = (),  # pyright: ignore[reportExplicitAny]
        kwargs: dict[str, Any] | None = None,  # pyright: ignore[reportExplicitAny]
    ) -> Any:  # pyright: ignore[reportExplicitAny]
        """Deep PyTorch integration for zero-overhead compilation."""
        # PyTorch's own dispatcher is what invokes __torch_dispatch__, so
        # torch is guaranteed already loaded here; the module-level `torch`
        # placeholder (None, for import-time budget) doesn't apply.
        from torch.utils import _pytree

        if kwargs is None:
            kwargs = {}

        def unwrap(x: Any) -> Any:  # pyright: ignore[reportExplicitAny]
            if isinstance(x, Quantity):
                # For torch dispatch, we act on the magnitude (Tensor)
                # We essentially strip the unit for the operation
                # Ideally, we should check unit consistency of args here?
                # But for 'Zero Overhead' compiled graphs, unit checks happen
                # at Trace time (if we trace the checks) or we rely on the user/compiler.
                # Here we blindly operate on magnitudes to let Torch see the tensors.
                return x.magnitude
            return x

        args_unwrapped = _pytree.tree_map(unwrap, args)
        kwargs_unwrapped = _pytree.tree_map(unwrap, kwargs)

        out = func(*args_unwrapped, **kwargs_unwrapped)

        def wrap(x: Any) -> Any:  # pyright: ignore[reportExplicitAny]
            return x

        return _pytree.tree_map(wrap, out)

    @classmethod
    def __get_pydantic_core_schema__(
        cls, source_type: Any, handler: Any
    ) -> Any:
        """Defines the Pydantic Core Schema for validation."""
        try:
            from pydantic_core import core_schema
        except ImportError as e:
            raise ImportError(
                "pydantic-core is required for Pydantic validation of Quantity. "
                "Install it with: pip install physure[pydantic]"
            ) from e

        def validate_from_str(value: Any) -> Quantity:
            if isinstance(value, Quantity):
                return value
            if isinstance(value, str):
                from physure.application.context import get_active_system
                from physure.domain.measurement.quantity_text import (
                    parse_quantity_text,
                )

                system = get_active_system()
                number, unit = parse_quantity_text(value, system)
                return Quantity.from_input(number, unit, system)
            raise ValueError(f"Cannot validate Quantity from {type(value)}")

        def validate_from_dict(value: Any) -> Quantity:
            if isinstance(value, dict):
                mag = value.get("magnitude")
                unit = value.get("unit")
                system_name = value.get("system", "SI")
                # Lazy import to avoid circular dependencies
                from physure.application.startup import create_system

                # "SI" is international.conf (its [System] name is SI);
                # there is no si.conf, which used to fall back in silence
                # to the base catalog
                stem = system_name.lower()
                stem = "international" if stem == "si" else stem
                try:
                    sys = create_system(f"{stem}.conf")
                except FileNotFoundError as missing:
                    raise ValueError(str(missing)) from missing
                if unit is None:
                    raise ValueError(
                        f"Cannot validate Quantity from {type(value)}"
                    )
                u = sys.get_unit(unit)
                return Quantity.from_input(mag, u, sys)
            raise ValueError(f"Cannot validate Quantity from {type(value)}")

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
        """The hash of the value in the coherent unit, so that `a == b` gives `hash(a) == hash(b)`.

        `1 m`, `100 cm` and `0.001 km` hash alike, and a dict keyed by one finds the
        others. Neither the uncertainty (`==` does not compare it) nor the unit it is
        written in nor the kind of a temperature enters the hash. A dimensionless quantity
        hashes as the number it equals (`hash(Q_(100, "percent")) == hash(1)`). An array
        magnitude is unhashable.

        Examples:
            >>> from physure import Q_
            >>> len({Q_(1, "m"), Q_(100, "cm"), Q_(2, "m", 0.1)})
            2
        """
        magnitude: Any = self.magnitude
        if self._backend.is_array(magnitude) and tuple(
            self._backend.shape(magnitude)
        ):
            raise TypeError(
                "unhashable type: 'Quantity' with an array magnitude"
            )
        try:
            _, value = self._coherent_value()
        except Exception:  # a value the core cannot convert (a tracer)
            value = magnitude
        try:
            if self.dimension.is_dimensionless:
                return hash(value)
            return hash((self.dimension, value))
        except TypeError:
            raise TypeError(
                "unhashable type: 'Quantity' with unhashable magnitude"
            ) from None

    def _coherent_value(self) -> tuple[CompoundUnit, Any]:
        """The value in the coherent unit of the quantity, and that unit (`m`, `K`, `kg·m²/s²`).

        What `==`, `hash` and `float` read: one double, rounded once. A real scalar in
        another unit is converted exactly and then rounded: its magnitude read as the
        decimal it prints as (as the core reads a double, `ExactScale::from_f64`), the
        core's plan to the coherent unit applied before its rounding
        (`UnitRegistry.exact_conversion`), and the result rounded to the nearest double.
        So `68 degF` and `20 degC` are both `293.15 K` (the rounded plan gives
        `293.15000000000003 K` for `68 degF`), `2.54 cm` is `0.0254 m` like `1 inch`, and
        `q.to(unit)` usually rounds back to the same double. A level, a unit known only
        as a double, or an array take the core's rounded plan, as `to` applies it.

        A NumPy scalar is read as the Python number it holds (`np.float32(0.1)` is
        `0.10000000149011612`), so `==` and `hash` see one number. A value beyond the
        range of a double stays the exact `Fraction` (`1e308 km`), which compares and
        hashes exactly; `float()` makes it an infinity.
        """
        coherent, exact_plan, rounded_plan = self._coherent_plan()
        magnitude: Any = _python_scalar(self.magnitude)
        if rounded_plan is None:
            return coherent, magnitude
        exact = _exact_number(magnitude) if exact_plan is not None else None
        if exact is not None:
            scale, shift = exact_plan
            value = exact * scale + shift
            try:
                return coherent, float(value)
            except OverflowError:
                return coherent, value
        value, _ = apply_conversion(
            rounded_plan, magnitude, 0.0, self._backend
        )
        return coherent, _python_scalar(value)

    def _coherent_plan(self) -> tuple[CompoundUnit, Any, Any]:
        """The coherent unit, the exact plan to it (Fractions) and the core's rounded one.

        Both plans are `None` for a unit that is its own coherent unit; the exact one is
        `None` when the core has none. Kept per unit on the system, which forgets them
        when its registry changes (`UnitSystem.extend_core`).
        """
        system = self.system
        unit = self.unit
        plans = system.__dict__.setdefault("_coherent_plans", {})
        known = plans.get(unit)
        if known is not None:
            return known
        coherent = unit.base_unit(system)
        if coherent == unit and not self._is_level():
            known = (coherent, None, None)
        else:
            registry = system.core_registry
            exact = registry.exact_conversion(unit, coherent)
            known = (
                coherent,
                None
                if exact is None
                else (Fraction(exact[0]), Fraction(exact[1])),
                registry.conversion(unit, coherent),
            )
        plans[unit] = known
        return known

    def _stripped_value(self, operation: str) -> Any:
        """The value in the coherent unit for a bare number, warning about what is lost.

        `UnitStrippedWarning` names the unit (unless the quantity is dimensionless and
        not a level, `50 percent` is the number `0.5`) and the uncertainty when there is
        one, and the explicit, silent way to take the number.
        """
        coherent, value = self._coherent_value()
        lost = []
        if self._is_level():
            lost.append(
                f"the level unit '{self.unit}' (the number is the ratio it stands for)"
            )
        elif not self.dimension.is_dimensionless:
            lost.append(f"the unit '{coherent}'")
        if self._has_uncertainty:
            lost.append(f"the uncertainty {self._numeric_std_dev}")
        if lost:
            import warnings

            from physure.domain.exceptions import UnitStrippedWarning

            target = "1" if coherent.is_dimensionless else str(coherent)
            uncertainty = " and q.uncertainty" if self._has_uncertainty else ""
            warnings.warn(
                f"{operation} of {self!r} drops {' and '.join(lost)}: the number "
                f"is the value in the coherent unit '{target}'. To take the number "
                f"on purpose and silently, write q.to('{target}').magnitude"
                f"{uncertainty}.",
                UnitStrippedWarning,
                stacklevel=3,
            )
        return value

    @property
    def _numeric_std_dev(self) -> Any:
        """Returns the numeric standard deviation, even if std_dev returns a model."""
        scalar_types: tuple[type, ...] = (int, float, complex)
        try:
            import numpy as np

            scalar_types = (*scalar_types, np.ndarray)
        except ImportError:
            pass

        u = self.std_dev
        if hasattr(u, "std_dev") and not isinstance(u, scalar_types):
            return u.std_dev
        return u

    @property
    def _has_uncertainty(self) -> bool:
        """Checks if uncertainty is non-zero, safely handling arrays and models."""
        unc = self._numeric_std_dev

        try:
            # Backend-aware check for numeric uncertainty
            if self._backend.is_array(unc):
                return bool(self._backend.any(self._backend.not_equal(unc, 0)))

            # Scalar check
            if isinstance(unc, (int, float, complex)):
                return unc != 0

            return bool(unc)
        except Exception:
            return True

    @property
    def m(self) -> ValueType:
        """Alias for .magnitude (pint-compatible shorthand)."""
        return self.magnitude

    @property
    def u(self) -> CompoundUnit:
        """Alias for .unit."""
        return self.unit

    def __repr__(self) -> str:
        """Returns string representation."""
        # Matches `__str__`/`_display_unit_str`: a dimensionless quantity has no unit to
        # show, whether it's an anonymous ratio (`3 m / 2 m`) or built with no unit at
        # all. Reading `self.unit.to_string(...)` directly here (bypassing that check)
        # is what let `Quantity(1.5, 1)` through -- a "1" that reads like a second
        # positional argument, not "no unit".
        unit_str = self._display_unit_str(use_alias=True)
        if self._has_uncertainty:
            # A scalar model reprs as a plain float. Dumping the model itself
            # would put its internals in the repr, and for a lineage-tracking
            # model that means a UUID the user never chose.
            unc = self._uncertainty_obj
            is_scalar_model = (
                unc is not None
                and getattr(unc, "vector_slice", None) is None
                and not self._backend.is_array(unc.std_dev)
            )
            unc_repr = (
                repr(self.uncertainty)
                if unc is None or is_scalar_model
                else repr(unc)
            )
            if unit_str:
                return (
                    f"Quantity({self.magnitude!r}, {unit_str}, "
                    f"uncertainty={unc_repr})"
                )
            return f"Quantity({self.magnitude!r}, uncertainty={unc_repr})"
        if unit_str:
            return f"Quantity({self.magnitude!r}, {unit_str})"
        return f"Quantity({self.magnitude!r})"

    def _display_unit_str(self, use_alias: bool = True) -> str:
        """Returns the unit's display string, or "" if dimensionless."""
        if self.unit.is_dimensionless:
            return ""
        if getattr(self, "_override_use_alias", None) is not None:
            use_alias = self._override_use_alias
        return self.unit.to_string(self.system, use_alias=use_alias)

    def __str__(self) -> str:
        """Returns a user-friendly string representation."""
        unit_str = self._display_unit_str(use_alias=True)
        if self._has_uncertainty:
            unc_str = str(self.uncertainty)
            return f"({self.magnitude} ± {unc_str}) {unit_str}".strip()
        return f"{self.magnitude} {unit_str}".strip()

    def __rich__(self) -> Any:
        """Rich console protocol for beautiful output."""
        try:
            from rich.text import Text
        except ImportError:
            return self.__str__()

        # Unit string (defaults to alias for clean display)
        unit_str = self._display_unit_str(use_alias=True)

        # Magnitude formatting
        mag_str = str(self.magnitude)

        # Text construction
        text = Text()
        text.append(mag_str, style="bold green")

        if self._has_uncertainty:
            unc_str = str(self.uncertainty)
            text.append(f" ± {unc_str}", style="dim")

        if unit_str:
            text.append(" ", style="none")
            text.append(unit_str, style="bold blue")

        return text

    def _format_as_fraction(self, unit_str: str) -> str | None:
        """Returns a fraction-formatted string, or None if not representable."""
        from fractions import Fraction

        try:
            f = Fraction(str(self.magnitude))
            return f"{f.numerator}/{f.denominator} {unit_str}".strip()
        except (ValueError, TypeError):
            return None

    def _format_with_magnitude_format(
        self, mag_fmt: str, unit_str: str
    ) -> str:
        """Formats magnitude (and uncertainty) using a Python format spec."""
        formatted_mag = format(self.magnitude, mag_fmt)
        if not self._has_uncertainty:
            return f"{formatted_mag} {unit_str}".strip()
        try:
            formatted_unc = format(self._numeric_std_dev, mag_fmt)
        except (TypeError, ValueError):
            formatted_unc = str(self._numeric_std_dev)
        return f"({formatted_mag} ± {formatted_unc}) {unit_str}".strip()

    def __format__(self, format_spec: str) -> str:
        """Formats the quantity according to the specification.

        Supports composable pipe-separated flags, e.g.:
          - `.2f`          -> 2 decimal places (uses clean alias by default)
          - `.3e`          -> scientific notation
          - `base` / `raw` -> force expanded SI base units (deactivates alias)
          - `alias`        -> force clean alias
          - `frac`         -> fraction magnitude representation
          - `.2f|base`     -> 2 decimal places + expanded SI base units
          - `.3e|base`     -> scientific notation + expanded SI base units
        """
        if not format_spec:
            return str(self)

        parts = [p.strip() for p in format_spec.split("|") if p.strip()]
        mag_fmt = ""
        use_alias = True
        is_frac = False

        for p in parts:
            if p in ("base", "raw", "expand", "noalias"):
                use_alias = False
            elif p == "alias":
                use_alias = True
            elif p == "frac":
                is_frac = True
            else:
                mag_fmt = p

        unit_str = self._display_unit_str(use_alias=use_alias)

        if is_frac:
            frac_result = self._format_as_fraction(unit_str)
            if frac_result is not None:
                return frac_result

        if mag_fmt:
            return self._format_with_magnitude_format(mag_fmt, unit_str)

        if self._has_uncertainty:
            unc_str = str(self._numeric_std_dev)
            return f"({self.magnitude} ± {unc_str}) {unit_str}".strip()

        return f"{self.magnitude} {unit_str}".strip()

    def to_base_units(self) -> Quantity[ValueType, UncType, UnitType]:
        """Converts the quantity to its canonical base SI units."""
        base_unit = self._base_unit_for_dimension(self.dimension)
        res = self.to(base_unit)
        object.__setattr__(res, "_override_use_alias", False)
        return res

    def to_latex(self) -> str:
        r"""Returns the LaTeX representation.

        Examples:
            >>> from physure import Q_
            >>> q = Q_(10.0, "m/s^2")
            >>> print(q.to_latex())
            10.0 \; \frac{m}{s^{2}}
        """
        unit_latex = "" if self.unit.is_dimensionless else self.unit.to_latex()
        sep = " \\; " if unit_latex else ""
        if self._has_uncertainty:
            return (
                f"({self.magnitude} \\pm {self.uncertainty}){sep}{unit_latex}"
            )
        return f"{self.magnitude}{sep}{unit_latex}"

    def _repr_latex_(self) -> str:
        """Returns LaTeX for Jupyter notebooks."""
        return f"${self.to_latex()}$"

    def _repr_html_(self) -> str:
        """HTML display for Jupyter notebooks."""
        unit_str = self.unit.to_latex() or "dimensionless"
        mag = self.magnitude
        if self._has_uncertainty:
            return (
                f'<span style="font-family:monospace">'
                f"{mag} &plusmn; {self.uncertainty} "
                f'<span style="color:#888">{unit_str}</span>'
                f"</span>"
            )
        return (
            f'<span style="font-family:monospace">'
            f'{mag} <span style="color:#888">{unit_str}</span>'
            f"</span>"
        )

    def _repr_mimebundle_(
        self,
        include=None,
        exclude=None,
        **kwargs,  # pyright: ignore[reportUnusedParameter]
    ) -> dict:
        """MIME bundle for Jupyter — lets the frontend pick the best format."""
        bundle = {
            "text/plain": repr(self),
            "text/latex": self._repr_latex_(),
            "text/html": self._repr_html_(),
        }
        if include:
            bundle = {k: v for k, v in bundle.items() if k in include}
        if exclude:
            bundle = {k: v for k, v in bundle.items() if k not in exclude}
        return bundle

    def to_hdf5(self, group: Any, dataset_name: str) -> Any:
        """Saves the quantity to an HDF5 group.

        Args:
            group: An h5py.Group or h5py.File object.
            dataset_name: The name for the new dataset.

        Returns:
            The created h5py.Dataset.
        """
        from physure.ext.io import to_hdf5

        return to_hdf5(self, group, dataset_name)

    @classmethod
    def from_hdf5(cls, dataset: Any) -> Quantity:
        """Loads a quantity from an HDF5 dataset.

        Args:
            dataset: An h5py.Dataset object.

        Returns:
            A new Quantity object.
        """
        from physure.ext.io import from_hdf5

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
    def uncertainty_model(self) -> str:
        """Returns the type of uncertainty model used by the core."""
        try:
            # Delegate to Rust CoreQuantity if available
            return getattr(self.magnitude, "uncertainty_model", "gaussian")
        except (AttributeError, RuntimeError):
            return "gaussian"

    @property
    def uncertainty(  # noqa: F811  # pyright: ignore[reportIncompatibleVariableOverride]
        self,
    ) -> Numeric:
        """Returns the standard deviation of the uncertainty."""
        # Source of truth: Rust Core std_dev
        try:
            return self.std_dev
        except (AttributeError, RuntimeError, TypeError):
            return 0.0

    @property
    def unit(  # pyright: ignore[reportIncompatibleVariableOverride]
        self,
    ) -> CompoundUnit:
        """Retrieves the unit of the quantity as a CompoundUnit."""
        # Zero-Overhead: Return stored attribute directly.
        # This bypasses super() calls and dynamic checks that break torch.compile graphs.
        try:
            return self._unit
        except AttributeError:
            # Fallback for unpickled objects or edge cases where _unit wasn't set
            import physure.domain.measurement.units as units_module

            CU = getattr(
                units_module,
                "_STABLE_COMPOUND_UNIT",
                units_module.CompoundUnit,
            )
            u = super().unit

            dims = getattr(u, "dimensions", None)
            if dims is None:
                dims = getattr(u, "exponents", {})

            # Always use CU(dims) to ensure identity preservation via Flyweight cache
            compound = CU(dims)
            object.__setattr__(self, "_unit", compound)
            return compound

    @staticmethod
    def _is_device_string(s: str) -> bool:
        """Returns True if the string looks like a PyTorch device specifier."""
        devices = {"cuda", "cpu", "mps"}
        return s.lower() in devices or (
            ":" in s and s.split(":")[0].lower() in devices
        )

    def _convert_by_plan(
        self,
        target_unit: CompoundUnit,
        written: CompoundUnit | None = None,
        *,
        kind_free: bool = False,
    ) -> Quantity[ValueType, UncType, UnitType]:
        """Converts by the plan that the core registry gives for the two units.

        `written` is the target as it was typed, before a recipe unit was replaced
        by its base units: the core flags the hertz by how a unit is written, and
        `Hz` is `s^-1` once simplified. A point converted to a unit without a
        zero point is a point (`20 degC => K`), as the core says. `kind_free`
        reads the number in the target unit whatever kind of temperature it is
        (a sum's operand: the sum decides the kind of the result).
        """
        source = self.unit.as_plain() if kind_free else self.unit
        try:
            plan = self.system.core_registry.conversion(
                source, written or target_unit
            )
        except _CORE_ERRORS as error:
            raise _python_error(error, self.unit, target_unit) from error
        except _TemperatureKindError as error:
            # a temperature point to an interval unit, or back: the core's
            # words, with the quantity as it prints
            raise IncompatibleUnitsError(
                self.unit,
                target_unit,
                _point_interval_conversion_message(
                    str(self), str(written or target_unit), self._is_point()
                ),
            ) from error
        magnitude, uncertainty = apply_conversion(
            plan,
            self.magnitude,  # pyright: ignore[reportArgumentType]
            self.uncertainty,
            self._backend,
        )
        converted = Quantity.from_input(
            magnitude,
            target_unit,
            self.system,
            uncertainty=uncertainty,
        )
        if not kind_free and self.dimension.exponents == _TEMPERATURE:
            # the same quantity in another unit is of the same kind, as the
            # core says: a point stays a point, an interval an interval
            kind = self.system.core_registry.converted_kind(
                self.unit, written or target_unit
            )
            if kind == "point":
                object.__setattr__(
                    converted, "_unit", converted.unit.as_point()
                )
            elif kind == "interval" and not converted.unit.interval:
                object.__setattr__(
                    converted, "_unit", converted.unit.as_interval()
                )
        # A conversion restates the same measurement in another unit, so it keeps the
        # sources it came from, rescaled: `x.to("mm") - x.to("mm")` is zero and the degrees
        # of freedom stay with it. `from_input` alone would mint a new, independent source.
        model = self._uncertainty_obj
        if (
            isinstance(model, LineageModel)
            and isinstance(uncertainty, (int, float))
            and uncertainty > 0
        ):
            converted._uncertainty_obj = LineageModel(
                native=model.native.rescaled_to(float(uncertainty))
            )
        return cast("Quantity[ValueType, UncType, UnitType]", converted)

    def plot(
        self,
        x: Any = None,
        kind: str | None = None,
        ax: Any = None,
        **kwargs: Any,
    ) -> Any:
        """Aesthetic plotting of this physical quantity.

        Imports `physure.plotting` dynamically to preserve startup time.
        """
        from physure.plotting import plot

        return plot(self, x=x, kind=kind, ax=ax, **kwargs)

    def plot_slices(
        self,
        slice_dim: int = 0,
        num_slices: int = 4,
        cmap: str = "plasma",
        **kwargs: Any,
    ) -> Any:
        """Plots multiple 2D slice grids of a 3D+ Quantity for N-D field visualization."""
        from physure.plotting import plot_slices

        return plot_slices(
            self,
            slice_dim=slice_dim,
            num_slices=num_slices,
            cmap=cmap,
            **kwargs,
        )

    def plot_interactive(
        self,
        slice_dims: list[int] | int = 0,
        cmap: str = "plasma",
        **kwargs: Any,
    ) -> Any:
        """Creates an interactive GUI plot with sliders to scrub through N-D dimensions."""
        from physure.plotting import plot_interactive

        return plot_interactive(
            self, slice_dims=slice_dims, cmap=cmap, **kwargs
        )

    def plot_covariance(self, ax: Any = None, **kwargs: Any) -> Any:
        """Plots the covariance/correlation matrix of this Quantity if available."""
        from physure.plotting import plot_covariance

        return plot_covariance(self, ax=ax, **kwargs)

    def plot_3d(
        self,
        x: Any = None,
        y: Any = None,
        backend: str = "auto",
        title: str | None = None,
        filename: str | None = None,
        **kwargs: Any,
    ) -> Any:
        """True interactive 3D surface plot for this physical quantity."""
        from physure.plotting import plot_3d

        return plot_3d(
            self,
            x=x,
            y=y,
            backend=backend,
            title=title,
            filename=filename,
            **kwargs,
        )

    def export_3d(
        self,
        filename: str = "plot_3d.stl",
        x: Any = None,
        y: Any = None,
        fmt: str | None = None,
        binary: bool = True,
        **kwargs: Any,
    ) -> Any:
        """Exports 3D mesh geometry into standard 3D file formats.

        Supported formats: STL, OBJ, glTF, PLY, HTML.
        """
        from physure.plotting import export_3d

        return export_3d(
            self,
            x=x,
            y=y,
            filename=filename,
            fmt=fmt,
            binary=binary,
            **kwargs,
        )

    def to(
        self,
        target_unit: CompoundUnit | UnitName,
        *,
        equivalencies: Any = None,
    ) -> Quantity[ValueType, UncType, UnitType]:
        """Converts the quantity to a different unit or moves to a device.

        Examples:
            >>> from physure import Q_
            >>> q = Q_(10, "m")
            >>> q.to("km")
            Quantity(0.01, km)
            >>> temp = Q_(25, "degC")
            >>> temp.to("K")
            Quantity(298.15, K)
        """
        written_text = None
        if isinstance(target_unit, str):
            # Check if target_unit is a device string (e.g. "cuda", "cpu")
            if self._is_device_string(target_unit):
                return self.to_device(target_unit)
            written_text = target_unit
            target_unit = self.system.get_unit(target_unit)

        # Fast path for same unit (`=> K` makes an interval in K a plain K, as in
        # the core)
        if (
            target_unit == self.unit
            and target_unit.interval == self.unit.interval
        ):
            return self

        if self.dimension != target_unit.dimension(self.system):
            # The core decides whether the dimensions differ only by an angle a
            # conversion may read as 1 (`rad/s * m => m/s`); only a refusal of
            # the core sends the pair on to the equivalencies.
            written = (
                parse_unit_string(written_text, CompoundUnit)
                if written_text not in (None, "dimensionless")
                else None
            )
            try:
                return self._convert_by_plan(target_unit, written)
            except IncompatibleUnitsError:
                return self._convert_via_equivalencies(
                    target_unit, equivalencies
                )

        return self._convert_by_plan(target_unit)

    def _base_unit_for_dimension(self, dimension: Any) -> Any:
        """Returns the canonical base unit for a dimension (e.g. L*T^-1 -> m*s^-1)."""
        return CompoundUnit.of_dimension(dimension, self.system)

    def _convert_via_equivalencies(
        self, target_unit: Any, equivalencies: Any
    ) -> Quantity[ValueType, UncType, UnitType]:
        """Cross-dimension conversion through active/passed equivalencies."""
        from physure.domain.measurement.equivalencies import (
            _ACTIVE_EQUIVALENCIES,
            find_conversion_path,
            numerical_derivative,
        )

        active_eqs = list(_ACTIVE_EQUIVALENCIES.get())
        if equivalencies is not None:
            if callable(equivalencies):
                equivalencies = equivalencies()
            active_eqs.extend(equivalencies)

        path = find_conversion_path(
            self.dimension,
            target_unit.dimension(self.system),
            active_eqs,
        )
        if path is None:
            raise IncompatibleUnitsError(self.unit, target_unit)

        base_unit_from = self._base_unit_for_dimension(self.dimension)
        base_unit_to = self._base_unit_for_dimension(
            target_unit.dimension(self.system)
        )

        # 1. Convert self to base_unit_from
        val = self.to(base_unit_from)

        # 2. Apply conversion path step-by-step, propagating uncertainty
        current_val = val.magnitude
        current_unc = val.uncertainty
        for func in path:
            deriv = numerical_derivative(func, current_val)
            current_unc = abs(deriv) * current_unc
            current_val = func(current_val)

        # 3. Land in the target's base unit, then convert to target_unit
        target_base = Quantity.from_input(
            current_val,
            base_unit_to,
            self.system,
            uncertainty=current_unc,
        )
        return target_base.to(target_unit)

    # ponytail: ValueType is an unbound TypeVar (numpy/torch/jax array or
    # plain scalar), so basedpyright can't prove `magnitude` satisfies
    # int()/round()/math.floor()/etc.'s protocols. These dunders are only
    # meaningful for scalar magnitudes; array-backed ones fail at runtime
    # with a clear TypeError from the builtin itself, which is acceptable.
    def __int__(self) -> int:
        """The value in the coherent unit, truncated; `UnitStrippedWarning` as `float` warns."""
        return int(self._stripped_value("int()"))

    def _round_in_core(self, op: str, *args: int) -> Quantity | None:
        """`op` (`round`, `floor`, `ceil` or `trunc`) of the Rust core, or `None` for an array.

        The core moves the mean and keeps everything else (`Quantity::shift_mean_to`): the
        unit, the uncertainty and its sources, so the degrees of freedom and the correlation
        with the unrounded value survive, and `round` ties away from zero without overflow.
        A scalar of the default model crosses with its lineage (`from_lineage`) and comes back
        with the core's; a sampling mode already holds the core quantity. The core is scalar:
        an array (or a value live in a JAX/torch graph) answers `None` and keeps its backend.
        """
        from physure._core import Lineage
        from physure._core import Quantity as RustCoreQuantity

        core = self._core_magnitude
        if isinstance(core, RustCoreQuantity):
            # the sampling modes, as `_check_and_handle_rust_propagation` delegates them
            return type(self).from_input(
                getattr(core, op)(*args), self.unit, self.system
            )
        magnitude: Any = self.magnitude
        if self._backend.is_array(magnitude) or self._backend.is_tracing(
            magnitude
        ):
            return None
        model = self._uncertainty_obj
        lineage = (
            model.native
            if isinstance(model, LineageModel)
            else Lineage.exact()
        )
        result = getattr(
            RustCoreQuantity.from_lineage(
                float(magnitude), _ensure_rational(self.unit), lineage
            ),
            op,
        )(*args)
        if isinstance(model, LineageModel):
            uncertainty: Any = LineageModel(native=result.lineage)
        elif model is not None:
            # `python_lineage()` and `uncorrelated` have no core form; the core keeps the
            # uncertainty as it is, so the model is kept as it is
            uncertainty = model
        else:
            uncertainty = self.uncertainty
        return type(self).from_input(
            result.magnitude, self.unit, self.system, uncertainty
        )

    def __round__(self, ndigits: int | None = None) -> Quantity:
        """The quantity rounded by the core: ties away from zero, as PHS rounds.

        `round(q)` is `round(q, 0)`; `ndigits` is an integer (a negative one rounds to a
        power of ten). The unit, the uncertainty, its sources and degrees of freedom are
        kept, so `q - round(q, 1)` has no uncertainty. An array-backed magnitude keeps its
        backend's own elementwise rounding (JAX: half to even).

        Examples:
            >>> from physure import Q_
            >>> round(Q_(2.5, "m"))
            Quantity(3.0, m)
            >>> round(Q_(0.125, "m"), 2)
            Quantity(0.13, m)
            >>> x = Q_(1.234, "m", 0.1)
            >>> (x - round(x, 1)).uncertainty
            0.0
        """
        places = 0 if ndigits is None else operator.index(ndigits)
        rounded = self._round_in_core("round", places)
        if rounded is not None:
            return rounded
        val = round(  # pyright: ignore[reportCallIssue]
            self.magnitude,  # pyright: ignore[reportArgumentType]
            ndigits,  # pyright: ignore[reportArgumentType]
        )
        return Quantity.from_input(
            val, self.unit, self.system, self.uncertainty
        )

    def __floor__(self) -> Quantity:
        """The core's floor: the integer below, the unit and uncertainty kept."""
        floored = self._round_in_core("floor")
        if floored is not None:
            return floored
        return Quantity.from_input(
            math.floor(  # pyright: ignore[reportCallIssue]
                self.magnitude  # pyright: ignore[reportArgumentType]
            ),
            self.unit,
            self.system,
            self.uncertainty,
        )

    def __ceil__(self) -> Quantity:
        """The core's ceil: the integer above, the unit and uncertainty kept."""
        ceiled = self._round_in_core("ceil")
        if ceiled is not None:
            return ceiled
        return Quantity.from_input(
            math.ceil(  # pyright: ignore[reportCallIssue]
                self.magnitude  # pyright: ignore[reportArgumentType]
            ),
            self.unit,
            self.system,
            self.uncertainty,
        )

    def __trunc__(self) -> Quantity:
        """The core's trunc: towards zero, the unit and uncertainty kept."""
        truncated = self._round_in_core("trunc")
        if truncated is not None:
            return truncated
        return Quantity.from_input(
            math.trunc(self.magnitude),  # pyright: ignore[reportArgumentType]
            self.unit,
            self.system,
            self.uncertainty,
        )

    # --- Container / Array Methods ---
    def __array__(self, dtype: Any = None, copy: bool | None = None) -> Any:
        """The quantity as NumPy sees it: an object array of quantities, never silent numbers.

        Without a `dtype` (or with `object`) the array holds the quantity itself (0-d for
        a scalar) or, for an array-backed quantity, its elements as scalar quantities
        with their units and uncertainties (`q[i]`). A list of quantities becomes such
        an array, so `np.sum`, `np.dot`, `np.cross`, `np.prod`, ... run through
        `Quantity` arithmetic and keep the unit and the uncertainty.

        A numeric `dtype` (`np.asarray(q, dtype=float)`) is a request for numbers: the
        value in the coherent unit, with a `UnitStrippedWarning` that names what is
        dropped. `q.to(unit).magnitude` is the explicit, silent way.
        """
        import numpy as np

        if dtype is None or np.dtype(dtype) == np.dtype(object):
            if copy is False:
                raise ValueError(
                    "a Quantity becomes a new object array of quantities, it cannot be "
                    "viewed without a copy: use q.magnitude for the numbers"
                )
            return self._object_array(np)
        values = self._stripped_value(f"np.asarray(dtype={np.dtype(dtype)})")
        if isinstance(values, Fraction):
            values = _as_float(values)
        return np.asarray(values, dtype=dtype)

    def _object_array(self, np: Any) -> Any:
        """The quantity, or its elements as scalar quantities, in an object array of its shape."""
        magnitude: Any = self.magnitude
        shape = (
            tuple(self._backend.shape(magnitude))
            if self._backend.is_array(magnitude)
            else ()
        )
        if not shape:
            # an element assignment stores the object itself, where np.array would ask
            # it for an array again
            holder = np.empty(1, dtype=object)
            holder[0] = self
            return holder.reshape(())
        elements = np.empty(math.prod(shape), dtype=object)
        if isinstance(magnitude, np.ndarray):
            self._fill_numpy_elements(np, elements, magnitude, shape)
        else:
            for position, index in enumerate(np.ndindex(*shape)):
                elements[position] = self[index]
        return elements.reshape(shape)

    def _fill_numpy_elements(
        self, np: Any, elements: Any, magnitude: Any, shape: tuple[int, ...]
    ) -> None:
        """`elements[i]` is `q[i]`, built from one element's state instead of `from_input`.

        Building each element through `from_input` took about 50 us, so
        `np.asarray` of 1e5 elements took 5 s. Every element shares the unit, system,
        dimension and backend of the first; its own are the magnitude and, as `q[i]`
        gives it, a new measurement of its standard deviation (`from_standard`).
        """
        first = self[(0,) * len(shape)]
        state = first.__dict__
        cls = type(first)
        values = magnitude.reshape(-1)
        sigmas = np.broadcast_to(
            np.asarray(self._numeric_std_dev, dtype=float), shape
        ).reshape(-1)
        for position in range(values.size):
            element = object.__new__(cls)
            element.__dict__.update(state)
            element.__dict__["_core_magnitude"] = values[position]
            sigma = sigmas[position]
            model = Uncertainty.from_standard(float(sigma)) if sigma else None
            element.__dict__["_core_uncertainty"] = (
                model.std_dev if model is not None else sigma
            )
            element.__dict__["_uncertainty_obj"] = model
            elements[position] = element

    def __float__(self) -> float:
        """The value in the coherent unit as a bare number.

        `float(Q_(50, "percent"))` is `0.5` and `float(Q_(1, "km"))` is `1000.0`. A
        quantity with a unit (other than a dimensionless one) or with an uncertainty
        warns (`UnitStrippedWarning`) with what is dropped; `q.to(unit).magnitude` is
        the explicit, silent way.
        """
        return _as_float(self._stripped_value("float()"))

    def __complex__(self) -> complex:
        """The value in the coherent unit as a complex number; warns as `float` does."""
        value = self._stripped_value("complex()")
        return complex(
            _as_float(value) if isinstance(value, Fraction) else value
        )

    # --- Math & Vector Ops ---

    def dot(self, other: Quantity) -> Quantity:
        """Computes dot product."""
        # ponytail: defensive runtime guard against duck-typed callers that
        # bypass the static type hint; basedpyright sees `other: Quantity`
        # as already-narrowed, so it flags this as unreachable/unnecessary.
        if not isinstance(  # pyright: ignore[reportUnnecessaryIsInstance]
            other, Quantity
        ):
            raise TypeError(  # pyright: ignore[reportUnreachable]
                f"dot requires Quantity, got {type(other)}"
            )

        mag = self._backend.dot(
            self.magnitude,  # pyright: ignore[reportArgumentType]
            other.magnitude,
        )
        new_unit = self.unit * other.unit
        # Uncertainty ignored for now
        return Quantity.from_input(mag, new_unit, self.system)

    def cross(self, other: Quantity) -> Quantity:
        """Computes cross product."""
        if not isinstance(  # pyright: ignore[reportUnnecessaryIsInstance]
            other, Quantity
        ):
            raise TypeError(  # pyright: ignore[reportUnreachable]
                f"cross requires Quantity, got {type(other)}"
            )

        mag = self._backend.cross(
            self.magnitude,  # pyright: ignore[reportArgumentType]
            other.magnitude,
        )
        new_unit = self.unit * other.unit
        # Uncertainty ignored for now
        return Quantity.from_input(mag, new_unit, self.system)

    def __bool__(self) -> bool:
        """Returns the boolean value of the magnitude."""
        if hasattr(self.magnitude, "ndim") and self.magnitude.ndim > 0:
            if self.magnitude.size > 1:
                raise ValueError(
                    "The truth value of a Quantity with more than one element is ambiguous. "
                    "Use a.any() or a.all()"
                )
            return bool(self.magnitude.item())
        return bool(self.magnitude)

    def __len__(self) -> int:
        """Returns length (if array)."""
        return len(self.magnitude)  # pyright: ignore[reportArgumentType]

    def __iter__(self) -> Iterator[Any]:
        """Scalar: yields (magnitude, unit). Array: yields elements."""
        try:
            n = len(self.magnitude)  # pyright: ignore[reportArgumentType]
        except TypeError:
            # Scalar case: magnitude has no len()
            yield self.magnitude
            yield self.unit
            return
        if n == 0:
            # numpy 0-d scalar-like: defines len() but returns 0
            yield self.magnitude
            yield self.unit
            return
        # Array case: iterate elements as before
        for i in range(n):
            yield self[i]

    # --- Redundant definitions removed ---
    # The __add__ and __sub__ methods are now defined earlier.
    # We remove these legacy functional-based implementations to ensure consistency.

    # We also remove __mul__ and __truediv__ redefinitions if they exist below,
    # as they should also use the backend/vectorized logic defined above.

    # (Redundant arithmetic methods removed)

    def __getitem__(self, key: Any) -> Quantity:
        """Slices the quantity."""
        new_mag = self.magnitude[key]  # pyright: ignore[reportIndexIssue]

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

        # `from_input` keeps the model and gives the element its numeric standard
        # deviation: `q[0].uncertainty` is a number, as for any scalar quantity (it
        # used to be the model object, which `np.asarray(q)` elements then carried)
        return type(self).from_input(
            new_mag,
            self.unit,
            self.system,
            uncertainty=new_unc_val if new_unc_obj is None else new_unc_obj,
        )

    def __setitem__(self, key: Any, value: Any) -> None:
        """Sets item in the quantity."""
        # This mutates magnitude.
        # We need to ensure units match.
        if isinstance(value, Quantity):
            val_converted = value.to(self.unit)
            self.magnitude[  # pyright: ignore[reportIndexIssue]
                key
            ] = val_converted.magnitude
            # We should also update uncertainty...
            # This is complex for immutable/updates.
            # If magnitude is mutable (numpy), this works for value.
            # Uncertainty update is ignored here (limitation).
        else:
            # Assume value is magnitude in same unit
            self.magnitude[  # pyright: ignore[reportIndexIssue]
                key
            ] = value

    @property
    def uncertainty_obj(self) -> Uncertainty:
        """Returns the rich uncertainty object (legacy support)."""
        if self._uncertainty_obj is not None:
            return self._uncertainty_obj
        # Fallback to VarianceModel if it was numeric
        from physure.domain.measurement.uncertainty import VarianceModel

        return VarianceModel(self.uncertainty)

    @property
    def vector_slice(self) -> slice | None:
        """Returns the vector slice if using vectorized uncertainty."""
        if hasattr(self._uncertainty_obj, "vector_slice"):
            return self._uncertainty_obj.vector_slice
        return None

    @property
    def lineage(self) -> dict[str, Any]:
        """Returns the lineage dictionary if using correlated uncertainty."""
        if hasattr(self._uncertainty_obj, "lineage"):
            return self._uncertainty_obj.lineage
        return {}

    # --- Comparison Methods ---
    def _point_against_interval(self, other: Quantity) -> bool:
        """A temperature point (degC) against an interval (delta_degC), as the core says.

        Both are of one dimension already; only a temperature asks the core.
        """
        return self.dimension.exponents == _TEMPERATURE and (
            self.system.core_registry.point_against_interval(
                self.unit, other.unit
            )
        )

    def _same_plain_unit(self, other: Quantity) -> bool:
        """One unit on both sides, neither made a point nor an interval.

        Then the two are of one kind (both degC, both delta_degC, both a K of
        unknown kind), so a comparison needs no question to the core.
        """
        unit, other_unit = self.unit, other.unit
        if unit is other_unit:
            # one flyweight: one kind, whatever it is (a point is never an
            # interval of itself)
            return True
        return (
            unit == other_unit
            and not (unit.point or unit.interval)
            and not (other_unit.point or other_unit.interval)
        )

    def _refuse_point_against_interval(self, other: Quantity) -> None:
        """Raises the core's words for a temperature point against an interval."""
        point, interval = (self, other) if self._is_point() else (other, self)
        raise IncompatibleUnitsError(
            self.unit,
            other.unit,
            _point_against_interval_message(str(point), str(interval)),
        )

    def _compare(self, other: Any, op: Any) -> Any:
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                raise IncompatibleUnitsError(self.unit, other.unit)
            # Convert to self unit for comparison
            # the same unit, neither made a point nor an interval: one kind
            if self._same_plain_unit(other):
                return op(self.magnitude, other.magnitude)
            # `5 delta_degC < 20 degC` has no answer: an interval is not where a
            # temperature is, also when both are written in K (a point and a
            # difference of two points)
            if self._point_against_interval(other):
                self._refuse_point_against_interval(other)
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
        """Equal values in the coherent unit (`1 m == 100 cm`); the uncertainty is not compared.

        Both sides are the number `_coherent_value` gives (converted exactly, rounded
        once), so `==` is an equivalence on them, `==` and `hash` cannot disagree, and
        `20 degC == 68 degF` holds. Two magnitudes in the same unit that is not the
        coherent one are compared through it too: comparing them directly would make
        `-55.2 arcsec == x rad == -55.199999999999996 arcsec` true and the two arcsec
        values unequal. A temperature point is never an interval
        (`20 degC != 5 delta_degC`); a plain `K` equals either by value.
        A dimensionless quantity equals the plain number of its coherent value
        (`Q_(100, "percent") == 1`).

        Examples:
            >>> from physure import Q_
            >>> Q_(20, "degC") == Q_(68, "degF"), Q_(1, "inch") == Q_(2.54, "cm")
            (True, True)
            >>> Q_(50, "percent") == 0.5
            True
        """
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                return False
            # a point is never an interval (`<` raises for the pair)
            if self._point_against_interval(other):
                return False
            try:
                unit, value = self._coherent_value()
                other_unit, other_value = other._coherent_value()
            except Exception:
                return False
            # `sr` and `rad` share the dimension A and are never the same quantity
            return unit == other_unit and value == other_value
        if _is_plain_real(other) and self.dimension.is_dimensionless:
            try:
                _, value = self._coherent_value()
            except Exception:
                return False
            return value == _python_scalar(other)
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


# --- PyTorch Integration ---
_torch_pytree_registered = False


def _register_torch_pytree() -> None:
    """Register Quantity as a torch pytree node. Idempotent; no-op without torch."""
    global _torch_pytree_registered
    if _torch_pytree_registered:
        return
    try:
        from torch.utils import _pytree
    except (ImportError, AttributeError):
        return

    def _torch_flatten_quantity(
        q: Quantity,
    ) -> tuple[
        list[Any],  # pyright: ignore[reportExplicitAny]
        tuple[Any, Any, Any],  # pyright: ignore[reportExplicitAny]
    ]:
        children, context = q.tree_flatten()
        return [children[0], children[1]], context

    def _torch_unflatten_quantity(
        children: Iterable[Any],  # pyright: ignore[reportExplicitAny]
        context: Any,  # pyright: ignore[reportAny, reportExplicitAny]
    ) -> Quantity:
        children_list = list(children)
        return Quantity.tree_unflatten(
            context, (children_list[0], children_list[1])
        )

    _pytree.register_pytree_node(
        Quantity,
        _torch_flatten_quantity,
        _torch_unflatten_quantity,
    )
    _torch_pytree_registered = True


# Registering eagerly here used to `from torch.utils import _pytree`, pulling
# all of torch (~2s) into every first use. Only register if torch is already
# imported; torch users who arrive later get it via the torch backend load.
if "torch" in sys.modules:
    _register_torch_pytree()


# --- JAX Integration ---
_jax_pytree_registered = False


def _register_jax_pytree() -> None:
    """Register Quantity as a JAX pytree node. Idempotent; no-op without JAX."""
    global _jax_pytree_registered
    if _jax_pytree_registered:
        return
    try:
        import jax
    except ImportError:
        return
    jax.tree_util.register_pytree_node(
        Quantity,
        Quantity.tree_flatten,
        Quantity.tree_unflatten,
    )
    _jax_pytree_registered = True


# The backends announce themselves when they load; the model asks to be told.
BackendManager.on_load("torch", _register_torch_pytree)
BackendManager.on_load("jax", _register_jax_pytree)
