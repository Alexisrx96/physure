"""Gradient Correctness Tests (Finite Differences vs Automatic Differentiation)."""

import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from measurekit import Quantity, get_unit
from tests.strategies import linear_units

# Check availability
try:
    import torch

    TORCH_AVAILABLE = True
except ImportError:
    TORCH_AVAILABLE = False

try:
    import jax
    import jax.numpy as jnp

    JAX_AVAILABLE = True
except ImportError:
    JAX_AVAILABLE = False


def central_difference_grad(func, x: Quantity, epsilon=1e-4):
    """Computes numerical gradient using central difference."""
    # We perturb the magnitude of x
    # x must be scalar-like for simple grad check here, or we compute directional derivative?
    # Simple case: scalar -> scalar function.

    # Create perturbation with same unit
    # eps needs to indicate a small step in magnitude.
    # relative step?
    step = epsilon
    if abs(x.magnitude) > 1.0:
        step = epsilon * abs(x.magnitude)

    # Construct x+h and x-h
    # Note: Preserving system implies using same factory/system
    # We manually construct to avoid overhead

    # We cast step to backend?
    # Assume float step works with backend overloading

    x_plus = Quantity.from_input(x.magnitude + step, x.unit, x.system)
    x_minus = Quantity.from_input(x.magnitude - step, x.unit, x.system)

    y_plus = func(x_plus)
    y_minus = func(x_minus)

    # dy/dx ~ (y_plus - y_minus) / (2*step)
    # The result has unit y.unit / x.unit

    diff = y_plus - y_minus
    # Only magnitude division, unit handled by Quantity truediv equivalent
    # diff is a Quantity.
    # We divide by (2*step). But 2*step is a scalar (dimensionless magnitude change? No).
    # The perturbation `step` is in units of `x`.
    # So we divide by Quantity(2*step, x.unit).

    denom = Quantity.from_input(2 * step, x.unit, x.system)

    return diff / denom


@settings(max_examples=20, deadline=None)
@given(st.floats(min_value=0.1, max_value=5.0), linear_units())
def test_gradient_pow2_scalar_jax(val, unit_name):
    """Checks gradient of x^2 using JAX."""
    if not JAX_AVAILABLE:
        pytest.skip("JAX not available")

    unit = get_unit(unit_name)
    # Create JAX quantity
    q = Quantity.from_input(jnp.array(val), unit, None)

    def func(x):
        return x * x  # x^2

    # Analytical / Auto Grad
    # Function must return scalar magnitude for jax.grad?
    # jax.grad differentiates a function that returns a scalar.
    # If func returns a Quantity, jax.grad won't work directly unless we wrap/unwrap
    # OR if Quantity is a Pytree, JAX sees the magnitude inside?
    # JAX differentiates with respect to the leaves.
    # Quantity leaf is (magnitude, uncertainty).
    # We want grad w.r.t magnitude.

    # Wrapper for JAX
    def scalar_func(mag):
        # reconstruct quantity
        qx = Quantity.from_input(mag, unit, None)
        y = func(qx)
        return y.magnitude  # return raw value for JAX

    grad_fn = jax.grad(scalar_func)
    analytical_mag = grad_fn(q.magnitude)

    # Numerical Grad
    num_grad_q = central_difference_grad(func, q)
    num_grad_mag = num_grad_q.magnitude

    # x^2 -> 2x. Unit: u^2 / u = u.
    # The analytical_mag returned by jax.grad is purely numerical (no unit knowledge).
    # num_grad_q has the correct unit.

    # Check value
    np.testing.assert_allclose(analytical_mag, num_grad_mag, rtol=1e-3)

    # Check unit of numerical grad
    # Expected: u^2 / u = u
    assert num_grad_q.unit == unit


@settings(max_examples=20, deadline=None)
@given(st.floats(min_value=0.1, max_value=5.0), linear_units())
def test_gradient_pow2_scalar_torch(val, unit_name):
    """Checks gradient of x^2 using Torch."""
    if not TORCH_AVAILABLE:
        pytest.skip("Torch not available")

    unit = get_unit(unit_name)
    # Create Torch quantity with requires_grad
    # We need to set requires_grad on the tensor
    t_val = torch.tensor(val, dtype=torch.float32, requires_grad=True)
    q = Quantity.from_input(t_val, unit, None)

    def func(x):
        return x * x

    # Forward
    y = func(q)
    # y is Quantity. y.magnitude is tensor.

    # Backward
    y.magnitude.backward()

    analytical_mag = t_val.grad.item()

    # Numerical
    # We need a fresh q for numerical to avoid graph issues? No, separate logic.
    # But central_difference_grad creates new Quantities/Tensors.
    # We should ensure we don't track gradients there if not needed, but it doesn't hurt.
    num_grad_q = central_difference_grad(
        func, Quantity.from_input(val, unit, None)
    )
    # Note: passing python float to central_diff for simplicity to avoid torch graph retention in diff key

    np.testing.assert_allclose(analytical_mag, num_grad_q.magnitude, rtol=1e-3)
    assert num_grad_q.unit == unit
