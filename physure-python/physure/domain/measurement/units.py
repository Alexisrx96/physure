# physure/domain/measurement/units.py
"""Defines the CompoundUnit class."""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from fractions import Fraction
from typing import TYPE_CHECKING, Any, ClassVar, cast, overload

# sympy imported lazily in to_latex()
from physure.core.dispatcher import BackendManager
from physure.core.registry import UnitRegistry
from physure.domain.exceptions import (
    IncompatibleUnitsError,
    UnknownUnitError,
)
from physure.domain.measurement.converters import (
    LinearConverter,
    UnitConverter,
)

if TYPE_CHECKING:
    from collections.abc import Callable

    from numpy.typing import NDArray

    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.notation.typing import ExponentsDict


# --- Dependency Injection for System ---


def normalize_exponents(exponents: ExponentsDict) -> dict[str, float | int]:
    """Normalizes a dictionary of exponents, removing zeros and converting tuples."""
    normalized = {}
    for k, v in exponents.items():
        if isinstance(v, (list, tuple)):
            v = v[0] / v[1]

        if v == 0:
            continue

        if isinstance(v, float) and v.is_integer():
            normalized[k] = int(v)
        else:
            normalized[k] = v
    return normalized


def get_default_system() -> UnitSystem:
    """Retrieves the currently active UnitSystem from the context.

    This proxies to `physure.application.context.get_current_system()`,
    ensuring thread-safety and correct context isolation.
    """
    # Import inside function to avoid circular import at module level
    # units -> context (ok) but safer if context imports units later indirectly
    from physure.application.context import get_current_system

    return get_current_system()


@dataclass(frozen=True)
class Unit:
    """Represents a single atomic unit definition."""

    name: str
    symbol: str
    dimension: Dimension
    converter: UnitConverter

    @property
    def conversion_factor(self) -> float:
        """Helper to maintain backward compatibility for linear units."""
        if isinstance(self.converter, LinearConverter):
            return self.converter.scale
        raise ValueError(f"La unidad {self.name} no es lineal.")


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


def _is_identity_unit(base_unit: Any, unit_name: str) -> bool:
    """Returns True if base_unit is a trivial identity wrapper for unit_name."""
    if not (
        hasattr(base_unit, "exponents") or hasattr(base_unit, "dimensions")
    ):
        return False
    deps = (
        base_unit.exponents
        if hasattr(base_unit, "exponents")
        else base_unit.dimensions
    )
    return len(deps) == 1 and deps.get(unit_name) == 1


def _raise_unknown_unit(unit_name: str, system: Any) -> None:
    """Raises UnknownUnitError with difflib suggestions."""
    import difflib

    known = list(system.UNIT_DIMENSIONS.keys())
    suggestions = difflib.get_close_matches(unit_name, known, n=3, cutoff=0.6)
    raise UnknownUnitError(unit_name, suggestions or None)


def _resolve_unit_dim(unit_name: str, system: Any, Dimension: type) -> Any:
    """Returns the Dimension for unit_name within the given system."""
    if unit_name in system.UNIT_DIMENSIONS:
        return system.UNIT_DIMENSIONS[unit_name]

    base_unit = system.get_unit(unit_name)
    if _is_identity_unit(base_unit, unit_name):
        _raise_unknown_unit(unit_name, system)

    if hasattr(base_unit, "dimension"):
        return base_unit.dimension(system)
    return Dimension({unit_name: 1})


from physure._core import RationalUnit
IS_CORE_AVAILABLE = True


from physure.domain.notation.base_entity import BaseExponentEntity


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

        dims = Dimension({})
        for unit_name, exp_val in self.exponents.items():
            exp = (
                exp_val[0] / exp_val[1]
                if isinstance(exp_val, tuple)
                else exp_val
            )
            unit_dim = _resolve_unit_dim(unit_name, system, Dimension)
            dims = dims * (unit_dim**exp)
        return dims

    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}

    # ponytail: the Flyweight cache is keyed by exponents only and always
    # stores/returns a plain CompoundUnit, so `Self` would overpromise for
    # subclasses; CompoundUnit is the honest contract here.
    def __new__(cls, exponents: ExponentsDict) -> CompoundUnit:  # noqa: PYI034
        """Create or retrieve a cached CompoundUnit instance."""
        normalized_exponents = normalize_exponents(exponents)
        # Sort keys to ensure consistent repr and hash
        sorted_items = sorted(normalized_exponents.items())
        key = tuple(sorted_items)

        # Check raw cache
        instance = cls._cache.get(key)
        if instance is not None:
            return instance

        instance = RationalUnit.__new__(cls, normalized_exponents)

        # Consistent order for exponents dict to ensure uniform __repr__
        ordered_exponents = dict(sorted_items)
        object.__setattr__(instance, "exponents", ordered_exponents)

        # Singleton pattern
        cls._cache[key] = cast("CompoundUnit", instance)
        return cast("CompoundUnit", instance)

    # ponytail: real initialization happens in __new__'s singleton cache;
    # this __init__ is intentionally a no-op, so it deliberately skips
    # calling super().__init__().
    def __init__(  # pyright: ignore[reportMissingSuperCall]
        self, exponents: ExponentsDict
    ) -> None:
        """Initializes the compound unit with a dictionary of exponents."""

    def __post_init__(self) -> None:
        """Eliminamos cualquier unidad con exponente 0."""
        clean_exponents = {k: v for k, v in self.exponents.items() if v != 0}
        object.__setattr__(self, "exponents", clean_exponents)

    def __reduce__(self) -> tuple[Any, ...]:
        """Custom pickling to ensure Flyweight pattern (cache) is used.

        By returning (reconstruct_compound_unit, (args,)), we bypass direct
        class lookups that can be corrupted by namespace shadowing.
        """
        return (reconstruct_compound_unit, (self.exponents,))

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
    def _compound_factor(self, system: UnitSystem) -> float:
        """Calculate the unit's total conversion factor relative to SI units."""
        factor = 1.0
        for unit, exp in self.exponents.items():
            if unit == "noprefix":
                continue
            _unit = system.get_unit(unit)
            dim = _unit.dimension(system)
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if unit_def and hasattr(unit_def.converter, "scale"):
                conv_scale = unit_def.converter.scale
                # Handle tuple exponents (num, den) from RationalUnit
                exponent_val = (
                    exp[0] / exp[1] if isinstance(exp, (list, tuple)) else exp
                )
                factor *= conv_scale**exponent_val
        return factor

    def _compound_factor_exact(self, system: UnitSystem) -> Fraction | None:
        """Exact rational compound factor, or None if not representable.

        Returns None when any component's converter lacks an exact scale
        or has a non-integer exponent; callers fall back to float math.
        """
        factor = Fraction(1)
        for unit, exp in self.exponents.items():
            if unit == "noprefix":
                continue
            _unit = system.get_unit(unit)
            dim = _unit.dimension(system)
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if not (unit_def and hasattr(unit_def.converter, "scale")):
                continue
            exact = getattr(unit_def.converter, "exact", None)
            if isinstance(exp, (list, tuple)):
                num, den = exp
                if den != 1:
                    return None
                exp = num
            if exact is None or exp != int(exp):
                return None
            factor *= exact ** int(exp)
        return factor

    def is_linear(self, system: UnitSystem) -> bool:
        """Checks if all components of the unit use linear converters."""
        for unit, _ in self.exponents.items():
            if unit == "noprefix":
                continue
            unit_def = system.get_definition(unit)
            if unit_def and not unit_def.converter.is_linear:
                return False
        return True

    def kind(self, system: UnitSystem) -> str:
        """Determines if the unit is 'absolute' (Point) or 'delta' (Vector)."""
        e_list = list(self.exponents.items())
        if len(e_list) == 1:
            unit_name, exp = e_list[0]
            # Handle rational tuple
            is_unity = (exp == 1) or (
                isinstance(exp, (list, tuple)) and exp[0] == 1 and exp[1] == 1
            )

            if is_unity and unit_name != "noprefix":
                unit_def = system.get_definition(unit_name)
                if unit_def:
                    return getattr(unit_def, "kind", "delta")
        return "delta"

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

        if self.dimension(system) != target.dimension(system):
            raise IncompatibleUnitsError(self, target)
        source_exact = self._compound_factor_exact(system)
        if source_exact is not None:
            target_exact = target._compound_factor_exact(system)
            if target_exact is not None:
                # Single rounding at the boundary instead of one per scale.
                return float(source_exact / target_exact)
        source_factor = self._compound_factor(system)
        target_factor = target._compound_factor(system)
        return source_factor / target_factor

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
        alias_preference: str | None = None,
    ) -> str:
        """Generate a human-readable string representation of the unit.

        Args:
        system (UnitSystem | None, optional): The system to check for aliases.
        use_alias (bool, optional): If True, uses a registered alias if one
        exists. Defaults to False.
        alias_preference (str | None, optional): A preferred alias to use if
        multiple exist. Defaults to None.

        Returns:
        str: The string representation of the unit.
        """
        if use_alias and system:
            key = tuple(
                sorted((k, v) for k, v in self.exponents.items() if v != 0)
            )
            aliases = system.ALIASES.get(key, [])
            if aliases:
                if alias_preference and alias_preference in aliases:
                    return alias_preference
                return aliases[0]

        return super().__str__()

    def __format__(self, format_spec: str) -> str:
        """Format the CompoundUnit using a format specification."""
        return self.to_string(use_alias=format_spec.startswith("alias"))

    def __pow__(self, exponent: float | tuple[int, int]) -> CompoundUnit:
        """Power support with float-to-rational conversion."""
        if isinstance(exponent, (int, float)):
            # Use Python-side BaseExponentEntity logic for pure exponents
            # to avoid Rust RationalUnit issues with fractional powers.
            # Convert float to int if integral
            if isinstance(exponent, float) and exponent.is_integer():
                exponent = int(exponent)

            new_exponents = {
                k: v * exponent for k, v in self.exponents.items()
            }
            return _CompoundUnit(new_exponents)

        # For rational tuples, we use Rust core if available
        # ponytail: the Rust RationalUnit core accepts a rational
        # (numerator, denominator) tuple here even though the Python
        # stub's __pow__ only declares float (same "Rust core is
        # optional/dynamic" pattern as elsewhere in this file).
        res = super().__pow__(exponent)  # pyright: ignore[reportArgumentType]
        if hasattr(res, "dimensions"):
            return _CompoundUnit(res.dimensions)
        return res

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
        new_exponents: dict[str, float] = defaultdict(float)

        for unit_symbol, exponent in self.exponents.items():
            if unit_symbol in system._UNIT_RECIPES:
                recipe_unit = system._UNIT_RECIPES[unit_symbol]
                simplified_recipe = recipe_unit.simplify(system)

                for base_unit, base_exp in simplified_recipe.exponents.items():
                    new_exponents[base_unit] += base_exp * exponent
            else:
                new_exponents[unit_symbol] += exponent

        return _CompoundUnit(new_exponents)


# Capture a stable reference to the class to survive namespace shadowing
_CompoundUnit = CompoundUnit

# --- Registry Initialization ---

# Initialize the global registry instance
units = UnitRegistry()


def _register_core_units() -> None:
    """Helper to populate the registry from the generated index."""
    try:
        # Import the index generated by scripts/compile_units.py
        from physure.units._index import UNIT_INDEX
    except ImportError:
        # If the index hasn't been generated yet, skip registration.
        # This allows bootstrapping or running without generated units.
        return

    # ponytail: this loader dynamically imports a generated unit-index module
    # and getattr()s the requested symbol out of it, so the loaded value's
    # real type is unknowable to the checker until import time — Any is the
    # honest type here, not a gap in our own annotations.
    def _make_loader(scope_module: str, unit_name: str) -> Callable[[], Any]:  # pyright: ignore[reportExplicitAny]
        def loader() -> Any:  # pyright: ignore[reportAny, reportExplicitAny]
            # Dynamically import the scope module (e.g., physure.units.core)
            module = __import__(  # pyright: ignore[reportAny]
                f"physure.units.{scope_module}", fromlist=[unit_name]
            )
            return getattr(module, unit_name)  # pyright: ignore[reportAny]

        return loader

    for name, scope in UNIT_INDEX.items():
        units.register_lazy(name, _make_loader(scope, name))


# Register core units immediately (eager but lightweight)
_register_core_units()

# Discover external units via entry points (lazy loading)
# units.discover_plugins() is now called lazily in UnitRegistry


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
    "_compound_factor",
    "conversion_factor_to",
    "to_latex",
    "_repr_latex_",
    "is_dimensionless",
    "is_linear",
    "kind",
]:
    if hasattr(CompoundUnit, method_name):
        setattr(
            RationalUnit, method_name, getattr(CompoundUnit, method_name)
        )

# Arithmetic methods to wrap with Flyweight cache
for op in [
    "__mul__",
    "__rmul__",
    "__truediv__",
    "__rtruediv__",
    "__pow__",
]:
    wrapped_op = wrap_arithmetic(op)
    if wrapped_op:
        setattr(RationalUnit, op, wrapped_op)

# Stable reference for internal use (resilience against shadowing)
_STABLE_COMPOUND_UNIT = CompoundUnit
