"""Utilities for Physics-Informed Neural Networks."""

from collections.abc import Sequence
from typing import Any

from measurekit.core.dispatcher import BackendManager


def null_space_basis(matrix: Any, rcond: float | None = None) -> Any:
    """Computes an orthonormal basis for the null space (kernel) of a matrix.

    Args:
        matrix: The input matrix (M x N).
        rcond: Relative condition number. If None, uses a safe default based on dtype.

    Returns:
        A matrix (N x (N - rank)) whose columns form an orthonormal basis
        for the null space of the input matrix.

    Note:
        This function dispatches to the appropriate backend implementation
        (NumPy, PyTorch, or JAX) based on the input type.
    """
    _ = BackendManager.get_backend(matrix)

    # Check for PyTorch tensor
    module_name = getattr(matrix.__class__, "__module__", "")

    if "torch" in module_name:
        import torch

        if isinstance(matrix, torch.Tensor):
            # Use strict full_matrices=True to get the complete Vh
            _, s, vh = torch.linalg.svd(matrix, full_matrices=True)

            # Determine rank
            m, n = matrix.shape
            if s.numel() > 0:
                if rcond is None:
                    # Default: eps * max(m, n)
                    # For float32, eps ~ 1e-7. For float64 ~ 2e-16.
                    eps = torch.finfo(matrix.dtype).eps
                    rcond_val = eps * max(m, n)
                else:
                    rcond_val = rcond

                tol = torch.max(s) * rcond_val
                rank = (s > tol).sum()
            else:
                rank = 0

            # The null space corresponds to the rows of Vh (cols of V)
            # associated with singular values (~0).
            # These are the last (n - rank) rows of Vh.
            null_space = vh[rank:].T
            return null_space

    # Check for JAX array
    elif "jax" in module_name or hasattr(matrix, "aval"):
        import jax.numpy as jnp

        # Use simple svd
        _, s, vh = jnp.linalg.svd(matrix, full_matrices=True)

        m, n = matrix.shape
        # Handle case where s might be empty or zero-rank?
        # jnp.sum returns a tracer or value.

        # Safely compute tolerance
        # rcond logic
        if rcond is None:
            # JAX epsilon
            # Assuming float32 default
            eps = jnp.finfo(matrix.dtype).eps
            rcond_val = eps * max(m, n)
        else:
            rcond_val = rcond

        max_s = jnp.max(s) if s.size > 0 else 0.0
        tol = max_s * rcond_val
        rank = jnp.sum(s > tol)

        # JAX dynamic slicing can be tricky with JIT, but for initialization
        # (usually outside JIT) this is fine. If inside JIT, rank is dynamic.
        # Assuming this is mostly for initialization or fixed D.

        # We need to slice vh.
        # vh is (N, N). We want rows [rank:].
        # vh[rank:] yields shape (N-rank, N). Transpose to (N, N-rank).

        # Note: If rank is a Tracer, this slicing will fail in JAX without static_slice or similar,
        # unless we are not JIT-ing this or D is constant.
        # Given D (dimensions) is usually constant, this should be fine.

        return vh[rank:].T.conj()

    # Fallback to NumPy
    # Explicitly import numpy to avoid assuming it's everywhere if user didn't install it
    # (though unlikely for this package).
    import numpy as np

    # Ensure it's an array
    if not isinstance(matrix, np.ndarray):
        matrix = np.asarray(matrix)

    u, s, vh = np.linalg.svd(matrix, full_matrices=True)
    m, n = matrix.shape
    if s.size > 0:
        if rcond is None:
            eps = np.finfo(matrix.dtype).eps
            rcond_val = eps * max(m, n)
        else:
            rcond_val = rcond

        tol = np.max(s) * rcond_val
        rank = np.sum(s > tol)
    else:
        rank = 0

    return vh[rank:].T.conj()


def extract_dimension_matrix(
    units_or_quantities: Sequence[Any], system: Any = None
) -> tuple[Any, list[str]]:
    """Constructs the dimensional constraint matrix D from a list of Units or Quantities.

    Args:
        units_or_quantities: Sequence of Quantity objects or CompoundUnit objects.
        system: The UnitSystem to use for resolving dimensions.
                If None, tries to infer from quantities or uses default.

    Returns:
        D: A matrix (N_dims x N_features) of exponents.
           Type varies (Torch Tensor or Numpy Array) depending on backend preference?
           Defaults to Numpy for initialization logic.
        basis_names: List of names of the base dimensions (rows of D).
    """
    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.units import (
        CompoundUnit,
        get_default_system,
    )

    if system is None:
        if len(units_or_quantities) > 0 and isinstance(
            units_or_quantities[0], Quantity
        ):
            system = units_or_quantities[0].system
        else:
            system = get_default_system()

    # 1. Extract dimensions for each feature
    dims_list = []
    for item in units_or_quantities:
        if isinstance(item, Quantity):
            dims_list.append(item.unit.dimension(system))
        elif isinstance(item, CompoundUnit):
            dims_list.append(item.dimension(system))
        else:
            # Fallback/Error?
            raise ValueError(f"Cannot extract dimension from {type(item)}")

    # 2. Identify all unique base dimensions
    all_bases = set()
    for d in dims_list:
        # Dimension object underlying storage depends on implementation
        # Assuming Dimension wraps a dict-like of {BaseDimension: exponent}
        if hasattr(d, "exponents"):
            all_bases.update(d.exponents.keys())
        elif isinstance(d, dict):
            all_bases.update(d.keys())
        else:
            # best effort
            pass

    sorted_bases = sorted(all_bases, key=str)

    # 3. Build Matrix
    import numpy as np

    D = np.zeros((len(sorted_bases), len(dims_list)))

    for j, d in enumerate(dims_list):
        # Access safely
        exps = getattr(d, "exponents", d)
        for i, base in enumerate(sorted_bases):
            D[i, j] = exps.get(base, 0)

    return D, sorted_bases
