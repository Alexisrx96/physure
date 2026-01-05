from __future__ import annotations

import logging
from collections.abc import Sequence
from typing import Any

try:
    import torch
except (ImportError, ModuleNotFoundError):
    torch = None

try:
    from jaxtyping import Array, Bool, Float
except (ImportError, ModuleNotFoundError):
    from typing import Any

    Array = Any
    Bool = Any
    Float = Any


from measurekit.core.protocols import BackendOps

log = logging.getLogger(__name__)


class TorchBackend(BackendOps):
    """PyTorch-based implementation of BackendOps."""

    def __init__(self):
        if torch is None:
            raise ImportError("PyTorch is not available.")

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is a torch Tensor."""
        return isinstance(obj, torch.Tensor)

    def is_tracing(self, obj: Any) -> bool:
        """Torch backend currently does not support tracing in this context."""
        return False

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

    def get_device(self, obj: Any) -> str | None:
        """Returns the device for a tensor."""
        if isinstance(obj, torch.Tensor):
            if hasattr(obj, "device"):
                return str(obj.device)
        return "cpu"

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
        if obj.is_sparse:
            return obj.to_dense().reshape(shape)
        return torch.reshape(obj, shape)

    def concatenate(self, arrays: Sequence[Array], axis: int = 0) -> Array:
        """Concatenates tensors."""
        return torch.cat(arrays, dim=axis)

    def eye(
        self, n: int, format: str = "csr", reference: Any = None
    ) -> Float[Array, "n n"]:
        """Returns an identity matrix."""
        device = reference.device if hasattr(reference, "device") else None
        return torch.eye(n, device=device)

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

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        # Fixed logic to use reference.device
        device = getattr(reference, "device", None)
        return torch.eye(size, device=device)

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
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
        return torch.sparse_coo_tensor(i, v, shape).coalesce()

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

        # Simplified implementation: construct dense then convert to sparse
        # For performance with many diagonals, a COO implementation is better
        res = torch.zeros(shape, device=self.asarray(diagonals[0]).device)
        for d, o in zip(diagonals, offsets, strict=False):
            diag_len = len(d)
            if o >= 0:
                indices = torch.arange(diag_len)
                res[indices, indices + o] = self.asarray(d)
            else:
                indices = torch.arange(diag_len)
                res[indices - o, indices] = self.asarray(d)
        return res.to_sparse()

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix of other matrices."""
        # PyTorch doesn't have a direct bmat for sparse tensors.
        # We manually accumulate COO indices and values.
        all_indices = []
        all_values = []

        row_offsets = [0]
        col_offsets = [0]

        # Calculate offsets
        for row in blocks:
            # Find first non-None block in row to get its height
            height = 0
            for b in row:
                if b is not None:
                    height = b.shape[0]
                    break
            row_offsets.append(row_offsets[-1] + height)

        for j in range(len(blocks[0])):
            width = 0
            for i in range(len(blocks)):
                if blocks[i][j] is not None:
                    width = blocks[i][j].shape[1]
                    break
            col_offsets.append(col_offsets[-1] + width)

        for i, row in enumerate(blocks):
            for j, block in enumerate(row):
                if block is None:
                    continue

                # Ensure block is coalesced if it's sparse
                if block.is_sparse:
                    block = block.coalesce()
                    indices = block.indices().clone()
                    values = block.values()
                else:
                    # Convert dense to sparse for consistent processing
                    sp_block = block.to_sparse().coalesce()
                    indices = sp_block.indices().clone()
                    values = sp_block.values()

                # Offset indices
                indices[0] += row_offsets[i]
                indices[1] += col_offsets[j]

                all_indices.append(indices)
                all_values.append(values)

        final_indices = torch.cat(all_indices, dim=1)
        final_values = torch.cat(all_values, dim=0)
        final_shape = (row_offsets[-1], col_offsets[-1])

        return torch.sparse_coo_tensor(
            final_indices, final_values, final_shape
        ).coalesce()

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Performs matrix multiplication where at least one operand may be sparse."""
        # Torch matmul does not support sparse @ sparse
        a_sparse = getattr(a, "is_sparse", False)
        b_sparse = getattr(b, "is_sparse", False)

        if a_sparse and b_sparse:
            # Fallback to dense math (warning: memory intensive)
            # Ideally we would only densify the smaller one or use a third-party kernel
            return torch.matmul(a.to_dense(), b.to_dense())

        # Sparse @ Dense -> Dense (supported)
        # Dense @ Sparse -> Dense (supported? Check)
        # Dense @ Sparse sometimes fails in older torch.
        if not a_sparse and b_sparse:
            # Dense @ Sparse
            # Convert a to sparse? Or b to dense?
            # b.to_dense() is safer for now.
            return torch.matmul(a, b.to_dense())

        return torch.matmul(a, b)

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        if a.is_sparse:
            # For sparse_coo, there is no direct diagonal() method in older torch
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
        # CAUTION: This requires realizing the indices, which might be large if slice is huge.
        # But typically we slice to get relatively small blocks.

        # Optimize for full slice
        if row_slice == slice(None) and col_slice == slice(None):
            return matrix

        rows = torch.arange(
            row_slice.start or 0,
            row_slice.stop if row_slice.stop is not None else matrix.shape[0],
            row_slice.step or 1,
            device=matrix.device,
        )
        cols = torch.arange(
            col_slice.start or 0,
            col_slice.stop if col_slice.stop is not None else matrix.shape[1],
            col_slice.step or 1,
            device=matrix.device,
        )

        # Select rows then cols
        # index_select(dim, index) -> sparse
        m = torch.index_select(matrix, 0, rows)
        m = torch.index_select(m, 1, cols)

        # index_select preserves absolute values but shifts dimensions?
        # No, it works like numpy indexing?
        # Actually, for sparse, it keeps the indices?
        # Let's verify if we need to coalesce.
        return m.coalesce()
