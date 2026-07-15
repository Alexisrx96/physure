import configparser
import pathlib
import pprint
import sys

# Ensure we can import measurekit from the project root
PROJECT_ROOT = pathlib.Path(__file__).parent.parent
sys.path.append(str(PROJECT_ROOT))


def load_config(config_path):
    """Parses the INI configuration file."""
    path = pathlib.Path(config_path)
    print(f"Reading configuration from: {path}")
    parser = configparser.ConfigParser()
    parser.optionxform = str
    try:
        parser.read(str(path), encoding="utf-8")
        return parser
    except Exception as e:
        print(f"Failed to read config file: {e}")
        sys.exit(1)


def parse_prefixes(parser):
    """Extracts prefixes from config."""
    prefixes = {}
    if "Prefixes" in parser:
        for name, value in parser["Prefixes"].items():
            parts = [p.strip() for p in value.split(",")]
            symbol = parts[0]
            prefixes[name] = symbol
    return prefixes


def parse_unit_entry(value_str):
    """Parses a single unit entry line."""
    aliases = []
    main_part = value_str
    if "[" in value_str:
        main_part, alias_block = value_str.split("[", 1)
        if "]" in alias_block:
            alias_content = alias_block.split("]")[0]
            aliases = [a.strip() for a in alias_content.split(",")]
        else:
            aliases = [a.strip() for a in alias_block.split(",")]
    else:
        main_part = main_part.split("#")[0]

    attrs = [p.strip() for p in main_part.split(",") if p.strip()]
    allow_prefixes = "noprefix" not in attrs
    return aliases, allow_prefixes


def _register_unit_and_prefixes(
    name, scope_name, allow_prefix, prefixes, unit_definitions, index_map
):
    """Registers a unit and its prefixed variants."""
    import keyword

    if not name.isidentifier():
        return
    if keyword.iskeyword(name):
        print(f"Skipping keyword alias: {name}")
        return

    unit_definitions.append(name)
    index_map[name] = scope_name

    if allow_prefix:
        for prefix_name in prefixes:
            prefixed_name = prefix_name + name
            unit_definitions.append(prefixed_name)
            index_map[prefixed_name] = scope_name


def parse_units(parser, prefixes):
    """Parses unit definitions and expands prefixes."""
    index_map = {}
    unit_definitions = []
    scope_name = "core"

    if "Units" in parser:
        print("Processing [Units] section...")
        for key, value_str in parser["Units"].items():
            aliases, allow_prefix = parse_unit_entry(value_str)
            all_names = sorted({key, *aliases})

            for name in all_names:
                _register_unit_and_prefixes(
                    name,
                    scope_name,
                    allow_prefix,
                    prefixes,
                    unit_definitions,
                    index_map,
                )

    return sorted(set(unit_definitions)), index_map


def generate_core_module(unique_units, output_dir):
    """Generates the core unit module."""
    lines = [
        "# GENERATED CODE - DO NOT EDIT",
        "from measurekit.domain.measurement.units import CompoundUnit",
        "",
        "# This module defines the static unit objects for the 'core' scope.",
        "# These are simple CompoundUnit wrappers.",
        "",
    ]
    for unit_name in unique_units:
        lines.append(f'{unit_name} = CompoundUnit({{"{unit_name}": 1}})')
    lines.append("")

    with open(output_dir / "core.py", "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"Generated core.py with {len(unique_units)} units.")


def generate_index_module(index_map, output_dir):
    """Generates the index module."""
    lines = [
        "# GENERATED CODE - DO NOT EDIT",
        f"UNIT_INDEX = {pprint.pformat(index_map)}",
        "",
    ]
    with open(output_dir / "_index.py", "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


def generate_init_module(output_dir):
    """Generates the __init__.py for the package."""
    content = [
        '"""Lazy-loading unit package."""',
        "from __future__ import annotations",
        "import typing",
        "",
        "if typing.TYPE_CHECKING:",
        "    from measurekit.domain.measurement.units import CompoundUnit",
        "",
        "try:",
        "    from ._index import UNIT_INDEX",
        "except ImportError:",
        "    UNIT_INDEX = {}",
        "",
        "def __getattr__(name: str) -> CompoundUnit:",
        "    if name in UNIT_INDEX:",
        "        scope = UNIT_INDEX[name]",
        "        module = __import__(",
        '            f"measurekit.units.{scope}", fromlist=[name]',
        "        )",
        "        return getattr(module, name)",
        "    raise AttributeError(",
        '        f"module {__name__!r} has no attribute {name!r}"',
        "    )",
        "",
        "def __dir__() -> list[str]:",
        "    return sorted(list(globals().keys()) + list(UNIT_INDEX.keys()))",
        "",
    ]
    with open(output_dir / "__init__.py", "w", encoding="utf-8") as f:
        f.write("\n".join(content))


def compile_units(config_path, output_dir):
    """Compiles units from config file to Python modules."""
    out_path = pathlib.Path(output_dir)
    out_path.mkdir(parents=True, exist_ok=True)

    parser = load_config(config_path)
    prefixes = parse_prefixes(parser)
    unique_units, index_map = parse_units(parser, prefixes)

    generate_core_module(unique_units, out_path)
    generate_index_module(index_map, out_path)
    generate_init_module(out_path)

    print("Compilation complete.")


if __name__ == "__main__":
    # Point to the measurekit.conf file
    # We assume CWD is project root d:\measurekit
    conf_file = "measurekit/infrastructure/config/measurekit.conf"
    compile_units(conf_file, "measurekit/units")
