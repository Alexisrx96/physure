from __future__ import annotations

import contextvars
import uuid
from collections import OrderedDict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, TypeVar

from physure._core import CovarianceStore as CoreStore
from physure._core import PruningConfig
from physure.backends.affine import propagate_affine  # noqa: F401

# Check libraries
if TYPE_CHECKING:
    import numpy as np
else:

    class _LazyNumpy:
        """Stands in for ``numpy`` and imports it on first attribute access.

        A scalar ``Q_('3 m')`` never touches numpy, and importing it costs
        60-100 ms of the first use (the budget is 500 ms).
        """

        def __getattr__(self, name: str) -> Any:
            import numpy

            globals()["np"] = numpy
            return getattr(numpy, name)

    np = _LazyNumpy()


def _dense_host(jac: Any) -> Any:
    """A Jacobian as a dense host array-like (sparse torch densified, autograd detached)."""
    if getattr(jac, "is_sparse", False):
        jac = jac.to_dense()
    return jac.detach().cpu() if hasattr(jac, "detach") else jac


# combinations of lineage sources a store keeps by their terms (`_lineage_operand`)
_LINEAGE_CACHE = 4096


def _native_lineage_terms(model: Any) -> list[tuple[int, float]] | None:
    """The `(source, coefficient)` terms of a scalar with the core's lineage, else None."""
    native = getattr(model, "native", None)
    terms = getattr(native, "terms", None)
    return list(terms()) if callable(terms) else None


def _scipy() -> Any:
    """Return the scipy module, or None if not installed.

    Lazy import: scipy costs ~0.2s and most sessions never build a sparse
    covariance, so importing it at module load taxed every bootstrap.
    """
    global _scipy_mod
    if _scipy_mod is False:
        try:
            # ponytail: bare `scipy` is used below, but basedpyright's
            # unused-import check for dotted `import a.b` only credits
            # explicit `a.b` attribute access, not the bare-name use.
            import scipy.sparse  # pyright: ignore[reportUnusedImport]

            _scipy_mod = scipy
        except ImportError:
            _scipy_mod = None
    return _scipy_mod


_scipy_mod: Any = False

if TYPE_CHECKING:
    from types import TracebackType

    from physure.core.protocols import BackendOps, Numeric

T = TypeVar("T")


@dataclass
class CovarianceStore:
    """Stateless-ready store for covariance management using Rust core."""

    backend: BackendOps | None
    config: PruningConfig = field(default_factory=PruningConfig)
    _core: CoreStore = field(init=False)
    _next_idx: int = 0
    _initialized: bool = False
    # who this store is: a model records the token of the store its slots are in
    # (`CovarianceModel.store_token`); random, so a pickled one never matches
    token: str = field(
        default_factory=lambda: uuid.uuid4().hex, init=False, repr=False
    )
    # model -> its slot: a scalar (or size-1) uncertainty used in several array
    # operations is one variable
    _model_slots: dict[int, tuple[Any, slice]] = field(
        default_factory=dict, init=False, repr=False
    )
    # a source of the core's lineage -> its block and index (`_lineage_operand`)
    _lineage_sources: dict[int, tuple[slice, int]] = field(
        default_factory=dict, init=False, repr=False
    )
    # the terms of a combination of sources -> its slot, most recent last
    _lineage_combinations: OrderedDict[tuple[Any, ...], tuple[slice, Any]] = (
        field(default_factory=OrderedDict, init=False, repr=False)
    )

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

        id1 = row_slice.start
        id2 = col_slice.start

        res = self._core.get_block_csr(id1, id2)

        if res is not None:
            data, indices, indptr, shape = res

            scipy = _scipy()
            if scipy is None:
                raise ImportError(
                    "Scipy is required for sparse matrix reconstruction"
                )

            csr = scipy.sparse.csr_matrix((data, indices, indptr), shape=shape)
            return self.backend.from_scipy_sparse(csr)

        shape = (
            row_slice.stop - row_slice.start,
            col_slice.stop - col_slice.start,
        )
        scipy = _scipy()
        if scipy:
            return self.backend.from_scipy_sparse(
                scipy.sparse.csr_matrix(shape)
            )

        return self.backend.zeros(shape)

    def _densify_sparse(self, val: Any) -> Any:
        # ponytail: val may be a scipy sparse matrix, torch sparse tensor,
        # or dense array — sparse types aren't part of the Numeric alias,
        # so this duck-typed converter stays Any at the boundary.
        """Converts sparse matrix representations to a dense array."""
        if hasattr(val, "toarray"):
            return val.toarray()
        if hasattr(val, "to_dense"):
            return val.to_dense()
        if hasattr(val, "todense"):
            return val.todense()
        return val

    # ponytail: val may still be sparse-derived or a bare Python scalar
    # here; same dynamic-boundary reasoning as _densify_sparse.
    def _to_numpy_or_tensor(self, val: Any) -> Any:
        """Converts val to numpy, or keeps it as a Torch tensor."""
        if self.backend.preserves_native_gradients():
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

    # ponytail: this jacobian pipeline moves values between sparse
    # matrices, dense arrays, and tensors via duck-typed attribute checks
    # (.ndim, .dtype, .toarray, ...) — Numeric's float|int members don't
    # carry those, so the internal helpers stay Any; the public entry
    # points (update_from_propagation, register_independent_array) are
    # typed Numeric at the boundary instead.
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

    def _broadcast_jacobian(
        self, val: Any, out_size: int, in_size: int
    ) -> Any:
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
        if self.backend.preserves_native_gradients():
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
        jacobians: list[Numeric],
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

        if isinstance(self._core, CoreStore):
            # The Rust core holds numpy arrays. A torch Jacobian keeps its autograd
            # graph only for the Python lineage store (`python_lineage()`); the core
            # takes its values, as `register_diagonal` does. A JAX array, or a
            # diagonal the backend rebuilt from a numpy vector, is converted the same way.
            # The core takes dense arrays, so a sparse torch operator (the
            # element-wise Jacobian is one) is densified here, as scipy's is above.
            # The core reads the buffer in row-major order: a column-major array
            # (a transpose, `np.kron(I, B.T)`) would be read transposed, without error.
            final_jacs = [
                np.ascontiguousarray(_dense_host(jac), dtype=np.float64)
                for jac in final_jacs
            ]
        self._core.propagate(out_id, input_ids, final_jacs)

    def _prepare_variance_diagonal(self, variance: Any) -> Any:
        """Converts a variance array to a flat float64 diagonal ready for the core."""
        if self.backend.preserves_native_gradients():
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

    def _register_diagonal_with_fallback(
        self, idx: int, diag: Any, size: int
    ) -> None:
        """Calls register_diagonal on the core store, falling back to register_variable."""
        try:
            self._core.register_diagonal(idx, diag)
        except TypeError:
            # Rust core only accepts numpy arrays; torch tensors reach here
            # when the torch backend is active.
            import numpy

            if hasattr(diag, "detach"):
                diag = diag.detach().cpu()
            self._core.register_diagonal(
                idx, numpy.asarray(diag, dtype=numpy.float64)
            )
        except AttributeError as err:
            if size < 1000:
                self._core.register_variable(idx, np.diag(diag))
            else:
                raise RuntimeError(
                    "Rust CoreStore.register_diagonal missing."
                ) from err

    def model_operand(self, model: Any, std_dev: Numeric) -> tuple[slice, Any]:
        """The slot of a scalar's error in this store and `d model/d slot` (`with_slot_scale`).

        The same error is the same slot on every use: registering it afresh made
        `(a * s) / s` treat the two `s` as independent errors, and the shared factor did
        not cancel. A scalar with the core's lineage is the combination of its sources
        it is (`_lineage_operand`), so two models of one source (`x` and `x.to("cm")`)
        are one error; any other model is one slot, kept per model object.
        """
        terms = _native_lineage_terms(model)
        if terms:
            return self._lineage_operand(terms)
        known = self._model_slots.get(id(model))
        if known is not None and known[0] is model:
            return known[1], 1.0
        slc = self.register_independent_array(std_dev)
        self._model_slots[id(model)] = (model, slc)
        return slc, 1.0

    def _lineage_operand(
        self, terms: list[tuple[int, float]]
    ) -> tuple[slice, Any]:
        """A scalar with the core's lineage as `sum c_k e_k` over its sources `e_k`.

        Each source is a unit-variance element of the store, kept for the store's
        life, so every model of it (`x`, `x.to("cm")`, a sum that holds `x`) is read
        from the same error. The cost is bounded so:

        - the sources first seen together get one slot (a block of unit variances):
          a scalar of 1e4 sources is one registration, not 1e4;
        - a scalar of one source is that source's slot scaled by its coefficient
          (`(slot, c)`), with no slot of its own, when the source is alone in its slot;
        - any other combination gets one slot, propagated once from the blocks of its
          sources (a row of coefficients each), and is kept by its terms (not by its
          model object, which is not kept alive) for the last `_LINEAGE_CACHE`
          combinations; a combination met again after that is propagated again from
          the same sources, which is the same error (exact, only slower).

        Every source slot and combination slot holds covariance blocks in the core's
        store, which visits all of its blocks at each propagation; a long loop that
        meets a new scalar at each step therefore grows the store (as each new array
        result does): its cost per step grows with the number of results correlated
        with the operands, on the base as well. Measured (2026-11-23, `arr + x` for a
        3-element `arr`): x of 1e2/1e3/1e4 sources 1.3/2.0/9.1 ms (without the
        lineage, one independent slot: about 1 ms each); a loop of `base + z` with a
        new scalar `z` of one source as fast as that (200 steps 3.2 s, 400 steps
        30.8 s), of two sources 1.5 times slower (4.8 s and 46 s against 3.2 s and
        30.5 s): its source block and combination slot are two more blocks per step.
        """
        key = tuple(terms)
        known = self._lineage_combinations.get(key)
        if known is not None:
            self._lineage_combinations.move_to_end(key)
            return known
        fresh = [sid for sid, _ in terms if sid not in self._lineage_sources]
        if fresh:
            block = self.allocate(len(fresh))
            self._register_diagonal_with_fallback(
                block.start, np.ones(len(fresh)), len(fresh)
            )
            for index, sid in enumerate(fresh):
                self._lineage_sources[sid] = (block, index)
        if len(terms) == 1:
            sid, coefficient = terms[0]
            block, _ = self._lineage_sources[sid]
            if block.stop - block.start == 1:
                return block, float(coefficient)
        rows: dict[int, tuple[slice, Any]] = {}
        for sid, coefficient in terms:
            block, index = self._lineage_sources[sid]
            if block.start not in rows:
                rows[block.start] = (
                    block,
                    np.zeros((1, block.stop - block.start)),
                )
            rows[block.start][1][0, index] += coefficient
        out = self.allocate(1)
        self.update_from_propagation(
            out,
            [block for block, _ in rows.values()],
            [row for _, row in rows.values()],
        )
        self._lineage_combinations[key] = (out, 1.0)
        if len(self._lineage_combinations) > _LINEAGE_CACHE:
            self._lineage_combinations.popitem(last=False)
        return out, 1.0

    def register_independent_array(self, std_dev: Numeric) -> slice:
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


class PhysureContext:
    """Context manager providing a scoped CovarianceStore."""

    def __init__(
        self,
        backend_type: str = "numpy",
        pruning_config: PruningConfig | None = None,
    ) -> None:
        self.backend_type = backend_type
        self.pruning_config = pruning_config
        self.token = None

    def __enter__(self) -> CovarianceStore:
        # Use default config if none provided
        config = self.pruning_config or PruningConfig()
        # backend is assigned lazily by ensure_store() on first real use.
        store = CovarianceStore(backend=None, config=config)
        self.token = _current_store.set(store)
        return store

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        # ponytail: self.token is None only if __enter__ never ran; the
        # context-manager protocol guarantees __exit__ only runs after it.
        _current_store.reset(self.token)  # pyright: ignore[reportArgumentType]


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
