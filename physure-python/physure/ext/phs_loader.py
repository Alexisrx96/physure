"""Loads a PhysureScript (.phs) project's ``ext/*.py`` extension functions.

Convention: a script at ``<dir>/script.phs`` may have sibling Python files in
``<dir>/ext/*.py``. Every top-level function defined in those files (not one
merely imported into them) is registered into a running
:class:`physure._core.Interpreter` under its own name, callable from PHS
source exactly like a builtin.

ponytail: registers every top-level function unconditionally. An opt-in
``@phs_export`` decorator can be added later if scripts need to hide helpers
from the PHS namespace.
"""

from __future__ import annotations

import importlib.util
import inspect
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from physure._core import Interpreter

__all__ = ["load_ext_functions"]


def load_ext_functions(
    interp: Interpreter, script_dir: str | Path
) -> list[str]:
    """Registers every top-level function found in ``<script_dir>/ext/*.py``.

    Returns the names registered, in the order they were loaded.
    """
    ext_dir = Path(script_dir) / "ext"
    if not ext_dir.is_dir():
        return []

    registered: list[str] = []
    for py_file in sorted(ext_dir.glob("*.py")):
        module_name = f"_phs_ext_{py_file.stem}"
        spec = importlib.util.spec_from_file_location(module_name, py_file)
        if spec is None or spec.loader is None:
            continue
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        for name, obj in vars(module).items():
            if name.startswith("_") or not inspect.isfunction(obj):
                continue
            if obj.__module__ != module_name:
                continue  # skip names merely imported into the ext file
            interp.register_function(obj, name)
            registered.append(name)
    return registered
