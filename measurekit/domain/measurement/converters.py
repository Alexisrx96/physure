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


@dataclass(frozen=True)
class LogarithmicConverter(UnitConverter):
    """For logarithmic units: y = factor * log10(x / reference).

    Specifically for Decibels (dB): dB = 10 * log10(P / P_ref) or 20 * log10(V / V_ref).
    We store the factor (10 or 20) and the reference value.
    """

    factor: float
    reference: float = 1.0

    def to_base(self, value: float) -> float:
        import numpy as np

        return self.reference * (10 ** (value / self.factor))

    def from_base(self, value: float) -> float:
        import numpy as np

        return self.factor * np.log10(value / self.reference)
