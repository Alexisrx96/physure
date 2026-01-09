# measurekit/domain/measurement/units.py
"""Defines the CompoundUnit class."""

from __future__ import annotations

import weakref
from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, ClassVar, cast, overload

import sympy as sp

from measurekit.core.dispatcher import BackendManager
from measurekit.core.registry import UnitRegistry
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.converters import (
    LinearConverter,
    UnitConverter,
)
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.notation.base_entity import BaseExponentEntity

if TYPE_CHECKING:
    from numpy.typing import NDArray

    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.system import UnitSystem
    from measurekit.domain.notation.typing import ExponentsDict


# --- Dependency Injection for System ---


def get_default_system() -> UnitSystem:
    """Retrieves the currently active UnitSystem from the context.

    This proxies to `measurekit.application.context.get_current_system()`,
    ensuring thread-safety and correct context isolation.
    """
    # Import inside function to avoid circular import at module level
    # units -> context (ok) but safer if context imports units later indirectly
    from measurekit.application.context import get_current_system

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
    from measurekit.domain.measurement.units import (
        CompoundUnit,
        _CompoundUnit,
        get_default_system,
    )

    # Ensure active system context to prevent registry detachment
    get_default_system()

    # Use the stable reference if the public one is corrupted
    cls = CompoundUnit
    if not isinstance(cls, type):
        cls = _CompoundUnit

    return cls(exponents)


try:
    from measurekit_core import RationalUnit

    IS_CORE_AVAILABLE = True
except ImportError:
    # Fallback to local stub if core not available
    class RationalUnit:
        def __init__(self, *args, **kwargs):
            pass

    IS_CORE_AVAILABLE = False


@dataclass(frozen=True)
class CompoundUnit(RationalUnit, BaseExponentEntity):
    """Represents a unit composed of base units raised to various powers."""

    def dimension(self, system: UnitSystem | None = None) -> Dimension:
        """Calculates the physical dimension of the composite unit."""
        from measurekit.domain.measurement.dimensions import Dimension

        if system is None:
            # Import here to avoid circularity
            from measurekit.domain.measurement.system import get_default_system

            system = get_default_system()

        # Dimension calculation
        dims = Dimension({})
        # Use .dimensions (Rust HashMap) if available for speed
        it = (
            self.dimensions.items()
            if hasattr(self, "dimensions")
            else self.exponents.items()
        )
        for unit_name, exp_val in it:
            exp = (
                exp_val[0] / exp_val[1]
                if isinstance(exp_val, tuple)
                else exp_val
            )
            if unit_name in system.UNIT_DIMENSIONS:
                unit_dim = system.UNIT_DIMENSIONS[unit_name]
            else:
                base_unit = system.get_unit(unit_name)

                # Loop detection: If base_unit is effectively self (identity wrapper)
                # and we haven't found a dimension yet, it's an unknown base unit.
                is_identity = False
                if isinstance(
                    base_unit, CompoundUnit
                ):  # Check strictly or duck-type
                    deps = (
                        base_unit.exponents
                        if hasattr(base_unit, "exponents")
                        else base_unit.dimensions
                    )
                    if len(deps) == 1 and deps.get(unit_name) == 1:
                        is_identity = True

                if is_identity:
                    raise ValueError(
                        f"Unknown dimension for unit '{unit_name}'"
                    )

                if hasattr(base_unit, "dimension"):
                    unit_dim = base_unit.dimension(system)
                else:
                    unit_dim = Dimension({unit_name: 1})
            dims *= unit_dim**exp
        return dims

    _cache: ClassVar[weakref.WeakValueDictionary[tuple, CompoundUnit]] = (
        weakref.WeakValueDictionary()
    )

    def __new__(cls, exponents: ExponentsDict):
        """Create or retrieve a cached CompoundUnit instance."""
        normalized_exponents = {}
        for k, v in exponents.items():
            # Handle rational tuples (num, den) from Rust
            if isinstance(v, (list, tuple)):
                v = v[0] / v[1]

            if v == 0:
                continue
            if isinstance(v, float) and v.is_integer():
                normalized_exponents[k] = int(v)
            else:
                normalized_exponents[k] = v

        key = tuple(sorted(normalized_exponents.items()))

        # Check raw cache to avoid resurrection issues if needed,
        # but WeakValueDictionary handles this.
        instance = cls._cache.get(key)
        if instance is not None:
            return instance

        instance = super().__new__(cls, normalized_exponents)
        # We need to initialize the object here because __init__ is not called
        # efficiently if we returned cached. Use object.__setattr__ if needed
        # but BaseExponentEntity sets exponents in __new__?
        # Actually Base is standard class?
        # But if we return existing, __init__ runs again for
        # dataclasses usually?
        # Singleton pattern in __new__:
        cls._cache[key] = cast("CompoundUnit", instance)
        return cast("CompoundUnit", instance)

    def __init__(self, exponents: ExponentsDict) -> None:
        """Initializes the compound unit with a dictionary of exponents."""
        pass

    def __post_init__(self):
        """Eliminamos cualquier unidad con exponente 0."""
        clean_exponents = {k: v for k, v in self.exponents.items() if v != 0}
        object.__setattr__(self, "exponents", clean_exponents)

    def __reduce__(self):
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
            r_unit (measurekit_core.RationalUnit): The Rust unit.

        Returns:
            CompoundUnit: The corresponding Python unit.
        """
        return cls(r_unit.exponents)

    # --- System-Dependent Methods ---
    def _compound_factor(self, system: UnitSystem) -> float:
        """Calculate the unit's total conversion factor relative to SI units."""
        factor = 1.0
        it = (
            self.dimensions.items()
            if hasattr(self, "dimensions")
            else self.exponents.items()
        )
        for unit, exp in it:
            if unit == "noprefix":
                continue
            _unit = system.get_unit(unit)
            dim = _unit.dimension(system)
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if unit_def and hasattr(unit_def.converter, "scale"):
                conv_scale = unit_def.converter.scale
                # Handle tuple exponents (num, den) from RationalUnit
                if isinstance(exp, (list, tuple)):
                    exponent_val = exp[0] / exp[1]
                else:
                    exponent_val = exp
                factor *= conv_scale**exponent_val
        return factor

    def is_linear(self, system: UnitSystem) -> bool:
        """Checks if all components of the unit use linear converters."""
        it = (
            self.dimensions.items()
            if hasattr(self, "dimensions")
            else self.exponents.items()
        )
        for unit, _ in it:
            if unit == "noprefix":
                continue
            unit_def = system.get_definition(unit)
            if unit_def and not unit_def.converter.is_linear:
                return False
        return True

    def kind(self, system: UnitSystem) -> str:
        """Determines if the unit is 'absolute' (Point) or 'delta' (Vector)."""
        it = (
            self.dimensions.items()
            if hasattr(self, "dimensions")
            else self.exponents.items()
        )
        e_list = list(it)
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
        source_factor = self._compound_factor(system)
        target_factor = target._compound_factor(system)
        return source_factor / target_factor

    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...

    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    def __rmul__(self, other: Any) -> Any:
        """Handle right-side multiplication, typically for creating a Quantity.

        This allows for intuitive syntax like 5 * meter.

        Args:
        other (Any): The scalar or array to be multiplied with the unit.

        Returns:
        Any: A new Quantity instance, or NotImplemented if the operation is
        not supported.
        """
        from measurekit.domain.measurement.quantity import Quantity

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
        if isinstance(exponent, float):
            # Check for integer float
            if exponent.is_integer():
                exponent = int(exponent)
            else:
                # Convert float to decent rational
                from fractions import Fraction

                frac = Fraction(exponent).limit_denominator(100)
                exponent = (frac.numerator, frac.denominator)

        # Call Rust implementation
        res = super().__pow__(exponent)

        # Ensure we return CompoundUnit (wrapping result)
        if hasattr(res, "dimensions"):  # Check if it's RationalUnit-like
            return _CompoundUnit(res.dimensions)
        return res

    def __rtruediv__(self, other: Any) -> CompoundUnit:
        """Right division (1 / unit)."""
        if other == 1:
            return self**-1
        return NotImplemented

    def to_latex(self) -> str:
        """Generate a LaTeX representation of the unit for display."""
        if not self.exponents:
            return ""

        symbols = {name: sp.Symbol(name) for name in self.exponents}

        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
            # Clean up exponent if it's a float-int (e.g. 1.0 -> 1)
            # This ensures latex output is clean: m^1 not m^1.0
            if isinstance(exponent, float) and exponent.is_integer():
                exponent = int(exponent)

            expr *= symbols[unit_name] ** exponent

        return sp.latex(expr, mul_symbol="dot")

    def _repr_latex_(self):
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


def _register_core_units():
    """Helper to populate the registry from the generated index."""
    try:
        # Import the index generated by scripts/compile_units.py
        from measurekit.units._index import UNIT_INDEX
    except ImportError:
        # If the index hasn't been generated yet, skip registration.
        # This allows bootstrapping or running without generated units.
        return

    def _make_loader(scope_module: str, unit_name: str):
        def loader():
            # Dynamically import the scope module (e.g., measurekit.units.core)
            module = __import__(
                f"measurekit.units.{scope_module}", fromlist=[unit_name]
            )
            return getattr(module, unit_name)

        return loader

    for name, scope in UNIT_INDEX.items():
        units.register_lazy(name, _make_loader(scope, name))


# Register core units immediately (eager but lightweight)
_register_core_units()

# Discover external units via entry points (lazy loading)
units.discover_plugins()


# Inject Python-side functionality into the Rust base class
# This ensures that results of Rust unit arithmetic (which return raw RationalUnit
# objects) still have access to the high-level Python methods.
if IS_CORE_AVAILABLE:

    def wrap_arithmetic(op_name):
        orig_op = getattr(RationalUnit, op_name, None)
        if orig_op:

            def wrapped(self, other):
                res = orig_op(self, other)
                if isinstance(res, RationalUnit):
                    # Wrap in CompoundUnit to use Flyweight cache
                    return _CompoundUnit(res.dimensions)
                return res

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
