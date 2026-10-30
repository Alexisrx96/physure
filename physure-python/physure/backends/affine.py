"""Affine propagation of a covariance matrix, written against a backend."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from physure.core.protocols import BackendOps, Numeric


def propagate_affine(
    matrix: Any,
    out_slice: slice,
    in_slices: list[slice],
    jacobians: list[Numeric],
    backend: BackendOps,
) -> Any:
    """Performs J * Sigma * J.T update on the matrix (Functional API).

    Computes:
       C = J * matrix (Cross-covariance)
       V = J * matrix * J.T = C * J.T (Output Variance)
       New_Matrix = [[matrix, C.T], [C, V]]

    Args:
        matrix: Current covariance matrix (N x N).
        out_slice: Slice for output variables (must follow matrix indices).
        in_slices: Slices for input variables (must be within matrix).
        jacobians: List of Jacobian blocks corresponding to in_slices.
        backend: Backend operations provider.

    Returns:
        The augmented covariance matrix ((N+M) x (N+M)).
    """
    # 1. Compute correlations: C = J @ Sigma (M x N)
    # We compute this by parts: sum(J_k @ Sigma[k])

    m_size = out_slice.stop - out_slice.start
    c_accum = None

    for in_slc, jac in zip(in_slices, jacobians, strict=False):
        # Sigma subset: rows corresponding to input
        sigma_part = backend.sparse_slice(matrix, in_slc, slice(None))

        term = backend.sparse_matmul(jac, sigma_part)

        c_accum = term if c_accum is None else backend.add(c_accum, term)

    # c_accum is now C (M x N)

    # 2. Compute variance: V = C @ J.T (M x M)
    # V = sum(C[:, k] @ J_k.T)

    v_accum = None
    if c_accum is not None:
        for in_slc, jac in zip(in_slices, jacobians, strict=False):
            c_part = backend.sparse_slice(c_accum, slice(None), in_slc)
            term = backend.sparse_matmul(c_part, backend.transpose(jac))

            v_accum = term if v_accum is None else backend.add(v_accum, term)

    if v_accum is None:
        # Fallback for empty jacobians (should unlikely happen in affine)
        # Create zero matrix M x M
        v_accum = backend.sparse_eye(m_size, reference=matrix)
        v_accum = backend.mul(v_accum, 0.0)

    # 3. Assemble
    # [[ matrix, C.T ], [ C, V ]]

    # Handling None c_accum (e.g. if jacobians empty)
    if c_accum is None:
        # Need zero block M x N.
        _ = backend.shape(matrix)[0]
        # This is hard without explicit zero constructor for sparse M x N
        # unless backend supports generic zeros.
        # Assuming sparse_matmul returned something valid or loop didn't run.
        # If loop didn't run, matrix has N rows.
        # Construct zero matrix?
        # For now assume jacobians not empty.

    new_mat = backend.sparse_bmat(
        [[matrix, backend.transpose(c_accum)], [c_accum, v_accum]]
    )

    return new_mat
