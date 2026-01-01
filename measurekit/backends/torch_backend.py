"""Torch backend implementation for measurekit."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

import torch

from measurekit.core.protocols import BackendOps


class TorchBackend(BackendOps):
    """PyTorch-based implementation of BackendOps."""

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a torch Tensor."""
        return isinstance(obj, torch.Tensor)

    def asarray(self, obj: Any) -> torch.Tensor:
        """Converts input to a torch Tensor."""
        if isinstance(obj, torch.Tensor):
            return obj
        return torch.as_tensor(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves a tensor to a specified device."""
        if isinstance(obj, torch.Tensor):
            return obj.to(device)
        return obj

    def add(self, x: Any, y: Any) -> Any:
        """Element-wise addition."""
        return torch.add(self.asarray(x), self.asarray(y))

    def sub(self, x: Any, y: Any) -> Any:
        """Element-wise subtraction."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.subtract(x_t, y_t)
            if hasattr(torch, "subtract")
            else torch.sub(x_t, y_t)
        )

    def mul(self, x: Any, y: Any) -> Any:
        """Element-wise multiplication."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.multiply(x_t, y_t)
            if hasattr(torch, "multiply")
            else torch.mul(x_t, y_t)
        )

    def truediv(self, x: Any, y: Any) -> Any:
        """Element-wise true division."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.true_divide(x_t, y_t)
            if hasattr(torch, "true_divide")
            else torch.div(x_t, y_t)
        )

    def pow(self, x: Any, y: Any) -> Any:
        """Element-wise power."""
        return torch.pow(self.asarray(x), self.asarray(y))

    def sqrt(self, x: Any) -> Any:
        """Element-wise square root."""
        return torch.sqrt(self.asarray(x))

    def exp(self, x: Any) -> Any:
        """Element-wise exponential."""
        return torch.exp(x)

    def log(self, x: Any) -> Any:
        """Element-wise natural logarithm."""
        return torch.log(x)

    def sin(self, x: Any) -> Any:
        """Element-wise sine."""
        return torch.sin(x)

    def cos(self, x: Any) -> Any:
        """Element-wise cosine."""
        return torch.cos(x)

    def tan(self, x: Any) -> Any:
        """Element-wise tangent."""
        return torch.tan(x)

    def dot(self, x: Any, y: Any) -> Any:
        """Dot product or matrix multiplication."""
        if (
            hasattr(x, "ndim")
            and x.ndim == 1
            and hasattr(y, "ndim")
            and y.ndim == 1
        ):
            return torch.dot(x, y)
        return torch.matmul(x, y)

    def cross(self, x: Any, y: Any) -> Any:
        """Cross product."""
        return torch.cross(x, y)

    def abs(self, x: Any) -> Any:
        """Element-wise absolute value."""
        return torch.abs(x)

    def sign(self, x: Any) -> Any:
        """Element-wise sign."""
        return torch.sign(x)

    def sum(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Sum of elements."""
        if axis is None:
            return torch.sum(obj)
        return torch.sum(obj, dim=axis)

    def mean(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Mean of elements."""
        if axis is None:
            return torch.mean(obj)
        return torch.mean(obj, dim=axis)

    def any(self, obj: Any) -> bool:
        """Returns True if any element is True."""
        return bool(torch.any(obj))

    def all(self, obj: Any) -> bool:
        """Returns True if all elements are True."""
        return bool(torch.all(obj))

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Checks if all elements are close."""
        return bool(torch.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Any:
        """Element-wise equality."""
        return torch.eq(x, y)

    def not_equal(self, x: Any, y: Any) -> Any:
        """Element-wise inequality."""
        return torch.ne(x, y)

    def less(self, x: Any, y: Any) -> Any:
        """Element-wise less than."""
        return torch.lt(x, y)

    def less_equal(self, x: Any, y: Any) -> Any:
        """Element-wise less than or equal."""
        return torch.le(x, y)

    def greater(self, x: Any, y: Any) -> Any:
        """Element-wise greater than."""
        return torch.gt(x, y)

    def greater_equal(self, x: Any, y: Any) -> Any:
        """Element-wise greater than or equal."""
        return torch.ge(x, y)

    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the tensor."""
        return tuple(obj.shape)

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the tensor."""
        return torch.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates tensors."""
        return torch.cat(arrays, dim=axis)

    def eye(self, N: int, format: str = "csr") -> Any:
        """Returns an identity matrix."""
        return torch.eye(N)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        """Constructs a diagonal matrix."""
        if len(diagonals) == 1:
            return torch.diag(diagonals[0], diagonal=offsets[0])

        res = torch.diag(diagonals[0], diagonal=offsets[0])
        for i in range(1, len(diagonals)):
            res = res + torch.diag(diagonals[i], diagonal=offsets[i])
        return res

    def ones(self, shape: tuple[int, ...]) -> Any:
        """Returns a tensor of ones."""
        return torch.ones(shape)
