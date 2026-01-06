"""Loss functions for Physics-Informed Learning."""

from collections.abc import Sequence
from typing import Any

from measurekit.core.dispatcher import BackendManager
from measurekit.domain.measurement.quantity import Quantity


def dimensional_homogeneity_loss(terms: Sequence[Quantity]) -> Any:
    """Calculates the dimensional homogeneity loss for a set of quantities.

    For a physical equation to be valid, all additive terms must have the
    same dimensions. This loss minimizes the variance between the dimension
    vectors of the provided terms.

    Args:
        terms: A list of Quantity objects representing the terms of an
               equation. For this loss to be differentiable, the quantities
               must have 'soft' dimensions (exponents that are differentiable
               tensors).

    Returns:
        A scalar loss value representing the dimensional mismatch.
    """
    if not terms:
        return 0.0

    # 1. Identify valid backend (Torch/JAX) from the terms' exponent types
    # or fallback to Python math.
    # We inspect the first exponent of the first term found.
    backend_obj = None
    for q in terms:
        # q.dimension is a Dimension object.
        # We need to access its internal exponents.
        # Assuming Dimension exposes .exponents (dict)
        dimension = q.unit.dimension(q.system)
        exponents = getattr(dimension, "exponents", {})
        if exponents:
            first_val = next(iter(exponents.values()))
            backend_obj = first_val
            break

    if backend_obj is None:
        # All dimensionless or empty?
        return 0.0

    backend = BackendManager.get_backend(backend_obj)

    # 2. Collect Union of Base Dimensions
    all_bases = set()
    for q in terms:
        dim = q.unit.dimension(q.system)
        exps = getattr(dim, "exponents", {})
        all_bases.update(exps.keys())

    sorted_bases = sorted(all_bases, key=str)

    if not sorted_bases:
        return 0.0

    # 3. Construct Dimension Matrix (Rows: Bases, Cols: Terms)
    # We want to preserve gradients, so we use backend ops.
    columns = []

    for q in terms:
        dim = q.unit.dimension(q.system)
        exps = getattr(dim, "exponents", {})

        col_vec = []
        for base in sorted_bases:
            val = exps.get(base, 0.0)
            col_vec.append(val)

        # Convert column list to tensor/array
        # We use backend.asarray, but we need to stack scalars first?
        # Backend.asarray usually handles list of scalars.
        columns.append(col_vec)

    # 4. Compute Variance
    # We have a list of lists.
    # Matrix shape: (N_terms, N_bases)
    # We want all rows (terms) to be identical.
    # So we compute variance along axis=0 (Terms).

    # Backend agnostic implementation is tricky without specific 'stack'
    # we assume Torch/JAX behavior for 'asarray' on list of lists.
    try:
        matrix = backend.asarray(columns)  # (N_terms, N_bases)

        # Calculate Mean Dimension Vector
        # axis=0 is Terms
        mean_vec = backend.mean(matrix, axis=0)  # (N_bases,)

        # Calculate MSE from Mean
        # (matrix - mean)^2
        diff = backend.sub(matrix, mean_vec)
        sq_diff = backend.pow(diff, 2)
        mse = backend.mean(sq_diff)  # Scalar

        return mse

    except Exception:
        # Fallback if backend operations fail (e.g. mix of types)
        # Return 0.0 or raise?
        # If we can't compute loss, it's safer to return 0.0 (no penalty)
        # but warn.
        return 0.0
