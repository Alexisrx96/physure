"""MeasureKit: A Python Library for Unit-Aware Scientific Calculations.

This library provides a comprehensive framework for performing calculations
with physical quantities, ensuring dimensional consistency and providing a
robust system for unit conversions. It is designed to be intuitive and easy,
allowing developers to focus on calculation logic without worrying about
unit management intricacies.
"""

# --- Application Assembly ---
# The default system is now lazily loaded by context.get_current_system().
# We expose a proxy or simply rely on get_current_system().

from typing import TYPE_CHECKING, Any

from measurekit.application.context import (
    get_current_system,
    get_propagation_mode,
    propagation_mode,
    use_system,
)
from measurekit.application.factories import QuantityFactory
from measurekit.application.startup import (
    create_default_system,
    create_system,
)
from measurekit.domain.measurement.units import get_default_system, units

# Expose the primary factory method (Inbound Port)
# QuantityFactory uses get_default_system() internally if no system provided.
Q_ = QuantityFactory()


# Configuration Access
class ConfigProxy:
    @property
    def propagation_mode(self):
        return propagation_mode

    def set_propagation_mode(self, mode: str):
        # This could set a global default if needed, but for now we use context
        # Or we could have a setter that updates the ContextVar's default.
        return propagation_mode(mode)


config = ConfigProxy()


# 3. Expose the `get_unit` function from the configured system instance.
def get_unit(unit_expression):
    """Retrieve a unit by its expression from the active unit system."""
    return get_current_system().get_unit(unit_expression)


# --- Expose Core Domain Objects and Exceptions ---
# IMPORTANT: get_active_system is an alias for get_current_system
from measurekit.application.context import get_active_system
from measurekit.domain.exceptions import (
    ConversionError,
    MeasureKitError,
    UnitNotFoundError,
)
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.uncertainty import Uncertainty
from measurekit.domain.measurement.units import CompoundUnit

__all__ = [
    "Q_",
    "CompoundUnit",
    "ConversionError",
    "MeasureKitError",
    "Quantity",
    "Uncertainty",
    "UnitNotFoundError",
    "create_default_system",
    "create_system",
    "default_system",
    "get_active_system",
    "get_current_system",
    "get_unit",
    "units",
    "use_system",
]

__version__ = "0.0.3"


def __getattr__(name: str) -> Any:
    """Implement lazy loading for default_system."""
    if name == "default_system":
        return get_default_system()
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


# Register Pandas Accessor if pandas is available
try:
    import pandas as pd

    from measurekit.ext import pandas_support
except (ImportError, AttributeError):
    pass

try:
    import numba

    import measurekit.ext.numba_support
except (ImportError, AttributeError):
    pass
