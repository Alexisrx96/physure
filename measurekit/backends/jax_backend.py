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
        try:
            return isinstance(obj, Tracer)
        except NameError:
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
