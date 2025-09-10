# measurekit/measurement/uncertainty.py
"""Este módulo define las clases para el manejo de la incertidumbre."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Generic, TypeVar, overload

import numpy as np
from numpy.typing import NDArray

UncType = TypeVar("UncType", float, NDArray[Any])

Numeric = int | float | np.ndarray


@dataclass(frozen=True)
class Uncertainty(Generic[UncType]):
    """Representa la incertidumbre de una cantidad.
    Es una clase inmutable y optimizada con __slots__.
    """

    __slots__ = ("std_dev",)

    std_dev: UncType

    def __post_init__(self):
        """Valida los datos después de la inicialización."""
        if np.any(np.asarray(self.std_dev) < 0):
            raise ValueError("La desviación estándar no puede ser negativa.")

        if isinstance(self.std_dev, np.ndarray):
            object.__setattr__(self.std_dev, "flags", {"WRITEABLE": False})

    def __repr__(self) -> str:
        """Representación legible de la incertidumbre."""
        return f"Uncertainty(std_dev={self.std_dev})"

    def add(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Calcula la incertidumbre de una suma/resta.

        Para z = x ± y, las incertidumbres absolutas (δx, δy) se suman
        en cuadratura, asumiendo que los errores no están correlacionados.

        Fórmula: δz = sqrt( (δx)² + (δy)² )
        """
        new_std_dev = (self.std_dev**2 + other.std_dev**2) ** 0.5
        return Uncertainty(new_std_dev)

    # 1. Si CUALQUIER valor (`val1`, `val2`, `result_value`) o incertidumbre
    # (`self` o `other`) es un array, el resultado es un `Uncertainty[NDArray]`.
    @overload
    def propagate_mul_div(
        self: Uncertainty[NDArray[Any]],
        other: Any,
        val1: Any,
        val2: Any,
        result_value: Any,
    ) -> Uncertainty[NDArray[Any]]: ...
    @overload
    def propagate_mul_div(
        self,
        other: Uncertainty[NDArray[Any]],
        val1: Any,
        val2: Any,
        result_value: Any,
    ) -> Uncertainty[NDArray[Any]]: ...
    @overload
    def propagate_mul_div(
        self, other: Any, val1: NDArray[Any], val2: Any, result_value: Any
    ) -> Uncertainty[NDArray[Any]]: ...
    @overload
    def propagate_mul_div(
        self, other: Any, val1: Any, val2: NDArray[Any], result_value: Any
    ) -> Uncertainty[NDArray[Any]]: ...
    @overload
    def propagate_mul_div(
        self, other: Any, val1: Any, val2: Any, result_value: NDArray[Any]
    ) -> Uncertainty[NDArray[Any]]: ...

    # 2. Solo si TODOS los valores e incertidumbres son escalares,
    # el resultado es un `Uncertainty[float]`.
    @overload
    def propagate_mul_div(
        self,
        other: Uncertainty[float],
        val1: float,
        val2: float,
        result_value: float,
    ) -> Uncertainty[float]: ...

    def propagate_mul_div(
        self, other: Any, val1: Any, val2: Any, result_value: Any
    ) -> Any:
        """Calcula la incertidumbre de una multiplicación/división."""
        # La implementación no necesita cambiar, su lógica ya es correcta.
        if np.any(np.asarray(val1) == 0) or np.any(np.asarray(val2) == 0):
            if isinstance(result_value, np.ndarray):
                return Uncertainty(np.zeros_like(result_value, dtype=float))
            return Uncertainty(0.0)

        rel_unc1_sq = (self.std_dev / np.abs(val1)) ** 2
        rel_unc2_sq = (other.std_dev / np.abs(val2)) ** 2

        new_std_dev = np.abs(result_value) * np.sqrt(rel_unc1_sq + rel_unc2_sq)
        return Uncertainty(new_std_dev)

    def power(self, exponent: float, value: UncType) -> Uncertainty[UncType]:
        """Calcula la incertidumbre de una potencia.

        Para z = x^n, la incertidumbre relativa se multiplica por el
        valor absoluto del exponente.

        Fórmula: (δz / |z|) = |n| * (δx / |x|)
        Despejando δz: δz = |z| * |n| * (δx / |x|)
        """
        if np.any(np.asarray(value) == 0):
            if isinstance(value, np.ndarray):
                return Uncertainty(np.zeros_like(value))
            return Uncertainty(0.0)

        new_value = value**exponent
        rel_unc = self.std_dev / value

        new_std_dev = np.abs(new_value * exponent) * rel_unc
        return Uncertainty(new_std_dev)
