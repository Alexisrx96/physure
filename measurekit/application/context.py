"""Manages the active unit system context for the MeasureKit library.

This module provides functions to get the currently active unit system and a
context manager to temporarily switch to a different system. This is crucial
for ensuring that quantity operations are performed within the correct set of
unit definitions, especially in applications that may need to work with
multiple, distinct unit systems simultaneously.
"""

from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterator

    from measurekit.system import UnitSystem

# 1. Global mutable placeholder for the UnitSystem
# This will be set by measurekit/__init__.py after initialization completes.
_global_default_system: UnitSystem | None = None


# 2. Internal function for setting the global default system
def _set_global_default_system(system: UnitSystem) -> None:
    """Internal function to set the global default system after it's built.

    This replaces the implicit import from measurekit.
    """
    global _global_default_system
    _global_default_system = system


# 3. Create a context variable to hold the active system.
_active_system: ContextVar[UnitSystem] = ContextVar("active_system")


def get_active_system() -> UnitSystem:
    """Returns the currently active unit system from the context.

    If no system is set in the context, it falls back to the global
    default_system set by _set_global_default_system.
    """
    # First, check the context variable for a temporary system
    system = _active_system.get(None)
    if system is not None:
        return system

    # Fall back to the global default system placeholder
    if _global_default_system is None:
        # If this happens, it means code is calling get_active_system()
        # before the system has been fully created and set.
        # The calling code (e.g., in startup.py) should explicitly pass the
        # UnitSystem object it is currently building during this bootstrap
        # phase.
        raise RuntimeError(
            "Attempted to access the default UnitSystem before its "
            "initialization was complete. Pass the current system explicitly "
            "to the Quantity factory during setup."
        )

    return _global_default_system


@contextmanager
def system_context(system: UnitSystem) -> Iterator[None]:
    """A context manager to temporarily set the active unit system."""
    token = _active_system.set(system)
    try:
        yield
    finally:
        _active_system.reset(token)


# --- Expose Core Domain Objects and Exceptions ---
# IMPORTANT: The public functions remain:
# from measurekit.context import get_active_system, system_context
