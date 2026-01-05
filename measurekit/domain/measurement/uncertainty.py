from __future__ import annotations

import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Generic, TypeVar, cast

from measurekit.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from numpy.typing import NDArray

    from measurekit.core.protocols import BackendOps

UncType = TypeVar("UncType")


class Uncertainty(ABC, Generic[UncType]):
    """Abstract base class for uncertainty models.

    Defines the interface for error propagation strategies.
    Implementations include Correlated (Covariance) and Uncorrelated.
    """

    @property
    @abstractmethod
    def std_dev(self) -> UncType:
        """Returns the standard deviation."""
        ...

    @abstractmethod
    def add(
        self,
        other: Uncertainty[UncType],
        jac_self: Any = 1.0,
        jac_other: Any = 1.0,
        out_magnitude: Any = None,
    ) -> Uncertainty[UncType]:
        """Propagates uncertainty for addition/subtraction."""
        ...

    def __add__(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Adds two uncertainty models."""
        return self.add(other)

    def __sub__(self, other: Uncertainty[UncType]) -> Uncertainty[UncType]:
        """Subtracts two uncertainty models."""
        return self.add(other, jac_other=-1.0)

    @abstractmethod
    def propagate_mul_div(
        self,
        other: Uncertainty[Any],
        val1: Any,
        val2: Any,
        result_value: Any,
        jac_self: Any = None,
        jac_other: Any = None,
    ) -> Uncertainty[Any]:
        """Propagates uncertainty for multiplication or division."""
        ...

    @abstractmethod
    def power(
        self, exponent: float, value: Any, jac: Any = None
    ) -> Uncertainty[Any]:
        """Propagates uncertainty for power."""
        ...

    @abstractmethod
    def scale(self, factor: float | NDArray[Any]) -> Uncertainty[UncType]:
        """Scales the uncertainty by a factor."""
        ...

    @classmethod
    def from_standard(
        cls, std_dev: UncType, measurement_id: str | None = None
    ) -> Uncertainty[UncType]:
        """Factory method to create the appropriate uncertainty model.

        Checks global context to select CovarianceModel or VarianceModel.
        """
        from measurekit.application.context import get_propagation_mode

        mode = get_propagation_mode()
        if mode == "uncorrelated":
            return VarianceModel.from_standard(std_dev)
        return CovarianceModel.from_standard(std_dev, measurement_id)


@dataclass(frozen=True, slots=True)
class VarianceModel(Uncertainty[UncType]):
    """Uncorrelated uncertainty model (Variance only).

    Stores the variance (std_dev^2) and performs element-wise operations.
    Space complexity: O(N).
    """

    variance: UncType

    @property
    def std_dev(self) -> UncType:
        """Returns the standard deviation."""
        backend = BackendManager.get_backend(self.variance)
        return cast("UncType", backend.sqrt(self.variance))

    @classmethod
    def from_standard(cls, std_dev: UncType) -> VarianceModel[UncType]:
        """Creates a VarianceModel from a standard deviation."""
        backend = BackendManager.get_backend(std_dev)
        # Handle zero variance safely
        var = backend.pow(std_dev, 2)
        return cls(variance=var)

    def _apply_jacobian(self, var: Any, jac: Any, backend: BackendOps) -> Any:
        """Applies a Jacobian to a variance vector: var_out = (J^2) @ var_in.

        For uncorrelated propagation, we only care about the variance mapping.
        If J is a matrix, this is a linear transformation of variances.
        """
        if jac is None:
            return var

        # Elements of Jacobian squared
        jac_sq = backend.pow(jac, 2)

        if not backend.is_array(var) and not backend.is_array(jac):
            # Scalar case
            return backend.mul(jac_sq, var)

        # Vector case
        # Ensure var is a flat vector for math if it's an array
        if backend.is_array(var):
            original_shape = backend.shape(var)
            size = backend.size(var)
            var_flat = backend.reshape(var, (size, 1))
        else:
            # Broadcast scalar to match Jacobian column count if possible
            # But sparse_matmul usually wants a vector.
            # If jac is (M, N), var should be (N, 1).
            # We don't know N easily from jac here without backend.shape.
            # Most cases where jac is matrix, var is already vector.
            # If not, let's try to convert to array first.
            var_flat = backend.reshape(backend.asarray(var), (1, 1))
            original_shape = ()

        if not backend.is_array(jac):
            # Scalar jacobian, vector variance
            return backend.mul(jac_sq, var)

        # Matrix jacobian, vector variance
        # result = J_sq @ var_flat
        try:
            res = backend.sparse_matmul(jac_sq, var_flat)
        except (ValueError, TypeError):
            # Fallback for scalar/matrix mismatch
            res = backend.mul(jac_sq, var)
            if hasattr(res, "diagonal"):
                res = res.diagonal()
            if hasattr(res, "todense"):
                res = res.todense()

        # Reshape back to match input or broadcasted output
        return (
            backend.reshape(res, original_shape)
            if original_shape != ()
            else backend.reshape(res, ())
        )

    def add(
        self,
        other: Uncertainty[UncType],
        jac_self: Any = 1.0,
        jac_other: Any = 1.0,
        out_magnitude: Any = None,
    ) -> VarianceModel[UncType]:
        """Adds two uncertainty models."""
        backend = BackendManager.get_backend(self.variance)

        # Ensure other has variance
        if isinstance(other, VarianceModel):
            other_var = other.variance
        else:
            other_var = backend.pow(other.std_dev, 2)

        v_self = self._apply_jacobian(self.variance, jac_self, backend)
        v_other = self._apply_jacobian(other_var, jac_other, backend)

        new_var = backend.add(v_self, v_other)
        return VarianceModel(new_var)

    def propagate_mul_div(
        self,
        other: Uncertainty[Any],
        val1: Any,
        val2: Any,
        result_value: Any,
        jac_self: Any = None,
        jac_other: Any = None,
    ) -> VarianceModel[Any]:
        """Propagates uncertainty for multiplication/division."""
        backend = BackendManager.get_backend(val1)

        if jac_self is None or jac_other is None:
            # Fallback calculation of Jacobians
            is_v2_zero = backend.all(backend.equal(val2, 0))
            is_division = False
            try:
                if not is_v2_zero:
                    quotient = backend.truediv(val1, val2)
                    if backend.allclose(result_value, quotient):
                        is_division = True
            except (ValueError, TypeError, AttributeError):
                pass

            if is_division:
                jac_self = backend.truediv(1.0, val2)
                denom = backend.pow(val2, 2)
                jac_other = backend.truediv(backend.mul(val1, -1.0), denom)
            else:
                jac_self = val2
                jac_other = val1

        return self.add(other, jac_self, jac_other, out_magnitude=result_value)

    def power(
        self, exponent: float, value: Any, jac: Any = None
    ) -> VarianceModel[Any]:
        """Propagates uncertainty for exponentiation."""
        backend = BackendManager.get_backend(value)
        if jac is None:
            term = backend.pow(value, exponent - 1)
            jac = backend.mul(term, exponent)

        new_var = self._apply_jacobian(self.variance, jac, backend)
        return VarianceModel(new_var)

    def scale(self, factor: float | NDArray[Any]) -> VarianceModel[UncType]:
        """Scales the uncertainty."""
        backend = BackendManager.get_backend(factor)
        new_var = backend.mul(self.variance, backend.pow(factor, 2))
        return VarianceModel(new_var)

    def __hash__(self) -> int:
        """Hash implementation for VarianceModel."""
        # Check if variance is an array
        from measurekit.core.dispatcher import BackendManager

        backend = BackendManager.get_backend(self.variance)
        if backend.is_array(self.variance):
            raise TypeError(
                "unhashable type: 'VarianceModel' with array variance"
            )
        return hash(self.variance)


@dataclass(frozen=True, slots=True)
class CovarianceModel(Uncertainty[UncType]):
    """Correlated uncertainty model (Lineage or Covariance Matrix).

    Tracks correlations using a lineage dictionary for scalars
    and a global CovarianceStore for vectors.
    """

    std_dev_internal: UncType
    lineage: dict[str, UncType] = field(default_factory=dict)
    vector_slice: slice | None = None

    @property
    def std_dev(self) -> UncType:
        """Returns the standard deviation."""
        return self.std_dev_internal

    @classmethod
    def from_standard(
        cls, std_dev: UncType, measurement_id: str | None = None
    ) -> CovarianceModel[UncType]:
        """Creates a CovarianceModel from a standard deviation."""
        backend = BackendManager.get_backend(std_dev)

        if backend.is_array(std_dev):
            from measurekit.domain.measurement.vectorized_uncertainty import (
                ensure_store,
            )

            store = ensure_store(backend)
            slc = store.register_independent_array(std_dev)
            return cls(std_dev_internal=std_dev, vector_slice=slc)

        uid = measurement_id or str(uuid.uuid4())
        is_pos = backend.greater(std_dev, 0)
        lineage = {uid: std_dev} if backend.any(is_pos) else {}

        return cls(std_dev_internal=std_dev, lineage=lineage)

    def __hash__(self) -> int:
        """Hash implementation for CovarianceModel."""
        # lineage is a dict, so we convert to items tuple
        lineage_items = tuple(sorted(self.lineage.items()))
        # Handle unhashable std_dev_internal (arrays)
        from measurekit.core.dispatcher import BackendManager

        backend = BackendManager.get_backend(self.std_dev_internal)
        if backend.is_array(self.std_dev_internal):
            raise TypeError(
                "unhashable type: 'CovarianceModel' with array std_dev"
            )

        return hash((self.std_dev_internal, lineage_items, self.vector_slice))

    def ensure_vector_slice(self, backend: BackendOps | None = None) -> slice:
        """Ensures the vector slice is allocated in the store."""
        if self.vector_slice:
            return self.vector_slice
        from measurekit.domain.measurement.vectorized_uncertainty import (
            ensure_store,
        )

        if backend is None:
            backend = BackendManager.get_backend(self.std_dev_internal)
        store = ensure_store(backend)
        return store.register_independent_array(self.std_dev_internal)

    def _compute_std_dev(
        self, lineage: dict[str, UncType], backend: BackendOps
    ) -> UncType:
        if not lineage:
            return cast("UncType", 0.0)
        values = list(lineage.values())
        squares = [backend.pow(v, 2) for v in values]
        sum_sq = squares[0]
        for s in squares[1:]:
            sum_sq = backend.add(sum_sq, s)
        return cast("UncType", backend.sqrt(sum_sq))

    def add(
        self,
        other: Uncertainty[UncType],
        jac_self: Any = 1.0,
        jac_other: Any = 1.0,
        out_magnitude: Any = None,
    ) -> Uncertainty[UncType]:
        """Adds two uncertainty models (correlated)."""
        backend = BackendManager.get_backend(self.std_dev_internal)

        # Vector Path
        if (
            out_magnitude is not None
            and backend.is_array(out_magnitude)
            and backend.size(out_magnitude) > 1
        ):
            from measurekit.domain.measurement.vectorized_uncertainty import (
                ensure_store,
            )

            store = ensure_store(backend)

            in_slices = []
            jacobians = []

            # Handle Self
            in_slices.append(self.ensure_vector_slice(backend))
            jacobians.append(jac_self)

            # Handle Other
            if isinstance(other, CovarianceModel):
                in_slices.append(other.ensure_vector_slice(backend))
                jacobians.append(jac_other)
            else:
                # Independent source for cross-model or simple inputs
                slc = store.register_independent_array(other.std_dev)
                in_slices.append(slc)
                jacobians.append(jac_other)

            out_size = backend.size(out_magnitude)
            out_slice = store.allocate(out_size)
            store.update_from_propagation(out_slice, in_slices, jacobians)

            out_cov = store.get_covariance_block(out_slice, out_slice)
            diag = backend.sparse_diagonal(out_cov)
            std_dev = backend.reshape(
                backend.sqrt(diag), backend.shape(out_magnitude)
            )

            return CovarianceModel(
                std_dev_internal=std_dev, vector_slice=out_slice
            )

        # Scalar Path (Lineage)
        other_lineage = (
            other.lineage
            if isinstance(other, CovarianceModel)
            else {str(uuid.uuid4()): other.std_dev}
        )

        new_lineage = {}
        for uid, coeff in self.lineage.items():
            new_lineage[uid] = backend.mul(coeff, jac_self)

        for uid, coeff in other_lineage.items():
            val = backend.mul(coeff, jac_other)
            if uid in new_lineage:
                new_lineage[uid] = backend.add(new_lineage[uid], val)
            else:
                new_lineage[uid] = val

        filtered_lineage = {
            k: v
            for k, v in new_lineage.items()
            if backend.any(backend.not_equal(v, 0))
        }
        new_std = self._compute_std_dev(filtered_lineage, backend)

        if (
            backend.is_array(out_magnitude)
            and backend.shape(out_magnitude) != ()
        ):
            new_std = backend.reshape(new_std, backend.shape(out_magnitude))

        return CovarianceModel(
            std_dev_internal=new_std, lineage=filtered_lineage
        )

    def propagate_mul_div(
        self,
        other: Uncertainty[Any],
        val1: Any,
        val2: Any,
        result_value: Any,
        jac_self: Any = None,
        jac_other: Any = None,
    ) -> CovarianceModel[Any]:
        """Propagates uncertainty for multiplication/division (correlated)."""
        backend = BackendManager.get_backend(val1)
        if jac_self is None or jac_other is None:
            is_v1_zero = backend.all(backend.equal(val1, 0))
            is_v2_zero = backend.all(backend.equal(val2, 0))
            if is_v1_zero and is_v2_zero:
                return CovarianceModel(backend.mul(result_value, 0))

            is_division = False
            try:
                if not is_v2_zero:
                    quotient = backend.truediv(val1, val2)
                    if backend.allclose(result_value, quotient):
                        is_division = True
            except (ValueError, TypeError, AttributeError):
                pass

            if is_division:
                jac_self = backend.truediv(1.0, val2)
                denom = backend.pow(val2, 2)
                jac_other = backend.truediv(backend.mul(val1, -1.0), denom)
            else:
                jac_self = val2
                jac_other = val1

        return cast(
            "CovarianceModel",
            self.add(other, jac_self, jac_other, out_magnitude=result_value),
        )

    def power(
        self, exponent: float, value: Any, jac: Any = None
    ) -> CovarianceModel[Any]:
        """Propagates uncertainty for exponentiation (correlated)."""
        backend = BackendManager.get_backend(value)
        if jac is None:
            term = backend.pow(value, exponent - 1)
            jac = backend.mul(term, exponent)

        new_lineage = {
            uid: backend.mul(coeff, jac) for uid, coeff in self.lineage.items()
        }
        filtered = {
            k: v
            for k, v in new_lineage.items()
            if backend.any(backend.not_equal(v, 0))
        }
        new_std = self._compute_std_dev(filtered, backend)

        return CovarianceModel(std_dev_internal=new_std, lineage=filtered)

    def scale(self, factor: float | NDArray[Any]) -> CovarianceModel[UncType]:
        """Scales the uncertainty (correlated)."""
        backend = BackendManager.get_backend(factor)
        new_lineage = {
            uid: backend.mul(coeff, factor)
            for uid, coeff in self.lineage.items()
        }
        new_std = backend.mul(self.std_dev_internal, backend.abs(factor))
        return CovarianceModel(std_dev_internal=new_std, lineage=new_lineage)
