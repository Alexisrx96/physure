"""A small uncertainty is not dropped, in any unit (audit B14).

The Rust covariance store used to drop every variance and Jacobian entry at or below ``1e-12`` (absolute, in the
caller's unit): ``[1, 2] m +- 1e-7 m`` gave ``x + x`` an uncertainty of ``[0, 0]``, and ``x * 1e-13`` lost its
uncertainty. Only exact zeros are dropped now (and, inside one registered covariance matrix, a correlation
coefficient of at most 1e-12). The expected values are the GUM's: ``u(x + x) = 2 u``, ``u(c x) = |c| u``.
"""

import numpy as np
import pytest

from physure import Q_


def uncertainty(q):
    u = q.uncertainty
    return np.asarray(getattr(u, "magnitude", u))


@pytest.mark.parametrize("unit", ["m", "mm", "km"])
@pytest.mark.parametrize("u", [1e-9, 1e-7, 1e-5])
def test_x_plus_x_has_twice_the_uncertainty(unit, u):
    x = Q_(np.array([1.0, 2.0]), unit, uncertainty=np.array([u, u]))
    np.testing.assert_array_equal(uncertainty(x + x), [2 * u, 2 * u])


@pytest.mark.parametrize("factor", [1e-11, 1e-13, 1e-20])
def test_a_small_factor_keeps_its_uncertainty(factor):
    x = Q_(np.array([1.0, 2.0]), "m", uncertainty=np.array([1e-3, 1e-3]))
    np.testing.assert_allclose(
        uncertainty(x * factor), [factor * 1e-3] * 2, rtol=1e-14
    )


def test_a_large_array_keeps_a_small_uncertainty():
    n = 2000
    x = Q_(np.arange(1.0, n + 1), "m", uncertainty=np.full(n, 1e-7))
    np.testing.assert_array_equal(uncertainty(x + x), np.full(n, 2e-7))


def test_a_repeated_operand_stays_correlated_with_itself():
    """``(x + x) - x`` is ``x``; ``(x + x) - 2 x`` is exact (it was ``sqrt(3) u`` and ``2 u``)."""
    x = Q_(np.array([1.0, 2.0]), "m", uncertainty=np.array([1e-3, 1e-3]))
    y = x + x
    np.testing.assert_allclose(uncertainty(y - x), [1e-3, 1e-3], rtol=1e-12)
    np.testing.assert_allclose(uncertainty(y - 2 * x), [0.0, 0.0], atol=1e-18)
