"""The `interval` flag of the catalog, as Python sees it: the core says which unit is an interval.

Spec `docs/superpowers/specs/2026-11-20-interval-flag-design.md`: a temperature interval is a unit the
catalog flags (`delta_celsius = 1.0, O, interval, [...]`), not a name that starts with `delta_`.
"""

import pytest

from physure.application.startup import create_default_system
from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.units import get_default_system

COMPARE = "cannot compare a temperature point"


@pytest.fixture
def q():
    return get_default_system().Q_


def test_the_core_says_which_unit_is_an_interval():
    registry = get_default_system().core_registry
    assert registry.facts("delta_degC")["interval"] is True
    assert registry.facts("kdelta_degC")["interval"] is True
    assert registry.facts("K")["interval"] is False
    assert registry.facts("degC")["interval"] is False


def test_a_kelvin_plus_an_interval_is_of_unknown_kind_in_either_order(q):
    """A plain K may be an absolute temperature: with an interval, the sum is too."""
    for total in (
        q(1, "K") + q(5, "delta_degC"),
        q(5, "delta_degC") + q(1, "K"),
    ):
        assert total < q(20, "degC")
        assert q(20, "degC") > total
        assert total.magnitude == pytest.approx(6.0)
        assert total.unit.interval is False
        # written in K: delta_degC would say interval
        assert str(total.unit) == "K"
    assert q(1, "K") + q(1, "K") < q(20, "degC")
    assert q(1, "K") + q(5, "delta_degC") < q(10, "delta_degC")
    # two known intervals make an interval
    with pytest.raises(IncompatibleUnitsError, match=COMPARE):
        _ = q(5, "delta_degC") + q(5, "delta_degC") < q(20, "degC")
    with pytest.raises(IncompatibleUnitsError, match=COMPARE):
        _ = (q(20, "degC") - q(10, "degC")) + q(5, "delta_degC") < q(
            20, "degC"
        )


def test_a_kelvin_that_is_an_absolute_temperature_moves_by_an_interval(q):
    moved = q(20, "degC").to("K") + q(5, "delta_degC")
    assert moved.to("degC").magnitude == pytest.approx(25.0)
    assert (q(300, "K") + q(5, "delta_degC")).to("degC").magnitude == (
        pytest.approx(31.85)
    )


def test_a_point_minus_a_point_is_an_interval_still_in_kelvin(q):
    difference = q(20, "degC") - q(10, "degC")
    assert difference.magnitude == pytest.approx(10.0)
    assert str(difference.unit) == "K"
    with pytest.raises(IncompatibleUnitsError, match=COMPARE):
        _ = difference < q(20, "degC")
    assert difference > q(5, "delta_degC")
    # a difference moves a point
    assert (q(20, "degC") + difference).to("degC").magnitude == pytest.approx(
        30.0
    )


def test_a_point_converted_to_an_interval_unit_raises(q):
    for value, point, interval in (
        (20, "degC", "delta_degC"),
        (68, "degF", "delta_degF"),
    ):
        with pytest.raises(
            IncompatibleUnitsError,
            match="cannot convert a temperature point",
        ):
            q(value, point).to(interval)
    with pytest.raises(
        IncompatibleUnitsError, match="cannot convert a temperature interval"
    ):
        q(5, "delta_degC").to("degC")
    assert q(10, "K").to("delta_degC").magnitude == pytest.approx(10.0)
    assert q(5, "delta_degC").to("K").magnitude == pytest.approx(5.0)
    assert q(9, "delta_degF").to("delta_degC").magnitude == pytest.approx(5.0)


def test_a_unit_of_a_system_flagged_interval_is_an_interval():
    system = create_default_system()
    system.extend_core(
        "[Units]\ndT = 1.0, O, interval, [dT]\ndU = 1.0, O, [dU]\n"
    )
    assert system.core_registry.facts("dT")["interval"] is True
    with pytest.raises(IncompatibleUnitsError, match=COMPARE):
        _ = system.Q_(5, "dT") < system.Q_(20, "degC")
    with pytest.raises(
        IncompatibleUnitsError, match="cannot convert a temperature point"
    ):
        system.Q_(20, "degC").to("dT")
    # without the flag a scale-1 temperature unit is a kelvin under another name
    assert system.Q_(5, "dU") < system.Q_(20, "degC")


def test_the_flag_on_a_point_is_refused():
    system = create_default_system()
    with pytest.raises(ValueError, match="interval"):
        system.extend_core("[Units]\ndC = 1.0, 273.15, O, interval, [dC]\n")


def test_a_unit_made_an_interval_equals_its_unit_and_survives_pickling(q):
    import pickle

    difference = q(20, "degC") - q(10, "degC")
    assert difference.unit.interval is True
    assert difference.unit == q(1, "K").unit
    restored = pickle.loads(pickle.dumps(difference.unit))
    assert restored.interval is True
    assert restored is difference.unit
    # what carries the unit over carries the flag: a negation, a plain factor
    for scaled in (-difference, difference * 2, 2 * difference):
        with pytest.raises(IncompatibleUnitsError, match=COMPARE):
            _ = scaled < q(20, "degC")
    # `=> K` reads it as a kelvin, as the core does
    assert difference.to("K") < q(20, "degC")


def test_a_refused_conversion_says_the_quantity_in_the_cores_words(q):
    with pytest.raises(IncompatibleUnitsError) as refused:
        (q(20, "degC") - q(10, "degC")).to("degC")
    assert str(refused.value).startswith(
        "cannot convert a temperature interval (`10.0 K`) to a temperature point"
    ), str(refused.value)
    with pytest.raises(IncompatibleUnitsError) as refused:
        q(20, "degC").to("delta_degC")
    assert str(refused.value).startswith(
        "cannot convert a temperature point (`20.0 degC`) to a temperature interval"
    ), str(refused.value)
