import pytest

try:
    import jax
    import jax.numpy as jnp
except ImportError:
    pytest.skip("JAX not available", allow_module_level=True)

try:
    from physure import Q_
except ImportError:
    # If package not installed in editable mode yet, adjust path
    import os
    import sys

    sys.path.append(
        os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../"))
    )
    from physure import Q_

from physure.domain.measurement.quantity import Quantity

try:
    from physure.backends.jax_backend import register_jax_behavior

    register_jax_behavior()
except (ImportError, AttributeError):
    pass


def test_jit_basic():
    """Test that Quantity can be passed to jax.jit compiled functions."""

    @jax.jit
    def double_quantity(q):
        # Python control flow on static unit is allowed
        # q inside is a Quantity with Tracer magnitude
        return q * 2

    q = Q_(10.0, "m")
    out = double_quantity(q)

    assert isinstance(out, Quantity)
    assert out.magnitude == 20.0
    assert str(out.unit) == "m"


def test_jit_control_flow_static():
    """Test safe control flow based on Units (static)."""

    @jax.jit
    def convert_safe(q):
        # This check happens at trace time (static)
        if str(q.unit) == "m":
            return q * 100  # return cm values roughly? No, just *100
        return q

    q = Q_(5.0, "m")
    # First call traces
    out1 = convert_safe(q)
    assert out1.magnitude == 500.0

    # Second call with SAME unit should use cached trace
    q2 = Q_(2.0, "m")
    out2 = convert_safe(q2)
    assert out2.magnitude == 200.0


def test_grad():
    """Test gradient computation through Quantity."""

    @jax.grad
    def quantity_loss(q):
        # Loss = sum(magnitude^2)
        # JAX tracks gradients through magnitude
        return jnp.sum(q.magnitude**2)

    # Gradient of x^2 is 2x. At x=3, grad=6.
    q = Q_(3.0, "m")

    # jax.grad returns the gradient w.r.t the first argument 'q'.
    # Since 'q' is a Pytree (Quantity), the gradient is a Quantity structure
    # containing the gradients of the leaves (magnitude).
    # aux_data (unit) has no gradient.

    grad_q = quantity_loss(q)

    assert isinstance(grad_q, Quantity)
    assert grad_q.magnitude == 6.0
    # Gradient of a value in meters is in 1/meters? Or meters?
    # Physically: d(m^2)/d(m) = m.
    # But JAX grad simply returns numerical gradient matching input shape/structure.
    # It does NOT perform unit analysis on gradients automatically unless we code it.
    # Quantity.tree_flatten passes (mag, unc) as differentiable children.
    # So `grad_q` reconstructs Quantity with `grad_mag` as magnitude.
    # The unit will be "m" (passed in aux_data).
    assert str(grad_q.unit) == "m"


def test_vmap():
    """Test vectorization over a batch of Quantities."""

    # Batch of 3 items
    # We must ensure uncertainty is also compatible if it's a child.
    # If uncertainty is 0.0 (default), it's a scalar.
    # vmap(in_axes=0) expects all leaves to have dimension 0 size 3.
    # scalar 0.0 fails this.
    # To test vmap successfully with default Quantity, we must prepare
    # a Quantity with batched uncertainty OR handle it.
    # Since Phase 2 hasn't fully revamped Uncertainty for JAX broadcasting,
    # we explicitly create a quantity with array uncertainty to be safe.

    mags = jnp.array([1.0, 2.0, 3.0])
    uncs = jnp.array([0.1, 0.1, 0.1])

    q_batch = Quantity(
        mags, Q_(1, "m").unit, uncertainty_obj=uncs
    )  # Hacky constr?
    # Better:
    q_batch = Q_(mags, "m")
    # Overwrite uncertainty or rely on Q_ handling array -> array uncertainty?
    # If Q_ detects array magnitude, does it make uncertainty array?
    # Q_.from_input: "if backend.is_array(value)... uncertainty = backend.mul(ones, uncertainty)"
    # So YES, Q_([1,2,3], "m") creates array uncertainty!

    @jax.vmap
    def add_scalar(q):
        # q here represents a SINGLE slice (scalar magnitude, scalar unc)
        return q + Q_(10.0, "m")

    out_batch = add_scalar(q_batch)

    assert jnp.array_equal(out_batch.magnitude, jnp.array([11.0, 12.0, 13.0]))
    assert str(out_batch.unit) == "m"


def test_tracer_safe_ops():
    """Test that operations inside JIT don't trigger bool() errors."""

    @jax.jit
    def complex_logic(q1, q2):
        # This will fail if __add__ or __sub__ uses loose 'if value'
        # or checks truthiness of array
        res = q1 + q2
        res = res * 2
        return res

    q1 = Q_(jnp.array([1.0, 2.0]), "m")
    q2 = Q_(jnp.array([3.0, 4.0]), "m")

    out = complex_logic(q1, q2)
    assert jnp.allclose(out.magnitude, jnp.array([8.0, 12.0]))
