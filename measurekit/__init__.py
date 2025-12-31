"""MeasureKit: A Python Library for Unit-Aware Scientific Calculations.

This library provides a comprehensive framework for performing calculations
with physical quantities, ensuring dimensional consistency and providing a
robust system for unit conversions. It is designed to be intuitive and easy to
use, allowing developers to focus on the logic of their calculations without
worrying about the intricacies of unit management.
"""

from measurekit.application.context import _set_global_default_system
from measurekit.application.factories import QuantityFactory
from measurekit.application.startup import create_default_system
from measurekit.domain.measurement.system import UnitSystem

# --- Application Assembly ---
# 1. Create the concrete adapter instance. This is our main application object.
default_system: UnitSystem = create_default_system(
    True
)  # <- This is the long-running call

_set_global_default_system(default_system)
u = default_system

# breakpoint()
# 2. Expose the primary factory method (Inbound Port) from our configured
# system.
#    This binds the `Q_` factory to our fully configured `default_system`.
Q_ = QuantityFactory()


# 3. Expose the `get_unit` function from the configured system instance.
def get_unit(unit_expression):
    """Retrieve a unit by its expression from the active unit system."""
    return get_active_system().get_unit(unit_expression)


# --- Expose Core Domain Objects and Exceptions ---
from measurekit.application.context import get_active_system, system_context
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
    "get_unit",
    "Quantity",
    "CompoundUnit",
    "Uncertainty",
    "MeasureKitError",
    "ConversionError",
    "UnitNotFoundError",
    "default_system",
    "system_context",
    "get_active_system",
    "u",
]

__version__ = "0.0.3"

# Register Pandas Accessor if pandas is available
try:
    import pandas as pd  # noqa: F401

    from measurekit.ext import pandas_support  # noqa: F401
except ImportError:
    pass

try:
    import numba

    import measurekit.ext.numba_support
except ImportError:
    pass
