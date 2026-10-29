# physure/domain/measurement/system.py
"""Defines the `UnitSystem` class."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from physure.application.factories import QuantityFactory
from physure.application.parsing import parse_unit_string
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.definitions_text import (
    prefix_definition_text,
    unit_definition_text,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import (
    _CORE_ERRORS,
    CompoundUnit,
    _python_error,
)

if TYPE_CHECKING:
    from fractions import Fraction

    from physure.domain.measurement.base_entity import ExponentsDict
    from physure.domain.measurement.definitions_text import Number

from physure._core import UnitRegistry as _RustUnitRegistry

log = logging.getLogger(__name__)


class UnitSystem:
    """A system of units: a registry of the core, and the names and settings around it.

    What a unit is (its dimension, its scale, its aliases, whether it takes a
    prefix) is the registry's answer; the system registers units by giving the
    registry conf text, and resolves a name or an expression by asking it.
    """

    def __init__(self, name: str | None = None, description: str = ""):
        """Initializes a new, clean unit system."""
        self.name = name
        self.description = description
        self._DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}
        self.settings: dict[str, str] = {}
        self.constant_definitions: dict[str, str] = {}
        CompoundUnit._cache.clear()
        Dimension._cache.clear()
        self.Q_ = QuantityFactory(self)

        # The registry of the core is built from the definitions of this
        # system, as conf text (see extend_core).
        self._core_registry = _RustUnitRegistry()
        self._core_texts: list[str] = []
        # the units that expressions resolved to, forgotten whenever the
        # registry changes
        self._units: dict[str, CompoundUnit] = {}

    @property
    def core_registry(self) -> _RustUnitRegistry:
        """The registry of the core, built from what this system registers."""
        return self._core_registry

    def extend_core(self, conf_text: str) -> None:
        """Adds conf text to the core registry of this system, all or nothing.

        Raises:
            ValueError: The core cannot read a line; every such line is named
                and the registry is left as it was.
        """
        self._core_registry.extend_conf(conf_text)
        self._core_texts.append(conf_text)
        self._units.clear()

    def __getstate__(self) -> dict[str, Any]:
        """Pickles the texts of the registry: the registry itself cannot be."""
        state = self.__dict__.copy()
        state["_core_registry"] = None
        state["_units"] = {}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        """Rebuilds the registry of the core from its texts."""
        self.__dict__.update(state)
        self.__dict__.setdefault("_units", {})
        self._core_registry = _RustUnitRegistry()
        for text in self.__dict__.get("_core_texts", ()):
            self._core_registry.extend_conf(text)

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
        exponents and given to the core registry: the core shows the first of
        them where those exponents are written, and the aliases resolve as
        units.

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
                aliases=aliases[1:],
                recipe=recipe,
            )
        )

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

    def register_dimension(self, dimension: Dimension, name: str):
        """Registers a descriptive name for a Dimension instance."""
        if dimension in self._DIMENSION_NAME_REGISTRY:
            log.warning("Dimension '%s' is being redefined.", dimension)
        self._DIMENSION_NAME_REGISTRY[dimension] = name

    def register_unit(
        self,
        symbol: str,
        dimension: Dimension,
        scale: Number | None = None,
        name: str | None = None,
        *aliases: str,
        offset: Number = 0,
        level: tuple[Number, Number] | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
    ) -> None:
        """Registers a unit and its aliases with the system.

        The unit is a scale of its dimension (``scale``, 1 when omitted), a
        scale with a zero point (``offset``, which makes it an absolute scale
        such as degC) or a level (``level=(factor, reference)``, such as dB).
        A `str` or a `Fraction` for a number is exact, a `float` is the
        decimal it prints as.

        The unit is written as conf text and given to the core registry, so a
        unit the core cannot take leaves the system as it was. `name` is a
        label that does not resolve: the names of the unit are the symbol and
        the aliases.

        Raises:
            ValueError: The core cannot take the unit, or the arguments
                contradict each other (a level has no scale).
        """
        self.extend_core(
            unit_definition_text(
                symbol,
                dimension,
                scale,
                aliases,
                offset=offset,
                level=level,
                recipe=recipe,
                allow_prefixes=allow_prefixes,
            )
        )

    def get_unit(self, unit_expression: str) -> CompoundUnit:
        """Retrieves a CompoundUnit from the system based on its notation.

        The expression is read by the parser of the core and every unit of it
        is looked up in the registry: a recipe unit (N) is its base units, a
        prefixed unit (km) is itself.

        Raises:
            UnknownUnitError: A unit of the expression is not known.
            ValueError: The expression is not a unit.
        """
        unit = self._units.get(unit_expression)
        if unit is None:
            if unit_expression == "dimensionless":
                unit = CompoundUnit({})
            else:
                parsed = parse_unit_string(unit_expression, CompoundUnit)
                try:
                    # the registry refuses a unit it does not know
                    self._core_registry.dimension_of(parsed)
                except _CORE_ERRORS as error:
                    raise _python_error(error, parsed, parsed) from error
                unit = parsed.simplify(self)
            self._units[unit_expression] = unit
        return unit

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
        """Checks if the registry of the system resolves a unit or alias."""
        try:
            self.get_unit(item)
        except (UnknownUnitError, ValueError):
            return False
        return True

    def __getattr__(self, name: str) -> CompoundUnit:
        """Allows accessing units as attributes."""
        try:
            return self.get_unit(name)
        except Exception as e:
            raise AttributeError(
                f"'{self.__class__.__name__}' object has no attribute '{name}'"
            ) from e
