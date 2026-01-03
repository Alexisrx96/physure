from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Generic, TypeVar, cast

from measurekit.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from numpy.typing import NDArray

UncType = TypeVar(
    "UncType"
)  # Removed constraints to avoid numpy dependency in definition

Numeric = Any  # Simplified


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
        # Use backend to check for negative std_dev
        backend = BackendManager.get_backend(self.std_dev)

        # Skip validation if tracing (e.g. JAX JIT/VMAP) as it can't handle concrete checks
        if self.vector_slice is None and not backend.is_tracing(self.std_dev):
            # Check if any element is negative
            try:
                is_neg = backend.less(self.std_dev, 0)
                if backend.any(is_neg):
                    raise ValueError("Standard deviation cannot be negative.")
            except (TypeError, ValueError):
                # If comparison fails (e.g. undetected JAX Tracer), we skip validation
                pass

        # Writeability check is specific to numpy arrays
        # We can try/except or check is_array
        if backend.is_array(self.std_dev):
            # If backend is numpy, it might have flags
            if hasattr(self.std_dev, "flags") and self.std_dev.flags.writeable:
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

        backend = BackendManager.get_backend(self.std_dev)
        if backend.is_array(self.std_dev):
            # Try to convert to list if possible (e.g. numpy)
            try:
                std_dev_hashable = tuple(self.std_dev.tolist())
            except AttributeError:
                # Fallback
                std_dev_hashable = str(self.std_dev)
        else:
            std_dev_hashable = self.std_dev

        lineage_hashable = frozenset(self.lineage.items())
        return hash((std_dev_hashable, lineage_hashable))

    @classmethod
    def from_standard(
        cls, std_dev: UncType, measurement_id: str | None = None
    ) -> Uncertainty[UncType]:
        """Creates an uncertainty from a standard deviation.

        If std_dev is an array, it registers it with CovarianceStore.
        """
        backend = BackendManager.get_backend(std_dev)

        if backend.is_array(std_dev):
            from measurekit.domain.measurement.vectorized_uncertainty import (
                ensure_store,
            )

            store = ensure_store(backend)
            slc = store.register_independent_array(std_dev)
            return cls(std_dev=std_dev, vector_slice=slc)

        import uuid

        uid = measurement_id or str(uuid.uuid4())
        # Check for non-zero (backend aware)
        is_pos = backend.greater(std_dev, 0)
        if backend.any(is_pos):
            lineage = {uid: std_dev}
        else:
            lineage = {}

        return cls(std_dev=std_dev, lineage=lineage)

    def ensure_vector_slice(self) -> slice:
        """Returns the existing vector slice or registers if it's a scalar."""
        if self.vector_slice:
            return self.vector_slice

        # Lazy import because CovarianceStore depends on numpy
        from measurekit.domain.measurement.vectorized_uncertainty import (
            ensure_store,
        )

        backend = BackendManager.get_backend(self.std_dev)
        store = ensure_store(backend)
        slc = store.register_independent_array(self.std_dev)
        return slc

    def _compute_std_dev(self, lineage: dict[str, UncType]) -> UncType:
        """Computes total std_dev from lineage using sum of squares."""
        if not lineage:
            return cast("UncType", 0.0)

        # We need a backend. Pick one from the first value.
        values = list(lineage.values())
        backend = BackendManager.get_backend(values[0])

        squares = [backend.pow(v, 2) for v in values]

        # Sum squares
        sum_sq = squares[0]
        for s in squares[1:]:
            sum_sq = backend.add(sum_sq, s)

        return cast("UncType", backend.sqrt(sum_sq))

    def add(
        self, other: Uncertainty[UncType], scale: float = 1.0
    ) -> Uncertainty[UncType]:
        """Propagates uncertainty for addition/subtraction (correlated)."""
        new_lineage = self.lineage.copy()
        backend = BackendManager.get_backend(self.std_dev)

        for uid, coeff in other.lineage.items():
            # Get backend for the other coefficient
            bk_coeff = BackendManager.get_backend(coeff)
            val = bk_coeff.mul(coeff, scale)

            if uid in new_lineage:
                current = new_lineage[uid]
                bk_curr = BackendManager.get_backend(current)
                new_lineage[uid] = bk_curr.add(current, val)
            else:
                new_lineage[uid] = val

        # Clean up zero terms and build filtered lineage
        filtered_lineage = {
            k: v
            for k, v in new_lineage.items()
            if (bk := BackendManager.get_backend(v)).any(bk.not_equal(v, 0))
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
        backend = BackendManager.get_backend(val1)

        # Check for zero
        # if val1 == 0 and val2 == 0 ...
        # Use backend logic
        is_v1_zero = backend.all(backend.equal(val1, 0))
        is_v2_zero = backend.all(backend.equal(val2, 0))

        if is_v1_zero and is_v2_zero:
            if backend.is_array(result_value):
                # Create zeros like result_value
                # shape = backend.shape(result_value)
                # zeros = backend.zeros(shape) # We don't have zeros in protocol yet?
                # fallback: mul(result_value, 0)
                return Uncertainty(backend.mul(result_value, 0))
            return Uncertainty(0.0)

        # Check if it's division by looking at result_value vs val1*val2
        is_division = False
        try:
            # result_value approx val1 / val2
            # Use a slightly more robust check
            # backend.allclose?
            if backend.all(backend.not_equal(val2, 0)):
                quotient = backend.truediv(val1, val2)
                if backend.allclose(result_value, quotient):
                    is_division = True
        except (ValueError, TypeError, AttributeError):
            pass

        if is_division:
            # z = x/y => dz = (1/y)dx - (x/y^2)dy
            # f_x = 1.0 / val2
            f_x = backend.truediv(1.0, val2)

            # f_y = -val1 / (val2**2)
            denom = backend.pow(val2, 2)
            num = backend.mul(val1, -1.0)
            f_y = backend.truediv(num, denom)
        else:
            # z = x*y => dz = y*dx + x*dy
            f_x = val2
            f_y = val1

        new_lineage = {}
        for uid, coeff in self.lineage.items():
            # new_coeff = f_x * coeff
            bk = BackendManager.get_backend(coeff)
            new_lineage[uid] = bk.mul(coeff, f_x)

        for uid, coeff in other.lineage.items():
            # val = f_y * coeff
            bk = BackendManager.get_backend(coeff)
            val = bk.mul(coeff, f_y)

            if uid in new_lineage:
                curr = new_lineage[uid]
                bk_curr = BackendManager.get_backend(curr)
                new_lineage[uid] = bk_curr.add(curr, val)
            else:
                new_lineage[uid] = val

        filtered_lineage = {}
        for k, v in new_lineage.items():
            bk = BackendManager.get_backend(v)
            if bk.any(bk.not_equal(v, 0)):
                filtered_lineage[k] = v

        return Uncertainty(
            std_dev=self._compute_std_dev(filtered_lineage),
            lineage=filtered_lineage,
        )

    def power(self, exponent: float, value: Any) -> Uncertainty[Any]:
        """Correlated propagation for power: z = x^n => dz = n * x^(n-1) * dx."""
        backend = BackendManager.get_backend(value)
        if backend.any(backend.equal(value, 0)):
            return Uncertainty(0.0)

        # deriv = exponent * (value ** (exponent - 1))
        term = backend.pow(value, exponent - 1)
        deriv = backend.mul(term, exponent)

        new_lineage = {}
        for uid, coeff in self.lineage.items():
            bk = BackendManager.get_backend(coeff)
            new_lineage[uid] = bk.mul(coeff, deriv)

        filtered_lineage = {}
        for k, v in new_lineage.items():
            bk = BackendManager.get_backend(v)
            if bk.any(bk.not_equal(v, 0)):
                filtered_lineage[k] = v

        return Uncertainty(
            std_dev=self._compute_std_dev(filtered_lineage),
            lineage=filtered_lineage,
        )

    def scale(self, factor: float | NDArray[Any]) -> Uncertainty[UncType]:
        """Scales the uncertainty by a factor."""
        # Use abs(factor) for std_dev but original factor for lineage
        backend = BackendManager.get_backend(factor)

        new_lineage = {}
        for uid, coeff in self.lineage.items():
            bk = BackendManager.get_backend(coeff)
            new_lineage[uid] = bk.mul(coeff, factor)

        bk_std = BackendManager.get_backend(self.std_dev)
        abs_factor = backend.abs(factor)
        new_std = bk_std.mul(self.std_dev, abs_factor)

        return Uncertainty(std_dev=new_std, lineage=new_lineage)
