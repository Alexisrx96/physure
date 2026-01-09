from __future__ import annotations

import contextvars
import dataclasses
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, TypeVar

try:
    from measurekit_core import CovarianceStore as CoreStore
    from measurekit_core import PruningConfig
except ImportError:

    @dataclass
    class PruningConfig:
        enabled: bool = False
        threshold: float = 1e-6

    class CoreStore:
        """Python fallback for CovarianceStore."""

        def __init__(self, config: PruningConfig):
            self.config = config
            self.current_size = 0

        def allocate(self, size: int) -> tuple[int, int]:
            start = self.current_size
            self.current_size += size
            return start, self.current_size

        def update_covariance(self, out_indices, in_indices):
            # No-op in python fallback, actual matrix math is handled by Python layer
            pass


if TYPE_CHECKING:
    from measurekit.core.protocols import BackendOps

T = TypeVar("T")


@dataclass
class CovarianceStore:
    """Stateless-ready store for covariance management using Rust core."""

    backend: BackendOps
    config: PruningConfig = dataclasses.field(default_factory=PruningConfig)
    _core: CoreStore = dataclasses.field(init=False)
    _matrix: Any = None
    _next_idx: int = 0
    _initialized: bool = False

    def __post_init__(self) -> None:
        """Initializes the core Rust store."""
        self._core = CoreStore(self.config)
        self._initialized = True

    def _ensure_initialized(self) -> None:
        if not self._initialized:
            # Initialize with an empty sparse matrix (0x0)
            self._matrix = self.backend.sparse_matrix(
                data=self.backend.asarray([]),
                indices=(self.backend.asarray([]), self.backend.asarray([])),
                shape=(0, 0),
            )
            self._initialized = True

    def allocate(self, size: int) -> slice:
        """Allocates a block of indices for a new array quantity."""
        start, end = self._core.allocate(size)
        return slice(start, end)

    def get_covariance_block(self, row_slice: slice, col_slice: slice) -> Any:
        """Retrieves a block from the global covariance matrix."""
        self._ensure_initialized()
        # Note: Basic slicing might not be supported for all sparse backends.
        # For Scipy it is. For Torch sparse it is NOT directly.
        # However, for propagation we usually need the full matrix or
        # specific blocks during update.
        # If backend is Torch, we might need specific sparse_slice method.
        # For now, we assume the backend handles it or we provide a fallback.
        try:
            return self._matrix[row_slice, col_slice]
        except (TypeError, AttributeError, RuntimeError):
            # Fallback or specific backend call if needed
            # RuntimeError catches 'aten::as_strided' for sparse (Torch)
            if hasattr(self.backend, "sparse_slice"):
                return self.backend.sparse_slice(
                    self._matrix, row_slice, col_slice
                )
            raise

    def update_from_propagation(
        self,
        out_slice: slice,
        in_slices: list[slice],
        jacobians: list[Any],
    ) -> None:
        """Updates the covariance matrix using affine transformation."""
        # 1. Update Rust Core (Stateful Metadata)
        in_indices = [(s.start, s.stop) for s in in_slices]
        self._core.update_covariance(
            (out_slice.start, out_slice.stop), in_indices
        )
        # Pruning simulation for Python matrix if enabled
        if self.config.enabled:
            pass  # Future: Sync python matrix with core pruning

        self._ensure_initialized()

        # 2. Perform Matrix Operation (Pure Logic)
        self._matrix = propagate_affine(
            self._matrix,
            out_slice,
            in_slices,
            jacobians,
            self.backend,
        )

    def register_independent_array(self, std_dev: Any) -> slice:
        """Registers a new independent array and returns its slice."""
        # Convert std_dev (standard deviation) to variance
        variance = self.backend.pow(std_dev, 2)
        size = self.backend.size(std_dev)
        slc = self.allocate(size)

        # Always wrap source variance in a sparse diagonal matrix
        var_diag = self.backend.sparse_diags(
            [self.backend.reshape(variance, (size,))], [0], shape=(size, size)
        )

        if self._matrix is None or (
            hasattr(self._matrix, "shape") and self._matrix.shape[0] == 0
        ):
            self._matrix = var_diag
            self._initialized = True
        else:
            # Append variance to diagonal
            self._matrix = self.backend.sparse_bmat(
                [[self._matrix, None], [None, var_diag]]
            )

        return slc


def _process_jacobian(
    jac: Any, start_idx: int, backend: BackendOps
) -> tuple[Any, Any, Any]:
    """Helper to extract sparse data from a Jacobian.

    Returns:
        (values, rows, cols) or (None, None, None) if extraction fails/empty.
    """
    # Handle different backend sparse formats
    # Try backend-specific extraction first
    if hasattr(backend, "to_coo"):
        res = backend.to_coo(jac)
        if res is not None:
            data, rows, cols = res
            return (
                backend.asarray(data),
                backend.asarray(rows),
                backend.asarray(cols + start_idx),
            )

    # Handle different backend sparse formats
    if hasattr(jac, "is_sparse") and jac.is_sparse:
        # Torch specific
        indices = jac.indices()
        return jac.values(), indices[0], indices[1] + start_idx
    elif hasattr(jac, "tocoo"):
        # Scipy specific
        coo = jac.tocoo()
        return (
            backend.asarray(coo.data),
            backend.asarray(coo.row),
            backend.asarray(coo.col + start_idx),
        )

    elif hasattr(jac, "to_sparse"):
        # Torch dense to sparse
        sp = jac.to_sparse()
        if hasattr(sp, "coalesce"):
            sp = sp.coalesce()
        indices = sp.indices()
        return sp.values(), indices[0], indices[1] + start_idx
    else:
        # Dense fallback
        try:
            if hasattr(jac, "nonzero"):
                rows, cols = jac.nonzero()
                vals = jac[rows, cols]
                return (
                    backend.asarray(vals),
                    backend.asarray(rows),
                    backend.asarray(cols + start_idx),
                )
            else:
                # Numpy fallback
                import numpy as np

                jac_np = np.atleast_2d(np.array(jac))
                rows, cols = jac_np.nonzero()
                vals = jac_np[rows, cols]
                return (
                    backend.asarray(vals),
                    backend.asarray(rows),
                    backend.asarray(cols + start_idx),
                )
        except Exception:
            return None, None, None


def propagate_affine(
    current_matrix: Any,
    out_slice: slice,
    in_slices: list[slice],
    jacobians: list[Any],
    backend: BackendOps,
) -> Any:
    """Pure functional implementation of affine covariance propagation.

    Args:
        current_matrix: The current covariance matrix (sparse).
        out_slice: The slice allocated for the new result.
        in_slices: Slices of the input variables.
        jacobians: Jacobians of the output w.r.t input variables.
        backend: The backend operations implementation.

    Returns:
        The updated covariance matrix.
    """
    out_size = out_slice.stop - out_slice.start
    total_old_size = 0
    if hasattr(current_matrix, "shape"):
        total_old_size = current_matrix.shape[0]

    # Cross-device safety: determine target device
    target_device = backend.get_device(current_matrix)

    all_data = []
    all_rows = []
    all_cols = []

    for slc, jac in zip(in_slices, jacobians, strict=False):
        # Move jac to target device if needed
        # Avoid asarray if sparse (BCOO/etc) to prevent densification or error
        is_sparse = (
            (hasattr(jac, "is_sparse") and jac.is_sparse)
            or (hasattr(jac, "indices") and hasattr(jac, "data"))
            or hasattr(jac, "tocoo")
            or hasattr(jac, "to_sparse")
        )

        if not is_sparse:
            jac = backend.asarray(jac)

        if target_device:
            curr_device = backend.get_device(jac)
            if curr_device != target_device:
                jac = backend.to_device(jac, target_device)

        vals, rows, cols = _process_jacobian(jac, slc.start, backend)

        if vals is not None:
            all_data.append(vals)
            all_rows.append(rows)
            all_cols.append(cols)

    if not all_data:
        # Identity or zero propagation
        j_in = backend.sparse_matrix(
            backend.asarray([]),
            (backend.asarray([]), backend.asarray([])),
            shape=(out_size, total_old_size),
        )
    else:
        j_in = backend.sparse_matrix(
            backend.concatenate(all_data),
            (
                backend.concatenate(all_rows),
                backend.concatenate(all_cols),
            ),
            shape=(out_size, total_old_size),
        )

    # Calculate cross-covariance: J_in * Sigma_old
    # print(f"DEBUG: j_in shape={j_in.shape}, current_matrix shape={current_matrix.shape}")
    cross_cov = backend.sparse_matmul(j_in, current_matrix)
    # print(f"DEBUG: cross_cov nonzero={backend.size(backend.asarray(cross_cov).nonzero()[0]) if not hasattr(cross_cov, 'nnz') else cross_cov.nnz}")

    # Calculate output covariance: cross_cov * J_in.T
    j_in_t = j_in.T if hasattr(j_in, "T") else backend.transpose(j_in)

    out_cov = backend.sparse_matmul(cross_cov, j_in_t)

    cross_cov_transposed = (
        cross_cov.T
        if hasattr(cross_cov, "T")
        else backend.transpose(cross_cov)
    )

    # Construct new matrix: [[Sigma_old, cross_cov.T], [cross_cov, out_cov]]
    return backend.sparse_bmat(
        [[current_matrix, cross_cov_transposed], [cross_cov, out_cov]]
    )


_current_store: contextvars.ContextVar[CovarianceStore | None] = (
    contextvars.ContextVar("current_store", default=None)
)


class MeasureKitContext:
    """Context manager for managing the active covariance store."""

    def __init__(self, backend_type: str = "numpy"):
        """Initializes the context."""
        self.backend_type = backend_type
        self.token = None

    def __enter__(self) -> CovarianceStore:
        """Enters the context."""
        store = CovarianceStore(backend=None)  # type: ignore
        self.token = _current_store.set(store)
        return store

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Exits the context."""
        _current_store.reset(self.token)


def get_current_store() -> CovarianceStore | None:
    """Retrieves the active covariance store from the context."""
    return _current_store.get()


_global_stores: dict[type, CovarianceStore] = {}


def ensure_store(backend: BackendOps) -> CovarianceStore:
    """Gets the current store or creates a new one tied to the backend."""
    store = get_current_store()
    if store is not None:
        if store.backend is None:
            store.backend = backend
        return store

    # Use global store keyed by backend class
    bk_type = type(backend)
    if bk_type not in _global_stores:
        _global_stores[bk_type] = CovarianceStore(backend=backend)

    return _global_stores[bk_type]


def clear_global_stores() -> None:
    """Clears all global covariance stores and active context stores."""
    _global_stores.clear()
    _current_store.set(None)
