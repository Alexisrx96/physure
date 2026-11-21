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
