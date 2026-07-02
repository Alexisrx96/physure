from __future__ import annotations

import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import (
    TYPE_CHECKING,
    Any,
    Generic,
    TypeVar,
    cast,
)

from measurekit.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

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
    ) -> Uncertainty[UncType] | None:
        """Factory method to create the appropriate uncertainty model.

        Checks global context to select CovarianceModel or VarianceModel.
        """
        # JAX structure validation uses object() sentinels
        if type(std_dev) is object:
            # Return a valid model structure without computation
            return VarianceModel(variance=std_dev)

        # Auto-detect requirement for CovarianceModel
        backend = BackendManager.get_backend(std_dev)

        if backend.is_array(std_dev) and backend.size(std_dev) > 1:
            # Check if all elements are zero to avoid unnecessary store registration
            mask = backend.not_equal(std_dev, 0)
            if not backend.any(mask):
                return VarianceModel(variance=std_dev)
            return CovarianceModel.from_standard(std_dev, measurement_id)

        # Scalar Context-aware dispatch
        from measurekit.domain.measurement.vectorized_uncertainty import (
            get_current_store,
        )

        if get_current_store() is not None:
            return CovarianceModel.from_standard(std_dev, measurement_id)

        # For simple scalars with no active store, return None to keep Quantity "simple"
        if backend.is_array(std_dev) or backend.any(
            backend.not_equal(std_dev, 0)
        ):
            return VarianceModel.from_standard(std_dev)
        return None

    @classmethod
    def propagate(
        cls,
        func: Callable[..., Any],
        values: Sequence[Any],
        uncertainties: Sequence[Uncertainty[Any]],
    ) -> tuple[Any, Uncertainty[Any]]:
        """Generic Autograd-driven propagation.

        Returns:
            (result_value, new_uncertainty_model)
        """
        from measurekit.core.autograd import AutogradPropagator

        # 1. Compute Jacobians
        # We assume func(*values) -> result
        # AutogradPropagator returns (result, (J1, J2, ...))
        result, jacs = AutogradPropagator.compute_jacobians(func, values)

        # 2. Dispatch based on model type
        # We check the first uncertainty to decide strategy
        if not uncertainties:
            # If no uncertainties, usually zero uncertainty model
            return result, cls.from_standard(0.0)

        # If any is CovarianceModel, we use Covariance logic
        if any(isinstance(u, CovarianceModel) for u in uncertainties):
            new_unc = CovarianceModel._propagate_from_jacobians(
                jacs, uncertainties, values, result
            )
        else:
            # Else VarianceModel
            new_unc = VarianceModel._propagate_from_jacobians(
                jacs, uncertainties, values, result
            )

        return result, new_unc


@dataclass(frozen=True, slots=True)
class VarianceModel(Uncertainty[UncType]):
    """Uncorrelated uncertainty model (Variance only).

    Stores the variance (std_dev^2) and performs element-wise operations.
    Space complexity: O(N).
    """

    variance: UncType
    vector_slice: slice | None = None

    @property
    def std_dev(self) -> UncType:
        """Returns the standard deviation."""
        # Handle JAX sentinel objects during tracing/structural validation
        if type(self.variance) is object:
            return self.variance

        backend = BackendManager.get_backend(self.variance)
        try:
            return cast("UncType", backend.sqrt(self.variance))
        except (TypeError, AttributeError):
            # Fallback for types that backend doesn't support (e.g., SymPy Zero)
            return self.variance**0.5

    @classmethod
    def from_standard(cls, std_dev: UncType) -> VarianceModel[UncType]:
        """Creates a VarianceModel from a standard deviation."""
        backend = BackendManager.get_backend(std_dev)
        # Handle zero variance safely
        var = backend.pow(std_dev, 2)
        return cls(variance=var)

    @classmethod
    def _propagate_from_jacobians(
        cls,
        jacs: tuple[Any, ...],
        uncertainties: Sequence[Uncertainty],
        values: Sequence[Any],
        result: Any,
    ) -> VarianceModel:
        """Internal propagation using pre-computed Jacobians."""
        backend = BackendManager.get_backend(values[0])

        total_var = 0.0

        for i, (u, jac) in enumerate(zip(uncertainties, jacs, strict=False)):
            if u is None:
                continue  # Treat as zero uncertainty?

            # Get variance
            var_i = (
                u.variance
                if isinstance(u, VarianceModel)
                else backend.pow(u.std_dev, 2)
            )

            # Apply J^2
            # Use _apply_jacobian helper logic for consistency?
            # But that is instance method. Let's make it static or assume logic here.

            jac_sq = backend.pow(jac, 2)

            # Simple element-wise multiplication for VarianceModel (uncorrelated)
            # Assuming broadcast compatibility between J and Var
            term = backend.mul(jac_sq, var_i)

            total_var = term if i == 0 else backend.add(total_var, term)

        return cls(variance=total_var)

    def _matmul_jac_sq_var(
        self, jac_sq: Any, var_flat: Any, var: Any, backend: BackendOps
    ) -> Any:
        """Multiplies squared Jacobian by variance, with a scalar fallback."""
        try:
            return backend.sparse_matmul(jac_sq, var_flat)
        except (ValueError, TypeError):
            res = backend.mul(jac_sq, var)
            if hasattr(res, "diagonal"):
                res = res.diagonal()
            if hasattr(res, "todense"):
                res = res.todense()
            return res

    def _apply_jacobian(self, var: Any, jac: Any, backend: BackendOps) -> Any:
        """Applies a Jacobian to a variance vector: var_out = (J^2) @ var_in.

        For uncorrelated propagation, we only care about the variance mapping.
        If J is a matrix, this is a linear transformation of variances.
        """
        if jac is None:
            return var

        jac_sq = backend.pow(jac, 2)

        both_scalar = not backend.is_array(var) and not backend.is_array(jac)
        if both_scalar:
            return backend.mul(jac_sq, var)

        if backend.is_array(var):
            original_shape = backend.shape(var)
            var_flat = backend.reshape(var, (backend.size(var), 1))
        else:
            var_flat = backend.reshape(backend.asarray(var), (1, 1))
            original_shape = ()

        jac_is_scalar = not backend.is_array(jac) or backend.size(jac) == 1
        if jac_is_scalar:
            return backend.mul(jac_sq, var)

        res = self._matmul_jac_sq_var(jac_sq, var_flat, var, backend)

        import math

        orig_size = math.prod(original_shape) if original_shape else 1
        shape_changed = backend.size(res) != orig_size
        if shape_changed:
            res_shape = backend.shape(res)
            is_col_vector = len(res_shape) == 2 and res_shape[1] == 1
            if is_col_vector:
                return backend.reshape(res, (res_shape[0],))
            return res

        return backend.reshape(res, original_shape)

    def add(
        self,
        other: Uncertainty[UncType],
        jac_self: Any = 1.0,
        jac_other: Any = 1.0,
        out_magnitude: Any = None,
    ) -> VarianceModel[UncType]:
        """Adds two uncertainty models."""
        backend = BackendManager.get_backend(self.variance)

        v_self = self._apply_jacobian(self.variance, jac_self, backend)

        if other is None:
            return VarianceModel(v_self)

        # Ensure other has variance
        if isinstance(other, VarianceModel):
            other_var = other.variance
        else:
            # Handle numeric or Uncertainty other
            std = getattr(other, "std_dev", other)
            other_var = backend.pow(std, 2)

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
        """Propagates uncertainty for multiplication/division.

        Callers must pass explicit Jacobians; the operation cannot be
        reliably inferred from the values (comparing result_value to
        val1/val2 misclassifies mul as div when val2 is close to +/-1).
        """
        if jac_self is None or jac_other is None:
            raise ValueError(
                "propagate_mul_div requires explicit jac_self and jac_other."
            )

        return self.add(other, jac_self, jac_other, out_magnitude=result_value)

    def power(
        self, exponent: float, value: Any = None, jac: Any = None
    ) -> VarianceModel[Any]:
        """Propagates uncertainty for exponentiation."""
        backend = BackendManager.get_backend(self.variance)
        if jac is None:
            if value is None:
                raise ValueError(
                    "Either value or jac must be provided to power()"
                )
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

    @classmethod
    def _vector_propagation_path(
        cls,
        jacs: tuple[Any, ...],
        uncertainties: Sequence[Uncertainty],
        result: Any,
        backend: Any,
    ) -> CovarianceModel:
        """Propagates via covariance store (vector/array path)."""
        from measurekit.domain.measurement.vectorized_uncertainty import (
            ensure_store,
        )

        store = ensure_store(backend)
        in_slices = []
        final_jacs = []

        for u, jac in zip(uncertainties, jacs, strict=False):
            if u is None:
                continue
            if isinstance(u, CovarianceModel):
                in_slices.append(u.ensure_vector_slice(backend))
            else:
                slc = store.register_independent_array(u.std_dev)
                in_slices.append(slc)
            final_jacs.append(jac)

        out_slice = store.allocate(backend.size(result))
        store.update_from_propagation(out_slice, in_slices, final_jacs)

        out_cov = store.get_covariance_block(out_slice, out_slice)
        diag = backend.sparse_diagonal(out_cov)
        std_dev = backend.reshape(backend.sqrt(diag), backend.shape(result))
        return cls(std_dev_internal=std_dev, vector_slice=out_slice)

    @staticmethod
    def _scalar_lineage_from_uncertainty(u: Uncertainty) -> dict:
        """Returns the lineage dict for a single uncertainty input."""
        if isinstance(u, CovarianceModel):
            return u.lineage
        # Treat non-CovarianceModel as an independent noise source.
        return {str(uuid.uuid4()): u.std_dev}

    @classmethod
    def _propagate_from_jacobians(
        cls,
        jacs: tuple[Any, ...],
        uncertainties: Sequence[Uncertainty],
        values: Sequence[Any],
        result: Any,
    ) -> CovarianceModel:
        """Internal propagation for CovarianceModel."""
        backend = BackendManager.get_backend(values[0])

        is_vector_result = (
            backend.is_array(result) and backend.size(result) > 1
        )
        if is_vector_result:
            return cls._vector_propagation_path(
                jacs, uncertainties, result, backend
            )

        # Scalar Path (Lineage): merge coeff_new(uid) = sum(J_k * coeff_k(uid))
        new_lineage: dict = {}
        for u, jac in zip(uncertainties, jacs, strict=False):
            if u is None:
                continue
            src_lineage = cls._scalar_lineage_from_uncertainty(u)
            for uid, coeff in src_lineage.items():
                term = backend.mul(coeff, jac)
                if uid in new_lineage:
                    new_lineage[uid] = backend.add(new_lineage[uid], term)
                else:
                    new_lineage[uid] = term

        filtered_lineage = {
            k: v
            for k, v in new_lineage.items()
            if backend.any(backend.not_equal(v, 0))
        }

        dummy_inst = cls(std_dev_internal=0.0)
        new_std = dummy_inst._compute_std_dev(filtered_lineage, backend)

        needs_reshape = (
            backend.is_array(result) and backend.shape(result) != ()
        )
        if needs_reshape:
            new_std = backend.reshape(new_std, backend.shape(result))

        return cls(std_dev_internal=new_std, lineage=filtered_lineage)

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

    def _add_other_to_store(
        self,
        other: Uncertainty,
        jac_other: Any,
        store: Any,
        backend: Any,
        in_slices: list,
        jacobians: list,
    ) -> None:
        """Appends the 'other' uncertainty's slice and jacobian to the store lists."""
        if other is None:
            return
        if isinstance(other, CovarianceModel):
            in_slices.append(other.ensure_vector_slice(backend))
        else:
            std = getattr(other, "std_dev", other)
            in_slices.append(store.register_independent_array(std))
        jacobians.append(jac_other)

    def _add_vector_path(
        self,
        other: Uncertainty,
        jac_self: Any,
        jac_other: Any,
        out_magnitude: Any,
        backend: Any,
    ) -> CovarianceModel:
        """Vector-path implementation of add() — uses the covariance store."""
        from measurekit.domain.measurement.vectorized_uncertainty import (
            ensure_store,
        )

        store = ensure_store(backend)
        in_slices = [self.ensure_vector_slice(backend)]
        jacobians = [jac_self]
        self._add_other_to_store(
            other, jac_other, store, backend, in_slices, jacobians
        )

        out_slice = store.allocate(backend.size(out_magnitude))
        store.update_from_propagation(out_slice, in_slices, jacobians)

        out_cov = store.get_covariance_block(out_slice, out_slice)
        diag = backend.sparse_diagonal(out_cov)
        std_dev = backend.reshape(
            backend.sqrt(diag), backend.shape(out_magnitude)
        )
        return CovarianceModel(
            std_dev_internal=std_dev, vector_slice=out_slice
        )

    def _merge_lineage_with_jac(
        self, other: Uncertainty, jac_self: Any, jac_other: Any, backend: Any
    ) -> dict:
        """Merges self.lineage and other's lineage scaled by their jacobians."""
        other_lineage: dict = {}
        if other is not None:
            if isinstance(other, CovarianceModel):
                other_lineage = other.lineage
            else:
                std = getattr(other, "std_dev", other)
                other_lineage = {str(uuid.uuid4()): std}

        new_lineage: dict = {
            uid: backend.mul(coeff, jac_self)
            for uid, coeff in self.lineage.items()
        }
        for uid, coeff in other_lineage.items():
            val = backend.mul(coeff, jac_other)
            if uid in new_lineage:
                new_lineage[uid] = backend.add(new_lineage[uid], val)
            else:
                new_lineage[uid] = val
        return {
            k: v
            for k, v in new_lineage.items()
            if backend.any(backend.not_equal(v, 0))
        }

    def add(
        self,
        other: Uncertainty[UncType],
        jac_self: Any = 1.0,
        jac_other: Any = 1.0,
        out_magnitude: Any = None,
    ) -> Uncertainty[UncType]:
        """Adds two uncertainty models (correlated)."""
        backend = BackendManager.get_backend(self.std_dev_internal)

        is_vector_path = out_magnitude is not None and backend.is_array(
            out_magnitude
        )
        if is_vector_path:
            return self._add_vector_path(
                other, jac_self, jac_other, out_magnitude, backend
            )

        # Scalar Path (Lineage)
        filtered_lineage = self._merge_lineage_with_jac(
            other, jac_self, jac_other, backend
        )
        new_std = self._compute_std_dev(filtered_lineage, backend)

        needs_reshape = (
            backend.is_array(out_magnitude)
            and backend.shape(out_magnitude) != ()
        )
        if needs_reshape:
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
        """Propagates uncertainty for multiplication/division (correlated).

        Callers must pass explicit Jacobians; the operation cannot be
        reliably inferred from the values (comparing result_value to
        val1/val2 misclassifies mul as div when val2 is close to +/-1).
        """
        if jac_self is None or jac_other is None:
            raise ValueError(
                "propagate_mul_div requires explicit jac_self and jac_other."
            )

        return cast(
            "CovarianceModel",
            self.add(other, jac_self, jac_other, out_magnitude=result_value),
        )

    def power(
        self, exponent: float, value: Any = None, jac: Any = None
    ) -> CovarianceModel[Any]:
        """Propagates uncertainty for exponentiation (correlated)."""
        backend = BackendManager.get_backend(self.std_dev_internal)
        if jac is None:
            if value is None:
                raise ValueError(
                    "Either value or jac must be provided to power()"
                )
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
