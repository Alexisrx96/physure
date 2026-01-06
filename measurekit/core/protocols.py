from __future__ import annotations

from typing import (
    TYPE_CHECKING,
    Any,
    Protocol,
    TypeVar,
    runtime_checkable,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

try:
    from jaxtyping import Array, Bool, Float
except ImportError:
    from typing import Any

    Array = Any
    Bool = Any
    Float = Any


T = TypeVar("T")

# Unified type for methods that support both arrays and scalars
Numeric = "Float[Array, '...'] | float | int"
Boolean = "Bool[Array, '...'] | bool"


@runtime_checkable
class BackendOps(Protocol):
    """Protocol defining the operations a backend must support."""

    # Creation
    def is_array(self, obj: Any) -> bool:
        """Returns True if the object is a supported array type."""
        ...

    def is_tracing(self, obj: Any) -> bool:
        """Returns True if the object is being traced."""
        ...

    def asarray(self, obj: Any) -> Float[Array, ...]:
        """Converts the input to an array type supported by this backend."""
        ...

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves the object to the specified device (e.g., 'cpu', 'cuda')."""
        ...

    def get_device(self, obj: Any) -> str | None:
        """Returns the device identifier for the given object."""
        ...

    # Math Operations
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Performs element-wise addition."""
        ...

    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Performs element-wise subtraction."""
        ...

    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Performs element-wise multiplication."""
        ...

    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Performs element-wise true division."""
        ...

    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Performs element-wise power."""
        ...

    def sqrt(self, x: Numeric) -> Numeric:
        """Computes element-wise square root."""
        ...

    def exp(self, x: Numeric) -> Numeric:
        """Computes element-wise exponential."""
        ...

    def log(self, x: Numeric) -> Numeric:
        """Computes element-wise natural logarithm."""
        ...

    def sin(self, x: Numeric) -> Numeric:
        """Computes element-wise sine."""
        ...

    def cos(self, x: Numeric) -> Numeric:
        """Computes element-wise cosine."""
        ...

    def tan(self, x: Numeric) -> Numeric:
        """Computes element-wise tangent."""
        ...

    def dot(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes dot product."""
        ...

    def cross(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes cross product."""
        ...

    def abs(self, x: Numeric) -> Numeric:
        """Computes element-wise absolute value."""
        ...

    def sign(self, x: Numeric) -> Numeric:
        """Computes element-wise sign."""
        ...

    # Reduction Operations
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the sum of elements along the specified axis."""
        ...

    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the mean of elements along the specified axis."""
        ...

    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True."""
        ...

    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True."""
        ...

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if elements are equal within a tolerance."""
        ...

    def equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise equality."""
        ...

    def not_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise inequality."""
        ...

    def less(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than."""
        ...

    def less_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than or equal."""
        ...

    def greater(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise greater than."""
        ...

    def greater_equal(self, x: Numeric, y: Numeric) -> Boolean:
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
        """Broadcasts inputs to a common shape; returns 1D arrays."""
        ...

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        ...

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given values."""
        ...

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        ...

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        ...

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix."""
        ...

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matrix multiplication where one operand may be sparse."""
        ...

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        ...

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix of size n x n."""
        ...

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of an array or matrix."""
        ...

    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Any:
        """Returns an array (or matrix) of ones with the given shape."""
        ...
