"""Direct tests for the Uncertainty model classes (Variance/Covariance)."""

import math

import numpy as np
import pytest

from physure.core.autograd import AutogradPropagator
from physure.domain.measurement.uncertainty import (
    CovarianceModel,
    Uncertainty,
    VarianceModel,
)

# --- Uncertainty.from_standard dispatch --------------------------------------


def test_from_standard_scalar_returns_variance_model():
    u = Uncertainty.from_standard(2.0)
    assert isinstance(u, VarianceModel)
    assert math.isclose(u.variance, 4.0)
    assert math.isclose(u.std_dev, 2.0)


def test_from_standard_zero_scalar_returns_none():
    assert Uncertainty.from_standard(0.0) is None


def test_from_standard_nonzero_array_returns_covariance_model():
    u = Uncertainty.from_standard(np.array([1.0, 2.0]))
    assert isinstance(u, CovarianceModel)
    assert u.vector_slice is not None


def test_from_standard_all_zero_array_returns_variance_model():
    u = Uncertainty.from_standard(np.zeros(3))
    assert isinstance(u, VarianceModel)


# --- VarianceModel ------------------------------------------------------------


def test_variance_add_quadrature():
    a = VarianceModel.from_standard(3.0)
    b = VarianceModel.from_standard(4.0)
    assert math.isclose((a + b).std_dev, 5.0)
    # Subtraction adds in quadrature too (jac_other=-1)
    assert math.isclose((a - b).std_dev, 5.0)


def test_variance_add_none_applies_jacobian_only():
    a = VarianceModel.from_standard(2.0)
    res = a.add(None, jac_self=3.0)
    assert math.isclose(res.std_dev, 6.0)


def test_variance_add_non_variance_other():
    a = VarianceModel.from_standard(3.0)
    other = CovarianceModel.from_standard(4.0)
    assert math.isclose(a.add(other).std_dev, 5.0)


def test_variance_mul_div_requires_jacobians():
    a = VarianceModel.from_standard(1.0)
    b = VarianceModel.from_standard(1.0)
    with pytest.raises(ValueError, match="explicit"):
        a.propagate_mul_div(b, 2.0, 3.0, 6.0)
    res = a.propagate_mul_div(b, 2.0, 3.0, 6.0, jac_self=3.0, jac_other=2.0)
    assert math.isclose(res.std_dev, math.hypot(3.0, 2.0))


def test_variance_power():
    a = VarianceModel.from_standard(0.1)
    # y = x^2 at x=10 -> J = 2*10 = 20 -> sigma = 2.0
    assert math.isclose(a.power(2, value=10.0).std_dev, 2.0)
    # Explicit jacobian wins
    assert math.isclose(a.power(2, jac=5.0).std_dev, 0.5)
    with pytest.raises(ValueError, match="value or jac"):
        a.power(2)


def test_variance_scale():
    a = VarianceModel.from_standard(2.0)
    assert math.isclose(a.scale(3.0).std_dev, 6.0)


def test_variance_hash_scalar_ok_array_raises():
    assert isinstance(hash(VarianceModel.from_standard(2.0)), int)
    with pytest.raises(TypeError):
        hash(VarianceModel(variance=np.array([1.0, 2.0])))


def test_variance_array_add_elementwise():
    a = VarianceModel.from_standard(np.array([3.0, 0.0]))
    b = VarianceModel.from_standard(np.array([4.0, 1.0]))
    res = a.add(b)
    assert np.allclose(res.std_dev, [5.0, 1.0])


# --- CovarianceModel (scalar lineage path) -------------------------------------


def test_covariance_from_standard_tracks_lineage():
    m = CovarianceModel.from_standard(1.5, measurement_id="x")
    assert m.lineage == {"x": 1.5}
    assert math.isclose(m.std_dev, 1.5)


def test_covariance_zero_std_has_empty_lineage():
    assert CovarianceModel.from_standard(0.0).lineage == {}


def test_covariance_full_correlation_cancels():
    # x - x must have exactly zero uncertainty (same lineage id)
    m = CovarianceModel.from_standard(1.0, measurement_id="x")
    res = m.add(m, jac_self=1.0, jac_other=-1.0)
    assert res.lineage == {}
    assert math.isclose(res.std_dev, 0.0)


def test_covariance_independent_adds_in_quadrature():
    a = CovarianceModel.from_standard(3.0, measurement_id="a")
    b = CovarianceModel.from_standard(4.0, measurement_id="b")
    assert math.isclose(a.add(b).std_dev, 5.0)


def test_covariance_add_non_covariance_other():
    a = CovarianceModel.from_standard(3.0, measurement_id="a")
    b = VarianceModel.from_standard(4.0)
    assert math.isclose(a.add(b).std_dev, 5.0)


def test_covariance_mul_div_requires_jacobians():
    a = CovarianceModel.from_standard(1.0, measurement_id="a")
    with pytest.raises(ValueError, match="explicit"):
        a.propagate_mul_div(a, 2.0, 2.0, 4.0)


def test_covariance_power_preserves_correlation():
    a = CovarianceModel.from_standard(0.1, measurement_id="a")
    res = a.power(2, value=10.0)
    assert math.isclose(res.std_dev, 2.0)
    assert set(res.lineage) == {"a"}
    with pytest.raises(ValueError, match="value or jac"):
        a.power(2)


def test_covariance_scale():
    a = CovarianceModel.from_standard(2.0, measurement_id="a")
    res = a.scale(-3.0)
    assert math.isclose(res.std_dev, 6.0)  # abs(factor) * std
    assert math.isclose(res.lineage["a"], -6.0)


def test_covariance_hash_scalar_ok_array_raises():
    assert isinstance(hash(CovarianceModel.from_standard(1.0, "a")), int)
    with pytest.raises(TypeError):
        hash(CovarianceModel(std_dev_internal=np.array([1.0, 2.0])))


def test_ensure_vector_slice_registers_in_store():
    m = CovarianceModel(std_dev_internal=np.array([1.0, 2.0]))
    slc = m.ensure_vector_slice()
    assert isinstance(slc, slice)
    # Already-sliced models return their slice unchanged
    m2 = CovarianceModel(std_dev_internal=np.array([1.0]), vector_slice=slc)
    assert m2.ensure_vector_slice() is slc


# --- Generic propagation (Uncertainty.propagate) -------------------------------


def test_propagate_no_uncertainties():
    result, _unc = Uncertainty.propagate(lambda: 42.0, [], [])
    assert result == 42.0


def test_propagate_variance_models_finite_diff():
    # f(x, y) = x * y at (2, 3): J = (3, 2)
    ux = VarianceModel.from_standard(0.1)
    uy = VarianceModel.from_standard(0.2)
    result, unc = Uncertainty.propagate(
        lambda x, y: x * y, [2.0, 3.0], [ux, uy]
    )
    assert math.isclose(result, 6.0)
    expected = math.hypot(3.0 * 0.1, 2.0 * 0.2)
    assert math.isclose(unc.std_dev, expected, rel_tol=1e-4)


def test_propagate_covariance_scalar_path():
    ux = CovarianceModel.from_standard(0.1, measurement_id="x")
    result, unc = Uncertainty.propagate(
        lambda x, y: x - y, [5.0, 5.0], [ux, ux]
    )
    assert math.isclose(result, 0.0)
    # Fully correlated: x - x has zero uncertainty
    assert math.isclose(unc.std_dev, 0.0, abs_tol=1e-9)


def test_propagate_covariance_vector_path():
    std = np.array([0.1, 0.2])
    u = CovarianceModel.from_standard(std)
    values = [np.array([1.0, 2.0])]
    result, unc = Uncertainty.propagate(lambda x: x * 2.0, values, [u])
    assert np.allclose(result, [2.0, 4.0])
    assert np.allclose(unc.std_dev, 2.0 * std, rtol=1e-4)


# --- AutogradPropagator backends ------------------------------------------------


def test_autograd_finite_diff_jacobians():
    result, jacs = AutogradPropagator.compute_jacobians(
        lambda x, y: x**2 + y, [3.0, 1.0]
    )
    assert math.isclose(result, 10.0)
    assert math.isclose(jacs[0], 6.0, rel_tol=1e-4)
    assert math.isclose(jacs[1], 1.0, rel_tol=1e-4)


def test_autograd_torch_jacobians():
    torch = pytest.importorskip("torch")
    result, jacs = AutogradPropagator.compute_jacobians(
        lambda x: x**2, [torch.tensor(3.0)]
    )
    assert math.isclose(float(result), 9.0)
    assert math.isclose(float(jacs[0]), 6.0)


def test_autograd_jax_jacobians():
    pytest.importorskip("jax")
    import jax.numpy as jnp

    result, jacs = AutogradPropagator.compute_jacobians(
        lambda x: x**2, [jnp.asarray(3.0)]
    )
    assert math.isclose(float(result), 9.0)
    assert math.isclose(float(jacs[0]), 6.0)


def test_triton_stub_raises_without_triton(monkeypatch):
    import importlib
    import sys

    from physure.backends.kernels import covariance

    # Reload with the triton import blocked so the stub path is exercised
    # even on machines where triton is installed.
    monkeypatch.setitem(sys.modules, "triton", None)
    try:
        stubbed = importlib.reload(covariance)
        assert not stubbed.HAS_TRITON
        with pytest.raises(RuntimeError, match="Triton"):
            stubbed.apply_covariance_update_triton(None, None)
    finally:
        monkeypatch.undo()
        importlib.reload(covariance)
