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

from physure._core import default_conf
from physure.application.context import normalize_propagation_mode, use_system
from physure.domain.measurement.dimensions import (
    Dimension,
)
from physure.domain.measurement.system import UnitSystem
from physure.domain.measurement.units import CompoundUnit

_CONF_FILE = "physure.conf"


def _conf_section(section: str, entries: dict[str, str]) -> str:
    """Writes entries of the merged conf as the text the core reads."""
    lines = "".join(f"{key} = {value}\n" for key, value in entries.items())
    return f"[{section}]\n{lines}"


class UnitSystemBuilder:
    """A builder class for constructing a UnitSystem instance."""

    def __init__(self, name: str | None = None, verbose: bool = False):
        """Initializes a new builder instance."""
        self._system = UnitSystem(name=name)
        self._verbose = verbose

    def add_settings(self, settings_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds settings from a dictionary of key-value pairs.

        Raises:
            ValueError: If `propagation_mode` names a strategy that does not
                exist. It is checked here, once, rather than on every read: a
                misspelt mode would otherwise fall back to "correlated" and
                report correlated results for a file that asked not to.
        """
        mode = settings_data.get("propagation_mode")
        if mode is not None:
            settings_data = dict(settings_data)
            settings_data["propagation_mode"] = normalize_propagation_mode(
                mode, source="[Settings] in physure.conf"
            )
        self._system.settings.update(settings_data)
        return self

    def add_prefixes(self, prefixes_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds prefixes from a dictionary of prefix definitions."""
        if not prefixes_data:
            return self
        if self._verbose:
            print("\n--- Phase 2: Initializing Prefixes ---")
        self._system.extend_core(_conf_section("Prefixes", prefixes_data))
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
            symbol = value_str.split(",")[0].strip()
            dim_instance = Dimension.from_string(symbol)
            self._system.register_dimension(dim_instance, name.capitalize())
        return self

    def add_units(self, units_data: dict[str, str]) -> UnitSystemBuilder:
        """Adds units from a dictionary of unit definitions."""
        if self._verbose:
            print("\n--- Phase 4: Initializing Units ---")
        self._system.extend_core(_conf_section("Units", units_data))
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
    """Loads the base physure.conf embedded in the native extension."""
    parser.read_string(default_conf())
    if verbose:
        print("  -> Loaded base config: physure._core.default_conf()")


def _load_extra_config_file(
    parser: configparser.ConfigParser, file_name: str, verbose: bool
) -> None:
    """Loads one extra config file, falling back to a direct path lookup."""
    try:
        sys_config_path = resources.files(
            "physure.infrastructure.config.systems"
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
    # interpolation=None: the catalog contains a literal "%"
    # (percent = 0.01, 1, [%, percent, ...]).
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str

    # 1. Always load the base physure.conf
    _load_base_config(parser, verbose)

    # 2. Load extra configuration files (e.g., specific system configs)
    for file_name in extra_config_files or []:
        _load_extra_config_file(parser, file_name, verbose)

    # 3. Load user override (physure.conf in CWD)
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
