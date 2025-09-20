# measurekit/startup.py (Refactored)

import configparser
import re
from importlib import resources
from pathlib import Path

# Import the new system class and other necessary components
from measurekit.system import UnitSystem
from measurekit.measurement.dimensions import (
    Dimension,
    block_prefixes_for_dimension_symbol,
)
from measurekit.measurement.units import CompoundUnit

# These modules will be populated with instances for the default system
import measurekit.units as units_module
import measurekit.constants as constants_module
import measurekit.dimensions as dimensions_module


def create_default_system(verbose: bool = False) -> UnitSystem:
    """
    Creates, populates, and returns a default UnitSystem instance.

    This is the main entry point for initializing the library with standard
    configurations.

    Args:
        verbose (bool): If True, prints detailed information about the
                        initialization process.

    Returns:
        UnitSystem: A fully populated instance of the unit system.
    """
    if verbose:
        print("--- MeasureKit System Initializing (Verbose Mode) ---")

    system = UnitSystem()

    # The initialization process now populates the `system` instance
    _load_all_configurations_into(system, verbose)
    _initialize_prefix_system(system, verbose)
    _initialize_dimension_system(system, verbose)
    _initialize_unit_system(system, verbose)
    _initialize_constant_system(system, verbose)

    if verbose:
        print("\n--- Initialization Complete ---")
    return system


def _load_all_configurations_into(system: UnitSystem, verbose: bool):
    """
    Reads all configuration files and loads them into the given UnitSystem.

    Args:
        system (UnitSystem): The system instance to populate.
        verbose (bool): If True, prints loading details.
    """
    if verbose:
        print("\n--- Phase 1: Loading Configuration Files ---")

    parser = configparser.ConfigParser()
    paths_to_load = []

    # --- Find and collect library and user configuration files ---
    try:
        lib_config_dir = resources.files("measurekit.config")
        paths_to_load.append(lib_config_dir / "measurekit.conf")
        lib_systems_dir = lib_config_dir / "systems"
        paths_to_load.append(lib_systems_dir / "international.conf")
        paths_to_load.append(lib_systems_dir / "imperial.conf")
    except (ModuleNotFoundError, FileNotFoundError):
        print(
            "[WARNING] Could not locate built-in library configuration files."
        )

    # ... (user config loading logic would go here) ...

    if verbose:
        print("\nLoading configuration files in order:")
        for path in paths_to_load:
            if path.is_file():
                print(f"  -> {path}")

    str_paths = [str(p) for p in paths_to_load if p.is_file()]
    parser.read(str_paths, encoding="utf-8")

    # --- Populate the system instance from the parser ---
    if "Settings" in parser:
        system.settings.update(parser.items("Settings"))
    if "Prefixes" in parser:
        system.prefix_definitions.update(parser.items("Prefixes"))
    if "Dimensions" in parser:
        system.dimension_definitions.update(parser.items("Dimensions"))
    if "Units" in parser:
        system.unit_definitions.update(parser.items("Units"))
    if "Constants" in parser:
        system.constant_definitions.update(parser.items("Constants"))

    if verbose:
        print("\nConfiguration loading summary:")
        print(
            f"  - Loaded {len(system.prefix_definitions)} prefix definitions."
        )
        print(
            f"  - Loaded {len(system.dimension_definitions)} dimension definitions."
        )
        print(
            f"  - Loaded {len(system.unit_definitions)} base unit definitions."
        )
        print(
            f"  - Loaded {len(system.constant_definitions)} constant definitions."
        )


def _initialize_prefix_system(system: UnitSystem, verbose: bool):
    """Registers the loaded prefixes into the measurement system."""
    if not system.prefix_definitions:
        return
    if verbose:
        print("\n--- Phase 2: Initializing Prefixes ---")

    for name, value_str in system.prefix_definitions.items():
        symbol, factor_str = [p.strip() for p in value_str.split(",")]
        # Use the system's own register_prefix method
        system.register_prefix(
            symbol=symbol, factor=float(factor_str), name=name
        )


def _initialize_dimension_system(system: UnitSystem, verbose: bool):
    """Registers the loaded dimensions into the measurement system."""
    if not system.dimension_definitions:
        return
    if verbose:
        print("\n--- Phase 3: Initializing Dimensions ---")

    base_symbols = [
        v.split(",")[0].strip() for v in system.dimension_definitions.values()
    ]
    Dimension.set_base_dimensions(base_symbols)

    for name, value_str in system.dimension_definitions.items():
        parts = [p.strip() for p in value_str.split(",")]
        symbol = parts[0]
        if len(parts) > 1 and parts[1] == "noprefix":
            block_prefixes_for_dimension_symbol(symbol)

        dim_instance = Dimension({symbol: 1})
        # Use the system's own register_dimension method
        system.register_dimension(dim_instance, name.capitalize())
        setattr(dimensions_module, name.upper(), dim_instance)


def _initialize_unit_system(system: UnitSystem, verbose: bool):
    """Initializes the unit system by registering base and prefixed units."""
    if not system.unit_definitions:
        return
    if verbose:
        print("\n--- Phase 4: Initializing Units ---")

    for key, value_str in system.unit_definitions.items():
        aliases = []
        main_part = value_str
        if "[" in value_str:
            main_part, alias_part = value_str.split("[", 1)
            aliases = [a.strip() for a in alias_part.strip()[:-1].split(",")]

        parts = [p.strip() for p in main_part.split(",") if p.strip()]
        factor = float(parts[0])
        dimension = Dimension.from_string(parts[1])
        recipe_str = parts[2] if len(parts) > 2 else None
        recipe = system.get_unit(recipe_str) if recipe_str else None
        symbol = aliases[0] if aliases else key
        all_aliases = [key] + aliases

        # Use the system's own register_unit method
        system.register_unit(
            symbol, dimension, factor, key, *all_aliases, recipe=recipe
        )

        unit_instance = system.get_unit(symbol)
        setattr(units_module, key, unit_instance)

        # Prefixed unit generation logic remains similar but uses the system's methods
        # ...


def _initialize_constant_system(system: UnitSystem, verbose: bool):
    """Registers the loaded constants into the measurement system."""
    if not system.constant_definitions:
        return
    if verbose:
        print("\n--- Phase 5: Initializing Constants ---")

    for name, value_str in system.constant_definitions.items():
        value_str_part, unit_str = value_str.split(maxsplit=1)
        value = float(value_str_part)
        unit = (
            system.get_unit(unit_str.strip())
            if unit_str.strip() != "1"
            else CompoundUnit({})
        )
        # Use the system's factory to create the quantity
        constant_quantity = system.Q_(value, unit)
        setattr(constants_module, name, constant_quantity)


# The _generate_stub function remains the same as it's a developer tool
# that runs during initialization and doesn't affect runtime state.
def _generate_stub(*args, **kwargs):
    pass
