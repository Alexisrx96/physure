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
from typing import TYPE_CHECKING, Callable

if TYPE_CHECKING:
    from physure._core import Interpreter

__all__ = ["load_ext_functions", "load_domain_module"]


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


def load_domain_module(script_dir: str | Path, stem: str) -> dict[str, Callable] | None:
    """Lazily imports ``<script_dir>/ext/<stem>.py``; returns ``{name: fn}``, or
    ``None`` if no such file exists.

    Used by the interpreter's ``use name from <stem>`` resolution — unlike
    :func:`load_ext_functions`, this doesn't touch a running interpreter and
    only loads the one file a script actually asked for.
    """
    py_file = Path(script_dir) / "ext" / f"{stem}.py"
    if not py_file.is_file():
        return None
    module_name = f"_phs_ext_{stem}"
    spec = importlib.util.spec_from_file_location(module_name, py_file)
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return {
        name: obj
        for name, obj in vars(module).items()
        if not name.startswith("_") and inspect.isfunction(obj) and obj.__module__ == module_name
    }
