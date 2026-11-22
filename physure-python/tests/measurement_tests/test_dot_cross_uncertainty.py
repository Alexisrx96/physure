"""`Quantity.dot` and `Quantity.cross` keep the uncertainty.

Spec: docs/superpowers/specs/2026-11-22-python-dot-cross-uncertainty-design.md. Every
uncertain result is compared with an analytic formula or with the same computation
written out on scalar quantities with lineage (`Q_(v, unit, sigma)` elements, a shared
scalar `x` standing for an error the elements share), never only with the
implementation's own output. Independent, lineage-correlated (a shared source) and
uncorrelated-mode arrays, exact arrays, 1-element and large arrays are all here.
"""

from __future__ import annotations

import functools
import math
import operator
import time

import numpy as np
import pytest

import physure
from physure import Q_


def _total(quantities):
    """The sum through `Quantity` arithmetic (the builtin `sum` starts at a plain 0)."""
    return functools.reduce(operator.add, quantities)


def _scalars(values, unit, sigma):
    """One scalar measurement per value: independent errors, each with its own lineage."""
    sigmas = np.broadcast_to(sigma, np.shape(values))
    return [Q_(float(v), unit, float(s)) for v, s in zip(values, sigmas, strict=True)]


def _independent_dot_sigma(a, b, sa, sb):
    """sqrt(sum((b_i s_a,i)^2 + (a_i s_b,i)^2)): the first-order uncertainty of a . b."""
    a, b = np.asarray(a, float), np.asarray(b, float)
    return math.sqrt(float(np.sum((b * sa) ** 2 + (a * sb) ** 2)))


def _cross_by_scalars(a, b):
    """a x b written out on scalar quantities, component by component."""
    if len(a) == 2:
        return [a[0] * b[1] - a[1] * b[0]]
    return [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]


@pytest.fixture
def a():
    return Q_(np.array([1.0, 2.0, 3.0]), "m", uncertainty=0.1)


@pytest.fixture
def b():
    return Q_(np.array([4.0, 5.0, 6.0]), "N", uncertainty=0.2)


# --- dot: section 1 of the spec ----------------------------------------------------


def test_dot_of_independent_arrays_propagates(a, b):
    dot = a.dot(b)
    assert dot.magnitude == pytest.approx(32.0)
    assert dot.dimension == Q_(1, "J").dimension
    expected = _independent_dot_sigma([1, 2, 3], [4, 5, 6], 0.1, 0.2)
    assert expected == pytest.approx(1.1532562594670797)
    assert dot.uncertainty == pytest.approx(expected, rel=1e-12)


def test_dot_equals_the_sum_of_the_elementwise_product(a, b):
    dot, summed = a.dot(b), np.sum(a * b)
    assert dot.magnitude == pytest.approx(summed.magnitude, rel=1e-15)
    assert dot.uncertainty == pytest.approx(summed.uncertainty, rel=1e-12)


def test_dot_equals_the_reduction_through_scalar_quantities():
    values_a, values_b = [1.5, -2.0, 3.25, 0.5], [4.0, 5.5, -6.0, 2.0]
    sigma_a, sigma_b = np.array([0.1, 0.2, 0.05, 0.3]), np.array([0.2, 0.1, 0.4, 0.0])
    a = Q_(np.array(values_a), "m", uncertainty=sigma_a)
    b = Q_(np.array(values_b), "N", uncertainty=sigma_b)
    reference = _total(
        x * y
        for x, y in zip(
            _scalars(values_a, "m", sigma_a),
            _scalars(values_b, "N", sigma_b),
            strict=True,
        )
    )
    dot = a.dot(b)
    assert dot.magnitude == pytest.approx(reference.magnitude, rel=1e-15)
    assert dot.uncertainty == pytest.approx(reference.uncertainty, rel=1e-12)
    assert dot.uncertainty == pytest.approx(
        _independent_dot_sigma(values_a, values_b, sigma_a, sigma_b), rel=1e-12
    )


def test_dot_of_an_array_with_itself_counts_each_error_twice(a):
    # d(sum a_i^2)/da_i = 2 a_i: the two factors are one measurement
    dot = a.dot(a)
    assert dot.magnitude == pytest.approx(14.0)
    expected = 2 * 0.1 * math.sqrt(14.0)
    assert dot.uncertainty == pytest.approx(expected, rel=1e-12)
    reference = _total(q * q for q in _scalars([1, 2, 3], "m", 0.1))
    assert reference.uncertainty == pytest.approx(expected, rel=1e-12)
    assert dot.uncertainty == pytest.approx(np.sum(a * a).uncertainty, rel=1e-12)


def test_dot_of_arrays_that_share_a_source_counts_it_once():
    # every element of `a` carries the one error of `x` (lineage-correlated)
    x = Q_(2.0, "m", 0.1)
    a = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1) + x
    c = Q_(np.array([4.0, -5.0, 6.0]), "N", uncertainty=0.2)
    elements = _scalars([1.0, 2.0, 4.0], "m", 0.1)
    others = _scalars([4.0, -5.0, 6.0], "N", 0.2)
    reference = _total((e + x) * o for e, o in zip(elements, others, strict=True))
    # analytic: d/dx = sum(c_i) = 5, d/db_i = c_i, d/dc_i = b_i + x
    analytic = math.sqrt(
        (5 * 0.1) ** 2
        + sum((ci * 0.1) ** 2 for ci in (4, -5, 6))
        + sum((ai * 0.2) ** 2 for ai in (3, 4, 6))
    )
    assert reference.uncertainty == pytest.approx(analytic, rel=1e-12)
    dot = a.dot(c)
    assert dot.magnitude == pytest.approx(reference.magnitude, rel=1e-15)
    assert dot.uncertainty == pytest.approx(analytic, rel=1e-12)
    # and with itself: sum((b_i + x)^2)
    self_reference = _total((e + x) * (e + x) for e in elements)
    assert a.dot(a).uncertainty == pytest.approx(
        self_reference.uncertainty, rel=1e-12
    )


def test_dot_in_the_uncorrelated_mode():
    with physure.propagation_mode("uncorrelated"):
        a = Q_(np.array([1.0, 2.0, 3.0]), "m", uncertainty=0.1)
        b = Q_(np.array([4.0, 5.0, 6.0]), "N", uncertainty=0.2)
        assert a.dot(b).uncertainty == pytest.approx(
            _independent_dot_sigma([1, 2, 3], [4, 5, 6], 0.1, 0.2), rel=1e-12
        )


def test_dot_refuses_elements_whose_shared_error_was_already_dropped():
    # the uncorrelated mode keeps variances only: the error of x shared by the
    # elements is gone, and a dot read from them would be too small
    with physure.propagation_mode("uncorrelated"):
        shifted = Q_(np.array([1.0, 2.0, 4.0]), "m") + Q_(0.5, "m", 0.3)
        other = Q_(np.array([1.0, 1.0, 1.0]), "N")
        with pytest.raises(TypeError, match=r"dot.*correlation"):
            shifted.dot(other)
        with pytest.raises(TypeError, match=r"dot.*correlation"):
            other.dot(shifted)


def test_dot_of_exact_arrays_is_unchanged():
    x = np.array([1.5, -2.25, 3.0, 7.125])
    y = np.array([4.0, 5.5, -6.0, 0.25])
    dot = Q_(x, "m").dot(Q_(y, "N"))
    assert dot.magnitude == np.dot(x, y)
    assert dot.uncertainty == 0
    assert dot.dimension == Q_(1, "J").dimension


def test_dot_of_an_exact_and_an_uncertain_array():
    exact = Q_(np.array([1.0, 2.0, 3.0]), "m")
    uncertain = Q_(np.array([4.0, 5.0, 6.0]), "N", uncertainty=np.array([0.0, 0.1, 0.2]))
    expected = _independent_dot_sigma([1, 2, 3], [4, 5, 6], 0.0, [0.0, 0.1, 0.2])
    assert exact.dot(uncertain).uncertainty == pytest.approx(expected, rel=1e-12)
    assert uncertain.dot(exact).uncertainty == pytest.approx(expected, rel=1e-12)


def test_dot_of_one_element_arrays():
    dot = Q_(np.array([2.0]), "m", uncertainty=0.1).dot(
        Q_(np.array([3.0]), "N", uncertainty=0.2)
    )
    assert dot.magnitude == pytest.approx(6.0)
    assert dot.uncertainty == pytest.approx(0.5, rel=1e-12)


def test_dot_of_large_arrays_is_fast_and_right():
    rng = np.random.default_rng(7)
    n = 100_000
    x, y = rng.normal(size=n), rng.normal(size=n)
    sx, sy = rng.uniform(0.0, 0.2, size=n), rng.uniform(0.0, 0.3, size=n)
    exact = Q_(x, "m").dot(Q_(y, "N"))
    assert exact.magnitude == np.dot(x, y)
    a, b = Q_(x, "m", uncertainty=sx), Q_(y, "N", uncertainty=sy)
    start = time.perf_counter()
    dot = a.dot(b)
    elapsed = time.perf_counter() - start
    assert dot.magnitude == pytest.approx(np.dot(x, y), rel=1e-12)
    assert dot.uncertainty == pytest.approx(
        _independent_dot_sigma(x, y, sx, sy), rel=1e-9
    )
    assert elapsed < 2.0


def test_dot_of_mismatched_shapes_raises(a):
    with pytest.raises(ValueError):
        a.dot(Q_(np.array([1.0, 2.0]), "N", uncertainty=0.1))
    with pytest.raises(ValueError):
        a.dot(Q_(np.array([1.0, 2.0]), "N"))
    # a 1-element array does not broadcast in a dot product
    with pytest.raises(ValueError):
        a.dot(Q_(np.array([2.0]), "N", uncertainty=0.1))


def test_dot_of_dimensionless_arrays():
    plain = Q_(np.array([1.0, 2.0]), "", uncertainty=0.1)
    percent = Q_(np.array([10.0, 20.0]), "percent", uncertainty=1.0)
    dot = plain.dot(percent)
    assert dot.to("percent").magnitude == pytest.approx(50.0)
    assert dot.to("percent").uncertainty == pytest.approx(
        _independent_dot_sigma([1, 2], [10, 20], 0.1, 1.0), rel=1e-12
    )


def test_dot_of_temperature_points_is_the_product_in_kelvin():
    # as `*` has it: absolute temperatures multiply as kelvins
    t = Q_(np.array([10.0, 20.0]), "degC")
    dot = t.dot(t)
    kelvins = np.array([283.15, 293.15])
    assert dot.to("K**2").magnitude == pytest.approx(np.dot(kelvins, kelvins))
    assert dot.to("K**2").magnitude == pytest.approx(np.sum(t * t).to("K**2").magnitude)
    uncertain = Q_(np.array([10.0, 20.0]), "degC", uncertainty=0.1)
    assert uncertain.dot(uncertain).uncertainty == pytest.approx(
        2 * 0.1 * math.sqrt(float(np.sum(kelvins**2))), rel=1e-12
    )


def test_dot_of_a_temperature_point_and_a_plain_number_is_refused():
    with pytest.raises(ValueError, match="absolute temperature"):
        Q_(np.array([10.0, 20.0]), "degC").dot(Q_(np.array([1.0, 2.0]), ""))


def test_dot_of_levels_is_refused():
    with pytest.raises(TypeError, match="level"):
        Q_(np.array([10.0, 20.0]), "dB").dot(Q_(np.array([1.0, 2.0]), "m"))


def test_dot_of_scalars_is_their_product():
    x, y = Q_(2.0, "m", 0.1), Q_(3.0, "N", 0.2)
    dot = x.dot(y)
    assert dot.magnitude == pytest.approx(6.0)
    assert dot.uncertainty == pytest.approx(0.5, rel=1e-12)
    assert x.dot(x).uncertainty == pytest.approx(0.4, rel=1e-12)
    assert Q_(2.0, "m").dot(Q_(3.0, "N")).uncertainty == 0


def test_dot_of_an_array_and_a_scalar_is_their_product(a):
    y = Q_(3.0, "N", 0.2)
    dot = a.dot(y)
    product = a * y
    np.testing.assert_allclose(dot.magnitude, [3.0, 6.0, 9.0])
    np.testing.assert_allclose(dot.uncertainty, product.uncertainty, rtol=1e-12)
    # the one error of y is in every element: the sum counts it once
    assert np.sum(dot).uncertainty == pytest.approx(
        math.sqrt(3 * (3 * 0.1) ** 2 + (6 * 0.2) ** 2), rel=1e-12
    )


def test_dot_of_a_matrix_and_a_vector_is_the_reduction_through_scalars():
    m_values = np.array([[1.0, 2.0, 0.5], [-3.0, 4.0, 2.0]])
    v_values = np.array([0.5, -1.0, 2.0])
    m = Q_(m_values, "m", uncertainty=0.1)
    v = Q_(v_values, "N", uncertainty=0.2)
    m_scalars = [_scalars(row, "m", 0.1) for row in m_values]
    v_scalars = _scalars(v_values, "N", 0.2)
    reference = [
        _total(x * y for x, y in zip(row, v_scalars, strict=True))
        for row in m_scalars
    ]
    dot = m.dot(v)
    np.testing.assert_allclose(dot.magnitude, m_values @ v_values)
    np.testing.assert_allclose(
        dot.uncertainty, [r.uncertainty for r in reference], rtol=1e-12
    )
    # the two rows share the errors of v: their sum counts them once
    assert np.sum(dot).uncertainty == pytest.approx(
        _total(reference).uncertainty, rel=1e-12
    )
    # vector . matrix
    w_values = np.array([1.0, -2.0])
    w = Q_(w_values, "s", uncertainty=0.3)
    w_scalars = _scalars(w_values, "s", 0.3)
    left = w.dot(m)
    left_reference = [
        _total(w_scalars[i] * m_scalars[i][j] for i in range(2)) for j in range(3)
    ]
    np.testing.assert_allclose(left.magnitude, w_values @ m_values)
    np.testing.assert_allclose(
        left.uncertainty, [r.uncertainty for r in left_reference], rtol=1e-12
    )


def test_dot_of_a_matrix_with_itself_is_the_reduction_through_scalars():
    values = np.array([[1.0, 2.0], [3.0, -1.0]])
    m = Q_(values, "m", uncertainty=np.array([[0.1, 0.2], [0.3, 0.05]]))
    s = [
        _scalars(row, "m", sig)
        for row, sig in zip(values, [[0.1, 0.2], [0.3, 0.05]], strict=True)
    ]
    reference = [
        [_total(s[i][k] * s[k][j] for k in range(2)) for j in range(2)]
        for i in range(2)
    ]
    product = m.dot(m)
    np.testing.assert_allclose(product.magnitude, values @ values)
    np.testing.assert_allclose(
        product.uncertainty,
        [[r.uncertainty for r in row] for row in reference],
        rtol=1e-12,
    )


def test_dot_of_exact_matrices_is_unchanged():
    m = np.array([[1.0, 2.0], [3.0, 4.0]])
    v = np.array([5.0, 6.0])
    np.testing.assert_array_equal(Q_(m, "m").dot(Q_(v, "N")).magnitude, m @ v)
    np.testing.assert_array_equal(Q_(m, "m").dot(Q_(m, "m")).magnitude, m @ m)


def test_dot_of_uncertain_arrays_beyond_two_dimensions_is_refused():
    cube = Q_(np.ones((2, 2, 2)), "m", uncertainty=0.1)
    with pytest.raises(TypeError, match="dot"):
        cube.dot(Q_(np.ones(2), "m", uncertainty=0.1))
    # exact ones keep working
    exact = Q_(np.ones((2, 2, 2)), "m").dot(Q_(np.ones(2), "m"))
    np.testing.assert_array_equal(exact.magnitude, np.full((2, 2), 2.0))


def test_dot_of_uncertain_lists_is_refused_and_exact_lists_still_work():
    with pytest.raises(TypeError, match="dot"):
        Q_([1.0, 2.0, 3.0], "m", uncertainty=0.1).dot(Q_([4.0, 5.0, 6.0], "N"))
    exact = Q_([1.0, 2.0, 3.0], "m").dot(Q_([4.0, 5.0, 6.0], "N"))
    assert exact.magnitude == 32.0


# --- cross ---------------------------------------------------------------------------


def test_cross_of_independent_vectors_propagates(a, b):
    cross = a.cross(b)
    np.testing.assert_allclose(cross.magnitude, [-3.0, 6.0, -3.0])
    assert cross.dimension == Q_(1, "J").dimension
    # c0 = a1 b2 - a2 b1: (b2 s_a)^2 + (b1 s_a)^2 + (a1 s_b)^2 + (a2 s_b)^2 ...
    av, bv = [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]
    analytic = [
        math.hypot(bv[2] * 0.1, bv[1] * 0.1, av[1] * 0.2, av[2] * 0.2),
        math.hypot(bv[0] * 0.1, bv[2] * 0.1, av[2] * 0.2, av[0] * 0.2),
        math.hypot(bv[1] * 0.1, bv[0] * 0.1, av[0] * 0.2, av[1] * 0.2),
    ]
    np.testing.assert_allclose(cross.uncertainty, analytic, rtol=1e-12)
    reference = _cross_by_scalars(_scalars(av, "m", 0.1), _scalars(bv, "N", 0.2))
    np.testing.assert_allclose(
        cross.uncertainty, [r.uncertainty for r in reference], rtol=1e-12
    )
    # the components share the errors of a and b: their sum counts them once
    assert np.sum(cross).uncertainty == pytest.approx(
        _total(reference).uncertainty, rel=1e-12
    )


def test_cross_of_vectors_that_share_a_source():
    x = Q_(2.0, "m", 0.1)
    a = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1) + x
    c = Q_(np.array([4.0, -5.0, 6.0]), "N", uncertainty=0.2)
    elements = [e + x for e in _scalars([1.0, 2.0, 4.0], "m", 0.1)]
    reference = _cross_by_scalars(elements, _scalars([4.0, -5.0, 6.0], "N", 0.2))
    cross = a.cross(c)
    np.testing.assert_allclose(cross.magnitude, [r.magnitude for r in reference])
    np.testing.assert_allclose(
        cross.uncertainty, [r.uncertainty for r in reference], rtol=1e-12
    )
    assert np.sum(cross).uncertainty == pytest.approx(
        _total(reference).uncertainty, rel=1e-12
    )


def test_cross_of_a_vector_with_itself_is_exactly_zero(a):
    # a x a = 0 for every value of a: no error survives
    cross = a.cross(a)
    np.testing.assert_array_equal(cross.magnitude, [0.0, 0.0, 0.0])
    np.testing.assert_allclose(cross.uncertainty, [0.0, 0.0, 0.0], atol=1e-15)


def test_cross_of_two_vectors_of_the_plane():
    a2 = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
    b2 = Q_(np.array([3.0, 4.0]), "N", uncertainty=0.2)
    cross = a2.cross(b2)
    assert float(cross.magnitude) == pytest.approx(-2.0)
    # z = a0 b1 - a1 b0
    analytic = math.hypot(4 * 0.1, 3 * 0.1, 2 * 0.2, 1 * 0.2)
    assert float(np.asarray(cross.uncertainty)) == pytest.approx(analytic, rel=1e-12)


def test_cross_of_exact_vectors_is_unchanged():
    x, y = np.array([1.5, -2.0, 3.0]), np.array([4.0, 5.0, -6.25])
    cross = Q_(x, "m").cross(Q_(y, "N"))
    np.testing.assert_array_equal(cross.magnitude, np.cross(x, y))
    np.testing.assert_array_equal(cross.uncertainty, [0.0, 0.0, 0.0])
    batch = np.array([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
    np.testing.assert_array_equal(
        Q_(batch, "m").cross(Q_(batch[::-1], "m")).magnitude,
        np.cross(batch, batch[::-1]),
    )


def test_cross_refuses_elements_whose_shared_error_was_already_dropped():
    with physure.propagation_mode("uncorrelated"):
        shifted = Q_(np.array([1.0, 2.0, 4.0]), "m") + Q_(0.5, "m", 0.3)
        other = Q_(np.array([1.0, 0.0, 0.0]), "N", uncertainty=0.1)
        with pytest.raises(TypeError, match=r"cross.*correlation"):
            shifted.cross(other)


def test_cross_of_uncertain_batches_is_refused():
    batch = Q_(np.ones((2, 3)), "m", uncertainty=0.1)
    with pytest.raises(TypeError, match="cross"):
        batch.cross(batch)


def test_cross_of_temperature_points_and_levels():
    t = Q_(np.array([10.0, 20.0, 30.0]), "degC")
    u = Q_(np.array([1.0, 2.0, 3.0]), "m")
    kelvins = np.array([283.15, 293.15, 303.15])
    np.testing.assert_allclose(
        t.cross(u).to("K*m").magnitude, np.cross(kelvins, [1.0, 2.0, 3.0])
    )
    with pytest.raises(TypeError, match="level"):
        Q_(np.array([1.0, 2.0, 3.0]), "dB").cross(u)


# --- the NumPy spellings route to the same code --------------------------------------


def test_np_dot_np_cross_and_matmul_route_to_the_methods(a, b):
    for result in (np.dot(a, b), a @ b):
        assert result.magnitude == pytest.approx(32.0)
        assert result.uncertainty == pytest.approx(a.dot(b).uncertainty, rel=1e-12)
    cross = np.cross(a, b)
    np.testing.assert_allclose(cross.magnitude, [-3.0, 6.0, -3.0])
    np.testing.assert_allclose(cross.uncertainty, a.cross(b).uncertainty, rtol=1e-12)


def test_np_dot_and_np_cross_with_options_or_plain_arrays_still_raise(a, b):
    with pytest.raises(TypeError):
        np.dot(a, b, np.empty(()))
    with pytest.raises(TypeError):
        np.cross(a, b, axis=0)
    with pytest.raises(TypeError):
        np.dot(a, np.array([1.0, 2.0, 3.0]))
    with pytest.raises(TypeError):
        Q_(2.0, "m") @ Q_(3.0, "m")
