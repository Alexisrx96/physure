"""Converting an uncertain array keeps its correlation with the source.

Spec: docs/superpowers/specs/2026-11-23-python-array-conversion-correlation-design.md.
`q.to(unit)` of an array is the same measurement in another unit: a linear (affine)
map of its source, so the covariance store must know it. Every uncertain result is
compared with an analytic value or with the same computation on scalar quantities
with lineage (which already carry it), never only with the implementation's own
output. Exact and uncertain, independent and correlated (a shared scalar `x`),
1-element (inside a store context) and 1e5-element arrays, temperature points and
intervals, levels and the uncorrelated mode are all here.
"""

from __future__ import annotations

import math
import time

import numpy as np
import pytest

import physure
from physure import Q_


def _zero(values, atol=1e-9):
    """Every element is zero (a rounding of the variance may leave ~1e-14)."""
    np.testing.assert_allclose(np.asarray(values, float), 0.0, atol=atol)


@pytest.fixture
def a():
    return Q_(np.array([1.0, 2.0, 3.0]), "m", uncertainty=0.1)


# --- section 1 of the spec ---------------------------------------------------


def test_converted_array_minus_its_source_is_exactly_zero(a):
    difference = a.to("cm") - a
    _zero(difference.magnitude)
    _zero(difference.uncertainty)


def test_converted_array_over_its_source_is_the_factor_with_no_uncertainty(a):
    ratio = a.to("cm") / a
    np.testing.assert_allclose(ratio.magnitude, 100.0, rtol=1e-15)  # cm/m
    np.testing.assert_allclose(ratio.to("1").magnitude, 1.0, rtol=1e-15)
    _zero(ratio.uncertainty)


def test_converted_array_plus_its_source_is_fully_correlated(a):
    total = a.to("cm") + a
    np.testing.assert_allclose(total.magnitude, [200.0, 400.0, 600.0])
    # 2 x in cm: d/dx = 2 * 100 cm/m, one error counted twice (independent: 14.1)
    np.testing.assert_allclose(total.uncertainty, 2 * 100 * 0.1, rtol=1e-12)
    # and it is twice the source, in value and in uncertainty
    np.testing.assert_allclose(total.to("m").magnitude, 2 * a.magnitude)
    np.testing.assert_allclose(
        total.to("m").uncertainty, 2 * a.uncertainty, rtol=1e-12
    )
    _zero((total - 2 * a).uncertainty)


def test_product_of_a_celsius_point_with_itself_is_t_squared_in_kelvin():
    t = Q_(np.array([20.0, 25.0]), "degC", uncertainty=0.5)
    square = t * t
    kelvin = np.array([293.15, 298.15])
    np.testing.assert_allclose(square.magnitude, kelvin**2, rtol=1e-12)
    # T = t + 273.15, dT/dt = 1; one measurement in both factors: d(T^2)/dt = 2T
    analytic = 2 * kelvin * 0.5
    np.testing.assert_allclose(analytic, [293.15, 298.15])
    np.testing.assert_allclose(square.uncertainty, analytic, rtol=1e-12)
    # the scalar route (lineage) agrees
    for value, expected in zip([20.0, 25.0], analytic, strict=True):
        scalar = Q_(value, "degC", 0.5)
        assert (scalar * scalar).uncertainty == pytest.approx(
            expected, rel=1e-12
        )


def test_sum_of_the_converted_array_is_the_converted_sum(a):
    # the spec's row reads `np.sum(a.to('cm')) - np.sum(a) * 100`: `np.sum(a) * 100`
    # is 600 m, and each np.sum is a new scalar measurement (np.sum(a) - np.sum(a)
    # is not 0 either), so the zero it means is the sum of the difference
    difference = np.sum(a.to("cm") - a)
    assert difference.magnitude == pytest.approx(0.0, abs=1e-12)
    assert difference.uncertainty == pytest.approx(0.0, abs=1e-9)
    summed = np.sum(a.to("cm"))
    assert summed.magnitude == pytest.approx(600.0)
    assert summed.uncertainty == pytest.approx(
        100 * np.sum(a).uncertainty, rel=1e-12
    )
    assert summed.uncertainty == pytest.approx(
        100 * 0.1 * math.sqrt(3), rel=1e-12
    )


# --- the analytic comparisons ------------------------------------------------


def test_converted_uncertainty_is_the_factor_times_the_source(a):
    np.testing.assert_allclose(
        a.to("cm").uncertainty, 100 * a.uncertainty, rtol=1e-15
    )
    np.testing.assert_allclose(
        a.to("km").uncertainty, a.uncertainty / 1000, rtol=1e-15
    )


def test_a_round_trip_is_the_source(a):
    back = a.to("km").to("m")
    np.testing.assert_allclose(back.magnitude, a.magnitude, rtol=1e-15)
    np.testing.assert_allclose(back.uncertainty, a.uncertainty, rtol=1e-12)
    _zero((back - a).uncertainty)
    _zero((a.to("cm").to("mm") - a.to("km")).uncertainty)


def test_converted_array_that_shares_a_source_keeps_it():
    x = Q_(2.0, "m", 0.1)
    b = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1) + x
    _zero((b.to("cm") - b).uncertainty)
    # b_cm - x = 100 (e + x) - 100 x = 100 e: x cancels, sigma = 100 * 0.1 cm
    without_x = b.to("cm") - x
    np.testing.assert_allclose(without_x.magnitude, [100.0, 200.0, 400.0])
    np.testing.assert_allclose(without_x.uncertainty, 10.0, rtol=1e-12)
    # x in another unit is still x (another model of the same source)
    np.testing.assert_allclose((b - x.to("cm")).uncertainty, 0.1, rtol=1e-12)
    # the scalar route agrees, element by element
    for value, sigma in zip(
        [1.0, 2.0, 4.0], without_x.uncertainty, strict=True
    ):
        scalar = (Q_(value, "m", 0.1) + x).to("cm") - x
        assert scalar.uncertainty == pytest.approx(sigma, rel=1e-12)


def test_dot_of_a_converted_array_with_its_source(a):
    # sum(100 a_i * a_i) in cm*m: d/da_i = 2 * 100 a_i
    dot = a.to("cm").dot(a)
    assert dot.magnitude == pytest.approx(1400.0)
    assert dot.uncertainty == pytest.approx(
        2 * 100 * 0.1 * math.sqrt(14.0), rel=1e-12
    )


def test_fahrenheit_points_to_kelvin_carry_five_ninths():
    t = Q_(np.array([68.0, 77.0]), "degF", uncertainty=0.9)
    kelvin = t.to("K")
    np.testing.assert_allclose(kelvin.magnitude, [293.15, 298.15], rtol=1e-12)
    np.testing.assert_allclose(kelvin.uncertainty, 0.5, rtol=1e-12)
    _zero((kelvin - t).uncertainty)
    # d(T^2)/dt_F = 2 T * 5/9
    analytic = 2 * np.array([293.15, 298.15]) * 0.9 * 5 / 9
    np.testing.assert_allclose(
        (kelvin * kelvin).uncertainty, analytic, rtol=1e-12
    )
    np.testing.assert_allclose((t * t).uncertainty, analytic, rtol=1e-12)
    np.testing.assert_allclose((kelvin * t).uncertainty, analytic, rtol=1e-12)


def test_celsius_points_to_millikelvin_and_back():
    t = Q_(np.array([20.0, 25.0]), "degC", uncertainty=0.5)
    milli = t.to("mK")
    np.testing.assert_allclose(milli.uncertainty, 500.0, rtol=1e-12)
    _zero((milli - t).magnitude)
    _zero((milli - t).uncertainty)
    _zero((t - t).uncertainty)
    _zero((milli.to("degC") - t).uncertainty)


def test_temperature_intervals_keep_their_source():
    d = Q_(np.array([1.0, 2.0]), "delta_degC", uncertainty=0.5)
    _zero((d.to("delta_degF") - d).uncertainty)
    _zero((d.to("K") - d).uncertainty)


def test_a_point_moved_by_a_converted_interval_and_back():
    # (t + d_F) - d: d cancels, the point keeps t's own error
    t = Q_(np.array([20.0, 25.0]), "degC", uncertainty=0.5)
    d = Q_(np.array([1.0, 2.0]), "delta_degC", uncertainty=0.3)
    moved = t + d.to("delta_degF")
    np.testing.assert_allclose(moved.magnitude, [21.0, 27.0], rtol=1e-12)
    np.testing.assert_allclose(
        moved.uncertainty, math.hypot(0.5, 0.3), rtol=1e-12
    )
    back = moved - d
    np.testing.assert_allclose(back.magnitude, [20.0, 25.0], rtol=1e-12)
    np.testing.assert_allclose(back.uncertainty, 0.5, rtol=1e-12)


def test_a_scalar_point_moved_by_an_interval_in_another_scale_and_back():
    # the scalar route of the same program (lineage)
    t = Q_(20.0, "degC", 0.5)
    d = Q_(1.0, "delta_degC", 0.3)
    back = (t + d.to("delta_degF")) - d
    assert back.magnitude == pytest.approx(20.0)
    assert back.uncertainty == pytest.approx(0.5, rel=1e-12)
    # the interval in the point's scale is the same interval: (t + d_F) - d_F
    d_f = Q_(1.8, "delta_degF", 0.54)
    assert ((t + d_f) - d_f).uncertainty == pytest.approx(0.5, rel=1e-12)


def test_levels_convert_keeping_their_source():
    level = Q_(np.array([10.0, 20.0]), "dB", uncertainty=0.1)
    ratio = level.to("1")
    # r = 10^(L/10): dr/dL = r ln10 / 10
    analytic = np.array([10.0, 100.0]) * math.log(10) / 10 * 0.1
    np.testing.assert_allclose(ratio.uncertainty, analytic, rtol=1e-12)
    _zero((level.to("1") - level.to("1")).uncertainty)
    # L + L = L + 10 log10(2): d/dL = 1, one error (independent: 0.0707)
    total = level + level
    np.testing.assert_allclose(
        total.magnitude, level.magnitude + 10 * math.log10(2), rtol=1e-12
    )
    np.testing.assert_allclose(total.uncertainty, 0.1, rtol=1e-12)


def test_one_element_arrays_inside_a_store_context():
    with physure.PhysureContext():
        one = Q_(np.array([2.0]), "m", uncertainty=0.1)
        _zero((one.to("cm") - one).uncertainty)
        np.testing.assert_allclose(
            (one.to("cm") + one).uncertainty, 20.0, rtol=1e-12
        )
        t = Q_(np.array([20.0]), "degC", uncertainty=0.5)
        np.testing.assert_allclose(
            (t * t).uncertainty, 2 * 293.15 * 0.5, rtol=1e-12
        )


def test_one_element_arrays_outside_a_context_hold_a_variance_as_before():
    # not in this change: a variance-only array converts by f^2, not correlated
    from physure.domain.measurement.uncertainty import VarianceModel

    one = Q_(np.array([2.0]), "m", uncertainty=0.1)
    converted = one.to("cm")
    assert isinstance(converted._uncertainty_obj, VarianceModel)
    np.testing.assert_allclose(converted.uncertainty, 10.0, rtol=1e-15)


def test_the_uncorrelated_mode():
    from physure.domain.measurement.uncertainty import VarianceModel

    with physure.propagation_mode("uncorrelated"):
        u = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
        # an array made there still knows its own elements (u - u is 0), so
        # its conversion is the same measurement too
        _zero((u - u).uncertainty)
        _zero((u.to("cm") - u).uncertainty)
        # an array of variances only converts by f^2, as before
        shifted = Q_(np.array([1.0, 2.0]), "m") + Q_(0.5, "m", 0.3)
        assert isinstance(shifted._uncertainty_obj, VarianceModel)
        converted = shifted.to("cm")
        assert isinstance(converted._uncertainty_obj, VarianceModel)
        np.testing.assert_allclose(converted.uncertainty, 30.0, rtol=1e-12)


def test_exact_arrays_convert_as_before():
    values = np.array([1.5, -2.25, 3.0])
    converted = Q_(values, "m").to("cm")
    np.testing.assert_array_equal(converted.magnitude, values * 100.0)
    np.testing.assert_array_equal(converted.uncertainty, 0.0)
    point = Q_(values, "degC").to("K")
    np.testing.assert_array_equal(point.magnitude, values + 273.15)
    np.testing.assert_array_equal(point.uncertainty, 0.0)


def test_a_materialized_slot_is_the_scaled_source(a):
    # anything that asks for the converted array's own slot gets one that the store
    # knows is 100 x the source: variance 100^2 s^2, covariance 100 s^2
    from physure.core.dispatcher import BackendManager
    from physure.domain.measurement.vectorized_uncertainty import ensure_store

    model = a.to("cm")._uncertainty_obj
    backend = BackendManager.get_backend(a.magnitude)
    store = ensure_store(backend)
    own = model.ensure_vector_slice(backend)
    assert model.ensure_vector_slice(backend) == own
    source = a._uncertainty_obj.vector_slice
    variance = np.asarray(store.get_covariance_block(own, own).toarray())
    covariance = np.asarray(store.get_covariance_block(own, source).toarray())
    np.testing.assert_allclose(variance, np.eye(3) * 100.0, rtol=1e-12)
    np.testing.assert_allclose(covariance, np.eye(3) * 1.0, rtol=1e-12)


def test_large_arrays_convert_fast_and_keep_the_correlation():
    rng = np.random.default_rng(11)
    n = 100_000
    values = rng.normal(size=n)
    sigmas = rng.uniform(0.01, 0.2, size=n)
    exact = Q_(values, "m")
    start = time.perf_counter()
    exact.to("cm")
    exact_time = time.perf_counter() - start
    a = Q_(values, "m", uncertainty=sigmas)
    start = time.perf_counter()
    converted = a.to("cm")
    uncertain_time = time.perf_counter() - start
    np.testing.assert_allclose(converted.uncertainty, 100 * sigmas, rtol=1e-15)
    # a dense n x n Jacobian (74 GiB) never appears: about the exact time
    assert uncertain_time < 20 * exact_time + 0.05
    summed = np.sum(converted)
    analytic = 100 * math.sqrt(float(np.sum(sigmas**2)))
    assert summed.uncertainty == pytest.approx(analytic, rel=1e-9)
    # (an element-wise difference of 1e5 uncertain elements, `converted - a` as
    # `a - a`, is the core's dense n x n Jacobian: not in this change)
    dot = converted.dot(Q_(np.ones(n), "m"))
    assert dot.uncertainty == pytest.approx(analytic, rel=1e-9)


def test_the_covariance_matrix_of_a_converted_array(a):
    from physure.plotting import get_covariance_matrix

    source = np.asarray(get_covariance_matrix(a))
    np.testing.assert_allclose(source, np.eye(3) * 0.01, rtol=1e-12)
    np.testing.assert_allclose(
        np.asarray(get_covariance_matrix(a.to("cm"))),
        source * 100.0**2,
        rtol=1e-12,
    )


# --- levels: one slope per element (review I1) -------------------------------


def _level(n, seed=3):
    rng = np.random.default_rng(seed)
    values = rng.uniform(0.0, 30.0, n)
    sigmas = rng.uniform(0.01, 0.2, n)
    return values, sigmas, Q_(values, "dB", uncertainty=sigmas)


@pytest.mark.parametrize("n", [1_000, 10_000, 100_000])
def test_a_level_converts_by_its_slope_without_a_dense_jacobian(n):
    values, sigmas, level = _level(n)
    start = time.perf_counter()
    ratio = level.to("1")
    elapsed = time.perf_counter() - start
    expected = 10 ** (values / 10)
    np.testing.assert_allclose(ratio.magnitude, expected, rtol=1e-12)
    # r = 10^(L/10): dr/dL = r ln10 / 10
    np.testing.assert_allclose(
        ratio.uncertainty, expected * math.log(10) / 10 * sigmas, rtol=1e-12
    )
    # an n x n Jacobian is 74.5 GiB at 1e5 (and 0.3 s at 1e4); the base took 6.5 ms
    assert elapsed < 0.25
    # the sum reads the slopes from the source's slot
    assert np.sum(ratio).uncertainty == pytest.approx(
        math.sqrt(float(np.sum((expected * math.log(10) / 10 * sigmas) ** 2))),
        rel=1e-9,
    )


def test_a_converted_level_is_its_source_chained_and_correlated():
    values, sigmas, level = _level(1_000)
    ratio = level.to("1")
    _zero((level.to("1") - ratio).uncertainty)
    # chained: the slopes multiply (percent is 100 x the ratio)
    percent = ratio.to("percent")
    np.testing.assert_allclose(
        percent.uncertainty, 100 * ratio.uncertainty, rtol=1e-12
    )
    _zero((percent - ratio).uncertainty)
    _zero((level.to("percent") - ratio).uncertainty)
    # a level sum with a shared scalar x: (L + x) as a ratio, minus x, is L's ratio
    x = Q_(3.0, "dB", 0.2)
    total = (level + x).to("1") - x.to("1")
    np.testing.assert_allclose(total.magnitude, 10 ** (values / 10), rtol=1e-9)
    np.testing.assert_allclose(
        total.uncertainty,
        10 ** (values / 10) * math.log(10) / 10 * sigmas,
        rtol=1e-9,
    )


def test_np_sum_of_a_level_is_still_refused():
    _, _, level = _level(10)
    with pytest.raises(TypeError, match="level"):
        np.sum(level)


def test_a_converted_level_survives_pickle_and_copy():
    import copy
    import pickle

    _, _, level = _level(50)
    ratio = level.to("1")
    for clone in (
        pickle.loads(pickle.dumps(ratio)),
        copy.copy(ratio),
        copy.deepcopy(ratio),
    ):
        np.testing.assert_allclose(clone.uncertainty, ratio.uncertainty)
        _zero((clone - ratio).uncertainty)


def test_a_scaled_slot_with_one_factor_per_element_hashes_by_its_numbers():
    from physure.domain.measurement.uncertainty import CovarianceModel

    def model(factor):
        return CovarianceModel(
            std_dev_internal=0.5, scaled_slot=(slice(0, 2), factor)
        )

    same = hash(model(np.array([1.0, 2.0])))
    assert same == hash(model(np.array([1.0, 2.0])))
    assert same != hash(model(np.array([1.0, 3.0])))
    assert hash(model(2.0)) == hash(model(2.0))


def test_a_scaled_slot_round_trips_through_a_jax_pytree():
    jax = pytest.importorskip("jax")
    from physure.domain.measurement.uncertainty import (
        CovarianceModel,
        _register_jax_pytrees,
    )

    _register_jax_pytrees()
    factor = np.array([1.0, 2.0, 3.0])
    original = CovarianceModel(
        std_dev_internal=np.array([0.1, 0.2, 0.3]),
        scaled_slot=(slice(4, 7), factor),
    )
    leaves, tree = jax.tree_util.tree_flatten(original)
    # a factor per element is data (a leaf), not static structure
    assert any(
        np.shape(leaf) == (3,) and np.allclose(leaf, factor) for leaf in leaves
    )
    rebuilt = jax.tree_util.tree_unflatten(tree, leaves)
    assert rebuilt.scaled_slot[0] == slice(4, 7)
    np.testing.assert_array_equal(rebuilt.scaled_slot[1], factor)
    scalar = CovarianceModel(
        std_dev_internal=np.array([0.1]), scaled_slot=(slice(0, 1), 100.0)
    )
    leaves, tree = jax.tree_util.tree_flatten(scalar)
    assert jax.tree_util.tree_unflatten(tree, leaves).scaled_slot == (
        slice(0, 1),
        100.0,
    )


def test_the_covariance_matrix_and_own_slot_of_a_converted_level():
    from physure.core.dispatcher import BackendManager
    from physure.domain.measurement.vectorized_uncertainty import ensure_store
    from physure.plotting import get_covariance_matrix

    values, sigmas, level = _level(4)
    ratio = level.to("1")
    slope = 10 ** (values / 10) * math.log(10) / 10
    np.testing.assert_allclose(
        np.asarray(get_covariance_matrix(ratio)),
        np.diag((slope * sigmas) ** 2),
        rtol=1e-12,
    )
    backend = BackendManager.get_backend(values)
    own = ratio._uncertainty_obj.ensure_vector_slice(backend)
    block = ensure_store(backend).get_covariance_block(own, own).toarray()
    np.testing.assert_allclose(
        block, np.diag((slope * sigmas) ** 2), rtol=1e-12
    )


# --- the store an array's slot belongs to (review I2) -------------------------

_OTHER_STORE = "another covariance store"


def _array():
    return Q_(np.array([1.0, 2.0, 3.0]), "m", uncertainty=[0.1, 0.2, 0.3])


_SUM_SIGMA = math.sqrt(0.14)


def test_an_array_made_and_used_outside_any_context():
    a = _array()
    assert np.sum(a).uncertainty == pytest.approx(_SUM_SIGMA, rel=1e-12)
    assert np.sum(a.to("cm")).uncertainty == pytest.approx(
        100 * _SUM_SIGMA, rel=1e-12
    )


def test_an_array_made_and_used_inside_one_context():
    with physure.PhysureContext():
        a = _array()
        assert np.sum(a.to("cm")).uncertainty == pytest.approx(
            100 * _SUM_SIGMA, rel=1e-12
        )
        _zero((a.to("cm") - a).uncertainty)


def test_an_array_made_outside_is_refused_inside_a_context():
    # its slot is in another store: read there it was a silent 0.0
    a = _array()
    converted = a.to("cm")
    with physure.PhysureContext():
        for use in (
            lambda: np.sum(a),
            lambda: np.sum(converted),
            lambda: a * 2,
            lambda: converted + a,
            lambda: a.dot(a),
        ):
            with pytest.raises(TypeError, match=_OTHER_STORE):
                use()
        # a conversion reads no slot: it is made, and refused where it is used
        inside = a.to("mm")
        with pytest.raises(TypeError, match=_OTHER_STORE):
            np.sum(inside)
    assert np.sum(inside).uncertainty == pytest.approx(
        1000 * _SUM_SIGMA, rel=1e-12
    )


def test_an_array_made_inside_is_refused_after_the_context():
    with physure.PhysureContext():
        a = _array()
        converted = a.to("cm")
    with pytest.raises(TypeError, match=_OTHER_STORE):
        np.sum(a)
    with pytest.raises(TypeError, match=_OTHER_STORE):
        converted - a


def test_nested_contexts_keep_each_array_in_its_own_store():
    # decision: an inner context is a store of its own, so an array of the outer
    # one is refused there, and usable again once the inner context is left
    with physure.PhysureContext():
        outer = _array()
        with physure.PhysureContext():
            with pytest.raises(TypeError, match=_OTHER_STORE):
                np.sum(outer)
            inner = _array()
            assert np.sum(inner).uncertainty == pytest.approx(
                _SUM_SIGMA, rel=1e-12
            )
        assert np.sum(outer.to("cm")).uncertainty == pytest.approx(
            100 * _SUM_SIGMA, rel=1e-12
        )


def test_scalars_and_exact_arrays_cross_contexts_freely():
    x = Q_(2.0, "m", 0.1)
    exact = Q_(np.array([1.0, 2.0]), "m")
    with physure.PhysureContext():
        b = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
        np.testing.assert_allclose(
            ((b + x) - x.to("cm")).uncertainty, 0.1, rtol=1e-12
        )
        np.testing.assert_allclose((exact + b).uncertainty, 0.1, rtol=1e-12)
        np.testing.assert_allclose(
            (exact.to("cm") + x).uncertainty, 10.0, rtol=1e-12
        )


# --- a scalar with lineage in the store (review I3) --------------------------


def _store():
    from physure.core.dispatcher import BackendManager
    from physure.domain.measurement.vectorized_uncertainty import ensure_store

    return ensure_store(BackendManager.get_backend(np.zeros(1)))


def test_a_scalar_of_many_sources_meets_an_array_fast_and_exactly():
    import functools
    import operator

    k = 10_000
    x = functools.reduce(operator.add, [Q_(1.0, "m", 0.01) for _ in range(k)])
    b = Q_(np.array([1.0, 2.0, 3.0]), "m", uncertainty=0.1)
    start = time.perf_counter()
    total = b + x
    elapsed = time.perf_counter() - start
    np.testing.assert_allclose(
        total.uncertainty, math.hypot(0.1, 0.01 * math.sqrt(k)), rtol=1e-12
    )
    np.testing.assert_allclose(((b + x) - x).uncertainty, 0.1, rtol=1e-9)
    # one registration of the 1e4 sources, not one per source (1.3 s)
    assert elapsed < 0.2


def test_scalars_that_share_some_sources():
    b = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
    p, q1, q2 = Q_(1.0, "m", 0.3), Q_(1.0, "m", 0.4), Q_(1.0, "m", 0.2)
    x1, x2 = p + q1, p + q2
    # b + x1 - x2 = b + q1 - q2
    np.testing.assert_allclose(
        ((b + x1) - x2).uncertainty, math.sqrt(0.01 + 0.16 + 0.04), rtol=1e-12
    )
    # b x1 - b x2 = b (q1 - q2)
    np.testing.assert_allclose(
        (b * x1 - b * x2).uncertainty,
        np.array([1.0, 2.0]) * math.hypot(0.4, 0.2),
        rtol=1e-12,
    )
    y = Q_(2.0, "m", 0.3) + Q_(1.0, "m", 0.4)  # sigma 0.5, two sources
    np.testing.assert_allclose(
        ((b + y) + y).uncertainty, math.hypot(0.1, 2 * 0.5), rtol=1e-12
    )
    np.testing.assert_allclose(
        ((b + y) - y.to("cm") - y.to("km")).uncertainty,
        math.hypot(0.1, 0.5),
        rtol=1e-12,
    )


def test_a_combination_met_again_after_the_cache_is_the_same_error(
    monkeypatch,
):
    import physure.domain.measurement.vectorized_uncertainty as store_module

    monkeypatch.setattr(store_module, "_LINEAGE_CACHE", 1)
    b = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
    x = Q_(2.0, "m", 0.3) + Q_(1.0, "m", 0.4)
    y = Q_(5.0, "m", 0.2) + Q_(1.0, "m", 0.2)
    first = b + x
    _ = b + y  # x's combination leaves the cache
    np.testing.assert_allclose((first - x).uncertainty, 0.1, rtol=1e-12)
    assert len(_store()._lineage_combinations) == 1


def test_the_store_keeps_no_scalar_model_alive():
    b = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
    for value in range(5):
        _ = b + Q_(float(value), "m", 0.01)
    assert _store()._model_slots == {}


def test_np_concatenate_of_uncertain_arrays_is_refused():
    # it gave an uncertainty of 0.0; exact arrays concatenate as before
    a = _array()
    exact = Q_(np.array([4.0, 5.0]), "m")
    for parts in ([a.to("cm"), a.to("cm")], [exact, a], [a, exact]):
        with pytest.raises(TypeError, match="concatenate"):
            np.concatenate(parts)
    joined = np.concatenate([exact, exact])
    np.testing.assert_array_equal(joined.magnitude, [4.0, 5.0, 4.0, 5.0])


def test_an_element_wise_operation_reads_a_level_slope_per_element():
    _, _, level = _level(20)
    ratio = level.to("1")
    # a sparse Jacobian (the sum) and an element-wise one (the product)
    np.testing.assert_allclose(
        (ratio + ratio).uncertainty, 2 * ratio.uncertainty, rtol=1e-12
    )
    np.testing.assert_allclose(
        (ratio * ratio).uncertainty,
        2 * ratio.magnitude * ratio.uncertainty,
        rtol=1e-12,
    )


def test_a_source_alone_in_its_slot_and_in_a_combination_is_one_error():
    b = Q_(np.array([1.0, 2.0]), "m", uncertainty=0.1)
    p, q = Q_(1.0, "m", 0.3), Q_(1.0, "m", 0.4)
    _ = b + p  # p's source gets a slot of its own
    # (b + (p + q)) - p = b + q
    np.testing.assert_allclose(
        ((b + (p + q)) - p).uncertainty, math.hypot(0.1, 0.4), rtol=1e-12
    )


def test_with_slot_scale_scales_every_form_of_jacobian_by_column():
    # torch and JAX hand dense or element-wise Jacobians; NumPy's are sparse
    from scipy import sparse

    from physure.domain.measurement.uncertainty import with_slot_scale

    factor = np.array([2.0, 3.0, 5.0])
    dense = np.arange(6.0).reshape(2, 3)
    np.testing.assert_allclose(
        with_slot_scale(dense, factor), dense @ np.diag(factor)
    )
    scaled = with_slot_scale(sparse.csr_matrix(np.eye(3)), factor)
    np.testing.assert_allclose(scaled.toarray(), np.diag(factor))
    element_wise = np.array([1.0, -1.0, 0.5])
    np.testing.assert_allclose(
        with_slot_scale(element_wise, factor), element_wise * factor
    )
    np.testing.assert_allclose(with_slot_scale(4.0, factor), 4.0 * factor)
    assert with_slot_scale(dense, 1.0) is dense
