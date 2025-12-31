from __future__ import annotations

from collections.abc import Sequence
from typing import Any, Protocol, TypeVar, Union, runtime_checkable

T = TypeVar("T")


@runtime_checkable
class BackendOps(Protocol):
    """Protocol defining the mathematical and structural operations a backend must support."""

    # Creation
    def is_array(self, obj: Any) -> bool:
        """Returns True if the object is an array type supported by this backend."""
        ...

    def asarray(self, obj: Any) -> Any:
        """Converts the input to an array type supported by this backend."""
        ...

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves the object to the specified device (e.g., 'cpu', 'cuda')."""
        ...

    # Math Operations
    def add(self, x: Any, y: Any) -> Any:
        """Performs element-wise addition."""
        ...

    def sub(self, x: Any, y: Any) -> Any:
        """Performs element-wise subtraction."""
        ...

    def mul(self, x: Any, y: Any) -> Any:
        """Performs element-wise multiplication."""
        ...

    def truediv(self, x: Any, y: Any) -> Any:
        """Performs element-wise true division."""
        ...

    def pow(self, x: Any, y: Any) -> Any:
        """Performs element-wise power."""
        ...

    def sqrt(self, x: Any) -> Any:
        """Computes element-wise square root."""
        ...

    def exp(self, x: Any) -> Any:
        """Computes element-wise exponential."""
        ...

    def log(self, x: Any) -> Any:
        """Computes element-wise natural logarithm."""
        ...

    def sin(self, x: Any) -> Any:
        """Computes element-wise sine."""
        ...

    def cos(self, x: Any) -> Any:
        """Computes element-wise cosine."""
        ...

    def tan(self, x: Any) -> Any:
        """Computes element-wise tangent."""
        ...

    def dot(self, x: Any, y: Any) -> Any:
        """Computes dot product."""
        ...

    def cross(self, x: Any, y: Any) -> Any:
        """Computes cross product."""
        ...

    def abs(self, x: Any) -> Any:
        """Computes element-wise absolute value."""
        ...

    def sign(self, x: Any) -> Any:
        """Computes element-wise sign."""
        ...

    # Reduction Operations
    def sum(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        """Computes the sum of elements along the specified axis."""
        ...

    def mean(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        """Computes the mean of elements along the specified axis."""
        ...

    def any(self, obj: Any) -> bool:
        """Returns True if any element is True."""
        ...

    def all(self, obj: Any) -> bool:
        """Returns True if all elements are True."""
        ...

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if two arrays are element-wise equal within a tolerance."""
        ...

    def equal(self, x: Any, y: Any) -> Any:
        """Element-wise equality."""
        ...

    def not_equal(self, x: Any, y: Any) -> Any:
        """Element-wise inequality."""
        ...

    def less(self, x: Any, y: Any) -> Any:
        """Element-wise less than."""
        ...

    def less_equal(self, x: Any, y: Any) -> Any:
        """Element-wise less than or equal."""
        ...

    def greater(self, x: Any, y: Any) -> Any:
        """Element-wise greater than."""
        ...

    def greater_equal(self, x: Any, y: Any) -> Any:
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
    def eye(self, N: int, format: str = "csr") -> Any:
        """Returns an N x N identity matrix, potentially sparse."""
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
