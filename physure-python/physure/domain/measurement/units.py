# physure/domain/measurement/units.py
"""Defines the CompoundUnit class."""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, ClassVar, cast, overload

# sympy imported lazily in to_latex()
from physure._core import RationalUnit, register_quantity_factory
from physure._core import UnitMismatchError as _CoreUnitMismatchError
from physure._core import UnitRegistry as _CoreUnitRegistry
from physure._core import UnknownUnitError as _CoreUnknownUnitError
from physure.core.dispatcher import BackendManager
from physure.core.registry import UnitRegistry
from physure.domain.exceptions import (
    IncompatibleUnitsError,
    UnknownUnitError,
)
from physure.domain.measurement.base_entity import (
    BaseExponentEntity,
    ExponentsDict,
    exact_power,
)

if TYPE_CHECKING:
    from collections.abc import Callable

    from numpy.typing import NDArray

    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem


# --- Dependency Injection for System ---


def get_default_system() -> UnitSystem:
    """Retrieves the currently active UnitSystem from the context.

    This proxies to `physure.application.context.get_current_system()`,
    ensuring thread-safety and correct context isolation.
    """
    # Import inside function to avoid circular import at module level
    # units -> context (ok) but safer if context imports units later indirectly
    from physure.application.context import get_current_system

    return get_current_system()


def reconstruct_compound_unit(exponents: ExponentsDict) -> CompoundUnit:
    """Factory function to reconstruct CompoundUnit instances.

    This implements the Reconstruct-by-Value pattern to decouple serialization
    from runtime namespace pollution.
    """
    from physure.domain.measurement.units import (
        CompoundUnit,
        _CompoundUnit,
        get_default_system,
    )

    # Ensure active system context to prevent registry detachment
    get_default_system()

    # Use the stable reference if the public one is corrupted
    cls = CompoundUnit
    # ponytail: guards against the public CompoundUnit name being
    # monkeypatched/corrupted at runtime; statically cls is always a
    # type, but this is a deliberate defensive check, not dead code.
    if not isinstance(cls, type):  # pyright: ignore[reportUnnecessaryIsInstance]
        cls = _CompoundUnit

    return cls(exponents)


_CORE_ERRORS = (_CoreUnknownUnitError, _CoreUnitMismatchError)


def _python_error(
    error: ValueError, source: CompoundUnit, target: CompoundUnit
) -> Exception:
    """The exception Python raises for an error of the core registry."""
    if isinstance(error, _CoreUnitMismatchError):
        return IncompatibleUnitsError(source, target)
    suggestion = getattr(error, "suggestion", None)
    return UnknownUnitError(
        getattr(error, "symbol", str(error)),
        [suggestion] if suggestion else None,
    )


@dataclass(frozen=True)
class CompoundUnit(  # pyright: ignore[reportUnsafeMultipleInheritance]
    BaseExponentEntity, RationalUnit
):
    """Represents a unit composed of base units raised to various powers."""

    _is_compound: ClassVar[bool] = True

    def dimension(self, system: UnitSystem | None = None) -> Dimension:
        """Calculates the physical dimension of the composite unit."""
        from physure.domain.measurement.dimensions import Dimension

        if system is None:
            system = get_default_system()

        try:
            return Dimension(system.core_registry.dimension_of(self))
        except _CORE_ERRORS as error:
            raise _python_error(error, self, self) from error

    # one instance per set of powers, found by the canonical key or by the
    # items that were given (so the usual call does no arithmetic at all)
    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}

    # ponytail: the Flyweight cache is keyed by exponents only and always
    # stores/returns a plain CompoundUnit, so `Self` would overpromise for
    # subclasses; CompoundUnit is the honest contract here.
    def __new__(cls, exponents: ExponentsDict) -> CompoundUnit:  # noqa: PYI034
        """Create or retrieve a cached CompoundUnit instance.

        The powers are exact: an int or a Fraction, whatever number they were
        given as (a float is the closest simple fraction).
        """
        items = tuple(exponents.items()) if exponents else ()
        try:
            found = cls._cache.get(items)
        except TypeError:  # an unhashable power: no shortcut
            items = None
            found = None
        if found is not None:
            return found

        # Sort keys to ensure consistent repr and hash
        key = tuple(
            sorted(
                (symbol, pair)
                for symbol, value in exponents.items()
                if (pair := exact_power(value))[0] != 0
            )
        )
        instance = cls._cache.get(key)
        if instance is None:
            instance = RationalUnit.__new__(cls, dict(key))
            # read-only: the instance is shared by everything that builds it
            object.__setattr__(
                instance,
                "exponents",
                MappingProxyType(
                    {
                        symbol: n if d == 1 else Fraction(n, d)
                        for symbol, (n, d) in key
                    }
                ),
            )
            cls._cache[key] = cast("CompoundUnit", instance)
        if items is not None:
            cls._cache[items] = cast("CompoundUnit", instance)
        return cast("CompoundUnit", instance)

    # ponytail: real initialization happens in __new__'s singleton cache;
    # this __init__ is intentionally a no-op, so it deliberately skips
    # calling super().__init__().
    def __init__(  # pyright: ignore[reportMissingSuperCall]
        self, exponents: ExponentsDict
    ) -> None:
        """Initializes the compound unit with a dictionary of exponents."""

    def __reduce__(self) -> tuple[Any, ...]:
        """Custom pickling to ensure Flyweight pattern (cache) is used.

        By returning (reconstruct_compound_unit, (args,)), we bypass direct
        class lookups that can be corrupted by namespace shadowing.
        """
        return (reconstruct_compound_unit, (dict(self.exponents),))

    def __hash__(self) -> int:
        """Returns a hash value for the compound unit."""
        return super().__hash__()

    @classmethod
    def from_rational_unit(cls, r_unit) -> CompoundUnit:
        """Creates a CompoundUnit from a Rust RationalUnit.

        Args:
            r_unit (physure._core.RationalUnit): The Rust unit.

        Returns:
            CompoundUnit: The corresponding Python unit.
        """
        return cls(r_unit.dimensions)

    # --- System-Dependent Methods ---
    @classmethod
    def of_dimension(
        cls, dimension: Dimension, system: UnitSystem
    ) -> CompoundUnit:
        """The unit that measures a dimension in the base units of the system.

        Raises:
            ValueError: A base dimension has no base unit in the system.
        """
        powers = {
            symbol: (Fraction(power).numerator, Fraction(power).denominator)
            for symbol, power in dimension.exponents.items()
        }
        return _CompoundUnit(
            system.core_registry.unit_of_dimension(powers).dimensions
        )

    def base_unit(self, system: UnitSystem) -> CompoundUnit:
        """This unit in base symbols, with no scale and no zero point (degC is K)."""
        try:
            return _CompoundUnit(system.core_registry.expand(self).dimensions)
        except _CORE_ERRORS as error:
            raise _python_error(error, self, self) from error

    def is_linear(self, system: UnitSystem) -> bool:
        """Checks if the unit is a plain scale: no zero point, no logarithm."""
        try:
            return system.core_registry.is_linear_of(self)
        except _CORE_ERRORS as error:
            raise _python_error(error, self, self) from error

    def kind(self, system: UnitSystem) -> str:
        """Determines if the unit is 'absolute' (Point) or 'delta' (Vector)."""
        try:
            return system.core_registry.kind_of(self)
        except _CORE_ERRORS as error:
            raise _python_error(error, self, self) from error

    def conversion_factor_to(
        self, target: CompoundUnit, system: UnitSystem | None = None
    ) -> float:
        """Calculate the conversion factor to a target unit within a system.

        Args:
        target (CompoundUnit): The unit to convert to.
        system (UnitSystem | None): The unit system for conversion.
                                    If None, the default system is used.

        Returns:
        float: The numerical factor to multiply by to convert to the target
        unit.

        Raises:
        IncompatibleUnitsError: If the units have incompatible dimensions.
        """
        if system is None:
            system = get_default_system()

        try:
            return system.core_registry.factor_between(self, target)
        except _CORE_ERRORS as error:
            raise _python_error(error, self, target) from error

    @overload
    def __rmul__(self, other: float) -> Quantity[float, float, Any]: ...

    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any], Any]: ...

    def __rmul__(self, other: Any) -> Any:
        """Handle right-side multiplication, typically for creating a Quantity.

        This allows for intuitive syntax like 5 * meter.

        Args:
        other (Any): The scalar or array to be multiplied with the unit.

        Returns:
        Any: A new Quantity instance, or NotImplemented if the operation is
        not supported.
        """
        from physure.domain.measurement.quantity import Quantity

        backend = BackendManager.get_backend(other)
        # Check if scalar (int/float) or array via backend
        is_valid = isinstance(other, float | int) or backend.is_array(other)

        if is_valid:
            # Implicitly use default system for syntactic sugar
            try:
                sys = get_default_system()
            except RuntimeError:
                # If no system is active, we cannot create a Quantity
                # with defaults
                return NotImplemented

            return Quantity.from_input(value=other, unit=self, system=sys)
        return NotImplemented

    def to_string(
        self,
        system: UnitSystem | None = None,
        use_alias: bool = False,
    ) -> str:
        """Generate a human-readable string representation of the unit.

        Args:
        system (UnitSystem | None, optional): The system to check for aliases.
        use_alias (bool, optional): If True, uses a registered alias if one
        exists. Defaults to False.

        Returns:
        str: The string representation of the unit.
        """
        if use_alias and system:
            try:
                return system.core_registry.alias_of(self)
            except _CORE_ERRORS as error:
                raise _python_error(error, self, self) from error

        return super().__str__()

    def __format__(self, format_spec: str) -> str:
        """Format the CompoundUnit using a format specification."""
        return self.to_string(use_alias=format_spec.startswith("alias"))

    def __pow__(self, power: float | tuple[int, int]) -> CompoundUnit:
        """Raises the unit to a power: an integer, or the fraction a float stands for."""
        if type(power) is int:
            return _CompoundUnit(
                {k: v * power for k, v in self.exponents.items()}
            )
        if isinstance(power, float) and power.is_integer():
            return self ** int(power)
        # a fraction: the algebra of the core, with the exact power
        return _CompoundUnit(
            RationalUnit.__pow__(self, exact_power(power)).dimensions  # pyright: ignore[reportArgumentType]
        )

    # ponytail: BaseExponentEntity.__rtruediv__ is a singledispatchmethod;
    # CompoundUnit intentionally replaces it with a plain method for the
    # `1 / unit` case instead of participating in the dispatch.
    def __rtruediv__(  # pyright: ignore[reportIncompatibleVariableOverride]
        self, other: Any
    ) -> CompoundUnit:
        """Right division (1 / unit)."""
        if other == 1:
            return self**-1
        return NotImplemented

    def to_latex(self) -> str:
        """Generate a LaTeX representation of the unit for display."""
        if not self.exponents:
            return ""

        import sympy as sp

        symbols = {name: sp.Symbol(name) for name in self.exponents}

        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
            # Clean up exponent if it's a float-int (e.g. 1.0 -> 1)
            # This ensures latex output is clean: m^1 not m^1.0
            if isinstance(exponent, float) and exponent.is_integer():
                exponent = int(exponent)

            expr *= symbols[unit_name] ** exponent

        return sp.latex(expr, mul_symbol="dot")

    def _repr_latex_(self) -> str:
        """Provide a LaTeX representation for automatic rendering in Jupyter.

        Returns:
        str: The LaTeX string wrapped in '$' for display.
        """
        return f"${self.to_latex()}$"

    @property
    def is_dimensionless(self) -> bool:
        """Check if the unit is dimensionless (i.e., has no components).

        Returns:
        bool: True if the unit is dimensionless, False otherwise.
        """
        return not bool(self.exponents)

    def simplify(self, system: UnitSystem) -> CompoundUnit:
        """Simplifies the unit by expanding derived units into base components.

        This method uses the unit "recipes" defined in the given system to
        recursively substitute derived units (like 'N' or 'J') until only
        base units remain. The exponents are then consolidated.

        Args:
            system (UnitSystem): The system containing the unit recipes.

        Returns:
            A new, simplified CompoundUnit instance.
        """
        try:
            simplified = system.core_registry.simplify(self)
        except _CORE_ERRORS as error:
            raise _python_error(error, self, self) from error
        return _CompoundUnit(simplified.dimensions)


# Capture a stable reference to the class to survive namespace shadowing
_CompoundUnit = CompoundUnit

# --- Registry Initialization ---

# Initialize the global registry instance: the units of physure.conf are the
# ones the core's registry of the embedded catalog knows, so none is listed
# here. Plugins are discovered lazily, by UnitRegistry itself.
units = UnitRegistry()
units.register_catalog(
    _CoreUnitRegistry.from_conf(), lambda name: CompoundUnit({name: 1})
)


def _quantity_from_sugar(value: Any, unit: Any) -> Any:
    """What `5 * unit` calls for a unit of the core: the same quantity `5 * meter` builds."""
    return CompoundUnit(unit.dimensions).__rmul__(value)


register_quantity_factory(_quantity_from_sugar)


# Inject Python-side functionality into the Rust base class
# This ensures that results of Rust unit arithmetic (which return raw RationalUnit
# objects) still have access to the high-level Python methods.
# ponytail: monkey-patches an operator onto the Rust RationalUnit class at
# runtime via getattr/setattr; both the wrapped operator and its operands
# are opaque to the checker at this reflection boundary, so Any is the
# honest type here, not a gap in our own annotations.
def wrap_arithmetic(
    op_name: str,
) -> Callable[[Any, Any], Any] | None:  # pyright: ignore[reportExplicitAny]
    """Wraps a RationalUnit operator to coerce plain numbers."""
    orig_op = getattr(RationalUnit, op_name, None)
    if orig_op:

        def wrapped(  # pyright: ignore[reportAny]
            self: Any,  # pyright: ignore[reportAny, reportExplicitAny]
            other: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        ) -> Any:
            if isinstance(other, (int, float, complex)):
                return self  # pyright: ignore[reportAny]
            res = orig_op(self, other)  # pyright: ignore[reportAny]
            if isinstance(res, RationalUnit):
                # Wrap in CompoundUnit to use Flyweight cache
                return _CompoundUnit(res.dimensions)
            return res  # pyright: ignore[reportAny]

        return wrapped
    return None


# Methods to copy from CompoundUnit to RationalUnit
for method_name in [
    "dimension",
    "simplify",
    "conversion_factor_to",
    "to_latex",
    "_repr_latex_",
    "is_dimensionless",
    "is_linear",
    "kind",
]:
    if hasattr(CompoundUnit, method_name):
        setattr(RationalUnit, method_name, getattr(CompoundUnit, method_name))

# Arithmetic methods to wrap with Flyweight cache
for op in [
    "__mul__",
    "__truediv__",
    "__rtruediv__",
    "__pow__",
]:
    wrapped_op = wrap_arithmetic(op)
    if wrapped_op:
        setattr(RationalUnit, op, wrapped_op)

# Stable reference for internal use (resilience against shadowing)
_STABLE_COMPOUND_UNIT = CompoundUnit
