# measurekit/__init__.py (Refactored)

from measurekit.startup import create_default_system
from measurekit.system import UnitSystem
from measurekit.exceptions import (
    ConversionError,
    MeasureKitError,
    UnitNotFoundError,
)
from measurekit.measurement.api import Q_
from measurekit.measurement.quantity import Quantity
from measurekit.measurement.uncertainty import Uncertainty
from measurekit.measurement.units import CompoundUnit, get_unit

# Create a single, default instance of the unit system for general use.
# Advanced users can create their own UnitSystem() instances if needed.
default_system: UnitSystem = create_default_system()

# The primary API object `Q_` is now implicitly bound to the default system.
# A deeper refactor might involve Q_(system=default_system), but for now,
# the global functions it relies on will be configured by `create_default_system`.

__all__ = [
    "Q_",
    "Quantity",
    "get_unit",
    "CompoundUnit",
    "Uncertainty",
    "MeasureKitError",
    "ConversionError",
    "UnitNotFoundError",
    "default_system",  # Exposing the default system can be useful for users
]

__version__ = "0.0.1"
