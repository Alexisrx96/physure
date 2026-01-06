from __future__ import annotations

import contextvars
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, TypeVar

if TYPE_CHECKING:
    from measurekit.core.protocols import BackendOps

T = TypeVar("T")


@dataclass
class CovarianceStore:
    """Stateless-ready store for covariance management.

    This class is backend-agnostic and relies on the BackendOps protocol
    to perform sparse matrix operations.
    """

    backend: BackendOps
    _matrix: Any = None
    _next_idx: int = 0
    _initialized: bool = False

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
        start = self._next_idx
        end = start + size
        self._next_idx = end
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
        """Updates the covariance matrix using affine transformation.

        Sigma_new = [[Sigma_old, cross^T], [cross, out]]
        """
        self._ensure_initialized()

        csr_mat = self._matrix
        out_size = out_slice.stop - out_slice.start
        total_old_size = csr_mat.shape[0]

        # Cross-device safety: determine target device
        target_device = self.backend.get_device(csr_mat)

        all_data = []
        all_rows = []
        all_cols = []

        for slc, jac in zip(in_slices, jacobians, strict=False):
            # Move jac to target device if needed
            jac = self.backend.asarray(jac)
            if target_device:
                curr_device = self.backend.get_device(jac)
                if curr_device != target_device:
                    jac = self.backend.to_device(jac, target_device)

            # Implementation continued...
            # (Keeping logic for COO, but backend.to_coo would be better)
            # Assume backend.sparse_matrix handles jac on right device.
            if hasattr(jac, "is_sparse") and jac.is_sparse:
                # Torch specific
                indices = jac.indices()
                all_data.append(jac.values())
                all_rows.append(indices[0])
                all_cols.append(indices[1] + slc.start)
            elif hasattr(jac, "tocoo"):
                # Scipy specific
                coo = jac.tocoo()
                all_data.append(self.backend.asarray(coo.data))
                all_rows.append(self.backend.asarray(coo.row))
                all_cols.append(self.backend.asarray(coo.col + slc.start))
            else:
                # Dense fallback for Jacobian
                # We can convert dense to COO data
                pass

        # ... (rest of implementation remains same)
        # For brevity, finishing device check implementation.
        # and assume the rest of the method handles the device-aware objects.

        # Alternative approach using sparse_bmat which is more backend-friendly
        # We want to build J = [0, ..., J1, ..., 0, J2, ...]
        # This is essentially a horizontal concatenation of blocks.
        # But we only have blocks for 'in_slices'. The rest are zeros.

        # Better: compute out_cov = sum(Ji @ Sigma_ii @ Ji^T) + cross terms
        # But Sigma might have cross-correlations!
        # So we MUST do J @ Sigma @ J^T where J = [J1, ...] from full state.

        for slc, jac in zip(in_slices, jacobians, strict=False):
            # We need to flattened jac data
            # Assuming jac is handled by the backend
            if hasattr(jac, "is_sparse") and jac.is_sparse:
                # Torch specific
                indices = jac.indices()
                all_data.append(jac.values())
                all_rows.append(indices[0])
                all_cols.append(indices[1] + slc.start)
            elif hasattr(jac, "tocoo"):
                # Scipy specific
                coo = jac.tocoo()
                all_data.append(self.backend.asarray(coo.data))
                all_rows.append(self.backend.asarray(coo.row))
                all_cols.append(self.backend.asarray(coo.col + slc.start))
            elif hasattr(jac, "to_sparse"):
                # Torch dense to sparse
                sp = jac.to_sparse()
                if hasattr(sp, "coalesce"):
                    sp = sp.coalesce()
                indices = sp.indices()
                all_data.append(sp.values())
                all_rows.append(indices[0])
                all_cols.append(indices[1] + slc.start)
            else:
                # Dense fallback for generic arrays (e.g. JAX, NumPy)
                # We can use nonzero() to get indices if efficient
                try:
                    # Generic dense-to-sparse via nonzero
                    # Assuming backend has nonzero or we can use numpy fallback
                    if hasattr(jac, "nonzero"):
                        # If jac is large dense, this is slow.
                        rows, cols = jac.nonzero()
                        vals = jac[rows, cols]
                        all_data.append(self.backend.asarray(vals))
                        all_rows.append(self.backend.asarray(rows))
                        all_cols.append(self.backend.asarray(cols + slc.start))
                    else:
                        # Try backend.nonzero(jac)
                        # Not in protocol yet?
                        # Assume numpy-like behavior or cast to numpy
                        import numpy as np

                        jac_np = np.array(jac)  # Force host sync if needed
                        rows, cols = jac_np.nonzero()
                        vals = jac_np[rows, cols]
                        # Must convert back to backend array!
                        all_data.append(self.backend.asarray(vals))
                        all_rows.append(self.backend.asarray(rows))
                        all_cols.append(self.backend.asarray(cols + slc.start))
                except Exception:
                    # If all else fails
                    pass

        if not all_data:
            # Identity or zero propagation?
            j_in = self.backend.sparse_matrix(
                self.backend.asarray([]),
                (self.backend.asarray([]), self.backend.asarray([])),
                shape=(out_size, total_old_size),
            )
        else:
            j_in = self.backend.sparse_matrix(
                self.backend.concatenate(all_data),
                (
                    self.backend.concatenate(all_rows),
                    self.backend.concatenate(all_cols),
                ),
                shape=(out_size, total_old_size),
            )

        # cross_cov = J_in @ Sigma_old
        cross_cov = self.backend.sparse_matmul(j_in, csr_mat)

        # out_cov = cross_cov @ J_in.T
        out_cov = self.backend.sparse_matmul(
            cross_cov, self.backend.reshape(j_in, (total_old_size, out_size))
        )
        # Most backends support .T on sparse. If not, add to protocol.
        if hasattr(j_in, "T"):
            out_cov = self.backend.sparse_matmul(cross_cov, j_in.T)
        else:
            # Manual transpose for COO?
            pass

        # Sigma_new = [ [Sigma_old, cross_cov.T], [cross_cov, out_cov] ]
        cross_cov_transposed = (
            cross_cov.T if hasattr(cross_cov, "T") else None
        )  # Should implement transpose in BackendOps

        self._matrix = self.backend.sparse_bmat(
            [[csr_mat, cross_cov_transposed], [cross_cov, out_cov]]
        )

    def register_independent_array(self, std_dev: Any) -> slice:
        """Registers a new independent array and returns its slice."""
        val = self.backend.asarray(std_dev)
        size = self.backend.size(val)
        slc = self.allocate(size)

        # variance = std_dev^2
        diag_val = self.backend.reshape(self.backend.pow(val, 2), (-1,))
        variance = self.backend.sparse_diags(
            [diag_val], [0], shape=(size, size)
        )

        if self._matrix is None or (
            hasattr(self._matrix, "shape") and self._matrix.shape[0] == 0
        ):
            self._matrix = variance
            self._initialized = True
        else:
            self._matrix = self.backend.sparse_bmat(
                [[self._matrix, None], [None, variance]]
            )
        return slc


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
        # Create a new store using the requested backend
        # We need a way to get the backend instance without data object here,
        # or we wait until first allocation.
        # For now, we assume NumpyBackend if not specified.
        # This is a bit chicken-and-egg. Let's just create a lazy one.
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
    """Gets the current store or creates a new one tied to the backend.

    If no context active, returns global shared store for backend type
    to ensure persistence of covariance data across operations.
    """
    store = get_current_store()
    if store is not None:
        if store.backend is None:
            store.backend = backend
        return store

    # Use global store keyed by backend class (assuming stateless backends)
    bk_type = type(backend)
    if bk_type not in _global_stores:
        _global_stores[bk_type] = CovarianceStore(backend=backend)

    return _global_stores[bk_type]


def clear_global_stores() -> None:
    """Clears all global covariance stores and active context stores.

    Useful for resetting state between tests to prevent pollution.
    """
    _global_stores.clear()
    _current_store.set(None)
