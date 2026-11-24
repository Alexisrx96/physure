"""The third state of a temperature, as Python sees it: a K converted from a point is a point.

Spec `docs/superpowers/specs/2026-11-20-temperature-kind-design.md`: a unit without a zero point
is a known point (`20 degC => K`), a known interval (`delta_degC`, `T2 - T1`) or of unknown kind
(`300 K` written by hand); the core's arithmetic says which, and Python asks it.
"""

import pickle

import pytest

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.units import get_default_system

COMPARE = "cannot compare a temperature point"


@pytest.fixture
def q():
    return get_default_system().Q_


def test_a_converted_point_is_a_point_and_a_kelvin_by_hand_is_not(q):
    t = q(20, "degC").to("K")
    assert t.magnitude == pytest.approx(293.15)
    assert str(t.unit) == "K"
    assert t.unit.point is True
    assert t.unit.interval is False
    assert q(300, "K").unit.point is False
    # a point stays one in another unit without a zero point
    assert q(20, "degC").to("K").to("degR").unit.point is True
    assert q(68, "degF").to("degR").unit.point is True
    # the kind is not part of the identity of the unit
    assert t.unit == q(1, "K").unit
    assert t == q(293.15, "K")


def test_a_point_is_not_compared_with_an_interval_nor_converted_to_one(q):
    t = q(20, "degC").to("K")
    for left, right in (
        (t, q(5, "delta_degC")),
        (q(5, "delta_degC"), t),
        (t + q(5, "delta_degC"), q(5, "delta_degC")),
        (q(5, "delta_degC") + t, q(5, "delta_degC")),
        (t - q(5, "delta_degC"), q(5, "delta_degC")),
        (q(30, "degC").to("K") - t, q(20, "degC")),
        (q(30, "degC").to("K") - t, t),
        (q(68, "degF").to("degR"), q(5, "delta_degF")),
    ):
        with pytest.raises(IncompatibleUnitsError, match=COMPARE):
            _ = left < right
        assert (left == right) is False
    with pytest.raises(
        IncompatibleUnitsError,
        match=r"cannot convert a temperature point \(`293.15 K`\)",
    ):
        t.to("delta_degC")


def test_heat_gas_and_average_programs_keep_working(q):
    t1, t2 = q(20, "degC").to("K"), q(30, "degC").to("K")
    assert (q(293.15, "K") + q(5, "delta_degC")).to("degC").magnitude == (
        pytest.approx(25.0)
    )
    assert t1 - q(5, "delta_degC") > q(10, "degC")
    assert q(5, "delta_degC") + t1 > q(10, "degC")
    heat = q(2, "kg") * q(4186, "J/(kg*K)") * (t2 - t1)
    assert heat.to("J").magnitude == pytest.approx(83720.0)
    assert ((t2 - t1) / q(1, "s")).magnitude == pytest.approx(10.0)
    assert t2 - t1 > q(5, "K")
    assert t2 - t1 > q(5, "delta_degC")
    average = (t1 + t2) / 2
    assert average.magnitude == pytest.approx(298.15)
    assert average > q(10, "degC")
    assert average.to("degC").magnitude == pytest.approx(25.0)
    r = q(8.314, "J/(mol*K)")
    assert (r * t1).magnitude == pytest.approx((r * q(20, "degC")).magnitude)
    # a scaled point is of unknown kind: no refusal
    for scaled in (t1 * 2, 2 * t1, t1 / 2, -t1):
        assert scaled.unit.point is False
        _ = scaled < q(5, "delta_degC")
    assert q(300, "K") > q(5, "delta_degC")
    assert q(300, "K").to("delta_degC").magnitude == pytest.approx(300.0)
    assert (t1 - q(300, "K")) < q(5, "delta_degC")
    assert t1 + t1 > q(10, "degC")


def test_a_point_moved_by_an_interval_is_written_in_the_points_unit(q):
    t = q(20, "degC").to("K")
    for moved in (t + q(5, "delta_degC"), q(5, "delta_degC") + t):
        assert str(moved.unit) == "K"
        assert moved.unit.point is True
        assert moved.magnitude == pytest.approx(298.15)


def test_a_unit_made_a_point_survives_pickling_and_the_cache(q):
    t = q(20, "degC").to("K")
    restored = pickle.loads(pickle.dumps(t.unit))
    assert restored.point is True
    assert restored is t.unit
    # the plain kelvin of the cache is not answered by the point, nor the point by it
    assert q(1, "K").unit.point is False
    assert q(20, "degC").to("K").unit.point is True
    # nor through the core's cache of units, which equality alone would merge
    registry = get_default_system().core_registry
    assert registry.expand(t.unit).point is True
    assert registry.expand(q(1, "K").unit).point is False
    assert registry.expand(t.unit).point is True


def _u(value, unit):
    return get_default_system().Q_(value, unit, uncertainty=0.1)


@pytest.mark.parametrize(
    ("make", "mean", "kind"),
    [
        (lambda: _u(20, "degC").to("K") + _u(1, "K"), 294.15, "unknown"),
        (lambda: _u(1, "K") + _u(20, "degC").to("K"), 294.15, "unknown"),
        (lambda: _u(20, "degC").to("K") - _u(1, "K"), 292.15, "unknown"),
        (
            lambda: _u(20, "degC").to("K") + _u(5, "delta_degC"),
            298.15,
            "point",
        ),
        (
            lambda: _u(5, "delta_degC") + _u(20, "degC").to("K"),
            298.15,
            "point",
        ),
        (
            lambda: _u(20, "degC").to("K") - _u(10, "degC").to("K"),
            10.0,
            "interval",
        ),
        (
            lambda: (_u(20, "degC") - _u(10, "degC")) + _u(1, "delta_degC"),
            11.0,
            "interval",
        ),
        (
            lambda: (_u(20, "degC") - _u(10, "degC")) + _u(1, "K"),
            11.0,
            "unknown",
        ),
        (
            lambda: (_u(20, "degC") - _u(10, "degC")) * 2 + _u(1, "K"),
            21.0,
            "unknown",
        ),
        (lambda: _u(20, "degC").to("K") + _u(1, "mK"), 293.151, "unknown"),
    ],
)
def test_an_uncertain_sum_of_temperatures_keeps_its_value_and_kind(
    make, mean, kind
):
    """Uncertain operands take the same-unit path of a sum (it recursed for a marked unit)."""
    result = make()
    assert result.magnitude == pytest.approx(mean)
    assert result.uncertainty > 0
    assert (result.unit.point, result.unit.interval) == (
        kind == "point",
        kind == "interval",
    )


def test_an_exact_point_with_an_uncertain_kelvin_and_back(q):
    for result in (
        q(20, "degC").to("K") + _u(1, "K"),
        _u(20, "degC").to("K") + q(1, "K"),
    ):
        assert result.magnitude == pytest.approx(294.15)
        assert result.uncertainty == pytest.approx(0.1)
    total = _u(20, "degC").to("mK") + _u(30, "degC").to("K")
    assert str(total.unit) == "mK"
    assert total.magnitude == pytest.approx(596300.0)


def test_array_sums_of_temperatures_keep_their_values_and_kinds(q):
    import numpy as np

    points = q(np.array([20.0, 30.0]), "degC").to("K")
    moved = points + q(1, "K")
    np.testing.assert_allclose(moved.magnitude, [294.15, 304.15])
    assert moved.unit.point is False
    difference = q(np.array([20.0, 30.0]), "degC") - q(
        np.array([10.0, 10.0]), "degC"
    )
    with_k = difference + q(1, "K")
    np.testing.assert_allclose(with_k.magnitude, [11.0, 21.0])
    assert with_k.unit.interval is False
    with_d = difference + q(1, "delta_degC")
    np.testing.assert_allclose(with_d.magnitude, [11.0, 21.0])
    assert with_d.unit.interval is True


def test_a_point_with_a_zero_point_and_a_marked_kelvin_are_two_points(q):
    """As the core: `20 degC - (10 degC => K)` was -263.15 degC, it is an interval of 10 K."""
    ten = q(10, "degC").to("K")
    for difference, mean in (
        (q(20, "degC") - ten, 10.0),
        (q(25, "degC") - q(0, "degC").to("K"), 25.0),
        (ten - q(20, "degC"), -10.0),
        (q(25, "degC").to("K") - q(0, "degC"), 25.0),
        (_u(20, "degC") - _u(10, "degC").to("K"), 10.0),
    ):
        assert difference.magnitude == pytest.approx(mean)
        assert str(difference.unit) == "K"
        assert difference.unit.interval is True
        with pytest.raises(IncompatibleUnitsError, match=COMPARE):
            _ = difference < q(20, "degC")
    for left, right in ((q(20, "degC"), ten), (ten, q(20, "degC"))):
        with pytest.raises(ValueError, match="Cannot add two absolute"):
            _ = left + right
    with pytest.raises(ValueError, match="K of unknown kind"):
        _ = q(10, "K") - q(20, "degC")
    resistance = q(100, "ohm") * (
        1 + q(0.00385, "1/K") * (q(25, "degC").to("K") - q(0, "degC"))
    )
    assert resistance.to("ohm").magnitude == pytest.approx(109.625)


def test_the_functional_sum_reads_a_marked_kelvin_as_a_point(q):
    from physure.core.functional import add_quantities, sub_quantities

    system = get_default_system()
    celsius = q(1, "degC").unit
    point = q(10, "degC").to("K").unit
    value, unit = sub_quantities(20.0, celsius, 283.15, point, system)
    assert (value, str(unit)) == (pytest.approx(10.0), "K")
    with pytest.raises(ValueError, match="Cannot add two absolute"):
        add_quantities(20.0, celsius, 283.15, point, system)
    value, unit = sub_quantities(283.15, point, 20.0, celsius, system)
    assert (value, str(unit)) == (pytest.approx(-10.0), "K")
    value, unit = sub_quantities(20.0, celsius, 283.15, q(1, "K").unit, system)
    assert (value, str(unit)) == (pytest.approx(-263.15), "degC")


def test_an_interval_converted_to_another_unit_is_still_an_interval(q):
    """`(5 delta_degC => K) < 20 degC` has no answer: a kelvin difference is not where a temperature is."""
    difference = q(30, "degC").to("K") - q(20, "degC").to("K")
    for converted in (
        q(5, "delta_degC").to("K"),
        q(5, "delta_degC").to("mK"),
        q(5, "delta_degC").to("delta_degF"),
        difference.to("K"),
        difference.to("mK"),
        (q(20, "degC") - q(10, "degC")).to("K"),
    ):
        assert converted.unit.interval is True
        with pytest.raises(IncompatibleUnitsError, match=COMPARE):
            _ = converted < q(20, "degC")
    in_k = q(5, "delta_degC").to("K")
    assert in_k.magnitude == pytest.approx(5.0)
    assert in_k == q(5, "K")
    assert (in_k + q(1, "K")) < q(20, "degC")
    heat = q(2, "kg") * q(4186, "J/(kg*K)") * difference.to("K")
    assert heat.to("J").magnitude == pytest.approx(83720.0)
    assert q(10, "K").to("mK").unit.interval is False
