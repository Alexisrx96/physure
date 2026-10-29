# physure/domain/measurement/system.py
"""Defines the `UnitSystem` class."""

from __future__ import annotations

import logging
import math
from collections import defaultdict
from typing import TYPE_CHECKING, Any, cast

from physure.application.factories import QuantityFactory
from physure.application.parsing import parse_unit_string
from physure.domain.measurement.conversions import UnitDefinition
from physure.domain.measurement.converters import (
    LinearConverter,
    UnitConverter,
)
from physure.domain.measurement.definitions_text import (
    prefix_definition_text,
    unit_definition_text,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.ports.unit_repository import IUnitRepository
from physure.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from fractions import Fraction

    from physure.domain.measurement.base_entity import ExponentsDict

from physure._core import UnitRegistry as _RustUnitRegistry

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

        # The registry of the core is built from the definitions of this
        # system, as conf text (see extend_core).
        self._core_registry = _RustUnitRegistry()
        self._core_texts: list[str] = []

    @property
    def core_registry(self) -> _RustUnitRegistry:
        """The registry of the core, built from what this system registers.

        It also knows the eight SI base units whatever the system registered,
        so it is not what answers ``in``.
        """
        return self._core_registry

    def extend_core(self, conf_text: str) -> None:
        """Adds conf text to the core registry of this system, all or nothing.

        Raises:
            ValueError: The core cannot read a line; every such line is named
                and the registry is left as it was.
        """
        self._core_registry.extend_conf(conf_text)
        self._core_texts.append(conf_text)

    def __getstate__(self) -> dict[str, Any]:
        """Pickles the texts of the registry: the registry itself cannot be."""
        state = self.__dict__.copy()
        state["_core_registry"] = None
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        """Rebuilds the registry of the core from its texts."""
        self.__dict__.update(state)
        self._core_registry = _RustUnitRegistry()
        for text in self.__dict__.get("_core_texts", ()):
            self._core_registry.extend_conf(text)

    def get_definition(self, unit_symbol: str) -> UnitDefinition | None:
        """Retrieves the definition for a given unit symbol."""
        return self.UNIT_SYMBOL_REGISTRY.get(unit_symbol)

    def get_setting(self, key: str, default: str | None = None) -> str | None:
        """Retrieves a configuration setting by key.

        Optionally returning a default if not found.
        """
        return self.settings.get(key, default)

    def get_constant(self, name: str) -> Any:
        """Retrieves a physical constant by name from the system's definitions."""
        defn = self.constant_definitions.get(name)
        if defn is None:
            return None
        return self.Q_(defn)

    def register_alias(self, exponents: ExponentsDict, *aliases: str) -> None:
        """Registers aliases for a given set of exponents.

        The aliases are written as a unit of scale one whose recipe is the
        exponents and given to the core registry first (see extend_core): the
        core shows the first of them where those exponents are written, and
        the aliases resolve as units.

        Raises:
            UnknownUnitError: A unit of the exponents is not known.
            ValueError: The core cannot take an alias as a name.
        """
        if not aliases:
            return
        recipe = CompoundUnit(exponents)
        self.extend_core(
            unit_definition_text(
                aliases[0],
                recipe.dimension(self),
                LinearConverter(1.0),
                aliases[1:],
                recipe=recipe,
            )
        )
        self._register_alias(exponents, *aliases)

    def _register_alias(self, exponents: ExponentsDict, *aliases: str) -> None:
        """Registers aliases in Python only (see extend_core)."""
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
                self.ALIASES[key].append(alias)
            self.ALIAS_TO_EXPONENTS[alias] = key

        # Prioritize short symbols (shortest alias length first)
        self.ALIASES[key].sort(key=len)

    def register_prefix(
        self,
        symbol: str,
        factor: float,
        name: str | None = None,
        exact: Fraction | None = None,
    ) -> None:
        """Registers a prefix with its symbol, factor, and optional name.

        Raises:
            ValueError: The core cannot take the prefix; nothing is registered.
        """
        self.extend_core(prefix_definition_text(symbol, factor, name, exact))
        self._register_prefix(symbol, factor, name, exact)

    def _register_prefix(
        self,
        symbol: str,
        factor: float,
        name: str | None = None,
        exact: Fraction | None = None,
    ) -> None:
        """Registers a prefix in Python only (see extend_core)."""
        if symbol in self.PREFIX_REGISTRY:
            log.warning("Prefix '%s' is being redefined.", symbol)
        self.PREFIX_REGISTRY[symbol] = {
            "factor": factor,
            "name": name or symbol,
            "exact": exact,
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
        """Registers a unit and its aliases with the system.

        The unit is written as conf text and given to the core registry
        first, so a unit the core cannot take leaves the system as it was.

        Raises:
            TypeError: The converter is not one a conf line can express.
            ValueError: The core cannot take the unit.
        """
        self.extend_core(
            unit_definition_text(
                symbol,
                dimension,
                converter,
                aliases,
                recipe=recipe,
                allow_prefixes=allow_prefixes,
                kind=kind,
            )
        )
        self._register_unit(
            symbol,
            dimension,
            converter,
            name,
            *aliases,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
            kind=kind,
        )

    def _register_unit(
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
        """Registers a unit in Python only (see extend_core)."""
        unit_def = UnitDefinition(
            symbol,
            dimension,
            converter,
            name,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
            kind=kind,
        )

        all_names = {symbol, *aliases}
        sorted_names = sorted(all_names, key=lambda x: (x != symbol, x))

        for unit_name in sorted_names:
            if unit_name in self.UNIT_SYMBOL_REGISTRY:
                log.warning("Unit '%s' is being redefined.", unit_name)

            self.UNIT_SYMBOL_REGISTRY[unit_name] = unit_def
            self.UNIT_DIMENSIONS[unit_name] = dimension
            self.UNIT_REGISTRY[dimension][unit_name] = unit_def

        self._register_unit_recipe_or_base(symbol, recipe, *aliases)

        # Prefixed variants (km, kilometer, ...) are no longer eagerly
        # cross-produced here -- see get_unit()'s lazy decomposition.

    def _register_unit_recipe_or_base(
        self,
        symbol: str,
        recipe: CompoundUnit | None,
        *aliases: str,
    ) -> None:
        """Registers the recipe (derived) or base-unit alias."""
        if recipe is None and self.UNIT_DIMENSIONS[symbol] == Dimension({}):
            # A unit explicitly declared with the dimensionless Dimension
            # and a linear, scale-1.0 converter (e.g. "unity" / "1") is
            # the multiplicative identity of the unit algebra: give it an
            # empty recipe so it resolves to CompoundUnit({}) instead of
            # persisting as a bogus, unpruned {symbol: 1} exponent that
            # survives arithmetic and breaks equality against quantities
            # that never touched it. Logarithmic dimensionless units
            # (dB, etc.) must NOT collapse this way -- their symbol is
            # load-bearing for LogarithmicConverter lookups -- so this
            # only applies to a scale-1.0 linear converter, matching the
            # guard already used for ordinary derived-unit recipes.
            scale = getattr(
                self.UNIT_SYMBOL_REGISTRY[symbol].converter, "scale", None
            )
            if scale is not None and math.isclose(scale, 1.0):
                recipe = CompoundUnit({})

        if recipe:
            # Index the recipe under every alias, not just the canonical
            # symbol: get_unit() looks up by whatever name the caller used,
            # and all aliases refer to the same derived unit.
            for alias_name in (symbol, *aliases):
                self._UNIT_RECIPES[alias_name] = recipe
            self._register_alias(recipe.exponents, symbol, *aliases)
        else:
            self._register_alias({symbol: 1}, symbol, *aliases)

    def _try_lazy_prefix(self, unit_expression: str) -> CompoundUnit | None:
        """On-demand prefix decomposition.

        Handles prefix-symbol+base-symbol (e.g. "km") or prefix-name+base-name
        (e.g. "kilometer") only -- not every alias combination (kmeter,
        kilometro, ...), since eagerly cross-producing prefixes x every alias
        at system-build time was the ~5.5MB standing-memory cost this
        replaces. Materializes the unit into the registries on first touch so
        later lookups hit the fast UNIT_DIMENSIONS path instead of
        re-running this.
        """
        for prefix_symbol, prefix_data in sorted(
            self.PREFIX_REGISTRY.items(),
            key=lambda kv: len(kv[0]),
            reverse=True,
        ):
            if unit_expression.startswith(prefix_symbol):
                base_key = unit_expression[len(prefix_symbol) :]
                unit = self._materialize_prefixed(
                    unit_expression,
                    prefix_symbol,
                    prefix_data,
                    base_key,
                    name_form=False,
                )
                if unit is not None:
                    return unit

        for prefix_symbol, prefix_data in sorted(
            self.PREFIX_REGISTRY.items(),
            key=lambda kv: len(kv[1]["name"]),
            reverse=True,
        ):
            prefix_name = prefix_data["name"]
            if unit_expression.startswith(prefix_name):
                base_key = unit_expression[len(prefix_name) :]
                unit = self._materialize_prefixed(
                    unit_expression,
                    prefix_symbol,
                    prefix_data,
                    base_key,
                    name_form=True,
                )
                if unit is not None:
                    return unit

        return None

    def _materialize_prefixed(
        self,
        unit_expression: str,
        prefix_symbol: str,
        prefix_data: dict[str, Any],
        base_key: str,
        *,
        name_form: bool,
    ) -> CompoundUnit | None:
        """Materializes `unit_expression`.

        Applies only if it is exactly prefix+base-symbol or
        prefix-name+base-name; returns None (no side effects) otherwise.
        """
        base_def = self._lookup_prefixable_base(base_key, name_form=name_form)
        if base_def is None:
            return None
        self._ensure_prefixed_registered(base_def, prefix_symbol, prefix_data)
        return CompoundUnit({unit_expression: 1})

    def _lookup_prefixable_base(
        self, base_key: str, *, name_form: bool
    ) -> UnitDefinition | None:
        """Base-unit lookup plus the "is this actually prefixable" guards."""
        if not base_key or base_key in self._PREFIX_BLOCKLIST:
            return None
        base_def = self.UNIT_SYMBOL_REGISTRY.get(base_key)
        if base_def is None or not base_def.allow_prefixes:
            return None
        if not isinstance(base_def.converter, LinearConverter):
            return None
        if base_key != (base_def.name if name_form else base_def.symbol):
            return None  # exact symbol- or name-form only, not alias combos
        return base_def

    def _ensure_prefixed_registered(
        self,
        base_def: UnitDefinition,
        prefix_symbol: str,
        prefix_data: dict[str, Any],
    ) -> None:
        """Materializes prefixed forms of `base_def` idempotently.

        Covers the symbol form (km) and, if distinct, the name form
        (kilometer) of `base_def` prefixed by `prefix_data`.
        """
        prefixed_symbol = prefix_symbol + base_def.symbol
        prefixed_name = (
            prefix_data["name"] + base_def.name
            if prefix_data["name"] and base_def.name
            else None
        )
        converter = base_def.converter
        assert isinstance(converter, LinearConverter), (
            "prefixes only apply to linear-converter units"
        )
        prefix_exact = prefix_data.get("exact")
        prefixed_factor = prefix_data["factor"] * converter.scale
        prefixed_exact = (
            prefix_exact * converter.exact
            if prefix_exact is not None and converter.exact is not None
            else None
        )

        if prefixed_symbol not in self.UNIT_SYMBOL_REGISTRY:
            self._create_prefixed_unit(
                prefixed_symbol,
                base_def.dimension,
                prefixed_factor,
                prefixed_name,
                prefixed_symbol,
                exact=prefixed_exact,
            )

        if (
            prefixed_name
            and prefixed_name != prefixed_symbol
            and prefixed_name not in self.UNIT_SYMBOL_REGISTRY
        ):
            self._create_prefixed_unit(
                prefixed_name,
                base_def.dimension,
                prefixed_factor,
                prefixed_name,
                prefixed_symbol,
                exact=prefixed_exact,
            )

    def _create_prefixed_unit(
        self,
        symbol: str,
        dimension: Dimension,
        factor: float,
        name: str | None,
        base_prefixed_symbol: str,  # For alias tracking
        exact: Fraction | None = None,
    ) -> None:
        """Registers a single prefixed unit instance."""
        prefixed_def = UnitDefinition(
            symbol,
            dimension,
            LinearConverter(factor, exact=exact),
            name,
            recipe=None,
            allow_prefixes=False,
        )
        self.UNIT_SYMBOL_REGISTRY[symbol] = prefixed_def
        self.UNIT_DIMENSIONS[symbol] = dimension
        self.UNIT_REGISTRY[dimension][symbol] = prefixed_def
        # Register alias for parser
        self._register_alias({symbol: 1}, symbol)
        # If this was a name alias (kilometer), ensure it maps to km
        if symbol != base_prefixed_symbol:
            self._register_alias({symbol: 1}, symbol, base_prefixed_symbol)

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

        # Lazy prefix decomposition (km, kilometer, ...): materializes the
        # exact prefixed unit on first use instead of the eager prefix x
        # base cross-product done at system-build time.
        lazy_unit = self._try_lazy_prefix(unit_expression)
        if lazy_unit is not None:
            return lazy_unit

        # Parse as a compound expression
        result = cast(
            "CompoundUnit", parse_unit_string(unit_expression, CompoundUnit)
        )

        # Simplify the result of the parsing
        return result.simplify(self)

    def resolve_unit(self, unit: str | CompoundUnit) -> CompoundUnit:
        """Resolves a unit expression or CompoundUnit to a CompoundUnit.

        Shared boundary parser for public entry points that accept either
        a unit notation string or an already-constructed CompoundUnit.
        """
        return self.get_unit(unit) if isinstance(unit, str) else unit

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

    def __contains__(self, item: str) -> bool:
        """Checks if a unit symbol or alias exists in the system."""
        return item in self.UNIT_DIMENSIONS or item in self.ALIAS_TO_EXPONENTS

    def __getattr__(self, name: str) -> CompoundUnit:
        """Allows accessing units as attributes."""
        try:
            return self.get_unit(name)
        except Exception as e:
            raise AttributeError(
                f"'{self.__class__.__name__}' object has no attribute '{name}'"
            ) from e
