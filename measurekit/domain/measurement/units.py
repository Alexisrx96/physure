# measurekit/domain/measurement/units.py
"""Defines the CompoundUnit class."""

from __future__ import annotations

import weakref
from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, ClassVar, cast, overload

import sympy as sp

from measurekit.core.dispatcher import BackendManager
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


@dataclass(frozen=True)
class CompoundUnit(BaseExponentEntity):
    """Represents a unit composed of base units raised to various powers."""

    _cache: ClassVar[weakref.WeakValueDictionary[tuple, CompoundUnit]] = (
        weakref.WeakValueDictionary()
    )

    def __new__(cls, exponents: ExponentsDict):
        """Create or retrieve a cached CompoundUnit instance."""
        normalized_exponents = {}
        for k, v in exponents.items():
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
        """Custom pickling to ensure Flyweight pattern (cache) is used on deserialization.

        By returning (CompoundUnit, (args,)), pickle calls the class constructor,
        which triggers __new__ (and our caching logic).
        """
        return (CompoundUnit, (self.exponents,))

    def __hash__(self) -> int:
        """Returns a hash value for the compound unit."""
        return super().__hash__()

    # --- System-Dependent Methods ---
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

    def _compound_factor(self, system: UnitSystem) -> float:
        """Calculate the unit's total conversion factor relative to SI units.

        This is a helper method used for conversions.

        Args:
        system (UnitSystem): The unit system providing conversion definitions.

        Returns:
        float: The unit's conversion factor.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
        factor = 1.0
        for unit, exp in self.exponents.items():
            if unit == "noprefix":
                continue
            _unit = system.get_unit(unit)
            dim = _unit.dimension(system)

            if dim is None:
                raise ValueError(
                    f"Unit '{unit}' not found in system for conversion."
                )
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if unit_def is None:
                raise ValueError(f"Unit definition for '{unit}' not found.")
            # Assume Linear for compound factors default path
            # If not linear, this naive multiplication is wrong,
            # but _compound_factor is legacy helper only for linear
            # combinations.
            if hasattr(unit_def.converter, "scale"):
                factor *= unit_def.converter.scale**exp
            else:
                # Fallback or error for non-linear in compound?
                # Assuming 1.0 if not scalable (e.g. Identity)?
                pass
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

    def dimension(self, system: UnitSystem) -> Dimension:
        """Determine the physical dimension of the unit within a system.

        Args:
        system (UnitSystem): The unit system that defines the dimensions of
        base units.

        Returns:
        Dimension: The resulting physical dimension of the compound unit.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
        overall = Dimension({})
        for unit, exp in self.exponents.items():
            if unit == "noprefix":
                continue
            if unit in system.UNIT_DIMENSIONS:
                overall *= system.UNIT_DIMENSIONS[unit] ** exp
            else:
                raise ValueError(
                    f"Unknown dimension for unit '{unit}'"
                    " in the provided system."
                )
        return overall

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

    def to_latex(self) -> str:
        """Generate a LaTeX representation of the unit for display."""
        if not self.exponents:
            return ""

        symbols = {name: sp.Symbol(name) for name in self.exponents}

        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
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

        return CompoundUnit(new_exponents)
