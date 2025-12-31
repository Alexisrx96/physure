# measurekit/domain/measurement/system.py
"""Defines the `UnitSystem` class."""

from __future__ import annotations

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
from measurekit.domain.measurement.units import CompoundUnit, ExponentsDict

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
    ) -> None:
        """Registers a unit and its aliases with the system."""
        unit_def = UnitDefinition(
            symbol,
            dimension,
            converter,
            name,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
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
        else:
            self.register_alias({symbol: 1}, symbol, *aliases)

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

            for prefix_symbol, prefix_data in self.PREFIX_REGISTRY.items():
                prefixed_symbol = prefix_symbol + unit_name

                if prefixed_symbol in self.UNIT_SYMBOL_REGISTRY:
                    continue

                # Prefixes only make sense for linear units
                if not isinstance(converter, LinearConverter):
                    continue

                desc_name = (
                    base_name
                    if (base_name and unit_name == base_symbol)
                    else unit_name
                )
                prefixed_name = prefix_data["name"] + desc_name
                prefixed_factor = prefix_data["factor"] * converter.scale

                prefixed_def = UnitDefinition(
                    prefixed_symbol,
                    dimension,
                    LinearConverter(prefixed_factor),
                    prefixed_name,
                    recipe=None,
                    allow_prefixes=False,
                )
                self.UNIT_SYMBOL_REGISTRY[prefixed_symbol] = prefixed_def
                self.UNIT_DIMENSIONS[prefixed_symbol] = dimension
                self.UNIT_REGISTRY[dimension][prefixed_symbol] = prefixed_def

    def get_unit(self, unit_expression: str) -> CompoundUnit:
        """Retrieves a CompoundUnit from the system based on its notation."""
        if unit_expression in self.UNIT_DIMENSIONS:
            # Check if this unit has a recipe and return the simplified unit
            if unit_expression in self._UNIT_RECIPES:
                return self._UNIT_RECIPES[unit_expression]
            return CompoundUnit({unit_expression: 1})

        # Check for aliases
        if unit_expression in self.ALIAS_TO_EXPONENTS:
            key = self.ALIAS_TO_EXPONENTS[unit_expression]
            return CompoundUnit(dict(key))

        # Parse as a compound expression
        result = cast(
            CompoundUnit, parse_unit_string(unit_expression, CompoundUnit)
        )

        # Simplify the result of the parsing
        return result.simplify(self)
