# measurekit/domain/measurement/uncertainty.py
"""This module defines classes for handling measurement uncertainty."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Generic, TypeVar, cast

import numpy as np
from numpy.typing import NDArray

UncType = TypeVar("UncType", float, NDArray[Any])

Numeric = int | float | np.ndarray


@dataclass(frozen=True, slots=True)
class Uncertainty(Generic[UncType]):
    """Represents the uncertainty of a quantity with lineage tracking.

    Supports correlated error propagation by tracking the linear coefficients
    of independent error sources (lineage).
    """

    std_dev: UncType
    lineage: dict[str, UncType] = field(default_factory=dict)
    vector_slice: slice | None = None

    def __post_init__(self):
        """Validates the data after initialization."""
        if self.vector_slice is None:
            if np.any(np.asarray(self.std_dev) < 0):
                raise ValueError("Standard deviation cannot be negative.")

        if (
            isinstance(self.std_dev, np.ndarray)
            and self.std_dev.flags.writeable
        ):
            self.std_dev.flags.writeable = False

    def __repr__(self) -> str:
        """Readable representation of the uncertainty."""
        if self.vector_slice:
            return f"Uncertainty(vector_slice={self.vector_slice})"
        return f"Uncertainty(std_dev={self.std_dev})"

    def __hash__(self) -> int:
        """Returns a hash for the uncertainty object."""
        if self.vector_slice:
            return hash(self.vector_slice)
        std_dev_hashable = (
            tuple(self.std_dev.tolist())
            if isinstance(self.std_dev, np.ndarray)
            else self.std_dev
        )
        lineage_hashable = frozenset(self.lineage.items())
        return hash((std_dev_hashable, lineage_hashable))

    @classmethod
    def from_standard(
        cls, std_dev: UncType, measurement_id: str | None = None
    ) -> Uncertainty[UncType]:
        """Creates an uncertainty from a standard deviation.

        If std_dev is an array, it registers it with CovarianceStore.
        """
        if isinstance(std_dev, np.ndarray):
            from measurekit.domain.measurement.vectorized_uncertainty import (
                CovarianceStore,
            )

            store = CovarianceStore()
            slc = store.register_independent_array(std_dev)
            return cls(std_dev=std_dev, vector_slice=slc)

        import uuid

        uid = measurement_id or str(uuid.uuid4())
        lineage = {uid: std_dev} if np.any(np.asarray(std_dev) > 0) else {}
        return cls(std_dev=std_dev, lineage=lineage)

    def ensure_vector_slice(self) -> slice:
        """Returns the existing vector slice or registers if it's a scalar."""
        if self.vector_slice:
            return self.vector_slice

        from scipy import sparse

        from measurekit.domain.measurement.vectorized_uncertainty import (
            CovarianceStore,
        )

        store = CovarianceStore()
        val = np.asarray(self.std_dev)
        slc = store.allocate(1)
        diag_val = val.flatten() ** 2
        store.set_covariance_block(
            slc, slc, sparse.diags(diagonals=[diag_val], offsets=[0])
        )
        return slc

    def _compute_std_dev(self, lineage: dict[str, UncType]) -> UncType:
        """Computes total std_dev from lineage using sum of squares."""
        if not lineage:
            return cast(UncType, 0.0)

        squares = [np.asarray(v) ** 2 for v in lineage.values()]
        sum_sq = sum(squares)
        return cast(UncType, np.sqrt(sum_sq))

    def add(
        self, other: Uncertainty[UncType], scale: float = 1.0
    ) -> Uncertainty[UncType]:
        """Propagates uncertainty for addition/subtraction (correlated)."""
        new_lineage = self.lineage.copy()
        for uid, coeff in other.lineage.items():
            val = scale * coeff
            if uid in new_lineage:
                new_lineage[uid] = new_lineage[uid] + val
            else:
                new_lineage[uid] = val

        # Clean up zero terms to keep lineage lightweight
        filtered_lineage = {
            k: v for k, v in new_lineage.items() if np.any(np.asarray(v) != 0)
        }
        return Uncertainty(
            std_dev=self._compute_std_dev(filtered_lineage),
            lineage=filtered_lineage,
        )

    def __add__(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Alias for add()."""
        return self.add(other)

    def __sub__(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Propagates uncertainty for subtraction."""
        return self.add(other, scale=-1.0)

    def propagate_mul_div(
        self, other: Uncertainty[Any], val1: Any, val2: Any, result_value: Any
    ) -> Uncertainty[Any]:
        """Correlated propagation for multiplication or division."""
        if np.any(np.asarray(val1) == 0) and np.any(np.asarray(val2) == 0):
            if isinstance(result_value, np.ndarray):
                return Uncertainty(np.zeros_like(result_value))
            return Uncertainty(0.0)

        # Check if it's division by looking at result_value vs val1*val2
        is_division = False
        try:
            # result_value approx val1 / val2
            # Use a slightly more robust check
            v1 = np.asarray(val1)
            v2 = np.asarray(val2)
            rv = np.asarray(result_value)
            if np.all(v2 != 0) and np.allclose(rv, v1 / v2):
                is_division = True
        except (ValueError, TypeError):
            pass

        if is_division:
            # z = x/y => dz = (1/y)dx - (x/y^2)dy
            f_x = 1.0 / val2
            f_y = -val1 / (val2**2)
        else:
            # z = x*y => dz = y*dx + x*dy
            f_x = val2
            f_y = val1

        new_lineage = {}
        for uid, coeff in self.lineage.items():
            new_lineage[uid] = f_x * coeff

        for uid, coeff in other.lineage.items():
            val = f_y * coeff
            if uid in new_lineage:
                new_lineage[uid] = new_lineage[uid] + val
            else:
                new_lineage[uid] = val

        filtered_lineage = {
            k: v for k, v in new_lineage.items() if np.any(np.asarray(v) != 0)
        }
        return Uncertainty(
            std_dev=self._compute_std_dev(filtered_lineage),
            lineage=filtered_lineage,
        )

    def power(self, exponent: float, value: Any) -> Uncertainty[Any]:
        """Correlated propagation for power: z = x^n => dz = n * x^(n-1) * dx."""
        if np.any(np.asarray(value) == 0):
            return Uncertainty(0.0)

        deriv = exponent * (value ** (exponent - 1))
        new_lineage = {
            uid: deriv * coeff for uid, coeff in self.lineage.items()
        }
        filtered_lineage = {
            k: v for k, v in new_lineage.items() if np.any(np.asarray(v) != 0)
        }

        return Uncertainty(
            std_dev=self._compute_std_dev(filtered_lineage),
            lineage=filtered_lineage,
        )

    def scale(self, factor: float | NDArray[Any]) -> Uncertainty[UncType]:
        """Scales the uncertainty by a factor."""
        # Use abs(factor) for std_dev but original factor for lineage
        # Wait, $z = kx \implies dz = k dx$. So coefficients should be scaled by k.
        # std_dev will naturally be sqrt(sum((k c_i)^2)) = |k| sqrt(sum(c_i^2)).
        new_lineage = {
            uid: factor * coeff for uid, coeff in self.lineage.items()
        }
        return Uncertainty(
            std_dev=np.abs(factor) * self.std_dev, lineage=new_lineage
        )
