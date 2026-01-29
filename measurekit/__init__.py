"""MeasureKit: A Python Library for Unit-Aware Scientific Calculations."""

import sys
from typing import Any

# Version
__version__ = "0.0.3"


def __getattr__(name: str) -> Any:
    """Implement lazy loading for all public API members."""
    if name == "save_state":
        from measurekit.application.io import save_state

        return save_state

    if name == "load_state":
        from measurekit.application.io import load_state

        return load_state

    if name == "Q_":
        from measurekit.application.factories import QuantityFactory

        return QuantityFactory()

    if name == "units":
        from measurekit.domain.measurement.units import units

        return units

    if name == "CompoundUnit":
        from measurekit.domain.measurement.units import CompoundUnit

        return CompoundUnit

    if name == "Quantity":
        from measurekit.domain.measurement.quantity import Quantity

        return Quantity

    if name == "Uncertainty":
        from measurekit.domain.measurement.uncertainty import Uncertainty

        return Uncertainty

    if name == "create_system":
        from measurekit.application.startup import create_system

        return create_system

    if name == "create_default_system":
        from measurekit.application.startup import create_default_system

        return create_default_system

    if name == "default_system":
        from measurekit.domain.measurement.units import get_default_system

        return get_default_system()

    if name == "jit":
        from measurekit._jit import jit

        return jit

    if name in (
        "get_current_system",
        "get_active_system",
        "get_propagation_mode",
        "propagation_mode",
        "uncertainty_mode",
        "system_context",
    ):
        from measurekit.application.context import (
            get_current_system,
            get_propagation_mode,
            propagation_mode,
            uncertainty_mode,
            use_system,
        )

        if name == "get_current_system":
            return get_current_system
        if name == "get_active_system":
            return get_current_system
        if name == "get_propagation_mode":
            return get_propagation_mode
        if name == "propagation_mode":
            return propagation_mode
        if name == "uncertainty_mode":
            return uncertainty_mode
        if name == "system_context":
            return use_system

    if name == "get_unit":

        def get_unit(unit_expression):
            from measurekit.application.context import get_current_system

            return get_current_system().get_unit(unit_expression)

        return get_unit

    if name in ("ConversionError", "MeasureKitError", "UnitNotFoundError"):
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

    if name in ("MeasureKitContext", "PruningConfig"):
        from measurekit.domain.measurement.vectorized_uncertainty import (
            MeasureKitContext,
            PruningConfig,
        )

        if name == "MeasureKitContext":
            return MeasureKitContext
        return PruningConfig

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


# Register Extensions only if libraries are already loaded
if "pandas" in sys.modules:
    try:
        from measurekit.ext import pandas_support
    except (ImportError, AttributeError):
        pass

if "numba" in sys.modules:
    try:
        import measurekit.ext.numba_support
    except (ImportError, AttributeError):
        pass

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
    "create_default_system",
    "create_system",
    "default_system",
    "get_active_system",
    "get_current_system",
    "get_unit",
    "jit",
    "load_state",
    "save_state",
    "system_context",
    "uncertainty_mode",
    "units",
]
