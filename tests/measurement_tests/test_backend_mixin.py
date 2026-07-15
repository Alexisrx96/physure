"""Tests for the NumPy/PyTorch dispatch protocols in BackendMixin."""

import math

import numpy as np
import pytest

from measurekit import Q_
from measurekit.domain.exceptions import IncompatibleUnitsError

# --- NumPy ufunc arithmetic -------------------------------------------------


def test_np_add_same_unit():
    q1 = Q_(np.array([1.0, 2.0]), "m")
    q2 = Q_(np.array([3.0, 4.0]), "m")
    res = np.add(q1, q2)
    assert np.allclose(res.magnitude, [4.0, 6.0])
    assert str(res.unit) == "m"


def test_np_add_incompatible_dimensions_raises():
    with pytest.raises(IncompatibleUnitsError):
        np.add(Q_(np.array([1.0]), "m"), Q_(np.array([1.0]), "s"))


def test_np_subtract_both_orders():
    q1 = Q_(np.array([5.0]), "")
    q2 = Q_(np.array([2.0]), "")
    assert np.allclose(np.subtract(q1, q2).magnitude, [3.0])
    # Reflected: scalar - Quantity goes through __rsub__
    res = np.subtract(10.0, Q_(2.0, ""))
    assert math.isclose(float(res.magnitude), 8.0)


def test_np_multiply_and_divide():
    q = Q_(np.array([6.0]), "m")
    assert np.allclose(np.multiply(q, 2.0).magnitude, [12.0])
    res = np.true_divide(q, Q_(np.array([2.0]), "s"))
    assert np.allclose(res.magnitude, [3.0])
    assert res.dimension == Q_(1, "m/s").dimension
    # Reflected division: 12 / (6 m) = 2 m^-1
    rres = np.true_divide(12.0, q)
    assert np.allclose(rres.magnitude, [2.0])


def test_np_power():
    q = Q_(np.array([3.0]), "m")
    res = np.power(q, 2)
    assert np.allclose(res.magnitude, [9.0])
    assert res.dimension == Q_(1, "m^2").dimension


def test_np_sqrt_square_abs():
    q = Q_(np.array([4.0]), "m^2")
    assert np.allclose(np.sqrt(q).magnitude, [2.0])
    assert np.sqrt(q).dimension == Q_(1, "m").dimension
    q2 = Q_(np.array([-3.0]), "m")
    assert np.allclose(np.square(q2).magnitude, [9.0])
    assert np.allclose(np.absolute(q2).magnitude, [3.0])


def test_np_sum_via_add_reduce():
    q = Q_(np.array([1.0, 2.0, 3.0]), "m")
    res = np.add.reduce(q)
    assert math.isclose(float(res.magnitude), 6.0)
    assert str(res.unit) == "m"


def test_np_non_add_reduce_not_supported():
    q = Q_(np.array([1.0, 2.0]), "m")
    with pytest.raises(TypeError):
        np.multiply.reduce(q)


# --- NumPy trig / dimensionless ufuncs ---------------------------------------


def test_np_sin_dimensionless():
    q = Q_(0.5, "")
    res = np.sin(q)
    assert math.isclose(float(res.magnitude), math.sin(0.5))
    assert res.dimension.is_dimensionless


def test_np_sin_with_uncertainty_propagates_derivative():
    q = Q_(0.5, "", uncertainty=0.01)
    res = np.sin(q)
    # sigma_out = |cos(0.5)| * 0.01 (central difference, so approximately)
    assert math.isclose(
        float(res.uncertainty), abs(math.cos(0.5)) * 0.01, rel_tol=1e-4
    )


def test_np_trig_on_dimensioned_quantity_raises():
    with pytest.raises(IncompatibleUnitsError):
        np.sin(Q_(1.0, "m"))


# --- NumPy __array_function__ -------------------------------------------------


def test_np_concatenate_same_unit():
    q1 = Q_(np.array([1.0, 2.0]), "m")
    q2 = Q_(np.array([3.0]), "m")
    res = np.concatenate([q1, q2])
    assert np.allclose(res.magnitude, [1.0, 2.0, 3.0])
    assert str(res.unit) == "m"


def test_np_concatenate_mixed_units_rejected():
    q1 = Q_(np.array([1.0]), "m")
    q2 = Q_(np.array([1.0]), "s")
    with pytest.raises(TypeError):
        np.concatenate([q1, q2])


def test_np_mean():
    q = Q_(np.array([2.0, 4.0]), "m")
    res = np.mean(q)
    assert math.isclose(float(res.magnitude), 3.0)
    assert str(res.unit) == "m"


# --- PyTorch __torch_function__ -----------------------------------------------

torch = pytest.importorskip("torch")


def test_torch_arithmetic_functions():
    q1 = Q_(torch.tensor([2.0]), "m")
    q2 = Q_(torch.tensor([3.0]), "m")
    assert torch.add(q1, q2).magnitude.item() == 5.0
    assert torch.sub(q1, q2).magnitude.item() == -1.0
    res = torch.mul(q1, q2)
    assert res.magnitude.item() == 6.0
    assert res.dimension == Q_(1, "m^2").dimension
    res = torch.div(q1, q2)
    assert math.isclose(res.magnitude.item(), 2.0 / 3.0, rel_tol=1e-6)
    assert res.dimension.is_dimensionless
    res = torch.pow(q1, 2)
    assert res.magnitude.item() == 4.0


def test_torch_unary_math():
    q = Q_(torch.tensor([4.0]), "m^2")
    assert torch.sqrt(q).magnitude.item() == 2.0
    q2 = Q_(torch.tensor([-2.0]), "m")
    assert torch.abs(q2).magnitude.item() == 2.0
    q3 = Q_(torch.tensor([0.5]), "")
    assert math.isclose(
        torch.sin(q3).magnitude.item(), math.sin(0.5), rel_tol=1e-6
    )


def test_torch_trig_on_dimensioned_quantity_raises():
    with pytest.raises(IncompatibleUnitsError):
        torch.sin(Q_(torch.tensor([1.0]), "m"))


def test_torch_fallback_preserves_unit():
    q = Q_(torch.tensor([-1.0, 2.0]), "m")
    res = torch.relu(q)
    assert torch.allclose(res.magnitude, torch.tensor([0.0, 2.0]))
    assert str(res.unit) == "m"


def test_backward_flows_to_magnitude():
    t = torch.tensor(3.0, requires_grad=True)
    q = Q_(t, "m")
    y = q * q
    y.backward()
    assert t.grad.item() == 6.0


def test_backward_on_plain_float_raises():
    with pytest.raises(TypeError):
        Q_(2.0, "m").backward()


def test_to_device_cpu_roundtrip():
    q = Q_(torch.tensor([1.0, 2.0]), "m", uncertainty=torch.tensor([0.1, 0.1]))
    moved = q.to_device("cpu")
    assert torch.allclose(moved.magnitude, q.magnitude)
    assert str(moved.unit) == "m"
