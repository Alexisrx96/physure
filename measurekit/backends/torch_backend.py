from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

try:
    import torch
except ImportError:
    torch = None

try:
    from jaxtyping import Array, Bool, Float
except ImportError:
    from typing import Any

    Array = Any
    Bool = Any
    Float = Any


from measurekit.core.dispatcher import enforce_tensor_contract
from measurekit.core.protocols import BackendOps, Boolean, Numeric

try:
    from measurekit.backends.kernels.covariance import (
        apply_covariance_update_triton,
    )

    HAS_TRITON = True
except ImportError:
    HAS_TRITON = False

log = logging.getLogger(__name__)


def _torch_block_offsets(
    blocks: Sequence[Sequence[Any | None]],
) -> tuple[list[int], list[int]]:
    """Computes row and column offsets for a block matrix."""
    row_offsets = [0]
    for row in blocks:
        height = next((b.shape[0] for b in row if b is not None), 0)
        row_offsets.append(row_offsets[-1] + height)

    col_offsets = [0]
    for j in range(len(blocks[0])):
        width = next(
            (
                blocks[i][j].shape[1]
                for i in range(len(blocks))
                if blocks[i][j] is not None
            ),
            0,
        )
        col_offsets.append(col_offsets[-1] + width)

    return row_offsets, col_offsets


def _torch_block_to_coo(block: Any) -> tuple[Any, Any]:
    """Extracts COO (indices, values) from a dense or sparse torch block."""
    if block.is_sparse:
        block = block.coalesce()
        return block.indices().clone(), block.values()
    sp_block = block.to_sparse().coalesce()
    return sp_block.indices().clone(), sp_block.values()


class TorchBackend(BackendOps):
    """PyTorch-based implementation of BackendOps."""

    def __init__(self):
        """Initializes the TorchBackend."""
        if torch is None:
            raise ImportError("PyTorch is not available.")

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a torch Tensor."""
        return isinstance(obj, torch.Tensor)

    def is_tracing(self, obj: Any) -> bool:
        """Torch backend currently does not support tracing in this context."""
        return False

    def asarray(self, obj: Any) -> Array:
        """Converts input to a torch Tensor, preserving gradients if tensors are present."""
        if isinstance(obj, torch.Tensor):
            return obj
        if isinstance(obj, (float, complex)):
            return torch.as_tensor(obj, dtype=torch.float64)
        if isinstance(obj, (list, tuple)):
            if not obj:
                return torch.tensor([], dtype=torch.float64)
            elements = [self.asarray(x) for x in obj]
            if any(isinstance(x, torch.Tensor) for x in elements):
                # Ensure float64 elements are promoted if needed, stack tensors
                return torch.stack(
                    [
                        x.to(torch.float64) if x.dtype != torch.float64 else x
                        for x in elements
                    ]
                )
            return torch.as_tensor(obj)
        return torch.as_tensor(obj)

    def to_device(self, obj: Any, device: str) -> Any:
        """Moves a tensor to a specified device."""
        if isinstance(obj, torch.Tensor):
            return obj.to(device)
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns the device for a tensor."""
        if isinstance(obj, torch.Tensor) and hasattr(obj, "device"):
            return str(obj.device)
        return "cpu"

    @enforce_tensor_contract
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise addition."""
        return torch.add(self.asarray(x), self.asarray(y))

    @enforce_tensor_contract
    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise subtraction."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.subtract(x_t, y_t)
            if hasattr(torch, "subtract")
            else torch.sub(x_t, y_t)
        )

    @enforce_tensor_contract
    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise multiplication."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.multiply(x_t, y_t)
            if hasattr(torch, "multiply")
            else torch.mul(x_t, y_t)
        )

    @enforce_tensor_contract
    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise true division."""
        x_t = self.asarray(x)
        y_t = self.asarray(y)
        return (
            torch.true_divide(x_t, y_t)
            if hasattr(torch, "true_divide")
            else torch.div(x_t, y_t)
        )

    @enforce_tensor_contract
    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Element-wise power."""
        return torch.pow(self.asarray(x), self.asarray(y))

    @enforce_tensor_contract
    def sqrt(self, x: Numeric) -> Numeric:
        """Element-wise square root."""
        return torch.sqrt(self.asarray(x))

    @enforce_tensor_contract
    def exp(self, x: Numeric) -> Numeric:
        """Element-wise exponential."""
        return torch.exp(self.asarray(x))

    @enforce_tensor_contract
    def log(self, x: Numeric) -> Numeric:
        """Element-wise natural logarithm."""
        return torch.log(self.asarray(x))

    @enforce_tensor_contract
    def sin(self, x: Numeric) -> Numeric:
        """Element-wise sine."""
        return torch.sin(self.asarray(x))

    @enforce_tensor_contract
    def cos(self, x: Numeric) -> Numeric:
        """Element-wise cosine."""
        return torch.cos(self.asarray(x))

    @enforce_tensor_contract
    def tan(self, x: Numeric) -> Numeric:
        """Element-wise tangent."""
        return torch.tan(self.asarray(x))

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

    @enforce_tensor_contract
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Sum of elements."""
        if axis is None:
            return torch.sum(self.asarray(obj))
        return torch.sum(self.asarray(obj), dim=axis)

    @enforce_tensor_contract
    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Mean of elements."""
        if axis is None:
            return torch.mean(self.asarray(obj))
        return torch.mean(self.asarray(obj), dim=axis)

    @enforce_tensor_contract
    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True."""
        return bool(torch.any(self.asarray(obj)))

    @enforce_tensor_contract
    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True."""
        return bool(torch.all(self.asarray(obj)))

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
        # Sparse check
        is_sparse = getattr(obj, "is_sparse", False)
        # Handle FakeTensor where is_sparse might be symbolic or require special handling?
        # Typically is_sparse returns a concrete bool even for fake tensors unless it's a tracer.

        if is_sparse:
            return obj.to_dense().reshape(shape)

        return torch.reshape(self.asarray(obj), shape)

    def concatenate(self, arrays: Sequence[Array], axis: int = 0) -> Array:
        """Concatenates tensors."""
        return torch.cat(arrays, dim=axis)

    def eye(self, n: int, format: str = "csr", reference: Any = None) -> Any:
        """Returns an identity matrix."""
        device = getattr(reference, "device", None)
        dtype = getattr(reference, "dtype", torch.float64)
        if dtype is None:
            dtype = torch.float64
        return torch.eye(n, device=device, dtype=dtype)

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix."""
        device = getattr(reference, "device", None)
        dtype = getattr(reference, "dtype", None)
        # Manual construction for broader compatibility
        indices = torch.stack([torch.arange(n, device=device)] * 2)
        if dtype is None:
            dtype = torch.float64
        values = torch.ones(n, device=device, dtype=dtype)
        return torch.sparse_coo_tensor(
            indices, values, (n, n), device=device, dtype=dtype
        ).coalesce()

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

    def ones(
        self, shape: tuple[int, ...], reference: Any = None
    ) -> Float[Array, ...]:
        """Returns a tensor of ones."""
        device = getattr(reference, "device", None)
        dtype = getattr(reference, "dtype", torch.float64)
        if dtype is None:
            dtype = torch.float64
        return torch.ones(shape, device=device, dtype=dtype)

    def zeros(
        self, shape: tuple[int, ...], reference: Any = None
    ) -> Float[Array, ...]:
        """Returns a tensor of zeros."""
        device = getattr(reference, "device", None)
        dtype = getattr(reference, "dtype", torch.float64)
        if dtype is None:
            dtype = torch.float64
        return torch.zeros(shape, device=device, dtype=dtype)

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        # Prefer torch.numel for tensors/proxies to ensure correct counting
        if self.is_array(obj) or (
            hasattr(obj, "numel") and callable(obj.numel)
        ):
            return torch.numel(obj)
        if hasattr(obj, "size") and not callable(obj.size):
            # numpy-like .size property
            return obj.size
        # Fallback for lists/tuples
        if hasattr(obj, "__len__"):
            return len(obj)
        return 1

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts, flattens inputs to common shape 1D arrays."""
        tensors = [self.asarray(x) for x in inputs]
        broadcasted = torch.broadcast_tensors(*tensors)
        return [torch.flatten(b) for b in broadcasted]

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity operator."""
        device = getattr(reference, "device", None)
        dtype = getattr(reference, "dtype", None)
        return torch.eye(size, device=device, dtype=dtype)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given diagonal values."""
        return torch.diag(self.asarray(diagonal))

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        i = torch.stack([self.asarray(indices[0]), self.asarray(indices[1])])
        v = self.asarray(data)
        return torch.sparse_coo_tensor(
            i, v, shape, dtype=torch.float64
        ).coalesce()

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        if shape is None:
            # Estimate shape based on first diagonal and offset
            n = len(diagonals[0]) + abs(offsets[0])
            shape = (n, n)

        # Optimization: Construct indices directly to avoid dense allocation
        device = self.asarray(diagonals[0]).device

        all_indices = []
        all_values = []

        for d, o in zip(diagonals, offsets, strict=False):
            d_tensor = self.asarray(d)
            n_diag = d_tensor.shape[0]

            if o >= 0:
                row = torch.arange(n_diag, device=device)
                col = row + o
            else:
                col = torch.arange(n_diag, device=device)
                row = col - o

            # Filter out of bounds (though diagonals should align with shape)
            mask = (row < shape[0]) & (col < shape[1])
            if not mask.all():
                row = row[mask]
                col = col[mask]
                d_tensor = d_tensor[mask]

            indices = torch.stack([row, col])
            all_indices.append(indices)
            all_values.append(d_tensor)

        if not all_indices:
            # Empty sparse tensor
            return torch.sparse_coo_tensor(
                torch.empty((2, 0), device=device, dtype=torch.long),
                torch.empty(0, device=device, dtype=torch.float64),
                size=shape,
            ).coalesce()

        final_indices = torch.cat(all_indices, dim=1)
        final_values = torch.cat(all_values)

        return torch.sparse_coo_tensor(
            final_indices, final_values, size=shape, dtype=torch.float64
        ).coalesce()

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix of other matrices."""
        row_offsets, col_offsets = _torch_block_offsets(blocks)

        all_indices = []
        all_values = []
        for i, row in enumerate(blocks):
            for j, block in enumerate(row):
                if block is None:
                    continue
                indices, values = _torch_block_to_coo(block)
                indices[0] += row_offsets[i]
                indices[1] += col_offsets[j]
                all_indices.append(indices)
                all_values.append(values)

        final_indices = torch.cat(all_indices, dim=1)
        final_values = torch.cat(all_values, dim=0)
        final_shape = (row_offsets[-1], col_offsets[-1])

        return torch.sparse_coo_tensor(
            final_indices, final_values, final_shape, dtype=torch.float64
        ).coalesce()

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matmul where at least one operand may be sparse."""
        print(f"DEBUG: sparse_matmul a={type(a)}, b={type(b)}")
        # Torch matmul does not support sparse @ sparse
        a_sparse = getattr(a, "is_sparse", False)
        b_sparse = getattr(b, "is_sparse", False)

        if a_sparse and b_sparse:
            # Fallback to dense math (warning: memory intensive)
            # Ideally densify smaller one or use third-party kernel
            return torch.matmul(a.to_dense(), b.to_dense())

        # Sparse @ Dense -> Dense (supported)
        # Dense @ Sparse -> Dense (supported? Check)
        # Dense @ Sparse sometimes fails in older torch.
        if not a_sparse and b_sparse:
            # Dense @ Sparse
            # Convert a to sparse? Or b to dense?
            # b.to_dense() is safer for now.
            b_dense = b.to_dense()
            if not isinstance(a, torch.Tensor):
                a = torch.as_tensor(
                    a, device=b_dense.device, dtype=b_dense.dtype
                )
            elif a.dtype != b_dense.dtype:
                a = a.to(b_dense.dtype)
            return torch.matmul(a, b_dense)

        if a.dtype != b.dtype:
            # Ensure same dtype for matmul
            if a.dtype == torch.float64 or b.dtype == torch.float64:
                a = a.to(torch.float64)
                b = b.to(torch.float64)
            else:
                a = a.to(torch.float32)
                b = b.to(torch.float32)

        return torch.matmul(a, b)

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        if a.is_sparse:
            # sparse_coo has no direct diagonal() in older torch
            # We can use a trick: matmul with a vector of ones? No.
            # Best: coalesce and filter indices where i == j.
            a = a.coalesce()
            indices = a.indices()
            values = a.values()
            mask = indices[0] == indices[1]

            diag_indices = indices[0][mask]
            diag_values = values[mask]

            # Reconstruct full diagonal tensor
            res = torch.zeros(min(a.shape), device=a.device, dtype=a.dtype)
            res[diag_indices] = diag_values
            return res
        return torch.diagonal(a)

    def transpose(self, a: Any) -> Any:
        """Returns the transpose of an array or matrix."""
        if isinstance(a, torch.Tensor):
            if a.is_sparse:
                return a.transpose(0, 1)
            return a.t() if a.ndim == 2 else a.transpose(-1, -2)
        return a

    def sparse_slice(
        self, matrix: Any, row_slice: slice, col_slice: slice
    ) -> Any:
        """Slices a sparse matrix."""
        # Torch sparse tensors do not support start:stop slicing directly.
        # We must use index_select.

        if not matrix.is_sparse:
            return matrix[row_slice, col_slice]

        # Convert slices to indices
        # CAUTION: Realizes indices, large if slice is huge.
        # But typically we slice to get relatively small blocks.
        """Slices a sparse matrix (or dense fallback)."""
        if matrix is None:
            return None

        # Determine indices
        # row_slice might be slice(start, stop)
        r_start = row_slice.start if row_slice.start is not None else 0
        r_stop = (
            row_slice.stop if row_slice.stop is not None else matrix.shape[0]
        )
        rows = torch.arange(r_start, r_stop, device=matrix.device)

        c_start = col_slice.start if col_slice.start is not None else 0
        c_stop = (
            col_slice.stop if col_slice.stop is not None else matrix.shape[1]
        )
        cols = torch.arange(c_start, c_stop, device=matrix.device)

        # For autograd stability, if we have gradients, densifying is often safer
        # as many sparse ops lack backward kernels (like index_add_ on SparseCPU)
        m = matrix
        if m.is_sparse:
            if m.requires_grad or m.grad_fn is not None:
                # Densify BEFORE slicing to ensure dense backward pass
                m = m.to_dense()
                return m[row_slice, col_slice]

            # For sparse, use index_select
            m = torch.index_select(m, 0, rows)
            m = torch.index_select(m, 1, cols)
            return m.coalesce()

        # Dense slicing
        return m[row_slice, col_slice]

    def quadratic_form(self, sigma: Any, jac: Any) -> Any:
        """Computes J @ Sigma @ J.T efficiently.

        Optimized for diagonal J (vector) using Triton if available.
        """
        if (
            HAS_TRITON
            and isinstance(sigma, torch.Tensor)
            and isinstance(jac, torch.Tensor)
            and sigma.is_cuda
            and jac.is_cuda
            and jac.ndim == 1  # Diagonal Jacobian represented as vector
            and jac.shape[0] == sigma.shape[0]  # same size as sigma rows
        ):
            try:
                return apply_covariance_update_triton(sigma, jac)
            except Exception:
                # Fallback if kernel fails (e.g. dimensions/stride)
                pass

        # Fallback to standard math
        # If jac is 1D, it represents diagonal
        if self.is_array(jac) and getattr(jac, "ndim", 0) == 1:
            # J @ Sigma @ J.T with J=diag(j) -> (j.reshape(-1,1) * Sigma) * j
            # Broadcasting: (N,1) * (N,N) * (N,) -> (N,N)
            j = self.asarray(jac)
            return (j.unsqueeze(1) * self.asarray(sigma)) * j

        # General case
        j = self.asarray(jac)
        s = self.asarray(sigma)
        # s @ j.T
        if j.is_sparse:
            # sparse matmul needed?
            # j @ s @ j.T
            # This is (j @ s) @ j.T
            temp = self.sparse_matmul(j, s)
            return self.sparse_matmul(temp, self.transpose(j))

        return torch.matmul(torch.matmul(j, s), j.transpose(-1, -2))


if torch is not None:
    # Quantity defers its torch pytree registration until torch is in play;
    # loading this backend is the signal that it is.
    from measurekit.domain.measurement.quantity import _register_torch_pytree

    _register_torch_pytree()
