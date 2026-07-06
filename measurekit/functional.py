"""Functional API for explicit covariance state management.

This module provides a functional interface for arithmetic operations on Quantities,
allowing the user to explicitly manage the covariance state (matrix) rather than
relying on a global context. This is essential for JAX transformations
(jit, vmap, pmap) and complex distributed scenarios.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, TypeVar

from measurekit.domain.measurement.vectorized_uncertainty import (
    CovarianceStore,
    propagate_affine,
)

if TYPE_CHECKING:
    from measurekit.domain.measurement.quantity import Quantity

T = TypeVar("T")


class FunctionalState:
    """Holds the explicit state for functional propagation.

    Wraps the allocator (metadata) and the current covariance matrix (data).
    """

    def __init__(
        self,
        store: CovarianceStore | None = None,
        matrix: Any = None,
        registry: dict[int, slice] | None = None,
    ):
        """Initializes the functional state.

        Args:
            store: The covariance store allocator.
            matrix: The explicit covariance data matrix.
            registry: Mapping of object IDs to allocated slices.
        """
        if store is None:
            # Create a detached store (no global backend default)
            # We need a backend to init store?
            # We defer until first usage or require a backend.
            # Assuming numpy for default if totally empty
            from measurekit.backends.numpy_backend import NumpyBackend

            store = CovarianceStore(backend=NumpyBackend())

        self.store = store
        # If matrix is provided, it overrides the store's internal matrix
        # This allows 'threading' the matrix through JAX graph while
        # keeping the store allocator consistent.
        if matrix is not None:
            self.matrix = matrix
        elif hasattr(store, "_matrix"):
            self.matrix = store._matrix
        else:
            # Initialize empty matrix using backend
            # We assume dense or sparse depending on backend defaults?
            # For functional API, usually sparse if possible.
            # Or call backend.zeros((0,0))?
            # Or backend.sparse_matrix(..., shape=(0,0))?

            # If backend is known:
            bk = store.backend
            if hasattr(bk, "sparse_matrix"):
                # Create empty sparse matrix
                self.matrix = bk.sparse_matrix([], ([], []), shape=(0, 0))
            else:
                self.matrix = bk.zeros((0, 0))

        self.registry = registry if registry is not None else {}

    def allocate(self, size: int) -> slice:
        """Allocates a slice in the state (mutates allocator metadata)."""
        return self.store.allocate(size)

    def ensure_registered(self, q: Quantity) -> tuple[slice, Any]:
        """Ensures a quantity is registered in the state, updating matrix if needed.

        Returns:
            (slice, updated_matrix)
        """
        # Checks if quantity already has a vector_slice
        # If not, allocates and updates matrix (diagonal variance)

        key = id(q)
        if key in self.registry:
            return self.registry[key], self.matrix

        # Access internal Uncertainty
        # We check if it's a CovarianceModel. It might be stored in
        # Rust's value.uncertainty (TensorBackend)
        # or it might be the result of a property access.
        unc = q.uncertainty
        from measurekit.domain.measurement.uncertainty import CovarianceModel

        backend = self.store.backend

        # If it's a CovarianceModel, it might already have a slice
        if isinstance(unc, CovarianceModel) and unc.vector_slice is not None:
            return unc.vector_slice, self.matrix

        # Need to register
        val = backend.asarray(q.uncertainty)
        size = backend.size(val)
        slc = self.allocate(size)  # Mutation of allocator

        # Calculate Variance Diag
        diag_val = backend.reshape(backend.pow(val, 2), (-1,))
        variance = backend.sparse_diags([diag_val], [0], shape=(size, size))

        # Append to matrix
        # matrix = bmat([[current, 0], [0, variance]])
        if self.matrix is None:
            new_matrix = variance
        else:
            # Check if matrix is empty/zero-shape
            if hasattr(self.matrix, "shape") and self.matrix.shape == (0, 0):
                new_matrix = variance
            else:
                new_matrix = backend.sparse_bmat(
                    [[self.matrix, None], [None, variance]]
                )

        self.registry[key] = slc
        return slc, new_matrix

    def tree_flatten(self):
        """Flattens the state for JAX."""
        children = (self.matrix,)
        # We treat store and registry as auxiliary data (static metadata)
        # Note: 'store' is mutable, so this is tricky. Effectively we capture
        # the state of the store at the time of flattening.
        aux_data = (self.store, self.registry)
        return children, aux_data

    @classmethod
    def tree_unflatten(cls, aux_data, children):
        """Reconstructs the state."""
        matrix = children[0]
        store, registry = aux_data
        return cls(store, matrix, registry)


def add(
    a: Quantity, b: Quantity, state: FunctionalState
) -> tuple[Quantity, FunctionalState]:
    """Functional addition: (a + b, new_state)."""
    return _apply_affine(a, b, state, 1.0, 1.0)


def sub(
    a: Quantity, b: Quantity, state: FunctionalState
) -> tuple[Quantity, FunctionalState]:
    """Functional subtraction: (a - b, new_state)."""
    return _apply_affine(a, b, state, 1.0, -1.0)


def _apply_affine(
    a: Quantity,
    b: Quantity,
    state: FunctionalState,
    jac_a: float,
    jac_b: float,
) -> tuple[Quantity, FunctionalState]:
    """Helper for affine operations."""
    backend = state.store.backend
    if backend is None:
        raise ValueError(
            "FunctionalState's CovarianceStore has no backend set."
        )

    # 1. Register inputs in state
    slc_a, mat_1 = state.ensure_registered(a)
    state.matrix = mat_1  # Update intermediate

    slc_b, mat_2 = state.ensure_registered(b)
    state.matrix = mat_2

    # 2. Compute Result Magnitude
    res_mag = (
        backend.add(a.magnitude, b.magnitude)
        if jac_b > 0
        else backend.sub(a.magnitude, b.magnitude)
    )

    # 3. Propagate
    out_size = backend.size(res_mag)
    out_slice = state.allocate(out_size)

    # Construct Jacobians
    val_a_ones = backend.ones((out_size,), reference=res_mag)
    diag_a = backend.mul(val_a_ones, jac_a)
    j_a = backend.sparse_diags([diag_a], [0], shape=(out_size, out_size))

    val_b_ones = backend.ones((out_size,), reference=res_mag)
    diag_b = backend.mul(val_b_ones, jac_b)
    j_b = backend.sparse_diags([diag_b], [0], shape=(out_size, out_size))

    jacs = [j_a, j_b]

    new_matrix = propagate_affine(
        state.matrix, out_slice, [slc_a, slc_b], jacs, backend
    )

    # 4. Construct Result Quantity
    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.uncertainty import CovarianceModel

    diag = backend.sparse_diagonal(new_matrix)
    res_diag = diag[out_slice]
    res_std = backend.reshape(backend.sqrt(res_diag), backend.shape(res_mag))

    res_unc = CovarianceModel(
        std_dev_internal=res_std,
        vector_slice=out_slice,
    )

    res_q = Quantity.from_input(
        value=res_mag,
        unit=a.unit,
        system=a.system,
        uncertainty=res_unc,
    )

    # Register the result in the new state to avoid re-registration
    new_registry = dict(state.registry)
    new_registry[id(res_q)] = out_slice

    return res_q, FunctionalState(state.store, new_matrix, new_registry)


def register_functional_pytree():
    """Registers FunctionalState as a JAX Pytree."""
    try:
        import jax

        jax.tree_util.register_pytree_node(
            FunctionalState,
            FunctionalState.tree_flatten,
            FunctionalState.tree_unflatten,
        )
    except (ImportError, NameError):
        pass
