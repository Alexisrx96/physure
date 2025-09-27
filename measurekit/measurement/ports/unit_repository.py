# measurekit/measurement/ports/unit_repository.py

from abc import ABC, abstractmethod
from typing import Optional

from measurekit.measurement.conversions import UnitDefinition


class IUnitRepository(ABC):
    """
    An interface (Port) for retrieving unit definitions.
    """

    @abstractmethod
    def get_definition(self, unit_symbol: str) -> Optional[UnitDefinition]:
        """
        Retrieves the definition for a given unit symbol.
        """
        pass
