"""Tests for physure.nn.utils (null-space basis + dimension matrix)."""

import numpy as np
import pytest

from physure import Q_
from physure.nn.utils import (
    _compute_rcond_val,
    _dim_exponents,
    extract_dimension_matrix,
    null_space_basis,
)


def _assert_null_basis(matrix, basis):
    basis = np.asarray(basis)
    assert basis.shape == (3, 1)
    assert np.allclose(np.asarray(matrix) @ basis, 0.0, atol=1e-6)
    assert np.allclose(basis.T @ basis, np.eye(1), atol=1e-6)


M = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]  # null space = z axis


def test_null_space_numpy():
    _assert_null_basis(M, null_space_basis(np.array(M)))


def test_null_space_numpy_from_list_and_rcond():
    _assert_null_basis(M, null_space_basis(M, rcond=1e-10))


def test_null_space_full_rank_is_empty():
    basis = null_space_basis(np.eye(3))
    assert basis.shape == (3, 0)


def test_null_space_torch():
    torch = pytest.importorskip("torch")
    basis = null_space_basis(torch.tensor(M))
    assert isinstance(basis, torch.Tensor)
    _assert_null_basis(M, basis.numpy())


def test_null_space_jax():
    pytest.importorskip("jax")
    import jax.numpy as jnp

    basis = null_space_basis(jnp.array(M))
    _assert_null_basis(M, np.asarray(basis))


def test_compute_rcond_val():
    assert _compute_rcond_val(0.5, 1e-16, 2, 3) == 0.5
    assert _compute_rcond_val(None, 1e-16, 2, 3) == 3e-16


def test_dim_exponents_accepts_dict_and_fallback():
    assert _dim_exponents({"L": 1}) == {"L": 1}
    assert _dim_exponents(object()) == {}


def test_extract_dimension_matrix_from_quantities():
    d, bases = extract_dimension_matrix([Q_(1, "m"), Q_(1, "s"), Q_(1, "m/s")])
    assert d.shape == (2, 3)
    # Columns: m -> (1, 0), s -> (0, 1), m/s -> (1, -1) in (L, T) order
    li = bases.index(next(b for b in bases if "L" in str(b)))
    ti = 1 - li
    assert (d[li, 0], d[ti, 0]) == (1, 0)
    assert (d[li, 1], d[ti, 1]) == (0, 1)
    assert (d[li, 2], d[ti, 2]) == (1, -1)


def test_extract_dimension_matrix_from_units():
    d, _bases = extract_dimension_matrix([Q_(1, "m").unit, Q_(1, "m^2").unit])
    assert d.shape == (1, 2)
    assert list(d[0]) == [1, 2]


def test_extract_dimension_matrix_rejects_garbage():
    with pytest.raises(ValueError, match="Cannot extract dimension"):
        extract_dimension_matrix(["not a quantity"])


def test_dimension_matrix_feeds_null_space():
    # Buckingham-pi style: velocity, length, time -> one dimensionless group
    d, _ = extract_dimension_matrix([Q_(1, "m/s"), Q_(1, "m"), Q_(1, "s")])
    basis = null_space_basis(d)
    assert basis.shape[1] == 1  # exactly one pi group
    assert np.allclose(d @ basis, 0.0, atol=1e-8)
