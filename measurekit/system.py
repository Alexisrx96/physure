# measurekit/system.py (Corrected with missing methods)

from __future__ import annotations

from collections import defaultdict
from typing import TYPE_CHECKING, Any, cast

import measurekit.measurement.quantity as quantity_module
from measurekit.measurement.conversions import UnitDefinition
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.units import CompoundUnit
from measurekit.notation.lexer import generate_tokens
from measurekit.notation.parsers import NotationParser


class UnitSystem:
    """
    Manages a self-contained system of dimensions, units, and configurations.
    """

    def __init__(self):
        """Initializes a new, clean unit system."""
        self.PREFIX_REGISTRY: dict[str, dict[str, Any]] = {}
        self.UNIT_SYMBOL_REGISTRY: dict[str, UnitDefinition] = {}
        self.UNIT_REGISTRY: dict[Dimension, dict[str, UnitDefinition]] = (
            defaultdict(dict)
        )
        self.UNIT_DIMENSIONS: dict[str, Dimension] = {}
        self._UNIT_RECIPES: dict[str, CompoundUnit] = {}
        self._DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}
        self._PREFIX_BLOCKLIST: set[str] = set()
        self.settings: dict[str, str] = {}
        self.prefix_definitions: dict[str, str] = {}
        self.dimension_definitions: dict[str, str] = {}
        self.unit_definitions: dict[str, str] = {}
        self.constant_definitions: dict[str, str] = {}

        CompoundUnit._cache.clear()
        CompoundUnit._aliases.clear()
        CompoundUnit._alias_to_exponents.clear()
        Dimension._cache.clear()

    def get_setting(self, key: str, default: str | None = None) -> str | None:
        return self.settings.get(key, default)

    # THIS IS THE FIX: Added the missing register_prefix method
    def register_prefix(
        self, symbol: str, factor: float, name: str | None = None
    ) -> None:
        """Registers a prefix within this unit system."""
        if symbol in self.PREFIX_REGISTRY:
            print(
                f"[WARNING] Prefix '{symbol}' is being redefined in this system."
            )
        self.PREFIX_REGISTRY[symbol] = {
            "factor": factor,
            "name": name or symbol,
        }

    # THIS IS THE FIX: Added the missing register_dimension method
    def register_dimension(self, dimension: Dimension, name: str):
        """Registers a dimension name within this unit system."""
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
        unit_def = UnitDefinition(
            symbol,
            dimension,
            factor_to_base,
            name,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
        )
        self.UNIT_REGISTRY[dimension][symbol] = unit_def
        self.UNIT_DIMENSIONS[symbol] = dimension
        for alias in aliases:
            self.UNIT_SYMBOL_REGISTRY[alias] = unit_def
        if recipe:
            self._UNIT_RECIPES[symbol] = recipe
            for alias in aliases:
                self._UNIT_RECIPES[alias] = recipe

    def get_unit(self, unit_expression: str) -> CompoundUnit:
        """
        Analyzes a unit expression, handling aliases, dynamic prefixes,
        and complex expressions.
        """
        # 1. Check for a known alias first.
        if unit_expression in CompoundUnit._alias_to_exponents:
            key = CompoundUnit._alias_to_exponents[unit_expression]
            return CompoundUnit(dict(key))

        # 2. Check if the unit has already been dynamically created.
        if unit_expression in self.UNIT_DIMENSIONS:
            return CompoundUnit({unit_expression: 1})

        # 3. Attempt to parse as a prefixed unit.
        # Sort prefixes by length descending (e.g., 'da' before 'd')
        sorted_prefixes = sorted(
            self.PREFIX_REGISTRY.keys(), key=len, reverse=True
        )

        for prefix_symbol in sorted_prefixes:
            if unit_expression.startswith(prefix_symbol):
                unit_symbol = unit_expression[len(prefix_symbol) :]

                # Check if the base unit exists in our system
                if unit_symbol in self.UNIT_SYMBOL_REGISTRY:
                    base_unit_def = self.UNIT_SYMBOL_REGISTRY[unit_symbol]

                    # Check if this unit is allowed to have prefixes
                    if base_unit_def.allow_prefixes:
                        prefix_data = self.PREFIX_REGISTRY[prefix_symbol]

                        # Dynamically register the new prefixed unit
                        new_factor = (
                            prefix_data["factor"]
                            * base_unit_def.factor_to_base
                        )
                        self.register_unit(
                            unit_expression,
                            base_unit_def.dimension,
                            new_factor,
                            f"{prefix_data['name']}{base_unit_def.name}",
                            recipe=CompoundUnit({base_unit_def.symbol: 1}),
                        )
                        CompoundUnit.register_alias(
                            {unit_expression: 1}, unit_expression
                        )

                        # Return the newly created unit
                        return CompoundUnit({unit_expression: 1})

        # 4. If all else fails, parse as a complex expression (e.g., "m/s")
        tokens = generate_tokens(unit_expression)
        parser = NotationParser(tokens, CompoundUnit)
        return cast(CompoundUnit, parser.parse())

    def Q_(self, *args, **kwargs):
        value, unit = args[0], args[1]
        uncertainty = kwargs.get("uncertainty", 0.0)

        if isinstance(unit, str):
            unit = self.get_unit(unit)

        return quantity_module.Quantity.from_input(
            value=value, unit=unit, system=self, uncertainty=uncertainty
        )
