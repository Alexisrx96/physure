"""JAX backend implementation for measurekit."""

from __future__ import annotations

import logging
from collections.abc import Sequence
from typing import Any

try:
    import jax
    import jax.numpy as jnp
    from jax.core import Tracer
except (ImportError, ModuleNotFoundError):
    jax = None
    jnp = None
    Tracer = None

try:
    from jaxtyping import Array, Bool, Float
except (ImportError, ModuleNotFoundError):
    from typing import Any

    Array = Any
    Bool = Any
    Float = Any


from measurekit.core.protocols import BackendOps

log = logging.getLogger(__name__)


class JaxBackend(BackendOps):
    """JAX-based implementation of BackendOps."""

    def __init__(self):
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
        Note: We return False for Tracers to bypass legacy vectorized uncertainty
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
                return str(obj.device())
            except (AttributeError, RuntimeError):
                pass
        return "cpu"

    def add(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise addition."""
        return jnp.add(x, y)

    def sub(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise subtraction."""
        return jnp.subtract(x, y)

    def mul(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise multiplication."""
        return jnp.multiply(x, y)

    def truediv(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise true division."""
        return jnp.true_divide(x, y)

    def pow(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise power."""
        return jnp.power(x, y)

    def sqrt(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise square root."""
        return jnp.sqrt(x)

    def exp(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise exponential."""
        return jnp.exp(x)

    def log(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise natural logarithm."""
        return jnp.log(x)

    def sin(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sine."""
        return jnp.sin(x)

    def cos(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise cosine."""
        return jnp.cos(x)

    def tan(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise tangent."""
        return jnp.tan(x)

    def dot(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Dot product or matrix multiplication."""
        return jnp.dot(x, y)

    def cross(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Cross product."""
        return jnp.cross(x, y)

    def abs(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise absolute value."""
        return jnp.abs(x)

    def sign(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sign."""
        return jnp.sign(x)

    def sum(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Sum of elements."""
        return jnp.sum(obj, axis=axis)

    def mean(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Mean of elements."""
        return jnp.mean(obj, axis=axis)

    def any(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if any element is True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.any(obj))

    def all(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if all elements are True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.all(obj))

    def allclose(
        self,
        a: Float[Array, ...],
        b: Float[Array, ...],
        rtol: float = 1e-5,
        atol: float = 1e-8,
    ) -> bool:
        """Checks if all elements are close. Returns False for Tracers."""
        if self._is_tracer(a) or self._is_tracer(b):
            return False
        return bool(jnp.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise equality."""
        return jnp.equal(x, y)

    def not_equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise inequality."""
        return jnp.not_equal(x, y)

    def less(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than."""
        return jnp.less(x, y)

    def less_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than or equal."""
        return jnp.less_equal(x, y)

    def greater(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than."""
        return jnp.greater(x, y)

    def greater_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than or equal."""
        return jnp.greater_equal(x, y)

    def shape(self, obj: Array) -> tuple[int, ...]:
        """Returns the shape of the array."""
        return jnp.shape(obj)

    def reshape(self, obj: Array, shape: tuple[int, ...]) -> Array:
        """Reshapes the array."""
        return jnp.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Array], axis: int = 0) -> Array:
        """Concatenates arrays."""
        return jnp.concatenate(arrays, axis=axis)

    def eye(self, n: int, format: str = "csr") -> Float[Array, "n n"]:
        """Returns an identity matrix."""
        return jnp.eye(n)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Float[Array, ...]:
        """Constructs a diagonal matrix."""
        if not diagonals:
            return jnp.zeros((0, 0))
        max_offset = max(offsets)
        min_offset = min(offsets)
        n = len(diagonals[0]) + max(0, -min_offset) + max(0, max_offset)
        res = jnp.zeros((n, n))
        for diag, offset in zip(diagonals, offsets, strict=False):
            res = res + jnp.diag(diag, k=offset)
        return res

    def ones(self, shape: tuple[int, ...]) -> Float[Array, ...]:
        """Returns an array of ones."""
        return jnp.ones(shape)

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        return jnp.size(obj)

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and returns them as flattened 1D arrays."""
        broadcasted = jnp.broadcast_arrays(*inputs)
        return [jnp.ravel(b) for b in broadcasted]

    def identity_operator(self, size: int) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        # JAX sparse support is experimental/limited, using dense for now as previous implementation did
        return jnp.eye(size)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
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
        """Constructs a sparse matrix from a block matrix of other matrices."""
        if self._has_sparse():
            from jax.experimental import sparse

            all_data = []
            all_indices = []
            row_offsets = [0]
            col_offsets = [0]

            # Calculate offsets
            for row in blocks:
                h = 0
                for b in row:
                    if b is not None:
                        h = b.shape[0]
                        break
                row_offsets.append(row_offsets[-1] + h)

            for j in range(len(blocks[0])):
                w = 0
                for row in blocks:
                    if row[j] is not None:
                        w = row[j].shape[1]
                        break
                col_offsets.append(col_offsets[-1] + w)

            final_shape = (row_offsets[-1], col_offsets[-1])

            for i, row in enumerate(blocks):
                for j, b in enumerate(row):
                    if b is not None:
                        # Extract COO data from b
                        if isinstance(b, sparse.BCOO):
                            all_data.append(b.data)
                            indices = b.indices
                            offset = jnp.array(
                                [row_offsets[i], col_offsets[j]]
                            )
                            all_indices.append(indices + offset)
                        else:
                            # Dense block
                            v_b = jnp.asarray(b)
                            mask = v_b != 0
                            indices = jnp.argwhere(mask)
                            all_data.append(v_b[mask])
                            offset = jnp.array(
                                [row_offsets[i], col_offsets[j]]
                            )
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

        # Convert None to zero matrices for dense fallback
        processed_blocks = []
        for i, row in enumerate(blocks):
            processed_row = []
            for j, b in enumerate(row):
                if b is None:
                    # Determine shape of the zero block
                    height = 0
                    for other_b in row:
                        if other_b is not None:
                            height = other_b.shape[0]
                            break
                    width = 0
                    for other_row in blocks:
                        if other_row[j] is not None:
                            width = other_row[j].shape[1]
                            break
                    processed_row.append(jnp.zeros((height, width)))
                else:
                    processed_row.append(b)
            processed_blocks.append(processed_row)

        return jnp.block(processed_blocks)

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Performs matrix multiplication where at least one operand may be sparse."""
        return jnp.matmul(a, b)

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        return jnp.diagonal(a)

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of an array or matrix."""
        return jnp.transpose(a)


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

        from measurekit.domain.measurement.uncertainty import Uncertainty

        def unc_flatten(unc):
            keys = tuple(sorted(unc.lineage.keys()))
            values = tuple(unc.lineage[k] for k in keys)
            return (unc.std_dev, values), (unc.vector_slice, keys)

        def unc_unflatten(aux, children):
            vector_slice, keys = aux
            std_dev, values = children
            lineage = dict(zip(keys, values))
            return Uncertainty(
                std_dev=std_dev, lineage=lineage, vector_slice=vector_slice
            )

        jax.tree_util.register_pytree_node(
            Uncertainty,
            unc_flatten,
            unc_unflatten,
        )

    except (ImportError, NameError):
        pass
