"""Manages the active unit system context for the MeasureKit library.

This module provides the mechanism for managing the "current" unit system
in a thread-safe and async-safe manner using Python's `contextvars`.
It eliminates global mutable state, ensuring that concurrent requests or tasks
can operate with different unit systems (e.g. SI vs Imperial) without
interference.
"""

from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Iterator

    from measurekit.domain.measurement.system import UnitSystem

# 1. Context Variable for Thread/Task Isolation
# Stores the active UnitSystem for the current context.
_current_unit_system: ContextVar[UnitSystem | None] = ContextVar(
    "current_unit_system", default=None
)

# Stores the active error propagation mode (e.g. "correlated", "uncorrelated").
_propagation_mode: ContextVar[str] = ContextVar(
    "propagation_mode", default="correlated"
)

# 2. Global fallback for the default system (SI)
# Loaded lazily to avoid circular imports and startup costs.
_global_default_system: UnitSystem | None = None


def _get_current_system_impl() -> UnitSystem:
    """Internal implementation of system retrieval.

    Resolution order:
    1. Check `_current_unit_system` ContextVar (set via `use_system`).
    2. Check `_global_default_system` (cache).
    3. Lazily load the default "International" (SI) system.
    """
    # 1. Check ContextVar
    system = _current_unit_system.get()
    if system is not None:
        return system

    # 2. Check Global Cache
    global _global_default_system
    if _global_default_system is not None:
        return _global_default_system

    # 3. Lazy Load Default System (SI)
    # Import here to avoid circular dependency: context -> startup -> units
    from measurekit.application.startup import create_default_system

    _global_default_system = create_default_system()
    return _global_default_system


# Helper for Dynamo: Eager execution to freeze the context
def get_current_system() -> UnitSystem:
    """Returns the currently active UnitSystem.

    Resolution order:
    1. ContextVar.
    2. Global Cache.
    3. Lazy Load SI.

    Dynamo Note: If compiling, this function runs eagerly to 'freeze' the
    system into the graph as a constant.
    """
    try:
        # Check if we are inside a torch.compile session without a full top-level import
        import sys

        if "torch" in sys.modules:
            import torch

            if torch.compiler.is_compiling():

                @torch.compiler.disable
                def _eager():
                    return _get_current_system_impl()

                return _eager()
    except (ImportError, AttributeError):
        pass

    return _get_current_system_impl()


@contextmanager
def use_system(system_name_or_obj: str | UnitSystem) -> Iterator[None]:
    """Context manager to temporarily switch the active unit system.

    This change is isolated to the current thread or asyncio task.

    Args:
        system_name_or_obj: A `UnitSystem` instance or a string name
                            (e.g., "imperial") to load from config.
    """
    system: UnitSystem

    if isinstance(system_name_or_obj, str):
        # Lazy import to avoid top-level cycles
        from measurekit.application.startup import create_system

        # We assume the user passed a config name like "imperial"
        # If they passed "imperial", we look for "imperial.conf"
        config_name = system_name_or_obj
        if not config_name.endswith(".conf"):
            config_name += ".conf"
        system = create_system(config_name)
    else:
        system = system_name_or_obj

    token = _current_unit_system.set(system)
    try:
        yield
    finally:
        _current_unit_system.reset(token)


@contextmanager
def propagation_mode(mode: str) -> Iterator[None]:
    """Context manager to temporarily switch the active propagation mode.

    Args:
        mode: The propagation strategy to use ("correlated", "uncorrelated",
              "montecarlo", "monte_carlo", "unscented").
    """
    normalized_mode = mode.lower()
    if normalized_mode == "montecarlo":
        normalized_mode = "monte_carlo"
    token = _propagation_mode.set(normalized_mode)
    try:
        yield
    finally:
        _propagation_mode.reset(token)


_UNCERTAINTY_MODE: ContextVar[tuple[str, dict[str, Any]]] = ContextVar(
    "uncertainty_mode", default=("python", {})
)


@contextmanager
def uncertainty_mode(mode: str, **kwargs) -> Iterator[None]:
    """Context manager to swap the uncertainty propagation strategy."""
    token = _UNCERTAINTY_MODE.set((mode.lower(), kwargs))
    try:
        yield
    finally:
        _UNCERTAINTY_MODE.reset(token)


def get_uncertainty_mode() -> tuple[str, dict[str, Any]]:
    """Returns the currently active uncertainty mode and its arguments."""
    return _UNCERTAINTY_MODE.get()


def get_propagation_mode() -> str:
    """Returns the currently active propagation mode."""
    mode = _propagation_mode.get()

    # Fallback to system settings if not explicitly set in context
    # Usually the default "correlated" is set, but we might want to check
    # the active system's settings if we want it to be configurable per system.
    # For now, we follow the spec which suggests a global/context mechanism.
    return mode


# For backward compatibility if needed, though get_current_system is preferred.
get_active_system = get_current_system


def reset_context() -> None:
    """Resets the global context state.

    Clears the global default system cache and resets context variables
    to their default values.
    """
    global _global_default_system
    _global_default_system = None
    _current_unit_system.set(None)
    _propagation_mode.set("correlated")
