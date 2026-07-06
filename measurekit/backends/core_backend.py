"""Backend for the Rust-based QuantityInner core."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from measurekit.core.protocols import BackendOps

if TYPE_CHECKING:
    from collections.abc import Sequence


class CoreBackend(BackendOps):
    """Backend for the Rust-based QuantityInner core."""

    def is_array(self, obj: Any) -> bool:
        """Core quantities are scalars, never arrays."""
        return False

    def is_tracing(self, obj: Any) -> bool:
        """The core backend never traces."""
        return False

    def add(self, x: Any, y: Any) -> Any:
        """Adds two core quantities or a core quantity and a scalar."""
        return x + y

    def sub(self, x: Any, y: Any) -> Any:
        """Subtracts two core quantities or a core quantity and a scalar."""
        return x - y

    def mul(self, x: Any, y: Any) -> Any:
        """Multiplies two core quantities or a core quantity and a scalar."""
        return x * y

    def truediv(self, x: Any, y: Any) -> Any:
        """Divides two core quantities or a core quantity and a scalar."""
        return x / y

    def pow(self, x: Any, y: Any) -> Any:
        """Computes power of a core quantity."""
        return x**y

    def sqrt(self, x: Any) -> Any:
        """Computes square root of a core quantity."""
        return x**0.5

    def exp(self, x: Any) -> Any:
        """Computes exponential of a core quantity."""
        return x.propagate_function("exp")

    def log(self, x: Any) -> Any:
        """Computes natural logarithm of a core quantity."""
        return x.propagate_function("log")

    def sin(self, x: Any) -> Any:
        """Computes sine of a core quantity."""
        return x.propagate_function("sin")

    def cos(self, x: Any) -> Any:
        """Computes cosine of a core quantity."""
        return x.propagate_function("cos")

    def tan(self, x: Any) -> Any:
        """Computes tangent of a core quantity."""
        return x.propagate_function("tan")

    def dot(self, x: Any, y: Any) -> Any:
        """Computes the dot product of two scalar core quantities."""
        return x * y

    def cross(self, x: Any, y: Any) -> Any:
        """Computes the cross product (not supported for scalars)."""
        raise NotImplementedError("Cross product not supported for scalars")

    def abs(self, x: Any) -> Any:
        """Returns the absolute value of a core quantity."""
        return abs(x)

    def sign(self, x: Any) -> Any:
        """Returns the sign of a core quantity (-1, 0, or 1)."""
        if x > 0:
            return 1.0
        if x < 0:
            return -1.0
        return 0.0

    def asarray(self, obj: Any) -> Any:
        """Converts to a form suitable for this backend (identity)."""
        return obj

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves the object to the specified device (identity)."""
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns the device of the object (always 'cpu')."""
        return "cpu"

    def sum(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Returns a scalar core quantity unchanged."""
        return obj

    def mean(self, obj: Any, axis: int | Sequence[int] | None = None) -> Any:
        """Returns a scalar core quantity unchanged."""
        return obj

    def any(self, obj: Any) -> bool:
        """Returns True if the scalar is truthy."""
        return bool(obj)

    def all(self, obj: Any) -> bool:
        """Returns True if the scalar is truthy."""
        return bool(obj)

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if two core quantities are equal."""
        return a == b

    def equal(self, x: Any, y: Any) -> Any:
        """Returns True if x equals y."""
        return x == y

    def not_equal(self, x: Any, y: Any) -> Any:
        """Returns True if x does not equal y."""
        return x != y

    def less(self, x: Any, y: Any) -> Any:
        """Returns True if x is less than y."""
        return x < y

    def less_equal(self, x: Any, y: Any) -> Any:
        """Returns True if x is less than or equal to y."""
        return x <= y

    def greater(self, x: Any, y: Any) -> Any:
        """Returns True if x is greater than y."""
        return x > y

    def greater_equal(self, x: Any, y: Any) -> Any:
        """Returns True if x is greater than or equal to y."""
        return x >= y

    def shape(self, obj: Any) -> tuple[int, ...]:
        """Core quantities are scalars, so the shape is always empty."""
        return ()

    def size(self, obj: Any) -> int:
        """Core quantities are scalars, so the size is always 1."""
        return 1

    # ponytail: core quantities are scalars (is_array is always False), so
    # array/matrix-shaped ops have no sensible scalar behavior to invent.
    CORE_NOT_SUPPORTED = (
        "Operation not supported for scalar CoreBackend quantities"
    )

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshaping is not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenation is not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasting is not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Identity operators are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Diagonal operators are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_bmat(self, blocks: Sequence[Sequence[Any | None]]) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_diagonal(self, a: Any) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def sparse_slice(
        self, matrix: Any, row_slice: slice, col_slice: slice
    ) -> Any:
        """Sparse matrices are not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def transpose(self, a: Any) -> Any:
        """Transpose is not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)

    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Any:
        """Array creation is not supported for scalar core quantities."""
        raise NotImplementedError(self.CORE_NOT_SUPPORTED)
