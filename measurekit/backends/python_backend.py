"""Fallback backend for native Python scalars/lists using the math module."""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, Any

from measurekit.core.dispatcher import enforce_tensor_contract
from measurekit.core.protocols import BackendOps, Boolean, Numeric

if TYPE_CHECKING:
    from collections.abc import Sequence

    try:
        from jaxtyping import Array
    except ImportError:
        Array = Any
else:
    Array = Any


class PythonBackend(BackendOps):
    """Fallback backend for native Python types using the math module."""

    SPARSE_NOT_SUPPORTED = "Sparse matrices not supported in PythonBackend"

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is an array."""
        return False

    def is_tracing(self, obj: Any) -> bool:
        """Checks if the object is being traced (always False here)."""
        return False

    def asarray(self, obj: Any) -> Array:
        """Converts to a form suitable for this backend (identity)."""
        return obj

    def to_device(self, obj: Any, device: str) -> Array:
        """Moves the object to the specified device (identity)."""
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns the device of the object (always 'cpu')."""
        return "cpu"

    @enforce_tensor_contract
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Adds two values (or lists element-wise)."""
        # ponytail: the Numeric union has no shared arithmetic protocol
        # (mirrors ty's unsupported-operator/invalid-return-type overrides
        # for this file), so every branch below needs a targeted ignore.
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a + b for a, b in zip(x, y, strict=False)]  # pyright: ignore[reportReturnType]
        if isinstance(x, (list, tuple)):
            return [a + y for a in x]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        if isinstance(y, (list, tuple)):
            return [x + b for b in y]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        return x + y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Subtracts two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a - b for a, b in zip(x, y, strict=False)]  # pyright: ignore[reportReturnType]
        if isinstance(x, (list, tuple)):
            return [a - y for a in x]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        if isinstance(y, (list, tuple)):
            return [x - b for b in y]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        return x - y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Multiplies two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a * b for a, b in zip(x, y, strict=False)]  # pyright: ignore[reportReturnType]
        if isinstance(x, (list, tuple)):
            return [a * y for a in x]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        if isinstance(y, (list, tuple)):
            return [x * b for b in y]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        return x * y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Divides two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a / b for a, b in zip(x, y, strict=False)]  # pyright: ignore[reportReturnType]
        if isinstance(x, (list, tuple)):
            return [a / y for a in x]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        if isinstance(y, (list, tuple)):
            return [x / b for b in y]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        return x / y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Raises x to the power of y (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a**b for a, b in zip(x, y, strict=False)]  # pyright: ignore[reportReturnType]
        if isinstance(x, (list, tuple)):
            return [a**y for a in x]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        if isinstance(y, (list, tuple)):
            return [x**b for b in y]  # pyright: ignore[reportReturnType, reportOperatorIssue]
        return x**y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def sqrt(self, x: Numeric) -> Numeric:
        """Returns the square root of x."""
        if isinstance(x, (list, tuple)):
            return [math.sqrt(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.sqrt(x)

    @enforce_tensor_contract
    def exp(self, x: Numeric) -> Numeric:
        """Returns the exponential of x."""
        if isinstance(x, (list, tuple)):
            return [math.exp(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.exp(x)

    @enforce_tensor_contract
    def log(self, x: Numeric) -> Numeric:
        """Returns the natural logarithm of x."""
        if isinstance(x, (list, tuple)):
            return [math.log(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.log(x)

    @enforce_tensor_contract
    def sin(self, x: Numeric) -> Numeric:
        """Returns the sine of x."""
        if isinstance(x, (list, tuple)):
            return [math.sin(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.sin(x)

    @enforce_tensor_contract
    def cos(self, x: Numeric) -> Numeric:
        """Returns the cosine of x."""
        if isinstance(x, (list, tuple)):
            return [math.cos(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.cos(x)

    @enforce_tensor_contract
    def tan(self, x: Numeric) -> Numeric:
        """Returns the tangent of x."""
        if isinstance(x, (list, tuple)):
            return [math.tan(val) for val in x]  # pyright: ignore[reportReturnType]
        return math.tan(x)

    @enforce_tensor_contract
    def dot(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes the dot product of two values."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            # Assuming dot product for 1D lists
            if len(x) != len(y):
                raise ValueError("Lists must have same length for dot product")
            return sum(a * b for a, b in zip(x, y, strict=False))
        return x * y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def cross(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes the cross product of two values (not supported)."""
        raise NotImplementedError("Cross product not supported for scalars")

    @enforce_tensor_contract
    def abs(self, x: Numeric) -> Numeric:
        """Returns the absolute value of x."""
        if isinstance(x, (list, tuple)):
            return [abs(val) for val in x]  # pyright: ignore[reportReturnType]
        return abs(x)

    @enforce_tensor_contract
    def sign(self, x: Numeric) -> Numeric:
        """Returns the sign of x (-1, 0, or 1)."""
        if isinstance(x, (list, tuple)):
            return [math.copysign(1, val) for val in x]  # pyright: ignore[reportReturnType]
        return math.copysign(1, x)

    @enforce_tensor_contract
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the sum of elements."""
        if isinstance(obj, (list, tuple)):
            return sum(obj)
        return obj

    @enforce_tensor_contract
    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the arithmetic mean."""
        if isinstance(obj, (list, tuple)):
            return sum(obj) / len(obj)
        return obj

    @enforce_tensor_contract
    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True."""
        if isinstance(obj, (list, tuple)):
            return any(obj)
        return bool(obj)

    @enforce_tensor_contract
    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True."""
        if isinstance(obj, (list, tuple)):
            return all(obj)
        return bool(obj)

    @enforce_tensor_contract
    def allclose(
        self, a: Numeric, b: Numeric, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if arrays are equal within a tolerance."""
        try:
            return math.isclose(a, b, rel_tol=rtol, abs_tol=atol)
        except TypeError:
            return a == b  # pyright: ignore[reportReturnType]

    @enforce_tensor_contract
    def equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x equals y."""
        return x == y

    @enforce_tensor_contract
    def not_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x does not equal y."""
        return x != y

    @enforce_tensor_contract
    def less(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is less than y."""
        return x < y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def less_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is less than or equal to y."""
        return x <= y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def greater(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is greater than y."""
        return x > y  # pyright: ignore[reportOperatorIssue]

    @enforce_tensor_contract
    def greater_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is greater than or equal to y."""
        return x >= y  # pyright: ignore[reportOperatorIssue]

    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the object."""
        if hasattr(obj, "__len__"):
            return (len(obj),)
        return ()

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the object to the specified shape."""
        if shape == () or shape == (1,):
            if isinstance(obj, (list, tuple)):
                if len(obj) == 1:
                    return obj[0]
                if len(obj) == 0:
                    return 0.0
            return obj

        if len(shape) == 1:
            return self._reshape_1d(obj, shape[0])

        if shape == (1, 1) and not isinstance(obj, (list, tuple)):
            return [[obj]]

        raise NotImplementedError(
            f"Reshape {shape} not fully supported in PythonBackend."
        )

    def _reshape_1d(self, obj: Any, total: int) -> Any:
        """Helper for 1D reshaping/flattening."""
        if isinstance(obj, (list, tuple)):
            # If nested, flatten
            if len(obj) > 0 and isinstance(obj[0], (list, tuple)):
                flat = [item for sublist in obj for item in sublist]
            else:
                flat = list(obj)

            if total == -1 or len(flat) == total:
                return flat

        # Scalar to vector
        if (total == 1 or total == -1) and not isinstance(obj, (list, tuple)):
            return [obj]
        return obj

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates arrays along the specified axis."""
        result = []
        for arr in arrays:
            if isinstance(arr, list):
                result.extend(arr)
            else:
                result.append(arr)
        return result

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        if hasattr(obj, "__len__"):
            return len(obj)
        return 1

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and flattens them."""
        # Basic scalar/list broadcasting simulation
        max_len = 0
        for x in inputs:
            max_len = (
                max(max_len, len(x))
                if isinstance(x, (list, tuple))
                else max(max_len, 1)
            )

        results = []
        for x in inputs:
            if isinstance(x, (list, tuple)):
                if len(x) == max_len:
                    results.append(list(x))
                elif len(x) == 1:
                    results.append(list(x) * max_len)
                else:
                    raise ValueError(f"Shape mismatch: {len(x)} vs {max_len}")
            else:
                results.append([x] * max_len)
        return results

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity matrix of the specified size."""
        # Python backend doesn't have devices/sparse operators really,
        # but we return an identity-like structure
        return [
            [1.0 if i == j else 0.0 for j in range(size)] for i in range(size)
        ]

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given values."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix of other matrices."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matrix multiplication where at least one operand is sparse."""
        return a @ b

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def eye(self, n: int, _format: str = "csr", _reference: Any = None) -> Any:
        """Returns a sparse identity matrix (not supported)."""
        return self.identity_operator(n)

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix (not supported)."""
        return self.identity_operator(n)

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of a nested list (identity otherwise)."""
        if (
            isinstance(a, (list, tuple))
            and a
            and isinstance(a[0], (list, tuple))
        ):
            return [list(row) for row in zip(*a, strict=False)]
        return a

    def sparse_slice(
        self, matrix: Any, row_slice: slice, col_slice: slice
    ) -> Any:
        """Slices a sparse matrix (not supported)."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def diags(
        self,
        diagonals: Sequence[Any],
        _offsets: Sequence[int],
        _format: str = "csr",
    ) -> Any:
        """Constructs a sparse matrix from diagonals (not supported)."""
        return self.diagonal_operator(diagonals[0])

    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Any:
        """Returns an array of ones with the specified shape."""
        if len(shape) == 0:
            return 1.0
        if len(shape) == 1:
            return [1.0] * shape[0]
        return (
            [[1.0] * shape[-1] for _ in range(shape[-2])]
            if len(shape) == 2
            else [1.0]
        )
