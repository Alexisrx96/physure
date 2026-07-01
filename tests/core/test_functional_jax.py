import numpy as np
import pytest

try:
    import jax
    import jax.numpy as jnp
    from jax import jit
except ImportError:
    pytest.skip("JAX not installed", allow_module_level=True)

from measurekit import Q_
from measurekit.backends.jax_backend import JaxBackend, register_jax_behavior
from measurekit.functional import FunctionalState, add

# Register Pytrees (Quantity, CovarianceModel)
register_jax_behavior()


def test_jax_functional_add_jit_execution():
    # 1. Setup State with JAX Backend support
    # We pass a distinct store initialized with JaxBackend to FunctionalState?
    # Or rely on FunctionalState to use the backend of the matrix?

    # We initialize matrix as a JAX array (empty sparse or zeroes)
    # FunctionalState defaults to NumpyBackend internally if store not passed.
    # We should explicitly pass a store with JaxBackend for JAX usage.
    from measurekit.domain.measurement.vectorized_uncertainty import (
        CovarianceStore,
    )

    backend = JaxBackend()
    store = CovarianceStore(backend=backend)

    # Create initial state
    # Start with empty matrix (Tracer-friendly empty)
    # But CovarianceStore expects 2D matrix.
    # We can let the first ensure_registered create it?
    # But JIT needs input shape.
    # Let's start with a dummy non-empty state to have fixed shape?
    # Or empty BCOO (0,0).

    # Empty BCOO
    from jax.experimental import sparse

    initial_matrix = sparse.BCOO(
        (jnp.zeros(0), jnp.zeros((0, 2), dtype=int)), shape=(0, 0)
    )

    state = FunctionalState(store=store, matrix=initial_matrix)

    # 2. Define Function to JIT
    @jit
    def graph_op(q1, q2, st):
        # q1 + q2 -> res, st2
        res, st2 = add(q1, q2, st)
        # q1 + res -> res2, st3
        res2, st3 = add(q1, res, st2)
        return res2, st3

    # 3. Create Inputs
    v1 = jnp.array([10.0])
    v2 = jnp.array([20.0])
    # JAX inputs usually need standard deviation as JAX array too
    u1 = jnp.array([1.0])
    u2 = jnp.array([2.0])

    q1 = Q_(v1, "m", uncertainty=u1)
    q2 = Q_(v2, "m", uncertainty=u2)

    # 4. Trace & Execute
    # Note: validation of shapes happens here.
    # If FunctionalState is not a Pytree, this will fail (JAX sees Python object)
    # if we register it, it works.

    # We need to register FunctionalState as a Pytree for this test to pass.
    # We will do this in functional.py, but for the test environment we might need to trigger it.

    from measurekit.functional import register_functional_pytree

    register_functional_pytree()

    res_out, _state_out = graph_op(q1, q2, state)

    # 5. Verify Results
    assert isinstance(res_out.magnitude, jax.Array)
    # 10 + 20 + 10 = 40
    assert float(res_out.magnitude[0]) == 40.0

    # Check uncertainty propagation (correctness)
    # q1 (1.0), q2 (2.0)
    # res = q1 + q2 -> var = 1+4 = 5
    # res2 = q1 + res -> var(q1 + (q1+q2)) = var(2*q1 + q2) = 4*1 + 4 = 8
    # Correlation between q1 and res should be handled.
    # q1 registered once.
    # 'res' registered in st2.
    # 'res2' registered in st3.

    # The final matrix in state_out should be large enough
    # q1 (1), q2 (1), res (1), res2 (1) -> 4 elements
    # Since we reused q1, it shouldn't re-register?

    # The output uncertainty should be correct
    assert res_out.uncertainty is not None
    # var(2*q1 + q2) = 4*var(q1) + var(q2) + 0 (independent) = 4*1 + 4 = 8
    # std = sqrt(8) = 2.828

    expected_std = np.sqrt(8.0)
    actual_std = float(res_out.uncertainty[0])
    assert abs(actual_std - expected_std) < 1e-4


if __name__ == "__main__":
    test_jax_functional_add_jit_execution()
