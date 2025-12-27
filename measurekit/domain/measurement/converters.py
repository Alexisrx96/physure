from abc import ABC, abstractmethod
from dataclasses import dataclass


class UnitConverter(ABC):
    """Clase base abstracta para cualquier conversión de unidades."""

    @abstractmethod
    def to_base(self, value: float) -> float:
        """Convierte valor de la unidad actual a la unidad base del sistema."""
        pass

    @abstractmethod
    def from_base(self, value: float) -> float:
        """Convierte valor de la unidad base a la unidad actual."""
        pass


@dataclass(frozen=True)
class LinearConverter(UnitConverter):
    """Para la mayoría de unidades: y = ax (ej: Metros a Kilómetros)."""

    scale: float

    def to_base(self, value: float) -> float:
        return value * self.scale

    def from_base(self, value: float) -> float:
        return value / self.scale


@dataclass(frozen=True)
class AffineConverter(UnitConverter):
    """Para unidades con desplazamiento: y = ax + b (ej: Celsius a Kelvin)."""

    scale: float
    offset: float

    def to_base(self, value: float) -> float:
        return (value * self.scale) + self.offset

    def from_base(self, value: float) -> float:
        return (value - self.offset) / self.scale
