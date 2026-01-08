# measurekit/domain/measurement/system.py
"""Defines the `UnitSystem` class."""

from __future__ import annotations

import contextlib
import logging
from collections import defaultdict
from typing import Any, cast

from measurekit.application.factories import QuantityFactory
from measurekit.application.parsing import parse_unit_string
from measurekit.domain.measurement.conversions import UnitDefinition
from measurekit.domain.measurement.converters import (
    LinearConverter,
    UnitConverter,
)
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.ports.unit_repository import IUnitRepository
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.notation.typing import ExponentsDict

log = logging.getLogger(__name__)


class UnitSystem(IUnitRepository):
    """Manages a self-contained system of dimensions, units, and config."""

    def __init__(self, name: str | None = None, description: str = ""):
        """Initializes a new, clean unit system."""
        self.name = name
        self.description = description
        self.PREFIX_REGISTRY: dict[str, dict[str, Any]] = {}
        self.UNIT_SYMBOL_REGISTRY: dict[str, UnitDefinition] = {}
        self.UNIT_REGISTRY: dict[Dimension, dict[str, UnitDefinition]] = (
            defaultdict(dict)
        )
        self.UNIT_DIMENSIONS: dict[str, Dimension] = {}
        self.ALIASES: dict[tuple, list[str]] = defaultdict(list)
        self.ALIAS_TO_EXPONENTS: dict[str, tuple] = {}
        self._UNIT_RECIPES: dict[str, CompoundUnit] = {}
        self._DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}
        self._PREFIX_BLOCKLIST: set[str] = set()
        self.settings: dict[str, str] = {}
        self.prefix_definitions: dict[str, str] = {}
        self.dimension_definitions: dict[str, str] = {}
        self.unit_definitions: dict[str, str] = {}
        self.constant_definitions: dict[str, str] = {}
        CompoundUnit._cache.clear()
        Dimension._cache.clear()
        self.Q_ = QuantityFactory(self)

        # Initialize Rust Core Registry
        try:
            from measurekit_core import UnitRegistry

            self._core_registry = UnitRegistry()
        except ImportError:
            log.warning("measurekit_core not found. Rust registry disabled.")
            self._core_registry = None

    def __getstate__(self) -> dict[str, Any]:
        """Custom pickling state to exclude unpickleable Rust registry."""
        state = self.__dict__.copy()
        state["_core_registry"] = None
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        """Custom unpickling state to restore (empty) Rust registry."""
        self.__dict__.update(state)
        # Re-initialize Rust Core Registry
        try:
            from measurekit_core import UnitRegistry

            self._core_registry = UnitRegistry()
            # Note: The restored registry is empty! We rely on Python dictionaries
            # being the source of truth for now. To fully restore Rust state,
            # we would need to walk UNIT_SYMBOL_REGISTRY and re-register everything.
            # For "Legacy Code" phase, this empty state is acceptable as get_unit
            # falls back to Python.
        except ImportError:
            self._core_registry = None

    def _to_rational_unit(self, cu: CompoundUnit) -> Any:
        """Converts a Python CompoundUnit to a Rust RationalUnit."""
        from fractions import Fraction

        try:
            from measurekit_core import RationalUnit
        except ImportError:
            return None

        dims = {}
        for base, exp in cu.exponents.items():
            if isinstance(exp, (int, float)):
                f = Fraction(exp).limit_denominator()
                dims[base] = (f.numerator, f.denominator)
            else:
                # Fallback/Error
                pass
        return RationalUnit(dims)

    def _from_rational_unit(self, ru: Any) -> CompoundUnit:
        """Converts a Rust RationalUnit to a Python CompoundUnit."""
        exponents = {}
        for base, (num, den) in ru.dimensions.items():
            exponents[base] = num if den == 1 else num / den
        return CompoundUnit(exponents)

    def get_definition(self, unit_symbol: str) -> UnitDefinition | None:
        """Retrieves the definition for a given unit symbol."""
        return self.UNIT_SYMBOL_REGISTRY.get(unit_symbol)

    def get_setting(self, key: str, default: str | None = None) -> str | None:
        """Retrieves a configuration setting by key.

        Optionally returning a default if not found.
        """
        return self.settings.get(key, default)

    def register_alias(self, exponents: ExponentsDict, *aliases: str) -> None:
        """Registers aliases for a given set of exponents."""
        # Normalizar exponentes a enteros para el registro de alias
        normalized_exponents = {}
        for k, v in exponents.items():
            if v == 0:
                continue
            if isinstance(v, float) and v.is_integer():
                normalized_exponents[k] = int(v)
            else:
                normalized_exponents[k] = v

        key = tuple(sorted(normalized_exponents.items()))

        for alias in aliases:
            if alias not in self.ALIASES[key]:
                self.ALIASES[key].insert(0, alias)
            self.ALIAS_TO_EXPONENTS[alias] = key

    def register_prefix(
        self, symbol: str, factor: float, name: str | None = None
    ) -> None:
        """Registers a prefix with its symbol, factor, and optional name."""
        if symbol in self.PREFIX_REGISTRY:
            log.warning("Prefix '%s' is being redefined.", symbol)
        self.PREFIX_REGISTRY[symbol] = {
            "factor": factor,
            "name": name or symbol,
        }

    def register_dimension(self, dimension: Dimension, name: str):
        """Registers a descriptive name for a Dimension instance."""
        if dimension in self._DIMENSION_NAME_REGISTRY:
            log.warning("Dimension '%s' is being redefined.", dimension)
        self._DIMENSION_NAME_REGISTRY[dimension] = name

    def register_unit(
        self,
        symbol: str,
        dimension: Dimension,
        converter: UnitConverter,
        name: str | None,
        *aliases: str,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
        kind: str = "delta",
    ) -> None:
        """Registers a unit and its aliases with the system."""
        unit_def = UnitDefinition(
            symbol,
            dimension,
            converter,
            name,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
            kind=kind,
        )

        all_names = set([symbol] + list(aliases))
        sorted_names = sorted(all_names, key=lambda x: (x != symbol, x))

        for unit_name in sorted_names:
            if unit_name in self.UNIT_SYMBOL_REGISTRY:
                log.warning("Unit '%s' is being redefined.", unit_name)

            self.UNIT_SYMBOL_REGISTRY[unit_name] = unit_def
            self.UNIT_DIMENSIONS[unit_name] = dimension
            self.UNIT_REGISTRY[dimension][unit_name] = unit_def

        if recipe:
            self._UNIT_RECIPES[symbol] = recipe
            self.register_alias(recipe.exponents, symbol, *aliases)

            # Rust Registry: Derived
            if self._core_registry:
                ru = self._to_rational_unit(recipe)
                if ru:
                    self._core_registry.add_derived_unit(symbol, ru)

        else:
            self.register_alias({symbol: 1}, symbol, *aliases)

            # Rust Registry: Base
            if self._core_registry:
                self._core_registry.add_base_unit(symbol)

        # Register Aliases in Rust
        if self._core_registry:
            for alias in aliases:
                with contextlib.suppress(Exception):
                    self._core_registry.register_alias(alias, symbol)

        # Automatically register prefixed units
        if allow_prefixes:
            self._register_prefixed_units(
                sorted_names, symbol, dimension, converter, name
            )

    def _register_prefixed_units(
        self,
        names: list[str],
        base_symbol: str,
        dimension: Dimension,
        converter: UnitConverter,
        base_name: str | None,
    ) -> None:
        """Helper to register all prefixed variants for a set of unit names."""
        for unit_name in names:
            if unit_name in self._PREFIX_BLOCKLIST:
                continue

            # Prefixes only make sense for linear units
            if not isinstance(converter, LinearConverter):
                continue

            self._apply_prefixes_to_unit(
                unit_name, base_symbol, dimension, converter.scale, base_name
            )

    def _apply_prefixes_to_unit(
        self,
        unit_name: str,
        base_symbol: str,
        dimension: Dimension,
        scale: float,
        base_name: str | None,
    ) -> None:
        """Applies all prefixes to a single unit name."""
        for prefix_symbol, prefix_data in self.PREFIX_REGISTRY.items():
            prefixed_symbol = prefix_symbol + unit_name
            if prefixed_symbol in self.UNIT_SYMBOL_REGISTRY:
                continue

            desc_name = (
                base_name
                if (base_name and unit_name == base_symbol)
                else unit_name
            )
            prefixed_name = prefix_data["name"] + desc_name
            prefixed_factor = prefix_data["factor"] * scale

            self._create_prefixed_unit(
                prefixed_symbol,
                dimension,
                prefixed_factor,
                prefixed_name,
                prefixed_symbol,
            )

            # Also name alias (e.g. kilometer)
            if prefixed_name and prefixed_name != prefixed_symbol:
                self._create_prefixed_unit(
                    prefixed_name,
                    dimension,
                    prefixed_factor,
                    prefixed_name,  # Name is same
                    prefixed_symbol,  # Original symbol context
                )

            # Rust Registration
            if self._core_registry:
                self._register_prefixed_rust(
                    unit_name, prefixed_symbol, prefixed_name
                )

    def _create_prefixed_unit(
        self,
        symbol: str,
        dimension: Dimension,
        factor: float,
        name: str | None,
        base_prefixed_symbol: str,  # For alias tracking
    ) -> None:
        """Registers a single prefixed unit instance."""
        prefixed_def = UnitDefinition(
            symbol,
            dimension,
            LinearConverter(factor),
            name,
            recipe=None,
            allow_prefixes=False,
        )
        self.UNIT_SYMBOL_REGISTRY[symbol] = prefixed_def
        self.UNIT_DIMENSIONS[symbol] = dimension
        self.UNIT_REGISTRY[dimension][symbol] = prefixed_def
        # Register alias for parser
        self.register_alias({symbol: 1}, symbol)
        # If this was a name alias (kilometer), ensure it maps to km
        if symbol != base_prefixed_symbol:
            self.register_alias({symbol: 1}, symbol, base_prefixed_symbol)

    def _register_prefixed_rust(
        self, unit_name: str, prefixed_symbol: str, prefixed_name: str
    ) -> None:
        """Registers the prefixed unit in the Rust registry."""
        try:
            base_ru = self._core_registry.get_unit(unit_name)
            self._core_registry.add_derived_unit(prefixed_symbol, base_ru)
            if prefixed_name and prefixed_name != prefixed_symbol:
                self._core_registry.register_alias(
                    prefixed_name, prefixed_symbol
                )
        except (ImportError, KeyError, Exception):
            pass

    def get_unit(self, unit_expression: str) -> CompoundUnit:
        """Retrieves a CompoundUnit from the system based on its notation."""
        if unit_expression == "dimensionless":
            return CompoundUnit({})

        # Python Memory Check (Legacy/Fast Path for Python objects)
        if unit_expression in self.UNIT_DIMENSIONS:
            if unit_expression in self._UNIT_RECIPES:
                return self._UNIT_RECIPES[unit_expression]
            return CompoundUnit({unit_expression: 1})

        # Check for aliases
        if unit_expression in self.ALIAS_TO_EXPONENTS:
            key = self.ALIAS_TO_EXPONENTS[unit_expression]
            return CompoundUnit(dict(key))

        # Rust Registry Check (New Path)
        if (
            self._core_registry
            and hasattr(self._core_registry, "contains")
            and self._core_registry.contains(unit_expression)
        ):
            try:
                ru = self._core_registry.get_unit(unit_expression)
                return self._from_rational_unit(ru)
            except Exception as e:
                log.warning(
                    f"Failed to retrieve unit '{unit_expression}' from Rust: {e}"
                )
                # Fallback to parsing

        # Parse as a compound expression
        result = cast(
            "CompoundUnit", parse_unit_string(unit_expression, CompoundUnit)
        )

        # Simplify the result of the parsing
        return result.simplify(self)

    def get_base_unit_for_dimension(
        self, dimension: Dimension
    ) -> CompoundUnit:
        """Returns the preferred base unit for a physical dimension."""
        base_units_str = self.settings.get("base_units", "")
        if not base_units_str:
            # Fallback to SI-like defaults if not specified
            # This is a bit simplified, ideally we look into a default mapping.
            return CompoundUnit({})

        # Parse "L:ft, M:lb, T:s"
        mapping = {}
        for entry in base_units_str.split(","):
            if ":" in entry:
                dim_sym, unit_sym = [s.strip() for s in entry.split(":")]
                mapping[Dimension.from_string(dim_sym)] = unit_sym

        # Build compound unit from dimension components
        result = CompoundUnit({})
        for dim_sym, exp in dimension.exponents.items():
            comp_dim = Dimension.from_string(dim_sym)
            if comp_dim in mapping:
                part = CompoundUnit({mapping[comp_dim]: 1})
                result = result * (part**exp)
            else:
                # Fallback to SI for missing components?
                # Or just keep as is.
                pass

        return result if not result.is_dimensionless else CompoundUnit({})

    def __getattr__(self, name: str) -> CompoundUnit:
        """Allows accessing units as attributes."""
        try:
            return self.get_unit(name)
        except Exception as e:
            raise AttributeError(
                f"'{self.__class__.__name__}' object has no attribute '{name}'"
            ) from e
