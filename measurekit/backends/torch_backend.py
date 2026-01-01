from __future__ import annotations

import logging
from collections.abc import Sequence
from typing import Any

import torch
from jaxtyping import Array, Bool, Float

from measurekit.core.protocols import BackendOps

log = logging.getLogger(__name__)


class TorchBackend(BackendOps):
    """PyTorch-based implementation of BackendOps."""

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a torch Tensor."""
        return isinstance(obj, torch.Tensor)

    def asarray(self, obj: Any) -> Array:
        """Converts input to a torch Tensor."""
        if isinstance(obj, torch.Tensor):
            return obj
        return torch.as_tensor(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves a tensor to a specified device."""
        if isinstance(obj, torch.Tensor):
            return obj.to(device)
        return obj

    def add(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise addition."""
        return torch.add(self.asarray(x), self.asarray(y))

    def sub(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise subtraction."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.subtract(x_t, y_t)
            if hasattr(torch, "subtract")
            else torch.sub(x_t, y_t)
        )

    def mul(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise multiplication."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.multiply(x_t, y_t)
            if hasattr(torch, "multiply")
            else torch.mul(x_t, y_t)
        )

    def truediv(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise true division."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.true_divide(x_t, y_t)
            if hasattr(torch, "true_divide")
            else torch.div(x_t, y_t)
        )

    def pow(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Element-wise power."""
        return torch.pow(self.asarray(x), self.asarray(y))

    def sqrt(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise square root."""
        return torch.sqrt(self.asarray(x))

    def exp(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise exponential."""
        return torch.exp(x)

    def log(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise natural logarithm."""
        return torch.log(x)

    def sin(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sine."""
        return torch.sin(x)

    def cos(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise cosine."""
        return torch.cos(x)

    def tan(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise tangent."""
        return torch.tan(x)

    def dot(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Dot product or matrix multiplication."""
        if (
            hasattr(x, "ndim")
            and x.ndim == 1
            and hasattr(y, "ndim")
            and y.ndim == 1
        ):
            return torch.dot(x, y)
        return torch.matmul(x, y)

    def cross(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Cross product."""
        return torch.cross(x, y)

    def abs(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise absolute value."""
        return torch.abs(x)

    def sign(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Element-wise sign."""
        return torch.sign(x)

    def sum(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Sum of elements."""
        if axis is None:
            return torch.sum(obj)
        return torch.sum(obj, dim=axis)

    def mean(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Mean of elements."""
        if axis is None:
            return torch.mean(obj)
        return torch.mean(obj, dim=axis)

    def any(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if any element is True."""
        return bool(torch.any(obj))

    def all(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if all elements are True."""
        return bool(torch.all(obj))

    def allclose(
        self,
        a: Float[Array, ...],
        b: Float[Array, ...],
        rtol: float = 1e-5,
        atol: float = 1e-8,
    ) -> bool:
        """Checks if all elements are close."""
        return bool(torch.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise equality."""
        return torch.eq(x, y)

    def not_equal(self, x: Any, y: Any) -> Bool[Array, ...]:
        """Element-wise inequality."""
        return torch.ne(x, y)

    def less(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than."""
        return torch.lt(x, y)

    def less_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than or equal."""
        return torch.le(x, y)

    def greater(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than."""
        return torch.gt(x, y)

    def greater_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than or equal."""
        return torch.ge(x, y)

    def shape(self, obj: Array) -> tuple[int, ...]:
        """Returns the shape of the tensor."""
        return tuple(obj.shape)

    def reshape(self, obj: Array, shape: tuple[int, ...]) -> Array:
        """Reshapes the tensor."""
        return torch.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Array], axis: int = 0) -> Array:
        """Concatenates tensors."""
        return torch.cat(arrays, dim=axis)

    def eye(self, N: int, format: str = "csr") -> Float[Array, "N N"]:
        """Returns an identity matrix."""
        return torch.eye(N)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Float[Array, ...]:
        """Constructs a diagonal matrix."""
        if len(diagonals) == 1:
            return torch.diag(diagonals[0], diagonal=offsets[0])

        res = torch.diag(diagonals[0], diagonal=offsets[0])
        for i in range(1, len(diagonals)):
            res = res + torch.diag(diagonals[i], diagonal=offsets[i])
        return res

    def ones(self, shape: tuple[int, ...]) -> Float[Array, ...]:
        """Returns a tensor of ones."""
        return torch.ones(shape)

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        if hasattr(obj, "numel"):
            return obj.numel()
        return 1

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and returns them as flattened 1D arrays."""
        tensors = [self.asarray(x) for x in inputs]
        broadcasted = torch.broadcast_tensors(*tensors)
        return [torch.flatten(b) for b in broadcasted]

    def identity_operator(self, size: int) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        return torch.eye(size)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
        return torch.diag(diagonal)
