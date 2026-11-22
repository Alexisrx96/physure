"""NumPy never strips a unit or an uncertainty silently; `==` and `hash` agree.

Spec: docs/superpowers/specs/2026-11-21-python-numpy-units-hash-design.md. Every row of
its section 1 is here, on both paths a quantity can take: a list of scalar quantities
(NumPy holds the quantities themselves in an object array) and one array-backed quantity
(the quantity's own `__array_function__`), each exact and with an uncertainty.
"""

from __future__ import annotations

import functools
import math
import operator
import warnings

import numpy as np
import pytest

import physure
from physure import Q_
from physure.domain.exceptions import IncompatibleUnitsError


def _reduce_add(quantities):
    """The sum through `Quantity` arithmetic, element after element.

    The builtin `sum` starts at the plain number 0, which `0 + 1 m` refuses, so the
    reference is the same chain of additions without that start.
    """
    return functools.reduce(operator.add, quantities)


@pytest.fixture
def measured():
    return [Q_(1, "m", 0.1), Q_(2, "m", 0.1), Q_(4, "m", 0.2)]


@pytest.fixture
def no_strip_warning():
    with warnings.catch_warnings():
        warnings.simplefilter("error", physure.UnitStrippedWarning)
        yield


# --- a list of scalar quantities goes through Quantity arithmetic ---------------


def test_np_sum_of_a_list_keeps_unit_and_uncertainty(measured):
    total = np.sum(measured)
    assert isinstance(total, physure.Quantity)
    assert str(total.unit) == "m"
    assert total.magnitude == 7.0
    assert total.uncertainty == pytest.approx(math.sqrt(0.06), rel=1e-12)
    assert total.uncertainty == pytest.approx(
        _reduce_add(measured).uncertainty, rel=1e-15
    )


def test_np_mean_of_a_list_keeps_unit_and_uncertainty(measured):
    mean = np.mean(measured)
    assert isinstance(mean, physure.Quantity)
    assert str(mean.unit) == "m"
    assert mean.magnitude == pytest.approx(7.0 / 3.0, rel=1e-15)
    assert mean.uncertainty == pytest.approx(math.sqrt(0.06) / 3.0, rel=1e-12)


def test_np_dot_of_lists_is_a_square_metre_with_its_uncertainty(measured):
    dot = np.dot(measured, measured)
    assert isinstance(dot, physure.Quantity)
    assert dot.dimension == Q_(1, "m^2").dimension
    assert dot.magnitude == pytest.approx(21.0)
    # d(sum x_i^2)/dx_i = 2 x_i: sqrt(0.2^2 + 0.4^2 + 1.6^2)
    assert dot.uncertainty == pytest.approx(math.sqrt(2.76), rel=1e-12)


def test_np_cross_of_lists_keeps_each_element():
    a = [Q_(1, "m", 0.1), Q_(2, "m", 0.1), Q_(3, "m", 0.1)]
    b = [Q_(3, "m", 0.1), Q_(2, "m", 0.1), Q_(1, "m", 0.1)]
    cross = np.cross(a, b)
    assert cross.shape == (3,)
    for element in cross:
        assert isinstance(element, physure.Quantity)
        assert element.dimension == Q_(1, "m^2").dimension
    first = a[1] * b[2] - a[2] * b[1]
    assert cross[0].magnitude == pytest.approx(first.magnitude)
    assert cross[0].uncertainty == pytest.approx(first.uncertainty, rel=1e-12)
    assert cross[0].uncertainty > 0


def test_np_prod_cumsum_max_of_lists_keep_unit_and_uncertainty(measured):
    prod = np.prod(measured)
    assert prod.dimension == Q_(1, "m^3").dimension
    assert prod.magnitude == pytest.approx(8.0)
    reference = measured[0] * measured[1] * measured[2]
    assert prod.uncertainty == pytest.approx(reference.uncertainty, rel=1e-12)

    running = np.cumsum(measured)
    assert [q.magnitude for q in running] == [1.0, 3.0, 7.0]
    assert running[-1].uncertainty == pytest.approx(math.sqrt(0.06))

    biggest = np.max(measured)
    assert biggest.magnitude == 4.0
    assert str(biggest.unit) == "m"
    assert biggest.uncertainty == pytest.approx(0.2)


def test_np_sum_of_a_correlated_list_counts_one_error_once():
    x = Q_(2, "m", 0.1)
    items = [x, x, 2 * x]
    assert np.sum(items).uncertainty == pytest.approx(0.4, rel=1e-12)
    assert np.mean(items).uncertainty == pytest.approx(0.4 / 3, rel=1e-12)


def test_np_sum_of_a_list_in_mixed_units_converts():
    total = np.sum([Q_(1, "m"), Q_(50, "cm")])
    assert total.to("m").magnitude == pytest.approx(1.5)


def test_np_sum_of_exact_quantities_is_exact():
    total = np.sum([Q_(1, "m"), Q_(2, "m")])
    assert total.magnitude == 3.0
    assert total.uncertainty == 0.0


# --- array-backed quantities: np.sum and np.mean propagate --------------------


def test_np_sum_and_mean_of_an_exact_array():
    a = Q_(np.array([1.0, 2.0, 4.0]), "m")
    assert np.sum(a).magnitude == 7.0
    assert str(np.sum(a).unit) == "m"
    assert np.mean(a).magnitude == pytest.approx(7.0 / 3.0)
    matrix = Q_(np.array([[1.0, 2.0], [3.0, 4.0]]), "s")
    assert np.allclose(np.sum(matrix, axis=0).magnitude, [4.0, 6.0])
    assert np.allclose(np.mean(matrix, axis=1).magnitude, [1.5, 3.5])


def test_np_mean_of_an_uncertain_array_is_not_exact():
    a = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1)
    mean = np.mean(a)
    assert mean.magnitude == pytest.approx(7.0 / 3.0)
    # independent errors: sqrt(3) * 0.1 / 3, not the zero it used to be
    assert mean.uncertainty == pytest.approx(math.sqrt(3) * 0.1 / 3, rel=1e-12)


def test_np_sum_and_mean_of_an_uncertain_array_agree_with_element_arithmetic():
    a = Q_(
        np.array([1.0, 2.0, 4.0]), "m", uncertainty=np.array([0.1, 0.2, 0.3])
    )
    reference = _reduce_add(list(a))
    total = np.sum(a)
    assert str(total.unit) == "m"
    assert total.magnitude == reference.magnitude
    assert total.uncertainty == pytest.approx(reference.uncertainty, rel=1e-15)
    mean = np.mean(a)
    assert mean.magnitude == pytest.approx(
        (reference / 3).magnitude, rel=1e-15
    )
    assert mean.uncertainty == pytest.approx(
        (reference / 3).uncertainty, rel=1e-15
    )


def test_np_sum_of_an_array_with_correlated_elements_counts_the_shared_error():
    # every element carries the one error of `x`: the covariance store holds it
    b = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1)
    x = Q_(2, "m", 0.1)
    a = b + x
    # sum(b_i + x) = sum(b_i) + 3 x, with lineage scalars standing for the elements
    b0, b1, b2 = (Q_(v, "m", 0.1) for v in (1.0, 2.0, 4.0))
    reference = (b0 + x) + (b1 + x) + (b2 + x)
    assert reference.uncertainty == pytest.approx(math.sqrt(3 * 0.01 + 0.09))
    assert np.sum(a).magnitude == pytest.approx(13.0)
    assert np.sum(a).uncertainty == pytest.approx(
        reference.uncertainty, rel=1e-12
    )
    assert np.mean(a).uncertainty == pytest.approx(
        reference.uncertainty / 3, rel=1e-12
    )
    # element arithmetic on `list(a)` cannot see the shared error (each `a[i]` is a
    # new measurement), which is why the sum is not taken that way
    assert _reduce_add(list(a)).uncertainty < reference.uncertainty


def test_np_sum_of_an_uncertain_array_in_uncorrelated_mode():
    with physure.propagation_mode("uncorrelated"):
        a = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1)
        assert np.sum(a).uncertainty == pytest.approx(math.sqrt(3) * 0.1)


def test_np_sum_of_an_uncertain_array_along_an_axis_is_refused():
    a = Q_(np.array([[1.0, 2.0], [3.0, 4.0]]), "m", uncertainty=0.1)
    with pytest.raises(TypeError, match="axis"):
        np.sum(a, axis=0)
    with pytest.raises(TypeError, match="axis"):
        np.mean(a, axis=1)


def test_np_sum_of_temperature_points_is_refused_and_their_mean_is_a_point():
    points = Q_(np.array([10.0, 20.0, 30.0]), "degC")
    with pytest.raises(ValueError, match="absolute"):
        np.sum(points)
    mean = np.mean(points)
    assert mean.magnitude == pytest.approx(20.0)
    assert mean == Q_(20.0, "degC")


def test_np_sum_and_mean_of_levels_are_refused():
    levels = Q_(np.array([10.0, 20.0]), "dB")
    with pytest.raises(TypeError, match="level"):
        np.sum(levels)
    with pytest.raises(TypeError, match="level"):
        np.mean(levels)


@pytest.mark.parametrize(
    "func",
    [np.std, np.var, np.median, np.linalg.norm],
    ids=["std", "var", "median", "norm"],
)
def test_reductions_not_implemented_on_an_array_still_raise(func):
    a = Q_(np.array([1.0, 2.0, 4.0]), "m", uncertainty=0.1)
    with pytest.raises(TypeError):
        func(a)


# --- division and multiplication by NumPy scalars -------------------------------


@pytest.mark.parametrize(
    "number", [np.int64(2), np.int32(2), np.float32(2.0), np.float64(2.0)]
)
def test_quantity_divided_by_a_numpy_scalar(number):
    q = Q_(2, "m", 0.1)
    half = q / number
    assert half.magnitude == pytest.approx(1.0)
    assert str(half.unit) == "m"
    assert half.uncertainty == pytest.approx(0.05)
    double = q * number
    assert double.magnitude == pytest.approx(4.0)
    assert double.uncertainty == pytest.approx(0.2)
    inverse = number / q
    assert inverse.magnitude == pytest.approx(1.0)
    assert inverse.dimension == Q_(1, "1/m").dimension


# --- implicit conversion keeps the quantity --------------------------------------


def test_np_asarray_of_a_scalar_holds_the_quantity(no_strip_warning):
    q = Q_(1, "m", 0.1)
    array = np.asarray(q)
    assert array.dtype == object
    assert array.shape == ()
    assert array[()] is q


def test_np_asarray_of_an_array_backed_quantity_holds_its_elements(
    no_strip_warning,
):
    a = Q_(
        np.array([1.0, 2.0, 4.0]), "m", uncertainty=np.array([0.1, 0.2, 0.3])
    )
    array = np.asarray(a)
    assert array.dtype == object
    assert array.shape == (3,)
    assert [e.magnitude for e in array] == [1.0, 2.0, 4.0]
    assert [e.uncertainty for e in array] == pytest.approx([0.1, 0.2, 0.3])
    assert all(str(e.unit) == "m" for e in array)
    grid = np.asarray(Q_(np.array([[1.0, 2.0], [3.0, 4.0]]), "s"))
    assert grid.shape == (2, 2)
    assert grid[1, 0].magnitude == 3.0


def test_np_asarray_of_a_list_holds_the_quantities(measured, no_strip_warning):
    array = np.asarray(measured)
    assert array.dtype == object
    assert array.shape == (3,)
    assert all(a is b for a, b in zip(array, measured, strict=True))


# --- explicit stripping works, with a warning that names the loss -------------


def test_float_of_a_length_warns_and_names_the_unit():
    with pytest.warns(physure.UnitStrippedWarning) as record:
        value = float(Q_(1, "m"))
    assert value == 1.0
    message = str(record[0].message)
    assert "'m'" in message
    assert ".to('m').magnitude" in message


def test_float_is_the_value_in_the_coherent_unit():
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(Q_(1, "km")) == 1000.0
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(Q_(20, "degC")) == 293.15
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(Q_(68, "degF")) == 293.15


def test_float_of_an_uncertain_quantity_names_the_uncertainty():
    with pytest.warns(physure.UnitStrippedWarning, match="0.1"):
        assert float(Q_(2, "m", 0.1)) == 2.0
    with pytest.warns(physure.UnitStrippedWarning, match="0.1"):
        assert float(Q_(0.5, "", 0.1)) == 0.5


def test_float_of_a_dimensionless_exact_quantity_is_silent(no_strip_warning):
    assert float(Q_(50, "percent")) == 0.5
    assert float(Q_(2, "")) == 2.0
    assert float(Q_(3, "m") / Q_(1, "m")) == 3.0


def test_float_of_a_level_warns_that_the_level_is_read_as_its_ratio():
    with pytest.warns(physure.UnitStrippedWarning, match="dB"):
        assert float(Q_(20, "dB")) == pytest.approx(100.0)


def test_int_and_complex_strip_with_a_warning():
    with pytest.warns(physure.UnitStrippedWarning):
        assert int(Q_(2.7, "m")) == 2
    with pytest.warns(physure.UnitStrippedWarning):
        assert complex(Q_(1, "m")) == 1 + 0j


def test_np_asarray_with_a_float_dtype_strips_with_a_warning(measured):
    with pytest.warns(physure.UnitStrippedWarning):
        array = np.asarray(Q_(1, "m"), dtype=float)
    assert array.dtype == np.float64
    assert array == 1.0
    with pytest.warns(physure.UnitStrippedWarning, match="uncertainty"):
        values = np.array(measured, dtype=float)
    assert values.tolist() == [1.0, 2.0, 4.0]
    with pytest.warns(physure.UnitStrippedWarning):
        stripped = np.asarray(Q_(np.array([1.0, 2.0]), "km"), dtype=float)
    assert stripped.tolist() == [1000.0, 2000.0]


def test_the_explicit_route_is_silent(measured, no_strip_warning):
    q = Q_(100, "cm", 0.1)
    assert q.to("m").magnitude == 1.0
    assert q.magnitude == 100.0
    assert [m.to("m").magnitude for m in measured] == [1.0, 2.0, 4.0]


def test_unit_stripped_warning_is_a_user_warning_the_filter_silences():
    assert issubclass(physure.UnitStrippedWarning, UserWarning)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        warnings.simplefilter("ignore", physure.UnitStrippedWarning)
        assert float(Q_(1, "m")) == 1.0


# --- == and hash agree --------------------------------------------------------

EQUAL_PAIRS = [
    ((1, "m"), (100, "cm")),
    ((0, "degC"), (273.15, "K")),
    ((20, "degC"), (68, "degF")),
    ((5, "delta_degC"), (5, "K")),
    ((1, "km"), (1000, "m")),
    ((1, "inch"), (2.54, "cm")),
    ((1, "ft"), (12, "inch")),
    ((1, "kWh"), (3.6e6, "J")),
    ((100, "percent"), (1, "")),
]


@pytest.mark.parametrize(("left", "right"), EQUAL_PAIRS, ids=str)
def test_equal_quantities_have_equal_hashes(left, right):
    a, b = Q_(*left), Q_(*right)
    assert a == b
    assert b == a
    assert hash(a) == hash(b)


def test_a_set_and_a_dict_see_one_length_in_two_units():
    assert len({Q_(1, "m"), Q_(100, "cm")}) == 1
    table = {Q_(1, "m"): "one metre"}
    assert table[Q_(100, "cm")] == "one metre"
    assert Q_(0.001, "km") in table


def test_the_uncertainty_is_neither_compared_nor_hashed():
    a, b = Q_(2, "m", 0.1), Q_(2, "m", 0.2)
    assert a == b
    assert hash(a) == hash(b) == hash(Q_(2, "m"))


def test_a_dimensionless_quantity_equals_and_hashes_as_its_coherent_number():
    assert Q_(100, "percent") == 1
    assert Q_(50, "percent") == 0.5
    assert 1 == Q_(100, "percent")  # noqa: SIM300  (the reflected comparison)
    assert hash(Q_(100, "percent")) == hash(1) == hash(1.0)
    assert hash(Q_(50, "percent")) == hash(0.5)
    assert Q_(0.1, "") == 0.1
    assert hash(Q_(0.1, "")) == hash(0.1)
    assert Q_(50, "percent") != 50


def test_an_array_magnitude_is_unhashable():
    with pytest.raises(TypeError):
        hash(Q_(np.array([1.0, 2.0]), "m"))


def test_a_point_is_never_an_interval():
    assert Q_(20, "degC") != Q_(5, "delta_degC")
    assert Q_(293.15, "K") == Q_(20, "degC")
    assert Q_(20, "degC") == Q_(20, "degC")


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ((1, "m"), (1.0000001, "m")),
        ((1, "m"), (1, "s")),
        ((1, "sr"), (1, "rad")),
        ((1, "m"), (99, "cm")),
    ],
    ids=str,
)
def test_unequal_quantities(left, right):
    assert Q_(*left) != Q_(*right)
    assert not Q_(*left) == Q_(*right)  # noqa: SIM201


@pytest.mark.parametrize(
    ("unit", "other"),
    [
        ("m", "cm"),
        ("inch", "mm"),
        ("ft", "inch"),
        ("degC", "degF"),
        ("degF", "K"),
        ("km", "mile"),
        ("kWh", "J"),
        ("percent", "ppm"),
        ("deg", "rad"),
        ("h", "s"),
    ],
)
@pytest.mark.parametrize(
    "value", [0.0, 0.1, 0.3, 1.0, 1.7, 2.54, 12.5, 1e-3, 123.456, 98.6]
)
def test_equal_implies_equal_hash_through_a_conversion(unit, other, value):
    a = Q_(value, unit)
    b = a.to(other)
    if a == b:
        assert hash(a) == hash(b)
    if b == a:
        assert hash(a) == hash(b)


def test_the_coherent_value_follows_a_unit_the_system_redefines():
    from physure.application.context import get_current_system
    from physure.ext.currency import enable_currency

    system = get_current_system()
    enable_currency()
    shipped = Q_(100, "EUR")
    with pytest.warns(physure.UnitStrippedWarning):
        before = float(shipped)
    assert shipped == Q_(before, "USD")
    system.extend_core("[Units]\nEUR = 1.2, $, noprefix, [euros]\n")
    redefined = Q_(100, "EUR")
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(redefined) == 120.0
    assert redefined == Q_(120, "USD")
    assert hash(redefined) == hash(Q_(120, "USD"))


# --- an exact array combined with an uncertain scalar shares the scalar's error -----


@pytest.fixture
def exact_lengths():
    return Q_(np.array([1.0, 2.0, 4.0]), "m")


@pytest.mark.parametrize("scalar_left", [False, True], ids=["e+x", "x+e"])
def test_an_exact_array_plus_an_uncertain_scalar_shares_its_error(
    exact_lengths, scalar_left
):
    x = Q_(0.5, "m", 0.3)
    shifted = x + exact_lengths if scalar_left else exact_lengths + x
    assert np.allclose(shifted.uncertainty, [0.3, 0.3, 0.3])
    # every element carries the one error of x: sum is 3 x, not sqrt(3) x
    assert np.sum(shifted).magnitude == pytest.approx(8.5)
    assert np.sum(shifted).uncertainty == pytest.approx(0.9, rel=1e-12)
    assert np.mean(shifted).uncertainty == pytest.approx(0.3, rel=1e-12)
    # and the same x taken away again leaves the exact array
    assert np.sum(shifted - x).uncertainty == pytest.approx(0.0, abs=1e-12)


def test_an_exact_array_times_an_uncertain_scalar_shares_its_error(
    exact_lengths,
):
    k = Q_(2, "", 0.1)
    scaled = exact_lengths * k
    assert np.allclose(scaled.uncertainty, [0.1, 0.2, 0.4])
    assert np.sum(scaled).uncertainty == pytest.approx(0.7, rel=1e-12)
    halved = exact_lengths / k
    # d(sum e / k)/dk = -sum(e) / k^2 = -7/4
    assert np.sum(halved).uncertainty == pytest.approx(0.175, rel=1e-12)


def test_np_sum_refuses_elements_whose_shared_error_was_already_dropped(
    exact_lengths,
):
    # the uncorrelated mode keeps variances only: the error of x shared by the
    # elements is gone, and a sum read from them would be too small
    with physure.propagation_mode("uncorrelated"):
        shifted = exact_lengths + Q_(0.5, "m", 0.3)
        for reduce in (np.sum, np.mean, np.add.reduce):
            with pytest.raises(TypeError, match="correlation"):
                reduce(shifted)


# --- == is an equivalence on the coherent value, hash follows it -------------------

_FAMILIES = [
    ["m", "cm", "mm", "km", "inch", "ft", "mile", "um"],
    ["s", "ms", "min", "h"],
    ["kg", "g", "lb", "oz"],
    ["K", "degC", "degF", "mK"],
    ["J", "kJ", "kWh", "cal", "eV"],
    ["rad", "deg", "arcsec", "arcmin"],
    ["percent", "ppm", ""],
]


def _random_quantities(rng, count):
    """Quantities in random units of one family, many of them equal by construction."""
    quantities = []
    for _ in range(count):
        family = _FAMILIES[rng.integers(len(_FAMILIES))]
        unit = family[rng.integers(len(family))]
        value = float(np.round(rng.normal(0, 100), int(rng.integers(0, 4))))
        q = Q_(value, unit)
        other = family[rng.integers(len(family))]
        quantities.append((q, q.to(other), family))
    return quantities


def test_float32_and_float64_magnitudes_compare_and_hash_as_one_number():
    a, b = Q_(np.float32(0.1), "m"), Q_(0.1, "m")
    assert (a == b) == (hash(a) == hash(b)) or a != b
    assert a != b  # 0.10000000149011612 m is not 0.1 m
    assert Q_(np.float32(0.5), "m") == Q_(0.5, "m")
    assert hash(Q_(np.float32(0.5), "m")) == hash(Q_(0.5, "m"))
    assert hash(Q_(np.int64(2), "km")) == hash(Q_(2000, "m"))
    assert Q_(np.float64(0.1), "cm") == Q_(1, "mm")


def test_equality_is_transitive_through_conversions():
    rng = np.random.default_rng(20261121)
    violations = 0
    triples = 0
    for q, converted, family in _random_quantities(rng, 1500):
        for third_unit in family[:3]:
            third = converted.to(third_unit)
            if (
                q._is_point() != third._is_point()
                and q.dimension.exponents == {"O": 1}
            ):
                continue
            triples += 1
            if q == converted and converted == third and q != third:
                violations += 1
    assert triples >= 3000
    assert violations == 0


def test_equal_quantities_always_hash_alike():
    rng = np.random.default_rng(7)
    pairs = _random_quantities(rng, 5000)
    scalar_types = (np.float64, np.float32, float)
    checked = 0
    for index, (q, converted, _) in enumerate(pairs):
        kind = scalar_types[index % 3]
        left = Q_(kind(q.magnitude), q.unit)
        for a, b in ((q, converted), (left, q), (left, converted)):
            checked += 1
            if a == b:
                assert hash(a) == hash(b), (a, b)
    assert checked >= 5000


def test_a_value_beyond_a_double_stays_exact_and_strips_to_an_infinity():
    big = Q_(1e308, "km")
    assert big == Q_(1e308, "km")
    assert big != Q_(1e308, "m")
    assert isinstance(hash(big), int)
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(big) == math.inf
    with pytest.warns(physure.UnitStrippedWarning):
        assert float(Q_(-1e308, "km")) == -math.inf
    with pytest.warns(physure.UnitStrippedWarning):
        assert np.asarray(big, dtype=float) == math.inf


# --- tolerance comparisons, sqrt and std ------------------------------------------


def test_np_isclose_and_allclose_compare_in_the_coherent_unit():
    a = Q_(np.array([1.0, 2.0]), "m")
    b = Q_(np.array([100.0, 200.0]), "cm")
    assert np.isclose(Q_(1, "m"), Q_(100, "cm"))
    assert np.isclose(a, b).tolist() == [True, True]
    assert np.allclose(a, b)
    assert np.allclose(a, [Q_(100.0, "cm"), Q_(2000, "mm")])
    assert not np.allclose(Q_(1, "m"), Q_(1.001, "m"))
    assert np.allclose(Q_(1, "m"), Q_(1.001, "m"), rtol=1e-2)
    assert np.allclose(Q_(1, "m"), Q_(1.001, "m"), atol=Q_(2, "mm"))
    assert np.isclose(Q_(20, "degC"), Q_(293.15, "K"))
    assert np.isclose(Q_(50, "percent"), 0.5)


def test_np_isclose_of_lists_takes_a_quantity_atol():
    left = [Q_(1.0, "m"), Q_(2.0, "m")]
    right = [Q_(100.0, "cm"), Q_(2000, "mm")]
    assert np.isclose(left, right, atol=Q_(0, "m")).tolist() == [True, True]
    np.testing.assert_allclose(Q_(1.0, "m"), Q_(100.0, "cm"), atol=Q_(0, "m"))


def test_np_isclose_refuses_what_has_no_common_unit():
    with pytest.raises(IncompatibleUnitsError):
        np.isclose(Q_(1, "m"), Q_(1, "s"))
    with pytest.raises(IncompatibleUnitsError):
        np.isclose(Q_(1, "m"), 1.0)
    with pytest.raises(IncompatibleUnitsError, match="interval"):
        np.isclose(Q_(20, "degC"), Q_(5, "delta_degC"))
    with pytest.raises(TypeError, match="atol"):
        np.isclose(Q_(1, "m"), Q_(1, "m"), atol=1e-3)
    with pytest.raises(IncompatibleUnitsError):
        np.isclose(Q_(1, "m"), Q_(1, "m"), atol=Q_(1, "s"))


def test_np_sqrt_of_a_list_takes_the_root_of_unit_and_uncertainty():
    roots = np.sqrt([Q_(4, "m^2", 0.4), Q_(9, "m^2")])
    assert roots[0].magnitude == 2.0
    assert roots[0].dimension == Q_(1, "m").dimension
    assert roots[0].uncertainty == pytest.approx(0.1)
    assert roots[1].uncertainty == 0.0
    assert Q_(1 + 2j, "m").conjugate().magnitude == 1 - 2j
    q = Q_(3, "m", 0.1)
    assert q.conjugate() is q


def test_np_std_of_a_list_is_first_order_in_the_errors():
    values = np.array([1.0, 2.0, 4.0])
    sigmas = np.array([0.1, 0.1, 0.2])
    spread = np.std(
        [Q_(v, "m", s) for v, s in zip(values, sigmas, strict=True)]
    )
    assert spread.magnitude == pytest.approx(np.std(values))
    assert spread.dimension == Q_(1, "m").dimension
    # d std / d x_i = (x_i - mean) / (n std)
    slopes = (values - values.mean()) / (3 * np.std(values))
    assert spread.uncertainty == pytest.approx(
        math.sqrt(np.sum((slopes * sigmas) ** 2)), rel=1e-12
    )
    assert np.std([Q_(100, "cm"), Q_(2, "m")]).to(
        "cm"
    ).magnitude == pytest.approx(50.0)


@pytest.mark.parametrize("unit", ["m", "degC", "delta_degC"])
def test_the_elements_of_np_asarray_are_the_elements_q_i(unit):
    q = Q_(
        np.array([[1.0, 2.0], [3.0, 4.0]]),
        unit,
        uncertainty=np.array([[0.1, 0.0], [0.2, 0.3]]),
    )
    elements = np.asarray(q)
    for index in np.ndindex(2, 2):
        element, reference = elements[index], q[index]
        assert element.magnitude == reference.magnitude
        assert element.unit == reference.unit
        assert element.unit.point == reference.unit.point
        assert element.uncertainty == reference.uncertainty
        assert type(element._uncertainty_obj) is type(
            reference._uncertainty_obj
        )
    # each element is its own measurement, as q[i] is
    assert (elements[0, 0] - elements[0, 0]).uncertainty == 0.0
    assert (elements[0, 0] - elements[1, 0]).uncertainty == pytest.approx(
        math.hypot(0.1, 0.2)
    )
    exact = np.asarray(Q_(np.arange(5.0), unit))
    assert [e.magnitude for e in exact] == [0.0, 1.0, 2.0, 3.0, 4.0]
    assert all(e.uncertainty == 0.0 for e in exact)
