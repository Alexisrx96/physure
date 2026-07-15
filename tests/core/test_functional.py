import pytest

from measurekit import Q_
from measurekit.backends.numpy_backend import NumpyBackend
from measurekit.domain.measurement.vectorized_uncertainty import (
    CovarianceStore,
)
from measurekit.functional import FunctionalState, add


def test_functional_add_numpy_basic():
    # Setup
    q1 = Q_(10.0, "m", 1.0)
    q2 = Q_(20.0, "m", 0.5)

    # State
    bk = NumpyBackend()
    store = CovarianceStore(backend=bk)
    state = FunctionalState(store)

    # Add
    q3, new_state = add(q1, q2, state)

    assert q3.magnitude == pytest.approx(30.0)
    # Expected variance: 1.0^2 + 0.5^2 = 1.0 + 0.25 = 1.25 -> sqrt = 1.118
    assert q3.uncertainty == pytest.approx(1.1180339887)

    # Check Matrix size
    # q1 (1) + q2 (1) + q3 (1) = 3
    # Note: FunctionalState logic registers inputs if they don't have slices.
    # q1 has no slice initially (it's created with from_standard ->
    # VarianceModel/CovarianceModel with Lineage)
    # The register logic converts/allocates them in the store.
    assert new_state.matrix.shape == (3, 3)

    # Perform another op with NEW state
    q4, _ = add(q3, q1, new_state)
    assert q4.magnitude == pytest.approx(40.0)
    # q4 = q3 + q1 = (q1 + q2) + q1 = 2*q1 + q2
    # Var = 4*Var(q1) + Var(q2) = 4*1 + 0.25 = 4.25 -> sqrt = 2.0615
    assert q4.uncertainty == pytest.approx(2.06155281)
