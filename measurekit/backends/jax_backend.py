"""JAX backend implementation for measurekit."""

from __future__ import annotations

import logging
from collections.abc import Sequence
from typing import Any

import jax
import jax.numpy as jnp
from jax.core import Tracer

from measurekit.core.protocols import BackendOps

log = logging.getLogger(__name__)


class JaxBackend(BackendOps):
    """JAX-based implementation of BackendOps."""

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

    def asarray(self, obj: Any) -> jax.Array:
        """Converts input to a JAX array."""
        return jnp.asarray(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves a JAX array to a specified device."""
        if self.is_array(obj) and not self._is_tracer(obj):
            try:
                for d in jax.devices():
                    if str(d) == device:
                        return jax.device_put(obj, d)
            except Exception as e:
                log.debug(f"Failed to move JAX array to device {device}: {e}")
        return obj

    def add(self, x: Any, y: Any) -> Any:
        """Element-wise addition."""
        return jnp.add(x, y)

    def sub(self, x: Any, y: Any) -> Any:
        """Element-wise subtraction."""
        return jnp.subtract(x, y)

    def mul(self, x: Any, y: Any) -> Any:
        """Element-wise multiplication."""
        return jnp.multiply(x, y)

    def truediv(self, x: Any, y: Any) -> Any:
        """Element-wise true division."""
        return jnp.true_divide(x, y)

    def pow(self, x: Any, y: Any) -> Any:
        """Element-wise power."""
        return jnp.power(x, y)

    def sqrt(self, x: Any) -> Any:
        """Element-wise square root."""
        return jnp.sqrt(x)

    def exp(self, x: Any) -> Any:
        """Element-wise exponential."""
        return jnp.exp(x)

    def log(self, x: Any) -> Any:
        """Element-wise natural logarithm."""
        return jnp.log(x)

    def sin(self, x: Any) -> Any:
        """Element-wise sine."""
        return jnp.sin(x)

    def cos(self, x: Any) -> Any:
        """Element-wise cosine."""
        return jnp.cos(x)

    def tan(self, x: Any) -> Any:
        """Element-wise tangent."""
        return jnp.tan(x)

    def dot(self, x: Any, y: Any) -> Any:
        """Dot product or matrix multiplication."""
        return jnp.dot(x, y)

    def cross(self, x: Any, y: Any) -> Any:
        """Cross product."""
        return jnp.cross(x, y)

    def abs(self, x: Any) -> Any:
        """Element-wise absolute value."""
        return jnp.abs(x)

    def sign(self, x: Any) -> Any:
        """Element-wise sign."""
        return jnp.sign(x)

    def sum(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Sum of elements."""
        return jnp.sum(obj, axis=axis)

    def mean(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Mean of elements."""
        return jnp.mean(obj, axis=axis)

    def any(self, obj: Any) -> bool:
        """Returns True if any element is True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.any(obj))

    def all(self, obj: Any) -> bool:
        """Returns True if all elements are True. Returns False for Tracers."""
        if self._is_tracer(obj):
            return False
        return bool(jnp.all(obj))

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Checks if all elements are close. Returns False for Tracers."""
        if self._is_tracer(a) or self._is_tracer(b):
            return False
        return bool(jnp.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Any:
        """Element-wise equality."""
        return jnp.equal(x, y)

    def not_equal(self, x: Any, y: Any) -> Any:
        """Element-wise inequality."""
        return jnp.not_equal(x, y)

    def less(self, x: Any, y: Any) -> Any:
        """Element-wise less than."""
        return jnp.less(x, y)

    def less_equal(self, x: Any, y: Any) -> Any:
        """Element-wise less than or equal."""
        return jnp.less_equal(x, y)

    def greater(self, x: Any, y: Any) -> Any:
        """Element-wise greater than."""
        return jnp.greater(x, y)

    def greater_equal(self, x: Any, y: Any) -> Any:
        """Element-wise greater than or equal."""
        return jnp.greater_equal(x, y)

    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the array."""
        return jnp.shape(obj)

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the array."""
        return jnp.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates arrays."""
        return jnp.concatenate(arrays, axis=axis)

    def eye(self, n: int, format: str = "csr") -> Any:
        """Returns an identity matrix."""
        return jnp.eye(n)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
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

    def ones(self, shape: tuple[int, ...]) -> Any:
        """Returns an array of ones."""
        return jnp.ones(shape)


def register_jax_behavior():
    """Registers Quantity as a JAX Pytree."""
    try:
        import jax

        def flatten_quantity(q: Any):
            """Flattens a Quantity into children and aux_data."""
            # Per Phase 3 requirement: magnitude is child, unit is aux_data
            return (q.magnitude,), q.unit

        def unflatten_quantity_base(cls, aux_data, children):
            """Reconstructs a specific Quantity class."""
            return cls(children[0], aux_data)

        # Register Quantity from both locations to ensure compatibility
        # during Core Decoupling (Phase 1/2/3).
        from measurekit.domain.measurement.quantity import (
            Quantity as DomainQuantity,
        )

        jax.tree_util.register_pytree_node(
            DomainQuantity,
            flatten_quantity,
            lambda aux, children: unflatten_quantity_base(
                DomainQuantity, aux, children
            ),
        )

        try:
            from measurekit.core.quantity import (
                Quantity as CoreQuantity,
            )

            if CoreQuantity is not DomainQuantity:
                jax.tree_util.register_pytree_node(
                    CoreQuantity,
                    flatten_quantity,
                    lambda aux, children: unflatten_quantity_base(
                        CoreQuantity, aux, children
                    ),
                )
        except ImportError:
            pass

    except (ImportError, NameError):
        pass
