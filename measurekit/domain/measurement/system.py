"""Defines the `UnitSystem` class, the central engine for unit management.

The `UnitSystem` acts as a comprehensive registry for all definitions,
including dimensions, prefixes, units, and their conversion factors. It is the
core component that orchestrates all unit-aware operations, from parsing unit
expressions to performing conversions and validating dimensional consistency.
It serves as the main "application" object in the library's architecture.
"""

from __future__ import annotations

import logging
from collections import defaultdict
from typing import Any, cast

from measurekit.application.factories import QuantityFactory
from measurekit.domain.measurement.conversions import UnitDefinition
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.ports.unit_repository import IUnitRepository
from measurekit.domain.measurement.units import CompoundUnit, ExponentsDict
from measurekit.domain.notation.lexer import generate_tokens
from measurekit.domain.notation.parsers import NotationParser

# Create a logger specific to this module
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
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        for alias in aliases:
            if alias not in self.ALIASES[key]:
                self.ALIASES[key].append(alias)
            self.ALIAS_TO_EXPONENTS[alias] = key

    def register_prefix(
        self, symbol: str, factor: float, name: str | None = None
    ) -> None:
        """Registers a prefix with its symbol, factor, and optional name."""
        if symbol in self.PREFIX_REGISTRY:
            log.warning(
                "Prefix '%s' is being redefined in this system.", symbol
            )
        self.PREFIX_REGISTRY[symbol] = {
            "factor": factor,
            "name": name or symbol,
        }

    def register_dimension(self, dimension: Dimension, name: str):
        """Registers a descriptive name for a Dimension instance."""
        if dimension in self._DIMENSION_NAME_REGISTRY:
            log.warning(
                "Dimension '%s' is being redefined in this system.", dimension
            )
        self._DIMENSION_NAME_REGISTRY[dimension] = name

    def register_unit(
        self,
        symbol: str,
        dimension: Dimension,
        factor_to_base: float,
        name: str | None,
        *aliases: str,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
    ) -> None:
        """Registers a unit and its aliases with the system."""
        unit_def = UnitDefinition(
            symbol,
            dimension,
            factor_to_base,
            name,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
        )

        all_names = set([symbol] + list(aliases))
        sorted_names = sorted(all_names)

        for unit_name in sorted_names:
            if unit_name in self.UNIT_SYMBOL_REGISTRY:
                log.warning("Unit '%s' is being redefined.", unit_name)

            self.UNIT_SYMBOL_REGISTRY[unit_name] = unit_def
            self.UNIT_DIMENSIONS[unit_name] = dimension

        self.UNIT_REGISTRY[dimension][symbol] = unit_def

        if recipe:
            self._UNIT_RECIPES[symbol] = recipe

        # Automatically register prefixed units
        if allow_prefixes and symbol not in self._PREFIX_BLOCKLIST:
            for prefix_symbol, prefix_data in self.PREFIX_REGISTRY.items():
                prefixed_symbol = prefix_symbol + symbol

                # Check if the prefixed unit is already registered.
                if prefixed_symbol in self.UNIT_SYMBOL_REGISTRY:
                    continue  # Skip to avoid redefinition warnings.

                prefixed_name = prefix_data["name"] + (name or symbol)
                prefixed_factor = prefix_data["factor"] * factor_to_base

                prefixed_def = UnitDefinition(
                    prefixed_symbol,
                    dimension,
                    prefixed_factor,
                    prefixed_name,
                    allow_prefixes=False,  # Prefixed units cannot have prefix
                )
                self.UNIT_SYMBOL_REGISTRY[prefixed_symbol] = prefixed_def
                self.UNIT_DIMENSIONS[prefixed_symbol] = dimension
                self.UNIT_REGISTRY[dimension][prefixed_symbol] = prefixed_def

    def get_unit(self, unit_expression: str) -> CompoundUnit:
        """Retrieves a CompoundUnit from the system based on its notation.

        This method is now a pure retrieval function with no side effects.
        """
        # 1. Check for simple units (including aliases and prefixed units)
        if unit_expression in self.UNIT_DIMENSIONS:
            # Check if this unit has a recipe and return the simplified unit
            if unit_expression in self._UNIT_RECIPES:
                return self._UNIT_RECIPES[unit_expression]
            return CompoundUnit({unit_expression: 1})

        # 2. Check for aliases
        if unit_expression in self.ALIAS_TO_EXPONENTS:
            key = self.ALIAS_TO_EXPONENTS[unit_expression]
            return CompoundUnit(dict(key))

        # 3. Parse as a compound expression
        tokens = generate_tokens(unit_expression)
        parser = NotationParser(tokens, CompoundUnit)
        result = cast(CompoundUnit, parser.parse())

        # Simplify the result of the parsing
        return result.simplify(self)
