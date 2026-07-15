"""Autograd wrapper for automatic differentiation backend abstraction."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from physure.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

    from physure.core.protocols import Numeric

log = logging.getLogger(__name__)

# Heavy libraries (torch, jax) are imported lazily inside methods.


class AutogradPropagator:
    """Propagator that uses backend AD to compute Jacobians."""

    @staticmethod
    def compute_jacobians(
        func: Callable[..., Numeric],
        primals: Sequence[Numeric],
    ) -> tuple[Numeric, tuple[Numeric, ...]]:
        """Computes the result and Jacobians of func with respect to primals.

        Args:
            func: The function to differentiate.
            primals: The input values (arguments to func).

        Returns:
            (result, jacobians) where jacobians is a tuple of J per input.
        """
        if not primals:
            return func(), ()

        # Detect backend from the first primal that is an array
        backend_name = "numpy"  # default / scalar
        for p in primals:
            backend = BackendManager.get_backend(p)
            # Check modules
            if "torch" in backend.__class__.__name__.lower():
                backend_name = "torch"
                break
            if "jax" in backend.__class__.__name__.lower():
                backend_name = "jax"
                break

        # Dispatch
        if backend_name == "torch":
            return AutogradPropagator._compute_torch(func, primals)
        if backend_name == "jax":
            return AutogradPropagator._compute_jax(func, primals)

        # Fallback for numpy/python (Numerical Diff)
        return AutogradPropagator._compute_finite_diff(func, primals)

    @staticmethod
    def _compute_torch(
        func: Callable[..., Numeric], primals: Sequence[Numeric]
    ) -> tuple[Numeric, tuple[Numeric, ...]]:
        try:
            import torch
            import torch.func
        except ImportError as err:
            raise ImportError("Torch not available for Autograd.") from err

        # primals might be a mix of tensors and scalars.
        inputs_t = []
        for p in primals:
            if not isinstance(p, torch.Tensor):
                inputs_t.append(torch.as_tensor(p))
            else:
                inputs_t.append(p)

        inputs_tuple = tuple(inputs_t)

        try:
            # Better strategy: define a function that takes N arguments
            jac_fn = torch.func.jacrev(
                func, argnums=tuple(range(len(inputs_tuple)))
            )

            # Execute
            result = func(*inputs_tuple)
            jacobians = jac_fn(*inputs_tuple)

            # jacobians is a tuple of Jacobians, one per input
            return result, jacobians

        except Exception as e:
            log.warning(f"Torch Autograd failed: {e}")
            raise e

    @staticmethod
    def _compute_jax(
        func: Callable[..., Numeric], primals: Sequence[Numeric]
    ) -> tuple[Numeric, tuple[Numeric, ...]]:
        try:
            import jax
            import jax.numpy as jnp
        except ImportError as err:
            raise ImportError("JAX not available for Autograd.") from err

        # Ensure arrays
        inputs = [jnp.asarray(p) for p in primals]

        # JAX jacfwd is generally better for forward-mode (wide input, narrow output?)
        # Actually Jacobian shape: (out, in).
        # Forward mode (jacfwd) good if in < out.
        # Reverse mode (jacrev) good if out < in.
        # Defaulting to jacfwd is safer for JAX often, or heuristic.
        jac_fn = jax.jacfwd(func, argnums=list(range(len(inputs))))

        result = func(*inputs)
        jacobians = jac_fn(*inputs)

        # If only 1 input, jax might return single item not tuple?
        if not isinstance(jacobians, (list, tuple)):
            jacobians = (jacobians,)

        return result, tuple(jacobians)

    @staticmethod
    def _compute_finite_diff(
        func: Callable[..., Numeric], primals: Sequence[Numeric]
    ) -> tuple[Numeric, tuple[Numeric, ...]]:
        """Simple finite difference fallback (Numerical Perturbation)."""
        # Central difference
        epsilon = 1e-5
        result = func(*primals)

        jacs = []
        for i, p in enumerate(primals):
            try:
                # Create copies of args
                args_plus = list(primals)
                args_minus = list(primals)

                # Perturb scalar
                args_plus[i] = p + epsilon
                args_minus[i] = p - epsilon

                f_plus = func(*args_plus)
                f_minus = func(*args_minus)

                # Derivative
                deriv = (f_plus - f_minus) / (2 * epsilon)
                jacs.append(deriv)
            except Exception:
                jacs.append(None)

        return result, tuple(jacs)
