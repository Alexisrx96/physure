# physure/__init__.pyi
# Typing stub for the main entry point of Physure.

from collections.abc import Callable
from contextlib import AbstractContextManager
from pathlib import Path
from typing import Any

from physure.application.factories import QuantityFactory
from physure.core.registry import UnitRegistry as UnitRegistry
from physure.domain.exceptions import (
    ConversionError as ConversionError,
)
from physure.domain.exceptions import (
    PhysureError as PhysureError,
)
from physure.domain.exceptions import (
    UnitNotFoundError as UnitNotFoundError,
)
from physure.domain.exceptions import (
    UnknownUnitError as UnknownUnitError,
)
from physure.domain.measurement.equivalencies import (
    equivalencies as equivalencies,
)
from physure.domain.measurement.equivalencies import (
    spectral as spectral,
)
from physure.domain.measurement.equivalencies import (
    thermodynamic as thermodynamic,
)
from physure.domain.measurement.quantity import Quantity as Quantity
from physure.domain.measurement.system import UnitSystem as UnitSystem
from physure.domain.measurement.uncertainty import (
    Uncertainty as Uncertainty,
)
from physure.domain.measurement.units import CompoundUnit as CompoundUnit
from physure.domain.measurement.vectorized_uncertainty import (
    PhysureContext as PhysureContext,
)
from physure.domain.measurement.vectorized_uncertainty import (
    PruningConfig as PruningConfig,
)

Q_: QuantityFactory
default_system: UnitSystem
units: UnitRegistry

def create_default_system() -> UnitSystem: ...
def create_system(config_path_or_name: str) -> UnitSystem: ...
def get_active_system() -> UnitSystem: ...
def get_current_system() -> UnitSystem: ...
def get_unit(unit_expression: str | CompoundUnit) -> CompoundUnit: ...
def jit(func: Callable[..., Any]) -> Callable[..., Any]: ...
def load_state(filepath: str | Path) -> None: ...
def save_state(filepath: str | Path, protocol: int = ...) -> None: ...
def system_context(
    system_name_or_obj: str | UnitSystem,
) -> AbstractContextManager[None]: ...
def uncertainty_mode(
    mode: str, **kwargs: Any
) -> AbstractContextManager[None]: ...

# For the configuration proxy
class _ConfigProxy:
    @property
    def propagation_mode(
        self,
    ) -> Callable[[str], AbstractContextManager[None]]: ...
    def set_propagation_mode(
        self, mode: str
    ) -> AbstractContextManager[None]: ...

config: _ConfigProxy

__all__ = [
    "Q_",
    "CompoundUnit",
    "ConversionError",
    "PhysureContext",
    "PhysureError",
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
