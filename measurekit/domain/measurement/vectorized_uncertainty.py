"""Vectorized uncertainty propagation using sparse global matrices."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import numpy as np
from scipy import sparse

if TYPE_CHECKING:
    from numpy.typing import NDArray


class CovarianceStore:
    """Singleton store for global covariance management."""

    _instance: CovarianceStore | None = None

    def __new__(cls) -> CovarianceStore:
        """Singleton pattern implementation."""
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._init_store()
        return cls._instance

    def _init_store(self) -> None:
        """Initializes the sparse matrix and index tracking."""
        self._matrix = sparse.csr_matrix((0, 0))
        self._next_idx = 0

    def allocate(self, size: int) -> slice:
        """Allocates a block of indices for a new array quantity."""
        start = self._next_idx
        end = start + size
        self._next_idx = end
        # Matrix is grown via bmat during update/register
        return slice(start, end)

    def get_covariance_block(
        self, row_slice: slice, col_slice: slice
    ) -> sparse.csr_matrix:
        """Retrieves a block from the global covariance matrix."""
        return self._matrix[row_slice, col_slice]

    def set_covariance_block(
        self, row_slice: slice, col_slice: slice, block: Any
    ) -> None:
        """Sets a block in the global covariance matrix, growing it if needed."""
        max_idx = max(row_slice.stop, col_slice.stop)
        current_size = self._matrix.shape[0]

        if max_idx > current_size:
            # Grow by converting to LIL (most robust for resizing/assignment)
            lil = self._matrix.tolil()
            lil.resize((max_idx, max_idx))
            self._matrix = lil.tocsr()

        # Perform assignment - using LIL if already modifying a block
        # is often more reliable than CSR assignment.
        lil = self._matrix.tolil()
        if hasattr(block, "toarray"):
            block = block.toarray()
        lil[row_slice, col_slice] = block
        self._matrix = lil.tocsr()

    def get_covariance(self) -> sparse.csr_matrix:
        """Returns the full covariance matrix."""
        return self._matrix

    def update_from_propagation(
        self,
        out_slice: slice,
        in_slices: list[slice],
        jacobians: list[sparse.spmatrix | NDArray[Any]],
    ) -> None:
        """Updates the covariance matrix using affine transformation.

        Sigma_new = [[Sigma_old, cross^T], [cross, out]]
        """
        csr_mat = self._matrix
        out_size = out_slice.stop - out_slice.start

        # 1. Compute out_cov = J * Sigma_in * J^T
        j_data, j_row, j_col = [], [], []
        for slc, jac in zip(in_slices, jacobians):
            coo = sparse.coo_matrix(jac)
            j_data.append(coo.data)
            j_row.append(coo.row)
            j_col.append(coo.col + slc.start)

        if not j_data:
            j_in = sparse.csr_matrix((out_size, csr_mat.shape[0]))
        else:
            j_in = sparse.csr_matrix(
                (
                    np.concatenate(j_data),
                    (np.concatenate(j_row), np.concatenate(j_col)),
                ),
                shape=(out_size, csr_mat.shape[0]),
            )

        out_cov = j_in @ csr_mat @ j_in.T
        cross_cov = j_in @ csr_mat

        # 3. Grow matrix using bmat
        self._matrix = sparse.bmat(
            [[csr_mat, cross_cov.T], [cross_cov, out_cov]], format="csr"
        )

    def register_independent_array(self, std_dev: NDArray[Any]) -> slice:
        """Registers a new independent array and returns its slice."""
        val = np.asarray(std_dev)
        size = val.size
        slc = self.allocate(size)

        diag_val = val.flatten() ** 2
        variance = sparse.diags(
            diagonals=[diag_val], offsets=[0], format="csr"
        )

        if self._matrix.shape[0] == 0:
            self._matrix = variance
        else:
            self._matrix = sparse.block_diag(
                (self._matrix, variance), format="csr"
            )
        return slc

    def register_broadcasted_scalar(
        self, variance: float, target_size: int
    ) -> slice:
        """Registers a perfectly correlated broadcasted scalar."""
        slc = self.allocate(target_size)
        block = sparse.csr_matrix(
            np.full((target_size, target_size), variance)
        )

        if self._matrix.shape[0] == 0:
            self._matrix = block
        else:
            self._matrix = sparse.block_diag(
                (self._matrix, block), format="csr"
            )
        return slc
