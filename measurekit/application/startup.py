"""Handles the initialization and creation of the default unit system.

This module is responsible for reading configuration files (`.conf`), parsing
them, and using a builder pattern (`UnitSystemBuilder`) to construct a fully
configured `UnitSystem` instance. It acts as the assembly root for the library,
piecing together all the definitions for dimensions, prefixes, units, and
constants into a coherent, usable system.
"""

from __future__ import annotations

import configparser
from importlib import resources
from pathlib import Path
from typing import cast

from measurekit.application.context import system_context
from measurekit.domain.measurement.conversions import UnitDefinition
from measurekit.domain.measurement.dimensions import (
    Dimension,
    block_prefixes_for_dimension_symbol,
)
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit


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
        "measurekit.conf",
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
        pass

    # 3. Add the user's override configuration file (High Priority)
    # Check the Current Working Directory (CWD), which is typically the
    # application's root.
    user_config_path = Path.cwd() / "measurekit.conf"

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

    def add_units(self, units_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds units from a dictionary of unit definitions."""
        if self._verbose:
            print("\n--- Phase 4: Initializing Units ---")

        self._system.unit_definitions.update(units_data)

        # Pass 1: Register all units
        for key, value_str in units_data.items():
            aliases = []
            main_part = value_str
            if "[" in value_str:
                main_part, alias_part = value_str.split("[", 1)
                aliases = [
                    a.strip() for a in alias_part.strip()[:-1].split(",")
                ]

            parts = [p.strip() for p in main_part.split(",") if p.strip()]

            allow_prefixes = True
            if "noprefix" in parts:
                allow_prefixes = False
                parts.remove("noprefix")

            factor = float(parts[0])
            dimension = Dimension.from_string(parts[1])

            symbol = aliases[0] if aliases else key
            all_aliases = set([key] + aliases)

            self._system.register_unit(
                symbol,
                dimension,
                factor,
                key,
                *all_aliases,
                allow_prefixes=allow_prefixes,
            )

        # Pass 2: Register recipes for derived units after all base
        # units are available
        for key, value_str in units_data.items():
            if "[" in value_str:
                main_part, _ = value_str.split("[", 1)
            else:
                main_part = value_str

            parts = [p.strip() for p in main_part.split(",") if p.strip()]
            recipe_str = parts[2] if len(parts) > 2 else None

            if recipe_str:
                unit_def = cast(
                    UnitDefinition, self._system.get_definition(key)
                )
                if unit_def and not unit_def.recipe:
                    # Obtain the CompoundUnit object from the recipe.
                    recipe_unit = self._system.get_unit(recipe_str)

                    # Simplify the recipe to its base unit components.
                    # This is crucial for conversion.
                    simplified_recipe = recipe_unit.simplify(self._system)

                    # Assign the simplified recipe to the unit definition
                    # object.
                    unit_def.recipe = simplified_recipe
                    self._system._UNIT_RECIPES[unit_def.symbol] = (
                        simplified_recipe
                    )

        return self

    def add_constants(
        self, constants_data: dict[str, str]
    ) -> UnitSystemBuilder:
        """Adds constants from a dictionary of constant definitions."""
        if not constants_data:
            return self
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
            with system_context(self._system):
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


def _get_config_parser(
    extra_config_files: list[str] = None, verbose: bool = False
) -> configparser.ConfigParser:
    """Creates a ConfigParser with base configs and optional extra files."""
    parser = configparser.ConfigParser()

    # 1. Always load the base measurekit.conf
    try:
        with resources.path(
            "measurekit.infrastructure.config", "measurekit.conf"
        ) as base_config:
            parser.read(str(base_config), encoding="utf-8")
            if verbose:
                print(f"  -> Loaded base config: {base_config}")
    except Exception as e:
        print(f"[WARNING] Could not load base configuration: {e}")

    # 2. Load extra configuration files (e.g., specific system configs)
    if extra_config_files:
        for file_name in extra_config_files:
            try:
                # Try finding it in the systems subpackage first
                with resources.path(
                    "measurekit.infrastructure.config.systems", file_name
                ) as sys_config:
                    parser.read(str(sys_config), encoding="utf-8")
                    if verbose:
                        print(f"  -> Loaded system config: {sys_config}")
            except Exception:
                # Fallback: try as a direct path or user file
                if Path(file_name).is_file():
                    parser.read(file_name, encoding="utf-8")
                    if verbose:
                        print(f"  -> Loaded external config: {file_name}")
                elif verbose:
                    print(f"  -> Config file not found: {file_name}")

    # 3. Load user override (measurekit.conf in CWD)
    user_config_path = Path.cwd() / "measurekit.conf"
    if user_config_path.is_file():
        parser.read(str(user_config_path), encoding="utf-8")
        if verbose:
            print(f"  -> Loaded user override: {user_config_path}")

    return parser


def create_system(
    config_name: str = None, verbose: bool = False
) -> UnitSystem:
    """Creates a UnitSystem, optionally loading a specific system config file.

    Args:
        config_name: The name of the config file (e.g., 'imperial.conf')
                     located in infrastructure/config/systems, or a path.
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
