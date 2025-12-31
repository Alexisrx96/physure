from __future__ import annotations

from collections.abc import Sequence
from typing import Any, Union

import numpy as np
from scipy import sparse

from measurekit.core.protocols import BackendOps


class NumpyBackend(BackendOps):
    """NumPy-based implementation of BackendOps."""

    def is_array(self, obj: Any) -> bool:
        return isinstance(obj, np.ndarray)

    def asarray(self, obj: Any) -> np.ndarray:
        return np.asarray(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        return obj

    def add(self, x: Any, y: Any) -> Any:
        return np.add(x, y)

    def sub(self, x: Any, y: Any) -> Any:
        return np.subtract(x, y)

    def mul(self, x: Any, y: Any) -> Any:
        return np.multiply(x, y)

    def truediv(self, x: Any, y: Any) -> Any:
        return np.true_divide(x, y)

    def pow(self, x: Any, y: Any) -> Any:
        return np.power(x, y)

    def sqrt(self, x: Any) -> Any:
        return np.sqrt(x)

    def exp(self, x: Any) -> Any:
        return np.exp(x)

    def log(self, x: Any) -> Any:
        return np.log(x)

    def sin(self, x: Any) -> Any:
        return np.sin(x)

    def cos(self, x: Any) -> Any:
        return np.cos(x)

    def tan(self, x: Any) -> Any:
        return np.tan(x)

    def dot(self, x: Any, y: Any) -> Any:
        return np.dot(x, y)

    def cross(self, x: Any, y: Any) -> Any:
        return np.cross(x, y)

    def abs(self, x: Any) -> Any:
        return np.abs(x)

    def sign(self, x: Any) -> Any:
        return np.sign(x)

    def sum(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        return np.sum(obj, axis=axis)

    def mean(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        return np.mean(obj, axis=axis)

    def any(self, obj: Any) -> bool:
        return bool(np.any(obj))

    def all(self, obj: Any) -> bool:
        return bool(np.all(obj))

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        return bool(np.allclose(a, b, rtol=rtol, atol=atol))

    def equal(self, x: Any, y: Any) -> Any:
        return np.equal(x, y)

    def not_equal(self, x: Any, y: Any) -> Any:
        return np.not_equal(x, y)

    def less(self, x: Any, y: Any) -> Any:
        return np.less(x, y)

    def less_equal(self, x: Any, y: Any) -> Any:
        return np.less_equal(x, y)

    def greater(self, x: Any, y: Any) -> Any:
        return np.greater(x, y)

    def greater_equal(self, x: Any, y: Any) -> Any:
        return np.greater_equal(x, y)

    def shape(self, obj: Any) -> tuple[int, ...]:
        if hasattr(obj, "shape"):
            return obj.shape
        return np.shape(obj)

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        return np.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        return np.concatenate(arrays, axis=axis)

    def eye(self, N: int, format: str = "csr") -> Any:
        return sparse.eye(N, format=format)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        return sparse.diags(
            diagonals=diagonals, offsets=offsets, format=format
        )

    def ones(self, shape: tuple[int, ...]) -> Any:
        return np.ones(shape)
