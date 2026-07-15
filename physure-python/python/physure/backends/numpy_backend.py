from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

    from numpy.typing import NDArray

try:
    import numpy as np
except ImportError:
    np = None


if TYPE_CHECKING:
    Array = NDArray[Any]
else:
    try:
        from jaxtyping import Array
    except ImportError:
        Array = Any

try:
    from scipy import sparse
except ImportError:
    sparse = None


from physure.core.dispatcher import enforce_tensor_contract
from physure.core.protocols import BackendOps, Boolean, Numeric

log = logging.getLogger(__name__)


class NumpyBackend(BackendOps):
    """NumPy-based implementation of BackendOps."""

    def __init__(self):
        """Initializes the NumPy backend."""
        if np is None:
            raise ImportError("NumPy is not available.")

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a NumPy array or sparse matrix."""
        return isinstance(obj, np.ndarray) or (
            sparse is not None and sparse.issparse(obj)
        )

    def is_tracing(self, obj: Any) -> bool:
        """NumPy backend does not support tracing."""
        return False

    def asarray(self, obj: Any) -> Array:
        """Converts input to a NumPy array."""
        if self.is_array(obj):
            return obj
        return np.asarray(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """No-op for NumPy backend."""
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns 'cpu' for NumPy backend."""
        return "cpu"

    def preserves_native_gradients(self) -> bool:
        """NumPy has no autograd; values are always coerced to NumPy arrays."""
        return False

    @enforce_tensor_contract
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise addition."""
        # ponytail: Numeric is a broad union (may include sparse
        # matrices); the static operator check can't verify every member
        # supports "+".
        if sparse is not None and (sparse.issparse(x) or sparse.issparse(y)):
            return x + y  # pyright: ignore[reportOperatorIssue]
        return np.add(x, y)

    @enforce_tensor_contract
    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise subtraction."""
        # ponytail: Numeric is a broad union (may include sparse
        # matrices); the static operator check can't verify every member
        # supports "-".
        if sparse is not None and (sparse.issparse(x) or sparse.issparse(y)):
            return x - y  # pyright: ignore[reportOperatorIssue]
        return np.subtract(x, y)

    @enforce_tensor_contract
    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise multiplication."""
        if sparse is not None and (sparse.issparse(x) or sparse.issparse(y)):
            # Scipy sparse multiplication (*) is element-wise for some, but @ is matmul.
            # Scipy 1.x * is element-wise for some types, matmul for others.
            # Better use .multiply() for explicit element-wise.
            # ponytail: Numeric is a broad union; static checker can't
            # verify x/y satisfy scipy's Number|complex "other" param.
            if hasattr(x, "multiply"):
                return x.multiply(y)  # pyright: ignore[reportArgumentType, reportCallIssue]
            if hasattr(y, "multiply"):
                return y.multiply(x)  # pyright: ignore[reportArgumentType, reportCallIssue]
            return x * y  # pyright: ignore[reportOperatorIssue]
        return np.multiply(x, y)

    @enforce_tensor_contract
    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise true division."""
        # ponytail: Numeric is a broad union (may include sparse
        # matrices); the static operator check can't verify every member
        # supports "/".
        if sparse is not None and sparse.issparse(x):
            return x / y  # pyright: ignore[reportOperatorIssue]
        return np.true_divide(x, y)

    @enforce_tensor_contract
    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise power."""
        if hasattr(x, "power"):
            return x.power(y)
        return np.power(x, y)

    @enforce_tensor_contract
    def sqrt(self, x: Numeric) -> Numeric:
        """Element-wise square root."""
        return np.sqrt(x)

    @enforce_tensor_contract
    def exp(self, x: Numeric) -> Numeric:
        """Element-wise exponential."""
        return np.exp(x)

    @enforce_tensor_contract
    def log(self, x: Numeric) -> Numeric:
        """Element-wise natural logarithm."""
        return np.log(x)

    @enforce_tensor_contract
    def sin(self, x: Numeric) -> Numeric:
        """Element-wise sine."""
        return np.sin(x)

    @enforce_tensor_contract
    def cos(self, x: Numeric) -> Numeric:
        """Element-wise cosine."""
        return np.cos(x)

    @enforce_tensor_contract
    def tan(self, x: Numeric) -> Numeric:
        """Element-wise tangent."""
        return np.tan(x)

    @enforce_tensor_contract
    def dot(self, x: Numeric, y: Numeric) -> Numeric:
        """Dot product or matrix multiplication."""
        return np.dot(x, y)

    @enforce_tensor_contract
    def cross(self, x: Numeric, y: Numeric) -> Numeric:
        """Cross product."""
        return np.cross(x, y)

    @enforce_tensor_contract
    def abs(self, x: Numeric) -> Numeric:
        """Element-wise absolute value."""
        return np.abs(x)

    @enforce_tensor_contract
    def sign(self, x: Numeric) -> Numeric:
        """Element-wise sign."""
        return np.sign(x)

    @enforce_tensor_contract
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Sum of elements."""
        return np.sum(obj, axis=axis)

    @enforce_tensor_contract
    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Mean of elements."""
        return np.mean(obj, axis=axis)

    @enforce_tensor_contract
    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True."""
        if sparse is not None and sparse.issparse(obj):
            return obj.nnz > 0
        return bool(np.any(obj))

    @enforce_tensor_contract
    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True."""
        return bool(np.all(obj))

    @enforce_tensor_contract
    def allclose(
        self,
        a: Numeric,
        b: Numeric,
        rtol: float = 1e-5,
        atol: float = 1e-8,
    ) -> bool:
        """Checks if all elements are close."""
        return bool(np.allclose(a, b, rtol=rtol, atol=atol))

    @enforce_tensor_contract
    def equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise equality."""
        return np.equal(x, y)

    @enforce_tensor_contract
    def not_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise inequality."""
        return np.not_equal(x, y)

    @enforce_tensor_contract
    def less(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than."""
        return np.less(x, y)

    @enforce_tensor_contract
    def less_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise less than or equal."""
        return np.less_equal(x, y)

    @enforce_tensor_contract
    def greater(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise greater than."""
        return np.greater(x, y)

    @enforce_tensor_contract
    def greater_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Element-wise greater than or equal."""
        return np.greater_equal(x, y)

    @enforce_tensor_contract
    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the array."""
        if hasattr(obj, "shape"):
            return obj.shape
        return np.shape(obj)

    @enforce_tensor_contract
    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the array."""
        return np.reshape(obj, shape)

    @enforce_tensor_contract
    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates arrays."""
        return np.concatenate(arrays, axis=axis)

    def eye(self, n: int, format: str = "csr", reference: Any = None) -> Any:
        """Returns an identity matrix."""
        dtype = getattr(reference, "dtype", None)
        # ponytail: scipy-stubs types dtype as type[float]; None/arbitrary
        # dtypes work fine at runtime.
        return sparse.eye(
            n,
            format=format,
            dtype=dtype,  # pyright: ignore[reportArgumentType]
        )

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix."""
        dtype = getattr(reference, "dtype", None)
        # ponytail: scipy-stubs types dtype as type[float]; None/arbitrary
        # dtypes work fine at runtime.
        return sparse.eye(
            n,
            format="csr",
            dtype=dtype,  # pyright: ignore[reportArgumentType]
        )

    @enforce_tensor_contract
    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
        reference: Any = None,
    ) -> Any:
        """Constructs a diagonal matrix.

        Args:
            diagonals: Sequence of diagonal values.
            offsets: Sequence of diagonal offsets.
            format: Sparse format (default 'csr').
            reference: Reference object for device/type inference.
        """
        # ponytail: scipy-stubs types offsets as a single int; a
        # Sequence[int] of multiple offsets works fine at runtime.
        return sparse.diags(
            diagonals=diagonals,
            offsets=offsets,  # pyright: ignore[reportArgumentType]
            format=format,
            dtype=getattr(reference, "dtype", None),
        )

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        if hasattr(obj, "shape"):
            return int(np.prod(obj.shape))
        return int(np.size(obj))

    @enforce_tensor_contract
    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to 1D arrays.

        Args:
            inputs: Sequence of input arrays.

        Returns:
            Sequence of flattened broadcasted arrays.
        """
        # np.broadcast_arrays broadcasts inputs against each other
        broadcasted = np.broadcast_arrays(*inputs)
        # Flatten each
        return [b.ravel() for b in broadcasted]

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity operator (matrix) of the given size."""
        return sparse.eye(size, format="csr")

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given values."""
        # ponytail: scipy-stubs types offsets as a single int; a list
        # works fine at runtime.
        return sparse.diags(
            [diagonal],
            [0],  # pyright: ignore[reportArgumentType]
            format="csr",
        )

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        return sparse.coo_matrix((data, indices), shape=shape).tocsr()

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        # ponytail: scipy-stubs types offsets as a single int; a
        # Sequence[int] of multiple offsets works fine at runtime.
        return sparse.diags(
            diagonals,
            offsets,  # pyright: ignore[reportArgumentType]
            shape=shape,
            format="csr",
        )

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix."""
        return sparse.bmat(blocks, format="csr")

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matrix multiplication where one operand may be sparse."""
        return a @ b

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        if hasattr(a, "diagonal"):
            return a.diagonal()
        return np.diagonal(a)

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of an array or matrix."""
        if hasattr(a, "transpose"):
            # Scipy sparse and numpy arrays support this
            return a.transpose()
        return np.transpose(a)

    def sparse_slice(
        self, matrix: Any, row_slice: slice, col_slice: slice
    ) -> Any:
        """Slices a sparse matrix."""
        return matrix[row_slice, col_slice]

    @enforce_tensor_contract
    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Numeric:
        """Returns an array of ones."""
        dtype = getattr(reference, "dtype", None)
        return np.ones(shape, dtype=dtype)

    @enforce_tensor_contract
    def zeros(self, shape: tuple[int, ...], reference: Any = None) -> Numeric:
        """Returns an array of zeros."""
        dtype = getattr(reference, "dtype", None)
        return np.zeros(shape, dtype=dtype)

    def from_scipy_sparse(self, matrix: Any) -> Any:
        """SciPy sparse matrices are already this backend's native representation."""
        return matrix
