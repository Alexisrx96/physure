from __future__ import annotations

from collections.abc import Sequence
from typing import Any, Protocol, TypeVar, runtime_checkable

from jaxtyping import Array, Bool, Float

T = TypeVar("T")


@runtime_checkable
class BackendOps(Protocol):
    """Protocol defining the mathematical and structural operations a backend must support."""

    # Creation
    def is_array(self, obj: Any) -> bool:
        """Returns True if the object is an array type supported by this backend."""
        ...

    def asarray(self, obj: Any) -> Array:
        """Converts the input to an array type supported by this backend."""
        ...

    def to_device(self, obj: Any, device: str) -> Array:
        """Moves the object to the specified device (e.g., 'cpu', 'cuda')."""
        ...

    # Math Operations
    def add(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Performs element-wise addition."""
        ...

    def sub(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Performs element-wise subtraction."""
        ...

    def mul(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Performs element-wise multiplication."""
        ...

    def truediv(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Performs element-wise true division."""
        ...

    def pow(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Performs element-wise power."""
        ...

    def sqrt(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise square root."""
        ...

    def exp(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise exponential."""
        ...

    def log(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise natural logarithm."""
        ...

    def sin(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise sine."""
        ...

    def cos(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise cosine."""
        ...

    def tan(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise tangent."""
        ...

    def dot(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Computes dot product."""
        ...

    def cross(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        """Computes cross product."""
        ...

    def abs(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise absolute value."""
        ...

    def sign(self, x: Float[Array, ...]) -> Float[Array, ...]:
        """Computes element-wise sign."""
        ...

    # Reduction Operations
    def sum(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Computes the sum of elements along the specified axis."""
        ...

    def mean(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        """Computes the mean of elements along the specified axis."""
        ...

    def any(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if any element is True."""
        ...

    def all(self, obj: Bool[Array, ...]) -> bool:
        """Returns True if all elements are True."""
        ...

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if two arrays are element-wise equal within a tolerance."""
        ...

    def equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise equality."""
        ...

    def not_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise inequality."""
        ...

    def less(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than."""
        ...

    def less_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise less than or equal."""
        ...

    def greater(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than."""
        ...

    def greater_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        """Element-wise greater than or equal."""
        ...

    # Shape and Structure
    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the object."""
        ...

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the object to the specified shape."""
        ...

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates a sequence of arrays along the specified axis."""
        ...

    # Sparse / Jacobian Helper
    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        ...

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and returns them as flattened 1D arrays."""
        ...

    def identity_operator(self, size: int) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        ...

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
        ...

    # Legacy (Keep for backward compatibility during refactor if needed, or remove if unused)
    def eye(self, n: int, format: str = "csr") -> Any:
        """Returns an n x n identity matrix, potentially sparse."""
        ...

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        ...

    def ones(self, shape: tuple[int, ...]) -> Any:
        """Returns an array (or matrix) of ones with the given shape."""
        ...
