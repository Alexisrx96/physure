"""Defines the abstract interface (Port) for a unit repository.

Following the principles of Ports and Adapters (Hexagonal Architecture), this
module defines the `IUnitRepository` interface. It specifies the contract that
any unit storage system must adhere to, decoupling the core application logic
from the concrete implementation of how units are stored and retrieved.
"""

from abc import ABC, abstractmethod

from measurekit.domain.measurement.conversions import UnitDefinition


class IUnitRepository(ABC):
    """An interface (Port) for retrieving unit definitions."""

    @abstractmethod
    def get_definition(self, unit_symbol: str) -> UnitDefinition | None:
        """Retrieves the definition for a given unit symbol."""
        pass
