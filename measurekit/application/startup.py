"""Handles the initialization and creation of the default unit system.

This module is responsible for reading configuration files (`.conf`), parsing
them, and using a builder pattern (`UnitSystemBuilder`) to construct a fully
configured `UnitSystem` instance. It acts as the assembly root for the library,
piecing together all the definitions for dimensions, prefixes, units, and
constants into a coherent, usable system.
"""

from __future__ import annotations

import configparser
import math
from importlib import resources
from pathlib import Path
from typing import TYPE_CHECKING, cast

from measurekit.application.context import use_system
from measurekit.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)
from measurekit.domain.measurement.dimensions import (
    Dimension,
    block_prefixes_for_dimension_symbol,
)
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit

if TYPE_CHECKING:
    from measurekit.domain.measurement.conversions import UnitDefinition

_CONF_FILE = "measurekit.conf"


def _load_all_configurations_into(
    parser: configparser.ConfigParser, verbose: bool
):
    """Reads all configuration files and loads them into the given parser.

    This function prioritizes the user's custom 'measurekit.conf' file
    found in the application's root (CWD) over the packaged defaults.
    """
    if verbose:
        print("\n--- Phase 1: Loading Configuration Files ---")

    # 1. Define the list of internal configuration files
    config_files = [
        _CONF_FILE,
        "systems/international.conf",
        "systems/imperial.conf",
    ]

    paths_to_read = []

    # 2. Add packaged library configuration files (Low Priority)
    try:
        lib_config_dir = resources.files("measurekit.infrastructure.config")
        for file_name in config_files:
            # Note: We must use str() because configparser.read()
            # might not support Path objects across all Python versions
            file_path = lib_config_dir / file_name
            if file_path.is_file():
                paths_to_read.append(str(file_path))
                if verbose:
                    print(
                        f"  -> Found (Library Default): {file_path.name} from"
                        " package"
                    )
            elif verbose:
                print(f"  -> Not found (Library Default): {file_path.name}")
    except ModuleNotFoundError:
        print(
            "[WARNING] Could not locate built-in library configuration files."
        )
        # Continue to check for user config even if library defaults fail

    # 3. Add the user's override configuration file (High Priority)
    # Check the Current Working Directory (CWD), which is typically the
    # application's root.
    user_config_path = Path.cwd() / _CONF_FILE

    if user_config_path.is_file():
        # Append the path LAST. The last file read overrides previous values.
        paths_to_read.append(str(user_config_path))
        if verbose:
            print(f"  -> Found (User Override): {user_config_path.resolve()}")
    elif verbose:
        print(
            f"  -> Not found (User Override): {user_config_path.name} in CWD."
        )

    if paths_to_read:
        parser.read(paths_to_read, encoding="utf-8")
    else:
        print("[WARNING] No configuration files were loaded.")


class UnitSystemBuilder:
    """A builder class for constructing a UnitSystem instance."""

    def __init__(self, name: str | None = None, verbose: bool = False):
        """Initializes a new builder instance."""
        self._system = UnitSystem(name=name)
        self._verbose = verbose

    def add_settings(self, settings_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds settings from a dictionary of key-value pairs."""
        self._system.settings.update(settings_data)
        return self

    def add_prefixes(self, prefixes_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds prefixes from a dictionary of prefix definitions."""
        if not prefixes_data:
            return self
        if self._verbose:
            print("\n--- Phase 2: Initializing Prefixes ---")
        for name, value_str in prefixes_data.items():
            symbol, factor_str = [p.strip() for p in value_str.split(",")]
            self._system.register_prefix(
                symbol=symbol, factor=float(factor_str), name=name
            )
        return self

    def add_dimensions(
        self, dimensions_data: dict[str, str]
    ) -> UnitSystemBuilder:
        """Adds dimensions from a dictionary of dimension definitions."""
        if not dimensions_data:
            return self
        if self._verbose:
            print("\n--- Phase 3: Initializing Dimensions ---")
        base_symbols = [
            v.split(",")[0].strip() for v in dimensions_data.values()
        ]
        Dimension.set_base_dimensions(base_symbols)
        for name, value_str in dimensions_data.items():
            parts = [p.strip() for p in value_str.split(",")]
            symbol = parts[0]
            if len(parts) > 1 and parts[1] == "noprefix":
                block_prefixes_for_dimension_symbol(symbol)
            dim_instance = Dimension.from_string(symbol)
            self._system.register_dimension(dim_instance, name.capitalize())
        return self

    @staticmethod
    def _split_aliases(value_str: str) -> tuple[str, list[str]]:
        """Splits a unit value string into its main part and alias list."""
        if "[" not in value_str:
            return value_str, []
        main_part, alias_part = value_str.split("[", 1)
        aliases = [a.strip() for a in alias_part.strip()[:-1].split(",")]
        return main_part, aliases

    @staticmethod
    def _parse_log_unit(
        parts: list[str],
    ) -> tuple[Dimension, LogarithmicConverter, str]:
        """Parses the components of a logarithmic unit definition."""
        factor = float(parts[1])
        reference = float(parts[2])
        dim_str = parts[3]
        dimension = (
            Dimension({}) if dim_str == "1" else Dimension.from_string(dim_str)
        )
        return dimension, LogarithmicConverter(factor, reference), "delta"

    @staticmethod
    def _parse_linear_unit(
        parts: list[str],
    ) -> tuple[Dimension, LinearConverter | OffsetConverter, str]:
        """Parses the components of a linear or offset unit definition."""
        factor = float(parts[0])
        offset = 0.0
        dim_index = 1

        if len(parts) > 2:
            try:
                offset = float(parts[1])
                dim_index = 2
            except ValueError:
                pass

        dimension = Dimension.from_string(parts[dim_index])

        if offset != 0:
            return dimension, OffsetConverter(factor, offset), "absolute"
        return dimension, LinearConverter(factor), "delta"

    def _register_unit_pass1(self, key: str, value_str: str) -> None:
        """Registers a single unit (pass 1: base registration)."""
        main_part, aliases = self._split_aliases(value_str)
        parts = [p.strip() for p in main_part.split(",") if p.strip()]

        allow_prefixes = True
        if "noprefix" in parts:
            allow_prefixes = False
            parts.remove("noprefix")

        if parts[0].lower() == "log":
            dimension, converter, kind = self._parse_log_unit(parts)
        else:
            dimension, converter, kind = self._parse_linear_unit(parts)

        symbol = aliases[0] if aliases else key
        all_aliases = {key, *aliases}

        self._system.register_unit(
            symbol,
            dimension,
            converter,
            key,
            *all_aliases,
            allow_prefixes=allow_prefixes,
            kind=kind,
        )

    def _register_unit_pass2(self, key: str, value_str: str) -> None:
        """Registers the recipe for a derived unit (pass 2)."""
        main_part, aliases = self._split_aliases(value_str)
        parts = [
            p.strip()
            for p in main_part.split(",")
            if p.strip() and p.strip() != "noprefix"
        ]

        is_log = parts[0].lower() == "log"
        if is_log:
            return

        has_offset = False
        if len(parts) > 2:
            try:
                float(parts[1])
                has_offset = True
            except ValueError:
                pass

        if has_offset:
            return

        recipe_str = parts[2] if len(parts) > 2 else None
        if not recipe_str:
            return

        unit_def = cast("UnitDefinition", self._system.get_definition(key))
        if not unit_def or unit_def.recipe:
            return

        # Recipe substitution replaces the unit by its recipe at lookup
        # time, so it is only valid when the unit IS its recipe (scale
        # exactly 1.0, e.g. N = kg*m/s^2). For scaled units such as
        # degree = 0.0174... rad, substituting silently dropped the
        # scale factor (Q_(90, "deg") used to become 90 rad).
        scale = getattr(unit_def.converter, "scale", None)
        if scale is None or not math.isclose(scale, 1.0):
            return

        all_aliases = [key, *aliases]

        # Obtain the CompoundUnit object from the recipe.
        recipe_unit = self._system.get_unit(recipe_str)

        # Simplify the recipe to its base unit components.
        # This is crucial for conversion.
        simplified_recipe = recipe_unit.simplify(self._system)

        # Assign the simplified recipe to the unit definition object.
        unit_def.recipe = simplified_recipe
        self._system._UNIT_RECIPES[unit_def.symbol] = simplified_recipe
        # Register the alias for the simplified recipe so that
        # to_string(use_alias=True) works for base-unit quantities.
        self._system.register_alias(
            simplified_recipe.exponents, *reversed(all_aliases)
        )

    def add_units(self, units_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds units from a dictionary of unit definitions."""
        if self._verbose:
            print("\n--- Phase 4: Initializing Units ---")

        self._system.unit_definitions.update(units_data)

        # Pass 1: Register all units
        for key, value_str in units_data.items():
            self._register_unit_pass1(key, value_str)

        # Pass 2: Register recipes for derived units after all base
        # units are available
        for key, value_str in units_data.items():
            self._register_unit_pass2(key, value_str)

        return self

    def add_constants(
        self, constants_data: dict[str, str]
    ) -> UnitSystemBuilder:
        """Adds constants from a dictionary of constant definitions."""
        if not constants_data:
            return self
        self._system.constant_definitions.update(constants_data)
        if self._verbose:
            print("\n--- Phase 5: Initializing Constants ---")
        for value_str in constants_data.values():
            value_str_part, unit_str = value_str.split(maxsplit=1)
            value = float(value_str_part)
            unit = (
                self._system.get_unit(unit_str.strip())
                if unit_str.strip() != "1"
                else CompoundUnit({})
            )
            with use_system(self._system):
                _ = self._system.Q_(value, unit)
        return self

    def add_system_info(
        self, system_data: dict[str, str]
    ) -> UnitSystemBuilder:
        """Adds system metadata from the [System] section."""
        if not system_data:
            return self

        if "name" in system_data:
            self._system.name = system_data["name"]

        # Store other system settings (like base_units) in the settings dict
        self._system.settings.update(system_data)
        return self

    def build(self) -> UnitSystem:
        """Returns the fully constructed and configured UnitSystem object."""
        if self._verbose:
            print("\n--- Initialization Complete ---")
        return self._system


def _load_base_config(
    parser: configparser.ConfigParser, verbose: bool
) -> None:
    """Loads the base measurekit.conf packaged with the library."""
    try:
        base_config_path = resources.files(
            "measurekit.infrastructure.config"
        ).joinpath(_CONF_FILE)
        with resources.as_file(base_config_path) as base_config:
            parser.read(str(base_config), encoding="utf-8")
            if verbose:
                print(f"  -> Loaded base config: {base_config}")
    except Exception as e:
        print(f"[WARNING] Could not load base configuration: {e}")


def _load_extra_config_file(
    parser: configparser.ConfigParser, file_name: str, verbose: bool
) -> None:
    """Loads one extra config file, falling back to a direct path lookup."""
    try:
        sys_config_path = resources.files(
            "measurekit.infrastructure.config.systems"
        ).joinpath(file_name)
        with resources.as_file(sys_config_path) as sys_config:
            parser.read(str(sys_config), encoding="utf-8")
            if verbose:
                print(f"  -> Loaded system config: {sys_config}")
        return
    except Exception:
        pass

    # Fallback: try as a direct path or user file
    if Path(file_name).is_file():
        parser.read(file_name, encoding="utf-8")
        if verbose:
            print(f"  -> Loaded external config: {file_name}")
    elif verbose:
        print(f"  -> Config file not found: {file_name}")


def _get_config_parser(
    extra_config_files: list[str] | None = None, verbose: bool = False
) -> configparser.ConfigParser:
    """Creates a ConfigParser with base configs and optional extra files."""
    parser = configparser.ConfigParser()
    parser.optionxform = str

    # 1. Always load the base measurekit.conf
    _load_base_config(parser, verbose)

    # 2. Load extra configuration files (e.g., specific system configs)
    for file_name in extra_config_files or []:
        _load_extra_config_file(parser, file_name, verbose)

    # 3. Load user override (measurekit.conf in CWD)
    user_config_path = Path.cwd() / _CONF_FILE
    if user_config_path.is_file():
        parser.read(str(user_config_path), encoding="utf-8")
        if verbose:
            print(f"  -> Loaded user override: {user_config_path}")

    return parser


def create_system(
    config_name: str | None = None, verbose: bool = False
) -> UnitSystem:
    """Creates a UnitSystem, optionally loading a specific system config file.

    Args:
        config_name: The name of the config file (e.g., 'imperial.conf')
                     located in infrastructure/config/systems, or a path.
        verbose: If True, prints loading details.
    """
    extra_files = [config_name] if config_name else []
    parser = _get_config_parser(extra_files, verbose)

    # Determine system name
    sys_name = "Default"
    if parser.has_section("System") and parser.has_option("System", "name"):
        sys_name = parser.get("System", "name")

    builder = UnitSystemBuilder(name=sys_name, verbose=verbose)

    return (
        builder.add_settings(
            dict(parser.items("Settings")) if "Settings" in parser else {}
        )
        .add_system_info(
            dict(parser.items("System")) if "System" in parser else {}
        )
        .add_prefixes(
            dict(parser.items("Prefixes")) if "Prefixes" in parser else {}
        )
        .add_dimensions(
            dict(parser.items("Dimensions")) if "Dimensions" in parser else {}
        )
        .add_units(dict(parser.items("Units")) if "Units" in parser else {})
        .add_constants(
            dict(parser.items("Constants")) if "Constants" in parser else {}
        )
        .build()
    )


def create_default_system(verbose: bool = False) -> UnitSystem:
    """Creates the default UnitSystem (SI)."""
    return create_system("international.conf", verbose=verbose)
