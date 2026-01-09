from __future__ import annotations

import contextvars
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, TypeVar

# Check libraries
try:
    import numpy as np
except ImportError:
    np = None

try:
    import scipy.sparse
except ImportError:
    scipy = None

try:
    from measurekit_core import CovarianceStore as CoreStore
    from measurekit_core import PruningConfig
except ImportError:

    @dataclass
    class PruningConfig:
        enabled: bool = False
        threshold: float = 1e-6

    class CoreStore:
        """Python fallback for CovarianceStore where Rust is missing."""

        def __init__(self, config: PruningConfig):
            self.config = config
            self.matrix = {}

        def register_variable(self, var_id, variance):
            pass

        def register_diagonal(self, var_id, variance_diag):
            pass

        def propagate(self, out_id, input_ids, jacobians):
            pass

        def get_block_csr(self, id1, id2):
            return None


if TYPE_CHECKING:
    from measurekit.core.protocols import BackendOps

T = TypeVar("T")


@dataclass
class CovarianceStore:
    """Stateless-ready store for covariance management using Rust core."""

    backend: BackendOps
    config: PruningConfig = field(default_factory=PruningConfig)
    _core: CoreStore = field(init=False)
    _next_idx: int = 0
    _initialized: bool = False

    def __post_init__(self) -> None:
        """Initializes the core Rust store."""
        self._core = CoreStore(self.config)
        self._initialized = True

    def allocate(self, size: int) -> slice:
        """Allocates a block of indices/ID for a new array quantity."""
        start = self._next_idx
        self._next_idx += size
        return slice(start, start + size)

    def get_covariance_block(self, row_slice: slice, col_slice: slice) -> Any:
        """Retrieves a block from the global covariance matrix."""
        id1 = row_slice.start
        id2 = col_slice.start

        res = self._core.get_block_csr(id1, id2)

        if res is not None:
            data, indices, indptr, shape = res

            if scipy is None:
                raise ImportError(
                    "Scipy is required for sparse matrix reconstruction"
                )

            csr = scipy.sparse.csr_matrix((data, indices, indptr), shape=shape)

            if self.backend.__class__.__name__ == "TorchBackend":
                coo = csr.tocoo()
                import torch

                indices = torch.tensor(
                    np.vstack((coo.row, coo.col)), dtype=torch.long
                )
                values = torch.tensor(coo.data, dtype=torch.float64)
                return torch.sparse_coo_tensor(indices, values, size=shape)

            elif hasattr(self.backend, "from_scipy_sparse"):
                return self.backend.from_scipy_sparse(csr)

            return csr

        shape = (
            row_slice.stop - row_slice.start,
            col_slice.stop - col_slice.start,
        )
        if self.backend.__class__.__name__ == "TorchBackend":
            import torch

            return torch.sparse_coo_tensor(size=shape)

        if scipy:
            return scipy.sparse.csr_matrix(shape)

        return self.backend.zeros(shape)

    def update_from_propagation(
        self,
        out_slice: slice,
        in_slices: list[slice],
        jacobians: list[Any],
    ) -> None:
        """Updates the covariance matrix using affine transformation via Rust backend."""
        out_id = out_slice.start
        input_ids = [s.start for s in in_slices]
        out_size = out_slice.stop - out_slice.start

        final_jacs = []
        for i, jac in enumerate(jacobians):
            val = jac
            in_size = in_slices[i].stop - in_slices[i].start

            # Densify sparse matrices if needed
            if hasattr(val, "toarray"):
                val = val.toarray()
            elif hasattr(val, "to_dense"):
                val = val.to_dense()
            elif hasattr(val, "todense"):
                val = val.todense()

            if hasattr(val, "detach"):
                val = val.detach()
            if hasattr(val, "cpu"):
                val = val.cpu()
            if hasattr(val, "numpy"):
                val = val.numpy()
            elif hasattr(val, "__array__"):
                val = np.array(val)

            if not isinstance(val, (np.ndarray, np.generic)):
                val = np.array(val)

            # Scalar/Vector Broadcasting logic
            if val.ndim == 0 or (val.ndim == 1 and val.size == 1):
                scalar = float(val)
                if out_size == in_size:
                    # Identity * scalar
                    val = np.eye(out_size, dtype=np.float64) * scalar
                else:
                    if in_size == 1:
                        # Column vector (out, 1) filled with scalar
                        val = np.full((out_size, 1), scalar, dtype=np.float64)
                    elif out_size == 1:
                        # Row vector (1, in) filled with scalar
                        val = np.full((1, in_size), scalar, dtype=np.float64)
                    else:
                        # Ambiguous scalar
                        # For element-wise ops, usually means Identity-like logic if broadcasting works?
                        # Assume strictly prohibited for now to find bugs unless diagonal.
                        # Try Diagonal embedding?
                        # If out > in, maybe (out, in) with diagonal?
                        # For now, create zeros + diagonal.
                        val = np.zeros((out_size, in_size), dtype=np.float64)
                        np.fill_diagonal(val, scalar)

            elif val.ndim == 1:
                # Vector. If Input Size matches, treat as Diagonal Matrix (Element-wise mul Jacobian)
                if val.size == in_size and out_size == in_size:
                    val = np.diag(val)
                elif val.size == out_size and in_size == 1:
                    # Column vector
                    val = val.reshape(-1, 1)
                else:
                    # Maybe it is a flattened matrix?
                    # Unsafe to guess.
                    pass

            # Ensure float64
            if str(val.dtype) != "float64":
                val = val.astype(np.float64, copy=False)

            final_jacs.append(val)

        self._core.propagate(out_id, input_ids, final_jacs)

    def register_independent_array(self, std_dev: Any) -> slice:
        """Registers a new independent array and returns its slice."""
        size = self.backend.size(std_dev)
        slc = self.allocate(size)

        variance = self.backend.pow(std_dev, 2)
        val = variance
        if hasattr(val, "detach"):
            val = val.detach()
        if hasattr(val, "cpu"):
            val = val.cpu()
        if hasattr(val, "numpy"):
            val = val.numpy()
        else:
            val = np.array(val)

        val = val.reshape(-1)
        if str(val.dtype) != "float64":
            val = val.astype(np.float64, copy=False)

        try:
            self._core.register_diagonal(slc.start, val)
        except AttributeError:
            if size < 1000:
                self._core.register_variable(slc.start, np.diag(val))
            else:
                raise RuntimeError("Rust CoreStore.register_diagonal missing.")

        return slc


_current_store: contextvars.ContextVar[CovarianceStore | None] = (
    contextvars.ContextVar("current_store", default=None)
)


class MeasureKitContext:
    def __init__(self, backend_type: str = "numpy"):
        self.backend_type = backend_type
        self.token = None

    def __enter__(self) -> CovarianceStore:
        store = CovarianceStore(backend=None)  # type: ignore
        self.token = _current_store.set(store)
        return store

    def __exit__(self, exc_type, exc_val, exc_tb):
        _current_store.reset(self.token)


def get_current_store() -> CovarianceStore | None:
    return _current_store.get()


_global_stores: dict[type, CovarianceStore] = {}


def ensure_store(backend: BackendOps) -> CovarianceStore:
    store = get_current_store()
    if store is not None:
        if store.backend is None:
            store.backend = backend
        return store

    bk_type = type(backend)
    if bk_type not in _global_stores:
        _global_stores[bk_type] = CovarianceStore(backend=backend)

    return _global_stores[bk_type]


def clear_global_stores() -> None:
    _global_stores.clear()
    _current_store.set(None)


def propagate_affine(
    matrix: Any,
    out_slice: slice,
    in_slices: list[slice],
    jacobians: list[Any],
    backend: BackendOps,
) -> Any:
    """Performs J * Sigma * J.T update on the matrix (Functional API).

    Computes:
       C = J * matrix (Cross-covariance)
       V = J * matrix * J.T = C * J.T (Output Variance)
       New_Matrix = [[matrix, C.T], [C, V]]

    Args:
        matrix: Current covariance matrix (N x N).
        out_slice: Slice for output variables (must follow matrix indices).
        in_slices: Slices for input variables (must be within matrix).
        jacobians: List of Jacobian blocks corresponding to in_slices.
        backend: Backend operations provider.

    Returns:
        The augmented covariance matrix ((N+M) x (N+M)).
    """
    # 1. Compute correlations: C = J @ Sigma (M x N)
    # We compute this by parts: sum(J_k @ Sigma[k])

    m_size = out_slice.stop - out_slice.start
    c_accum = None

    for in_slc, jac in zip(in_slices, jacobians):
        # Sigma subset: rows corresponding to input
        sigma_part = matrix[in_slc]  # Select rows

        # term = jac @ sigma_part
        term = backend.sparse_matmul(jac, sigma_part)

        if c_accum is None:
            c_accum = term
        else:
            c_accum = backend.add(c_accum, term)

    # c_accum is now C (M x N)

    # 2. Compute variance: V = C @ J.T (M x M)
    # V = sum(C[:, k] @ J_k.T)

    v_accum = None
    if c_accum is not None:
        for in_slc, jac in zip(in_slices, jacobians):
            # c_part = C[:, in_slc]
            c_part = c_accum[:, in_slc]  # Slicing columns

            # term = c_part @ jac.T
            term = backend.sparse_matmul(c_part, backend.transpose(jac))

            if v_accum is None:
                v_accum = term
            else:
                v_accum = backend.add(v_accum, term)

    if v_accum is None:
        # Fallback for empty jacobians (should unlikely happen in affine)
        # Create zero matrix M x M
        v_accum = backend.sparse_eye(m_size, reference=matrix)
        v_accum = backend.mul(v_accum, 0.0)

    # 3. Assemble
    # [[ matrix, C.T ], [ C, V ]]

    # Handling None c_accum (e.g. if jacobians empty)
    if c_accum is None:
        # Need zero block M x N.
        n_size = backend.shape(matrix)[0]
        # This is hard without explicit zero constructor for sparse M x N
        # unless backend supports generic zeros.
        # Assuming sparse_matmul returned something valid or loop didn't run.
        # If loop didn't run, matrix has N rows.
        # Construct zero matrix?
        # For now assume jacobians not empty.
        pass

    new_mat = backend.sparse_bmat(
        [[matrix, backend.transpose(c_accum)], [c_accum, v_accum]]
    )

    return new_mat
