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
        """Python fallback for the Rust covariance pruning config."""

        max_age: int = 100
        enabled: bool = False
        corr_threshold: float = 1e-6

    class CoreStore:
        """Python fallback for CovarianceStore where Rust is missing."""

        def __init__(self, config: PruningConfig = None):
            self.config = config or PruningConfig()
            self.matrix = None

        def register_variable(self, _var_id, variance):
            """Appends a variance block to the covariance matrix."""
            if self.matrix is None:
                self.matrix = scipy.sparse.csr_matrix(variance)
            else:
                self.matrix = scipy.sparse.bmat(
                    [[self.matrix, None], [None, variance]], format="csr"
                )

        def register_diagonal(self, var_id, variance_diag):
            """Registers a diagonal variance vector."""
            _ = len(variance_diag)
            sp = scipy.sparse.diags([variance_diag], [0], format="csr")
            self.register_variable(var_id, sp)

        def propagate(self, out_id, input_ids, jacobians):
            """Applies Jacobians to propagate covariance to the output."""
            # Compute sizes from jacobians
            out_size = jacobians[0].shape[0]
            out_slice = slice(out_id, out_id + out_size)

            in_slices = []
            for i, i_id in enumerate(input_ids):
                in_size = jacobians[i].shape[1]
                in_slices.append(slice(i_id, i_id + in_size))

            # Need a backend for propagate_affine
            from measurekit.backends.numpy_backend import NumpyBackend

            backend = NumpyBackend()

            self.matrix = propagate_affine(
                self.matrix, out_slice, in_slices, jacobians, backend
            )

        def get_covariance_block(
            self, row_slice: slice, col_slice: slice
        ) -> Any:
            """Returns the covariance sub-matrix for the given slices."""
            if self.matrix is None:
                return None
            return self.matrix[row_slice, col_slice]


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
        if hasattr(self._core, "allocate"):
            return self._core.allocate(size)

        start = self._next_idx
        self._next_idx += size
        return slice(start, start + size)

    def get_covariance_block(self, row_slice: slice, col_slice: slice) -> Any:
        """Retrieves a block from the global covariance matrix."""
        if hasattr(self._core, "get_covariance_block"):
            return self._core.get_covariance_block(row_slice, col_slice)

        if row_slice is None or col_slice is None:
            # Return zero block if no slice info (e.g. independent variables)
            # We don't have enough shape info here to return a specific size
            # but usually this is called when slices are expected to exist.
            # Returning None or raising a clearer error might be better,
            # but let's try to infer if we can.
            return None

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
                return torch.sparse_coo_tensor(
                    indices, values, size=shape, dtype=torch.float64
                )

            elif hasattr(self.backend, "from_scipy_sparse"):
                return self.backend.from_scipy_sparse(csr)

            return csr

        shape = (
            row_slice.stop - row_slice.start,
            col_slice.stop - col_slice.start,
        )
        if self.backend.__class__.__name__ == "TorchBackend":
            import torch

            return torch.sparse_coo_tensor(size=shape, dtype=torch.float64)

        if scipy:
            return scipy.sparse.csr_matrix(shape)

        return self.backend.zeros(shape)

    def _densify_sparse(self, val: Any) -> Any:
        """Converts sparse matrix representations to a dense array."""
        if hasattr(val, "toarray"):
            return val.toarray()
        if hasattr(val, "to_dense"):
            return val.to_dense()
        if hasattr(val, "todense"):
            return val.todense()
        return val

    def _to_numpy_or_tensor(self, val: Any) -> Any:
        """Converts val to numpy, or keeps it as a Torch tensor."""
        if self.backend.__class__.__name__ == "TorchBackend":
            return self.backend.asarray(val)
        if hasattr(val, "detach"):
            val = val.detach()
        if hasattr(val, "cpu"):
            val = val.cpu()
        if hasattr(val, "numpy"):
            return val.numpy()
        if hasattr(val, "__array__"):
            return np.array(val)
        if not isinstance(val, (np.ndarray, np.generic)):
            return np.array(val)
        return val

    def _broadcast_scalar_jacobian(
        self, scalar: Any, out_size: int, in_size: int
    ) -> Any:
        """Expands a scalar jacobian to matrix form given out/in sizes."""
        if out_size == in_size:
            mat = self.backend.identity_operator(out_size)
            return self.backend.mul(mat, scalar)
        if in_size == 1:
            # Column vector (out, 1) filled with scalar
            return self.backend.mul(self.backend.ones((out_size, 1)), scalar)
        if out_size == 1:
            # Row vector (1, in) filled with scalar
            return self.backend.mul(self.backend.ones((1, in_size)), scalar)
        # Ambiguous scalar — fall back to numpy fill_diagonal
        arr = np.zeros((out_size, in_size), dtype=np.float64)
        np.fill_diagonal(arr, scalar)
        return self.backend.asarray(arr)

    def _broadcast_jacobian(self, val: Any, out_size: int, in_size: int) -> Any:
        """Expands scalar or 1-D jacobian values to matrix form."""
        is_scalar = val.ndim == 0 or (
            val.ndim == 1 and self.backend.size(val) == 1
        )
        if is_scalar:
            scalar = float(val) if not self.backend.is_array(val) else val
            return self._broadcast_scalar_jacobian(scalar, out_size, in_size)
        if val.ndim == 1:
            size_matches = (
                self.backend.size(val) == in_size and out_size == in_size
            )
            is_col_vec = self.backend.size(val) == out_size and in_size == 1
            if size_matches:
                return self.backend.diagonal_operator(val)
            if is_col_vec:
                return self.backend.reshape(val, (-1, 1))
        return val

    def _ensure_float64_jac(self, val: Any) -> Any:
        """Casts val to float64; also re-densifies any sparse result."""
        if self.backend.__class__.__name__ == "TorchBackend":
            import torch

            if not isinstance(val, torch.Tensor):
                # Scalar jacobians arrive as numpy arrays
                val = torch.as_tensor(val)
            if val.dtype != torch.float64:
                val = val.to(torch.float64)
            return val
        has_non_float64 = hasattr(val, "dtype") and str(val.dtype) != "float64"
        if has_non_float64:
            if hasattr(val, "astype"):
                val = val.astype(np.float64, copy=False)
            else:
                val = np.asarray(val, dtype=np.float64)
        # Rust core requires dense arrays; broadcasting may produce sparse.
        if hasattr(val, "toarray"):
            val = val.toarray()
        elif hasattr(val, "todense"):
            val = np.asarray(val.todense())
        return val

    def _prepare_jacobian(self, jac: Any, out_size: int, in_size: int) -> Any:
        """Full pipeline: densify → to numpy/tensor → broadcast → float64."""
        val = self._densify_sparse(jac)
        val = self._to_numpy_or_tensor(val)
        if not isinstance(
            val, (np.ndarray, np.generic)
        ) and not self.backend.is_array(val):
            # Wrap plain scalars; backend arrays (torch tensors) must
            # pass through untouched or autograd gradients are lost.
            val = np.array(val)
        val = self._broadcast_jacobian(val, out_size, in_size)
        return self._ensure_float64_jac(val)

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

        final_jacs = [
            self._prepare_jacobian(
                jac,
                out_size,
                in_slices[i].stop - in_slices[i].start,
            )
            for i, jac in enumerate(jacobians)
        ]

        self._core.propagate(out_id, input_ids, final_jacs)

    def _prepare_variance_diagonal(self, variance: Any) -> Any:
        """Converts a variance array to a flat float64 diagonal ready for the core."""
        if self.backend.__class__.__name__ == "TorchBackend":
            import torch
            val = self.backend.reshape(variance, (-1,))
            if val.dtype != torch.float64:
                val = val.to(torch.float64)
            return val
        val = variance
        if hasattr(val, "detach"):
            val = val.detach()
        if hasattr(val, "cpu"):
            val = val.cpu()
        val = val.numpy() if hasattr(val, "numpy") else np.array(val)
        val = val.reshape(-1)
        has_non_float64 = hasattr(val, "dtype") and str(val.dtype) != "float64"
        if has_non_float64:
            if hasattr(val, "astype"):
                val = val.astype(np.float64, copy=False)
            else:
                val = np.asarray(val, dtype=np.float64)
        return val

    def _register_diagonal_with_fallback(self, idx: int, diag: Any, size: int) -> None:
        """Calls register_diagonal on the core store, falling back to register_variable."""
        try:
            self._core.register_diagonal(idx, diag)
        except AttributeError as err:
            if size < 1000:
                self._core.register_variable(idx, np.diag(diag))
            else:
                raise RuntimeError(
                    "Rust CoreStore.register_diagonal missing."
                ) from err

    def register_independent_array(self, std_dev: Any) -> slice:
        """Registers a new independent array and returns its slice."""
        size = self.backend.size(std_dev)
        slc = self.allocate(size)
        variance = self.backend.pow(std_dev, 2)
        diag = self._prepare_variance_diagonal(variance)
        self._register_diagonal_with_fallback(slc.start, diag, size)
        return slc


_current_store: contextvars.ContextVar[CovarianceStore | None] = (
    contextvars.ContextVar("current_store", default=None)
)


class MeasureKitContext:
    """Context manager providing a scoped CovarianceStore."""

    def __init__(
        self,
        backend_type: str = "numpy",
        pruning_config: PruningConfig | None = None,
    ):
        self.backend_type = backend_type
        self.pruning_config = pruning_config
        self.token = None

    def __enter__(self) -> CovarianceStore:
        # Use default config if none provided
        config = self.pruning_config or PruningConfig()
        store = CovarianceStore(backend=None, config=config)  # type: ignore
        self.token = _current_store.set(store)
        return store

    def __exit__(self, exc_type, exc_val, exc_tb):
        _current_store.reset(self.token)


def get_current_store() -> CovarianceStore | None:
    """Returns the context-local covariance store, if any."""
    return _current_store.get()


def get_active_store() -> CovarianceStore | None:
    """Returns the current store, falling back to the global store if unique."""
    store = _current_store.get()
    if store is not None:
        return store
    if len(_global_stores) == 1:
        return next(iter(_global_stores.values()))
    return None


_global_stores: dict[type, CovarianceStore] = {}


def ensure_store(backend: BackendOps) -> CovarianceStore:
    """Returns the active store, creating a per-backend global if needed."""
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
    """Clears all global stores (used between tests)."""
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

    for in_slc, jac in zip(in_slices, jacobians, strict=False):
        # Sigma subset: rows corresponding to input
        sigma_part = backend.sparse_slice(matrix, in_slc, slice(None))

        term = backend.sparse_matmul(jac, sigma_part)

        c_accum = term if c_accum is None else backend.add(c_accum, term)

    # c_accum is now C (M x N)

    # 2. Compute variance: V = C @ J.T (M x M)
    # V = sum(C[:, k] @ J_k.T)

    v_accum = None
    if c_accum is not None:
        for in_slc, jac in zip(in_slices, jacobians, strict=False):
            c_part = backend.sparse_slice(c_accum, slice(None), in_slc)
            term = backend.sparse_matmul(c_part, backend.transpose(jac))

            v_accum = term if v_accum is None else backend.add(v_accum, term)

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
        _ = backend.shape(matrix)[0]
        # This is hard without explicit zero constructor for sparse M x N
        # unless backend supports generic zeros.
        # Assuming sparse_matmul returned something valid or loop didn't run.
        # If loop didn't run, matrix has N rows.
        # Construct zero matrix?
        # For now assume jacobians not empty.

    new_mat = backend.sparse_bmat(
        [[matrix, backend.transpose(c_accum)], [c_accum, v_accum]]
    )

    return new_mat
