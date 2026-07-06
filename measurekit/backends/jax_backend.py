"""JAX backend implementation for measurekit."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

try:
    import jax
    import jax.numpy as jnp
    from jax.core import Tracer
except ImportError:
    jax = None
    jnp = None
    Tracer = None

try:
    from jaxtyping import Array, Float
except ImportError:
    from typing import Any

    Array = Any
    Float = Any


from measurekit.core.dispatcher import enforce_tensor_contract
from measurekit.core.protocols import BackendOps, Boolean, Numeric

log = logging.getLogger(__name__)


def _jax_block_offsets(
    blocks: Sequence[Sequence[Any | None]],
) -> tuple[list[int], list[int]]:
    """Computes row and column offsets for a JAX block matrix."""
    row_offsets = [0]
    for row in blocks:
        h = next((b.shape[0] for b in row if b is not None), 0)
        row_offsets.append(row_offsets[-1] + h)

    col_offsets = [0]
    for j in range(len(blocks[0])):
        w = next((row[j].shape[1] for row in blocks if row[j] is not None), 0)
        col_offsets.append(col_offsets[-1] + w)

    return row_offsets, col_offsets


def _jax_bcoo_coo(b: Any, sparse: Any) -> tuple[Any, Any]:
    """Extracts (data, indices) from a BCOO or dense JAX block."""
    if isinstance(b, sparse.BCOO):
        return b.data, b.indices
    v_b = jnp.asarray(b)
    mask = v_b != 0
    return v_b[mask], jnp.argwhere(mask)


def _jax_sparse_bmat(
    blocks: Sequence[Sequence[Any | None]], sparse: Any
) -> Any:
    """Assembles a JAX BCOO sparse block matrix."""
    row_offsets, col_offsets = _jax_block_offsets(blocks)
    final_shape = (row_offsets[-1], col_offsets[-1])

    all_data: list = []
    all_indices: list = []
    for i, row in enumerate(blocks):
        for j, b in enumerate(row):
            if b is None:
                continue
            data, indices = _jax_bcoo_coo(b, sparse)
            offset = jnp.array([row_offsets[i], col_offsets[j]])
            all_data.append(data)
            all_indices.append(indices + offset)

    if not all_data:
        return sparse.BCOO(
            (jnp.zeros(0), jnp.zeros((0, 2), dtype=int)),
            shape=final_shape,
        )
    return sparse.BCOO(
        (jnp.concatenate(all_data), jnp.concatenate(all_indices)),
        shape=final_shape,
    )


def _jax_dense_bmat(blocks: Sequence[Sequence[Any | None]]) -> Any:
    """Assembles a dense block matrix, filling None slots with zeros."""
    processed_blocks = []
    for row in blocks:
        processed_row = []
        for j, b in enumerate(row):
            if b is None:
                height = next((x.shape[0] for x in row if x is not None), 0)
                width = next(
                    (r[j].shape[1] for r in blocks if r[j] is not None), 0
                )
                processed_row.append(jnp.zeros((height, width)))
            else:
                processed_row.append(b)
        processed_blocks.append(processed_row)
    return jnp.block(processed_blocks)


class JaxBackend(BackendOps):
    """JAX-based implementation of BackendOps."""

    def __init__(self):
        """Initializes the JAX backend."""
        if jax is None:
            raise ImportError("JAX is not available.")

    def _is_tracer(self, obj: Any) -> bool:
        """Returns True if the object is a JAX Tracer (used in JIT)."""
        if Tracer is not None and isinstance(obj, Tracer):
            return True

        # Concrete JAX arrays also have 'aval', so satisfy check carefully.
        # If it is a concrete array, it is NOT a tracer.
        if jax is not None and isinstance(obj, jax.Array):
            return False

        # Fallback: check class hierarchy names for 'Tracer'
        try:
            for cls in type(obj).__mro__:
                if "Tracer" in cls.__name__:
                    return True
        except (AttributeError, TypeError):
            pass
        return False

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a concrete JAX array.

        Note: We return False for Tracers to bypass legacy paths that rely on
        propagation paths that rely on Scipy/NumPy, as JAX handles its own
        vectorization and differentiation.
        """
        try:
            if self._is_tracer(obj):
                return False
            return isinstance(obj, jax.Array)
        except (NameError, AttributeError):
            return False

    def is_tracing(self, obj: Any) -> bool:
        """Returns True if the object is a JAX Tracer."""
        return self._is_tracer(obj)

    def asarray(self, obj: Any) -> Array:
        """Converts input to a JAX array."""
        return jnp.asarray(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        # Implementation remains the same but signatures are updated
        """Moves a JAX array to a specified device."""
        if self.is_array(obj) and not self._is_tracer(obj):
            try:
                for d in jax.devices():
                    if str(d) == device:
                        return jax.device_put(obj, d)
            except Exception as e:
                log.debug(f"Failed to move JAX array to device {device}: {e}")
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns the device identifier for a JAX array."""
        if self.is_array(obj) and not self._is_tracer(obj):
            try:
                # JAX array device access varies by version.
                # We check for .device attribute or method.
                d = getattr(obj, "device", None)
                if callable(d):
                    return str(d())
                return str(d)
            except (AttributeError, RuntimeError):
                pass
        return "cpu"

    @enforce_tensor_contract
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise addition."""
        if self._has_sparse():
            from jax.experimental import sparse

            if isinstance(x, sparse.BCOO) or isinstance(y, sparse.BCOO):
                return x + y  # pyright: ignore[reportOperatorIssue]
        return jnp.add(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise subtraction."""
        return jnp.subtract(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise multiplication."""
        return jnp.multiply(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise true division."""
        return jnp.true_divide(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise power."""
        return jnp.power(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def sqrt(self, x: Numeric) -> Numeric:
        """Element-wise square root."""
        return jnp.sqrt(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def exp(self, x: Numeric) -> Numeric:
        """Element-wise exponential."""
        return jnp.exp(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def log(self, x: Numeric) -> Numeric:
        """Element-wise natural logarithm."""
        return jnp.log(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def sin(self, x: Numeric) -> Numeric:
        """Element-wise sine."""
        return jnp.sin(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def cos(self, x: Numeric) -> Numeric:
        """Element-wise cosine."""
        return jnp.cos(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def tan(self, x: Numeric) -> Numeric:
        """Element-wise tangent."""
        return jnp.tan(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def dot(self, x: Numeric, y: Numeric) -> Numeric:
        """Dot product or matrix multiplication."""
        # Fix for 0D arrays in JAX matmul
        if self.shape(x) == () or self.shape(y) == ():
            return self.mul(x, y)
        return jnp.matmul(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def cross(self, x: Numeric, y: Numeric) -> Numeric:
        """Cross product."""
        return jnp.cross(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def abs(self, x: Numeric) -> Numeric:
        """Element-wise absolute value."""
        return jnp.abs(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def sign(self, x: Numeric) -> Numeric:
        """Element-wise sign."""
        return jnp.sign(x)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Sum of elements."""
        if self._has_sparse():
            from jax.experimental import sparse

            if isinstance(obj, sparse.BCOO):
                return obj.sum(
                    axis=axis  # pyright: ignore[reportCallIssue]
                )
        return jnp.sum(obj, axis=axis)

    @enforce_tensor_contract
    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Mean of elements."""
        return jnp.mean(obj, axis=axis)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.any(obj))  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.all(obj))  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def allclose(
        self,
        a: Numeric,
        b: Numeric,
        rtol: float = 1e-5,
        atol: float = 1e-8,
    ) -> bool:
        """Checks if all elements are close. Returns False for Tracers."""
        if self._is_tracer(a) or self._is_tracer(b):
            return False
        return bool(
            jnp.allclose(
                a,  # pyright: ignore[reportArgumentType]
                b,  # pyright: ignore[reportArgumentType]
                rtol=rtol,
                atol=atol,
            )
        )

    @enforce_tensor_contract
    def equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise equality."""
        return jnp.equal(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def not_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise inequality."""
        return jnp.not_equal(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def less(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than."""
        return jnp.less(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def less_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than or equal."""
        return jnp.less_equal(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def greater(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise greater than."""
        return jnp.greater(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def greater_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise greater than or equal."""
        return jnp.greater_equal(x, y)  # pyright: ignore[reportArgumentType]

    @enforce_tensor_contract
    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the array or tracer."""
        if hasattr(obj, "shape"):
            return obj.shape
        return jnp.shape(obj)

    @enforce_tensor_contract
    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the array."""
        if hasattr(obj, "reshape"):
            return obj.reshape(shape)
        return jnp.reshape(obj, shape)

    @enforce_tensor_contract
    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates arrays."""
        return jnp.concatenate(arrays, axis=axis)

    def eye(
        self,
        n: int,
        format: str = "csr",  # pyright: ignore[reportUnusedParameter]
        reference: Any = None,
    ) -> Any:
        """Returns an identity matrix.

        format is kept for parity with numpy_backend's signature; JAX has
        no dense/sparse format switch here.
        """
        dtype = getattr(reference, "dtype", None)
        return jnp.eye(n, dtype=dtype)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",  # pyright: ignore[reportUnusedParameter]
    ) -> Float[Array, ...]:
        """Constructs a diagonal matrix.

        format is kept for parity with numpy_backend's signature; JAX
        has no dense/sparse format switch here.
        """
        if not diagonals:
            return jnp.zeros((0, 0))
        max_offset = max(offsets)
        min_offset = min(offsets)
        n = len(diagonals[0]) + max(0, -min_offset) + max(0, max_offset)
        res = jnp.zeros((n, n))
        for diag, offset in zip(diagonals, offsets, strict=False):
            res = res + jnp.diag(diag, k=offset)
        return res

    @enforce_tensor_contract
    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Numeric:
        """Returns an array of ones."""
        dtype = getattr(reference, "dtype", None)
        return jnp.ones(shape, dtype=dtype)

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        return jnp.size(obj)

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape; returns 1D arrays."""
        broadcasted = jnp.broadcast_arrays(*inputs)
        return [jnp.ravel(b) for b in broadcasted]

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        # JAX sparse support is experimental; using dense fallback.
        dtype = getattr(reference, "dtype", None)
        return jnp.eye(size, dtype=dtype)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given values."""
        return jnp.diag(diagonal)

    def _has_sparse(self) -> bool:
        try:
            from jax.experimental import sparse

            return hasattr(sparse, "BCOO")
        except ImportError:
            return False

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data.

        Using JAX BCOO if available, otherwise falling back to dense.
        """
        if self._has_sparse():
            from jax.experimental import sparse

            return sparse.BCOO((data, jnp.stack(indices, axis=1)), shape=shape)
        res = jnp.zeros(shape)
        return res.at[indices].set(data)

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        if not diagonals:
            return jnp.zeros((0, 0))

        if shape is None:
            max_offset = max(offsets)
            min_offset = min(offsets)
            n = len(diagonals[0]) + max(0, -min_offset) + max(0, max_offset)
            shape = (n, n)

        if self._has_sparse():
            from jax.experimental import sparse

            all_data = []
            all_indices = []
            for diag, offset in zip(diagonals, offsets, strict=False):
                d = jnp.asarray(diag)
                n_elements = d.shape[0]
                row = jnp.arange(n_elements) + max(0, -offset)
                col = jnp.arange(n_elements) + max(0, offset)
                all_data.append(d)
                all_indices.append(jnp.stack([row, col], axis=1))

            data = jnp.concatenate(all_data)
            indices = jnp.concatenate(all_indices)
            return sparse.BCOO((data, indices), shape=shape)

        res = jnp.zeros(shape)
        for diag, offset in zip(diagonals, offsets, strict=False):
            res = res + jnp.diag(diag, k=offset)
        return res

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix."""
        if self._has_sparse():
            from jax.experimental import sparse

            return _jax_sparse_bmat(blocks, sparse)

        return _jax_dense_bmat(blocks)

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matrix multiplication where at least one operand is sparse."""
        return a @ b

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix of size n."""
        dtype = getattr(reference, "dtype", None)
        if self._has_sparse():
            from jax.experimental import sparse

            if hasattr(sparse, "eye"):
                return sparse.eye(n, dtype=dtype)

            indices = jnp.stack([jnp.arange(n), jnp.arange(n)], axis=1)
            data = jnp.ones(n, dtype=dtype)
            return sparse.BCOO((data, indices), shape=(n, n))
        return jnp.eye(n, dtype=dtype)

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        if hasattr(a, "indices") and hasattr(a, "data"):
            # JAX BCOO support
            try:
                # Assuming 2D matrix
                if getattr(a, "ndim", 0) == 2:
                    idxs = a.indices
                    data = a.data
                    mask = idxs[:, 0] == idxs[:, 1]

                    diag_idxs = idxs[mask, 0]
                    diag_vals = data[mask]

                    n = min(a.shape)
                    res = jnp.zeros((n,), dtype=a.dtype)
                    # Use .add() to handle uncoalesced duplicates
                    res = res.at[diag_idxs].add(diag_vals)
                    return res
            except Exception:
                # Fallback to dense if something exotic happens
                pass

            if hasattr(a, "todense"):
                return jnp.diagonal(a.todense())

        return jnp.diagonal(a)

    def to_coo(self, a: Any) -> tuple[Any, Any, Any] | None:
        """Extracts (data, rows, cols) from a sparse matrix."""
        # Check for BCOO
        if hasattr(a, "indices") and hasattr(a, "data"):
            try:
                indices = a.indices
                # JAX BCOO properties might be tracers.
                # If indices is a property returning array:
                if hasattr(indices, "shape"):  # It is array-like
                    rows = indices[:, 0]
                    cols = indices[:, 1]
                    return a.data, rows, cols
            except Exception:
                pass

        # Check for other JAX sparse types or fallbacks?
        return None

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of an array or matrix."""
        if hasattr(a, "T"):
            return a.T
        return jnp.transpose(a)

    def sparse_slice(
        self, matrix: Any, row_slice: slice, col_slice: slice
    ) -> Any:
        """Slices a sparse matrix (or dense fallback)."""
        if matrix is None:
            # Determine shapes? This is hard without more info.
            # But propagate_affine should probably handle this.
            return None

        # JAX BCOO slicing returns BCOO
        return matrix[row_slice, col_slice]


_jax_registered = False


def register_jax_behavior():
    """Registers Quantity as a JAX Pytree."""
    global _jax_registered
    if _jax_registered:
        return
    _jax_registered = True
    try:
        import jax

        # Register Quantity
        from measurekit.domain.measurement.quantity import Quantity

        jax.tree_util.register_pytree_node(
            Quantity,
            Quantity.tree_flatten,
            Quantity.tree_unflatten,
        )

        from measurekit.domain.measurement.uncertainty import (
            CovarianceModel,
            VarianceModel,
        )

        def cov_flatten(m):
            keys = tuple(sorted(m.lineage.keys()))
            values = tuple(m.lineage[k] for k in keys)
            return (m.std_dev_internal, values), (m.vector_slice, keys)

        def cov_unflatten(aux, children):
            vector_slice, keys = aux
            std_dev, values = children
            lineage = dict(zip(keys, values, strict=False))
            return CovarianceModel(
                std_dev_internal=std_dev,
                lineage=lineage,
                vector_slice=vector_slice,
            )

        jax.tree_util.register_pytree_node(
            CovarianceModel,
            cov_flatten,
            cov_unflatten,
        )

        def var_flatten(m):
            return (m.variance,), ()

        def var_unflatten(_aux, children):
            return VarianceModel(variance=children[0])

        jax.tree_util.register_pytree_node(
            VarianceModel,
            var_flatten,
            var_unflatten,
        )

    except (ImportError, NameError):
        pass
