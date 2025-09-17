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
    """Represents the uncertainty of a quantity.

    The uncertainty is a measure of the precision of a quantity,
    typically represented by a standard deviation.

    Parameters:
    ----------
    std_dev : UncType
        The standard deviation of the uncertainty.

    Notes:
    -----
    The uncertainty is typically represented by a standard deviation.
    In the context of the measurement system, the uncertainty is used
    to propagate the uncertainty of a quantity to other quantities.

    Examples:
    --------
    >>> from measurekit import units
    >>> from measurekit.measurement import Quantity
    >>> from measurekit.measurement.uncertainty import Uncertainty
    >>> q = Quantity(1.0, units.meter, uncertainty=Uncertainty(0.1))
    >>> q.uncertainty
    Uncertainty(std_dev=0.1)

    """

    __slots__ = ("std_dev",)

    std_dev: UncType

    def __post_init__(self):
        """Validates the data after initialization.

        Checks that the standard deviation is not negative and that it is
        immutable.

        Raises:
        ValueError: If the standard deviation is negative.
        """
        if np.any(np.asarray(self.std_dev) < 0):
            raise ValueError("La desviación estándar no puede ser negativa.")

        if (
            isinstance(self.std_dev, np.ndarray)
            and self.std_dev.flags.writeable
        ):
            self.std_dev.flags.writeable = False

    def __repr__(self) -> str:
        """Readable representation of the uncertainty.

        Returns a string with the standard deviation of the uncertainty.

        Notes:
        -----
        The string is formatted as "Uncertainty(std_dev=<std_dev>)" where
        <std_dev> is the standard deviation of the uncertainty.

        Examples:
        --------
        >>> uncertainty = Uncertainty(0.1)
        >>> repr(uncertainty)
        'Uncertainty(std_dev=0.1)'
        """
        return f"Uncertainty(std_dev={self.std_dev})"

    def add(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Calculate the uncertainty of a sum or difference.

        For z = x ± y, the absolute uncertainties (δx, δy) are added in
        quadrature, assuming that the errors are not correlated.

        Formula: δz = sqrt( (δx)² + (δy)² )

        Parameters:
        ----------
        other : Uncertainty[UncType]
            The uncertainty of the other value.

        Returns:
        -------
        Uncertainty[UncType]
            The uncertainty of the result.
        """
        new_std_dev = (self.std_dev**2 + other.std_dev**2) ** 0.5
        return Uncertainty(new_std_dev)

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
