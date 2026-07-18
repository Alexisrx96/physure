"""Utilities for Physics-Informed Neural Networks."""

from __future__ import annotations

from typing import TYPE_CHECKING

from physure.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from collections.abc import Sequence

    import numpy as np

    from physure.core.protocols import Numeric
    from physure.domain.measurement.base_entity import ExponentsDict
    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit


def _compute_rcond_val(
    rcond: float | None, eps: float, m: int, n: int
) -> float:
    """Returns the effective rcond value, applying the default rule when None."""
    if rcond is not None:
        return rcond
    return eps * max(m, n)


def _null_space_torch(matrix: Numeric, rcond: float | None) -> Numeric:
    """Computes the null-space basis using PyTorch SVD."""
    import torch

    _, s, vh = torch.linalg.svd(matrix, full_matrices=True)
    m, n = matrix.shape

    if s.numel() == 0:
        return vh[0:0].T

    eps = torch.finfo(matrix.dtype).eps
    rcond_val = _compute_rcond_val(rcond, eps, m, n)
    tol = torch.max(s) * rcond_val
    rank = (s > tol).sum()
    return vh[rank:].T


def _null_space_jax(matrix: Numeric, rcond: float | None) -> Numeric:
    """Computes the null-space basis using JAX SVD."""
    import jax.numpy as jnp

    _, s, vh = jnp.linalg.svd(matrix, full_matrices=True)
    m, n = matrix.shape

    eps = jnp.finfo(matrix.dtype).eps
    rcond_val = _compute_rcond_val(rcond, float(eps), m, n)
    max_s = jnp.max(s) if s.size > 0 else 0.0
    tol = max_s * rcond_val
    rank = jnp.sum(s > tol)
    return vh[rank:].T.conj()


def _null_space_numpy(matrix: Numeric, rcond: float | None) -> Numeric:
    """Computes the null-space basis using NumPy SVD."""
    import numpy as np

    if not isinstance(matrix, np.ndarray):
        matrix = np.asarray(matrix)

    _, s, vh = np.linalg.svd(matrix, full_matrices=True)
    m, n = matrix.shape

    if s.size == 0:
        return vh[0:0].T.conj()

    eps = np.finfo(matrix.dtype).eps
    rcond_val = _compute_rcond_val(rcond, float(eps), m, n)
    tol = np.max(s) * rcond_val
    rank = int(np.sum(s > tol))
    return vh[rank:].T.conj()


def null_space_basis(matrix: Numeric, rcond: float | None = None) -> Numeric:
    """Computes an orthonormal basis for the null space (kernel) of a matrix.

    Args:
        matrix: The input matrix (M x N).
        rcond: Relative condition number. If None, uses a safe default.

    Returns:
        A matrix (N x (N - rank)) whose columns form an orthonormal basis
        for the null space of the input matrix.

    Note:
        This function dispatches to the appropriate backend implementation
        (NumPy, PyTorch, or JAX) based on the input type.
    """
    _ = BackendManager.get_backend(matrix)

    module_name = getattr(matrix.__class__, "__module__", "")

    if "torch" in module_name:
        import torch

        if isinstance(matrix, torch.Tensor):
            return _null_space_torch(matrix, rcond)

    if "jax" in module_name or hasattr(matrix, "aval"):
        return _null_space_jax(matrix, rcond)

    return _null_space_numpy(matrix, rcond)


def _resolve_system(
    units_or_quantities: Sequence[Quantity | CompoundUnit],
    system: UnitSystem | None,
) -> UnitSystem:
    """Returns ``system`` unchanged, or infers it from the first element."""
    if system is not None:
        return system
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.units import get_default_system

    if units_or_quantities and isinstance(units_or_quantities[0], Quantity):
        return units_or_quantities[0].system
    return get_default_system()


def _item_dimension(
    item: Quantity | CompoundUnit, system: UnitSystem
) -> Dimension:
    """Returns the dimension of a Quantity or CompoundUnit item."""
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.units import CompoundUnit

    if isinstance(item, Quantity):
        return item.unit.dimension(system)
    if isinstance(item, CompoundUnit):
        return item.dimension(system)
    raise ValueError(f"Cannot extract dimension from {type(item)}")


def _dim_exponents(d: Dimension | ExponentsDict) -> ExponentsDict:
    """Returns a dict-like mapping of base-dimension -> exponent."""
    if hasattr(d, "exponents"):
        return d.exponents
    if isinstance(d, dict):
        return d
    return {}


def extract_dimension_matrix(
    units_or_quantities: Sequence[Quantity | CompoundUnit],
    system: UnitSystem | None = None,
) -> tuple[np.ndarray, list[str]]:
    """Constructs the dimensional constraint matrix D from a list of Units or Quantities.

    Args:
        units_or_quantities: Sequence of Quantity objects or CompoundUnit objects.
        system: The UnitSystem to use for resolving dimensions.
                If None, tries to infer from quantities or uses default.

    Returns:
        D: A matrix (N_dims x N_features) of exponents (NumPy array).
        basis_names: List of names of the base dimensions (rows of D).
    """
    system = _resolve_system(units_or_quantities, system)

    # 1. Extract dimensions for each feature
    dims_list = [_item_dimension(item, system) for item in units_or_quantities]

    # 2. Identify all unique base dimensions
    all_bases: set[str] = set()
    for d in dims_list:
        all_bases.update(_dim_exponents(d).keys())

    sorted_bases = sorted(all_bases, key=str)

    # 3. Build Matrix
    import numpy as np

    dim_matrix = np.zeros((len(sorted_bases), len(dims_list)))
    for j, d in enumerate(dims_list):
        exps = _dim_exponents(d)
        for i, base in enumerate(sorted_bases):
            dim_matrix[i, j] = exps.get(base, 0)

    return dim_matrix, sorted_bases
