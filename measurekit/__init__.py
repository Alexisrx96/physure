"""MeasureKit: A Python Library for Unit-Aware Scientific Calculations."""

import contextlib
import sys
from typing import Any

# Version
__version__ = "0.1.8"

_IO_ATTRS = {"save_state", "load_state"}
_UNITS_ATTRS = {"units", "CompoundUnit"}
_STARTUP_ATTRS = {"create_system", "create_default_system"}
_CONTEXT_ATTRS = {
    "get_current_system",
    "get_active_system",
    "get_propagation_mode",
    "propagation_mode",
    "uncertainty_mode",
    "system_context",
}
_EXCEPTION_ATTRS = {
    "ConversionError",
    "MeasureKitError",
    "UnitNotFoundError",
    "UnknownUnitError",
}
_VECTORIZED_ATTRS = {"MeasureKitContext", "PruningConfig"}


def _load_io_attr(name: str) -> Any:
    from measurekit.application.io import load_state, save_state

    return save_state if name == "save_state" else load_state


def _load_units_attr(name: str) -> Any:
    from measurekit.domain.measurement.units import CompoundUnit, units

    return units if name == "units" else CompoundUnit


def _load_startup_attr(name: str) -> Any:
    from measurekit.application.startup import (
        create_default_system,
        create_system,
    )

    return create_system if name == "create_system" else create_default_system


def _load_context_attr(name: str) -> Any:
    from measurekit.application.context import (
        get_current_system,
        get_propagation_mode,
        propagation_mode,
        uncertainty_mode,
        use_system,
    )

    _context_map = {
        "get_current_system": get_current_system,
        "get_active_system": get_current_system,
        "get_propagation_mode": get_propagation_mode,
        "propagation_mode": propagation_mode,
        "uncertainty_mode": uncertainty_mode,
        "system_context": use_system,
    }
    return _context_map[name]


def _load_vectorized_attr(name: str) -> Any:
    from measurekit.domain.measurement.vectorized_uncertainty import (
        MeasureKitContext,
        PruningConfig,
    )

    return MeasureKitContext if name == "MeasureKitContext" else PruningConfig


def __getattr__(name: str) -> Any:
    """Implement lazy loading for all public API members."""
    if name in _IO_ATTRS:
        return _load_io_attr(name)

    if name == "Q_":
        from measurekit.application.factories import QuantityFactory

        return QuantityFactory()

    if name in _UNITS_ATTRS:
        return _load_units_attr(name)

    if name == "Quantity":
        from measurekit.domain.measurement.quantity import Quantity

        return Quantity

    if name == "Uncertainty":
        from measurekit.domain.measurement.uncertainty import Uncertainty

        return Uncertainty

    if name in _STARTUP_ATTRS:
        return _load_startup_attr(name)

    if name == "default_system":
        from measurekit.domain.measurement.units import get_default_system

        return get_default_system()

    if name == "jit":
        from measurekit._jit import jit

        return jit

    if name in _CONTEXT_ATTRS:
        return _load_context_attr(name)

    if name == "get_unit":

        def get_unit(unit_expression):
            from measurekit.application.context import get_current_system

            return get_current_system().get_unit(unit_expression)

        return get_unit

    if name in _EXCEPTION_ATTRS:
        import measurekit.domain.exceptions as exc

        return getattr(exc, name)

    if name == "config":

        class ConfigProxy:
            @property
            def propagation_mode(self):
                from measurekit.application.context import propagation_mode

                return propagation_mode

            def set_propagation_mode(self, mode: str):
                from measurekit.application.context import propagation_mode

                return propagation_mode(mode)

        return ConfigProxy()

    if name in _VECTORIZED_ATTRS:
        return _load_vectorized_attr(name)

    if name in {"equivalencies", "spectral", "thermodynamic"}:
        from measurekit.domain.measurement.equivalencies import (
            equivalencies,
            spectral,
            thermodynamic,
        )

        _eq_map = {
            "equivalencies": equivalencies,
            "spectral": spectral,
            "thermodynamic": thermodynamic,
        }
        return _eq_map[name]

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


# Register Extensions only if libraries are already loaded
if "pandas" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        from measurekit.ext import pandas_support

if "numba" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        import measurekit.ext.numba_support

__all__ = [
    "Q_",
    "CompoundUnit",
    "ConversionError",
    "MeasureKitContext",
    "MeasureKitError",
    "PruningConfig",
    "Quantity",
    "Uncertainty",
    "UnitNotFoundError",
    "UnknownUnitError",
    "create_default_system",
    "create_system",
    "default_system",
    "equivalencies",
    "get_active_system",
    "get_current_system",
    "get_unit",
    "jit",
    "load_state",
    "save_state",
    "spectral",
    "system_context",
    "thermodynamic",
    "uncertainty_mode",
    "units",
]
