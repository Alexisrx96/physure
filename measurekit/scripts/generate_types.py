# measurekit/scripts/generate_types.py
# AUTO-GENERATED FILE. DO NOT EDIT. Run 'measurekit sync-types' to update.

import os

from measurekit.domain.measurement.units import units


def generate():
    """Generates type definitions and stubs for MeasureKit units."""
    # Ensure units are discovered
    units.discover_plugins()

    names = units.available_units

    # 1. Generate Literal for .to() method
    if not names:
        # Fallback if no units found (unlikely but safe)
        literal_type = "str"
    else:
        # Format names as a list of strings
        names_str = ", ".join(f'"{name}"' for name in names)
        literal_type = f"Literal[{names_str}]"

    generated_types_path = os.path.join(
        os.path.dirname(os.path.dirname(__file__)), "_generated_types.py"
    )

    with open(generated_types_path, "w", encoding="utf-8") as f:
        f.write("# AUTO-GENERATED FILE. DO NOT EDIT.\n")
        f.write("# Run 'measurekit sync-types' to update.\n\n")
        f.write("from typing import Literal, Union\n\n")
        f.write(f"UnitName = {literal_type}\n")

    # 2. Generate Stub for Registry attributes
    registry_stub_path = os.path.join(
        os.path.dirname(os.path.dirname(__file__)), "core", "registry.pyi"
    )

    with open(registry_stub_path, "w", encoding="utf-8") as f:
        f.write("# AUTO-GENERATED FILE. DO NOT EDIT.\n")
        f.write("# Run 'measurekit sync-types' to update.\n\n")
        f.write("from typing import Any\n")
        f.write("from measurekit.domain.measurement.units import Unit\n\n")
        f.write("class UnitRegistry:\n")
        f.write("    _registry: dict[str, Any]\n")
        f.write("    _lazy_loaders: dict[str, Any]\n")
        f.write("    _discovered: bool\n")
        for name in names:
            f.write(f"    {name}: Unit\n")
        f.write("    def register(self, name: str, unit: Any) -> None:\n")
        f.write("        ...\n")
        f.write(
            "    def register_lazy("
            "self, name: str, loader_func: Any) -> None:\n"
        )
        f.write("        ...\n")
        f.write("    def discover_plugins(self) -> None:\n")
        f.write("        ...\n")
        f.write("    @property\n")
        f.write("    def available_units(self) -> list[str]:\n")
        f.write("        ...\n")


if __name__ == "__main__":
    generate()
