from __future__ import annotations

from typing import TYPE_CHECKING, Any

import torch

if TYPE_CHECKING:
    from physure.backends.torch_backend import TorchBackend


class AutogradCovarianceStore:
    """A PyTorch Autograd-compatible implementation of CovarianceStore.

    This store maintains the covariance matrix as a PyTorch sparse tensor,
    allowing operations to be tracked by the Autograd engine. This enables
    backpropagation through uncertainty estimates.
    """

    def __init__(self, backend: TorchBackend):
        self.backend = backend
        self._matrix = None  # torch.Tensor (Sparse)
        self._next_idx = 0
        try:
            from physure_core import PruningConfig

            self.config = PruningConfig()
        except ImportError:
            # Fallback mock config
            from dataclasses import dataclass

            @dataclass
            class MockConfig:
                enabled: bool = False
                max_age: int = 100
                corr_threshold: float = 1e-6

            self.config = MockConfig()

        self._sizes: dict[int, int] = {}  # ID -> Size map

    def allocate(self, size: int) -> slice:
        """Allocates indices."""
        start = self._next_idx
        self._next_idx += size
        self._sizes[start] = size
        return slice(start, start + size)

    def register_variable(self, _var_id: int, variance: Any):
        """Registers a variable with a variance matrix block."""
        variance = self.backend.asarray(variance)
        if hasattr(variance, "to_sparse"):
            variance = variance.to_sparse()

        if self._matrix is None:
            self._matrix = variance
        else:
            # Check for empty matrix
            if hasattr(self._matrix, "shape") and self._matrix.shape == (0, 0):
                self._matrix = variance
            else:
                self._matrix = self.backend.sparse_bmat(
                    [[self._matrix, None], [None, variance]]
                )

    def register_diagonal(self, var_id: int, variance_diag: Any):
        """Registers a variable with a diagonal variance vector."""
        diag = self.backend.asarray(variance_diag)
        size = diag.shape[0]
        # Create diagonal matrix
        variance = self.backend.sparse_diags([diag], [0], shape=(size, size))
        self.register_variable(var_id, variance)

    def propagate(
        self, out_id: int, input_ids: list[int], jacobians: list[Any]
    ):
        """Propagates covariance using functional API logic."""
        out_size = self._sizes.get(out_id)
        if out_size is None:
            raise ValueError(f"Output ID {out_id} not allocated.")

        out_slice = slice(out_id, out_id + out_size)

        in_slices = []
        for i_id in input_ids:
            size = self._sizes.get(i_id)
            if size is None:
                raise ValueError(f"Input ID {i_id} not allocated.")
            in_slices.append(slice(i_id, i_id + size))

        print(
            f"DEBUG: AutogradCovarianceStore.propagate called for out_id={out_id}"
        )
        from physure.domain.measurement.vectorized_uncertainty import (
            propagate_affine,
        )

        self._matrix = propagate_affine(
            self._matrix, out_slice, in_slices, jacobians, self.backend
        )

    def get_covariance_block(self, row_slice: slice, col_slice: slice) -> Any:
        """Retrieves a block (differentiable)."""
        if self._matrix is None:
            res = self.backend.zeros(
                (
                    row_slice.stop - row_slice.start,
                    col_slice.stop - col_slice.start,
                )
            )
            if hasattr(res, "to"):
                res = res.to(torch.float64)
            return res

        return self.backend.sparse_slice(self._matrix, row_slice, col_slice)
