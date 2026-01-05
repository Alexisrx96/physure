import numpy as np

from measurekit.backends.numpy_backend import NumpyBackend
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.units import (
    get_default_system,
)
from measurekit.domain.measurement.vectorized_uncertainty import (
    ensure_store,
)


def test_correlated_arrays():
    system = get_default_system()
    meter = system.get_unit("meter")
    store = ensure_store(NumpyBackend())

    val_base = np.array([1.0, 2.0])
    unc_base = np.array([0.1, 0.2])
    q_base = Quantity.from_input(val_base, meter, system, uncertainty=unc_base)

    # q1 = q_base + 10
    q1 = q_base + Quantity.from_input(
        np.array([10.0, 10.0]), meter, system, uncertainty=0.0
    )
    # q2 = q_base * 2
    q2 = q_base * 2.0

    # q3 = q1 + q2 = 3 * q_base + 10
    q3 = q1 + q2

    print(f"q1 uncertainty: {q1.uncertainty}")
    print(f"q2 uncertainty: {q2.uncertainty}")
    print(f"q3 uncertainty: {q3.uncertainty}")

    expected_unc = 3.0 * unc_base
    np.testing.assert_allclose(q3.uncertainty, expected_unc)
    print(
        "Success: q3 uncertainty matches theoretical 3 * q_base uncertainty."
    )

    # Verify cross-correlation: Cov(q1, q2) = 2*Var(q_base)
    s1 = q1.uncertainty_obj.vector_slice
    s2 = q2.uncertainty_obj.vector_slice
    cov_12 = store.get_covariance_block(s1, s2).toarray()

    expected_cov_12 = 2.0 * np.diag(unc_base**2)
    np.testing.assert_allclose(cov_12, expected_cov_12)
    print("Success: Cross-cov matches theoretical 2 * Var(q_base).")

    # Verify q3 = q1 + q2 via full covariance:
    # Var(q3) = Var(q1) + Var(q2) + 2*Cov(q1, q2)
    # Var(q1) = Var(q_base) = [0.01, 0.04]
    # Var(q2) = 4 * Var(q_base) = [0.04, 0.16]
    # 2 * Cov(q1, q2) = 4 * Var(q_base) = [0.04, 0.16]
    # Total Var(q3) = [0.09, 0.36]
    # StdDev(q3) = [0.3, 0.6]. Matches!


if __name__ == "__main__":
    test_correlated_arrays()
