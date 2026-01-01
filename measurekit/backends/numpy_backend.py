from __future__ import annotations

import logging
from collections.abc import Sequence
from typing import Any

import numpy as np
from jaxtyping import Array, Bool, Float
from scipy import sparse

from measurekit.core.protocols import BackendOps

log = logging.getLogger(__name__)


class NumpyBackend(BackendOps):
    """NumPy-based implementation of BackendOps."""

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a NumPy array."""
        return isinstance(obj, np.ndarray)

    def asarray(self, obj: Any) -> Array:
        """Converts input to a NumPy array."""
        return np.asarray(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """No-op for NumPy backend."""
        return obj

    def add(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise addition."""
        return np.add(x, y)

    def sub(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise subtraction."""
        return np.subtract(x, y)

    def mul(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise multiplication."""
        return np.multiply(x, y)

    def truediv(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise true division."""
        return np.true_divide(x, y)

    def pow(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise power."""
        return np.power(x, y)

    def sqrt(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise square root."""
        return np.sqrt(x)

    def exp(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise exponential."""
        return np.exp(x)

    def log(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise natural logarithm."""
        return np.log(x)

    def sin(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sine."""
        return np.sin(x)

    def cos(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise cosine."""
        return np.cos(x)

    def tan(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise tangent."""
        return np.tan(x)

    def dot(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Dot product or matrix multiplication."""
        return np.dot(x, y)

    def cross(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Cross product."""
        return np.cross(x, y)

    def abs(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise absolute value."""
        return np.abs(x)

    def sign(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sign."""
        return np.sign(x)

    def sum(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Sum of elements."""
        return np.sum(obj, axis=axis)

    def mean(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Mean of elements."""
        return np.mean(obj, axis=axis)

    def any(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if any element is True."""
        return bool(np.any(obj))

    def all(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if all elements are True."""
        return bool(np.all(obj))

    def allclose(
        self,
        a: Float[Array, ...],
        b: Float[Array, ...],
        rtol: float = 1e-5,
        atol: float = 1e-8,
    ) -> bool:
        """Checks if all elements are close."""
        return bool(np.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise equality."""
        return np.equal(x, y)

    def not_equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise inequality."""
        return np.not_equal(x, y)

    def less(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than."""
        return np.less(x, y)

    def less_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than or equal."""
        return np.less_equal(x, y)

    def greater(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than."""
        return np.greater(x, y)

    def greater_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than or equal."""
        return np.greater_equal(x, y)

    def shape(self, obj: Array) -> tuple[int, ...]:
        """Returns the shape of the array."""
        if hasattr(obj, "shape"):
            return obj.shape
        return np.shape(obj)

    def reshape(self, obj: Array, shape: tuple[int, ...]) -> Array:
        """Reshapes the array."""
        return np.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Array], axis: int = 0) -> Array:
        """Concatenates arrays."""
        return np.concatenate(arrays, axis=axis)

    def eye(self, N: int, format: str = "csr") -> Any:
        """Returns an identity matrix."""
        return sparse.eye(N, format=format)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        """Constructs a diagonal matrix."""
        return sparse.diags(
            diagonals=diagonals, offsets=offsets, format=format
        )

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        return np.size(obj)

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and returns them as flattened 1D arrays."""
        # np.broadcast_arrays broadcasts inputs against each other
        broadcasted = np.broadcast_arrays(*inputs)
        # Flatten each
        return [b.ravel() for b in broadcasted]

    def identity_operator(self, size: int) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        return sparse.eye(size, format="csr")

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
        return sparse.diags([diagonal], [0], format="csr")

    def ones(self, shape: tuple[int, ...]) -> Float[Array, ...]:
        """Returns an array of ones."""
        return np.ones(shape)
