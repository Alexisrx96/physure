# measurekit/domain/measurement/uncertainty.py
"""This module defines classes for handling measurement uncertainty."""

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
    In the context of the measurement system, the uncertainty is used
    to propagate errors during arithmetic operations between quantities.

    Examples:
    --------
    >>> from measurekit import get_unit
    >>> from measurekit.domain.measurement.quantity import Quantity
    >>> from measurekit.domain.measurement.uncertainty import Uncertainty
    >>> u = get_unit("m")
    >>> q = Quantity.from_input(1.0, u, None, uncertainty=Uncertainty(0.1))
    >>> q.uncertainty
    0.1
    """

    __slots__ = ("std_dev",)

    std_dev: UncType

    def __post_init__(self):
        """Validates the data after initialization.

        Checks that the standard deviation is not negative and that it is
        immutable (if it is an array).

        Raises:
        ValueError: If the standard deviation is negative.
        """
        if np.any(np.asarray(self.std_dev) < 0):
            raise ValueError("Standard deviation cannot be negative.")

        if (
            isinstance(self.std_dev, np.ndarray)
            and self.std_dev.flags.writeable
        ):
            self.std_dev.flags.writeable = False

    def __repr__(self) -> str:
        """Readable representation of the uncertainty.

        Returns:
            str: A string formatted as "Uncertainty(std_dev=<value>)".
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
            The new uncertainty object for the result.
        """
        new_std_dev = (self.std_dev**2 + other.std_dev**2) ** 0.5
        return Uncertainty(new_std_dev)

    def __add__(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Alias for add()."""
        return self.add(other)

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
        """Calculates the uncertainty for multiplication or division.

        This method adds relative uncertainties in quadrature.
        """
        if np.any(np.asarray(val1) == 0) or np.any(np.asarray(val2) == 0):
            if isinstance(result_value, np.ndarray):
                return Uncertainty(np.zeros_like(result_value, dtype=float))
            return Uncertainty(0.0)

        rel_unc1_sq = (self.std_dev / np.abs(val1)) ** 2
        rel_unc2_sq = (other.std_dev / np.abs(val2)) ** 2

        new_std_dev = np.abs(result_value) * np.sqrt(rel_unc1_sq + rel_unc2_sq)
        return Uncertainty(new_std_dev)

    def power(self, exponent: float, value: UncType) -> Uncertainty[UncType]:
        """Calculates the uncertainty of a power operation.

        For z = x^n, the relative uncertainty is multiplied by the
        absolute value of the exponent.

        Formula: δz = |z| * |n| * (δx / |x|)
        """
        if np.any(np.asarray(value) == 0):
            if isinstance(value, np.ndarray):
                return Uncertainty(np.zeros_like(value))
            return Uncertainty(0.0)

        new_value = value**exponent
        rel_unc = self.std_dev / value

        new_std_dev = np.abs(new_value * exponent) * rel_unc
        return Uncertainty(new_std_dev)
