"""A1 stage 2, step 4: points, intervals and levels combine as the core says."""

import math

import numpy as np
import pytest

from physure import Q_
from physure.application.context import get_current_system
from physure.core.functional import add_quantities, sub_quantities
from physure.domain.exceptions import IncompatibleUnitsError

# --- levels add as the quantities they stand for -----------------------------


def test_levels_add_as_the_quantities_they_stand_for():
    total = Q_(10.0, "dB") + Q_(10.0, "dB")
    assert total.magnitude == pytest.approx(10 * math.log10(20))
    assert total.unit == Q_(1.0, "dB").unit
    difference = Q_(13.0, "dB") - Q_(10.0, "dB")
    assert difference.magnitude == pytest.approx(
        10 * math.log10(10**1.3 - 10.0)
    )


def test_a_level_under_another_name_adds_too():
    assert (Q_(10.0, "dB") + Q_(10.0, "decibel")).magnitude == pytest.approx(
        10 * math.log10(20)
    )


def test_the_uncertainty_of_a_sum_of_levels_goes_through_the_ratio():
    total = Q_(10.0, "dB", uncertainty=0.5) + Q_(10.0, "dB", uncertainty=0.5)
    # P = 10 each, u(P) = P * ln(10) / 10 * 0.5; the sum is 20 and d(dB)/dP = 10 / (P ln 10)
    u_power = 10.0 * math.log(10) / 10 * 0.5
    expected = math.sqrt(2) * u_power * 10 / (20 * math.log(10))
    assert total.uncertainty == pytest.approx(expected)


def test_a_level_does_not_add_to_a_plain_quantity():
    with pytest.raises(ValueError, match="level and a plain quantity"):
        Q_(10.0, "dB") + Q_(1.0, "1")
    with pytest.raises(ValueError, match="level and a plain quantity"):
        Q_(1.0, "1") - Q_(10.0, "dB")


def test_levels_of_an_array_add_elementwise():
    total = Q_(np.array([10.0, 20.0]), "dB") + Q_(np.array([10.0, 20.0]), "dB")
    assert total.magnitude == pytest.approx(
        10 * np.log10(2 * 10 ** (np.array([10.0, 20.0]) / 10))
    )


# --- a point moved by an interval, two points make an interval ----------------


def test_an_interval_moves_a_point_in_the_scale_of_the_point():
    assert (Q_(20.0, "degC") + Q_(5.0, "K")).magnitude == 25.0
    assert (Q_(5.0, "K") + Q_(20.0, "degC")).magnitude == 25.0
    assert (Q_(20.0, "degC") - Q_(9.0, "delta_fahrenheit")).magnitude == 15.0
    assert (Q_(20.0, "degC") + Q_(5.0, "K")).unit == Q_(1.0, "degC").unit


def test_two_points_make_an_interval_in_kelvin():
    interval = Q_(25.0, "degC") - Q_(20.0, "degC")
    assert interval.unit == Q_(1.0, "K").unit
    assert interval.magnitude == pytest.approx(5.0)
    assert (Q_(77.0, "degF") - Q_(20.0, "degC")).magnitude == pytest.approx(
        5.0
    )


def test_what_has_no_meaning_is_refused():
    with pytest.raises(ValueError, match="two absolute quantities"):
        Q_(20.0, "degC") + Q_(5.0, "degC")
    with pytest.raises(
        ValueError, match="absolute quantity from a difference"
    ):
        Q_(5.0, "K") - Q_(20.0, "degC")
    with pytest.raises(IncompatibleUnitsError):
        Q_(20.0, "degC") + Q_(5.0, "m")


def test_the_uncertainty_of_a_point_and_an_interval_adds_up():
    # the arithmetic used to drop it
    total = Q_(20.0, "degC", uncertainty=1.0) + Q_(5.0, "K", uncertainty=0.5)
    assert total.magnitude == 25.0
    assert total.uncertainty == pytest.approx(math.hypot(1.0, 0.5))
    interval = Q_(25.0, "degC", uncertainty=0.3) - Q_(
        20.0, "degC", uncertainty=0.4
    )
    assert interval.uncertainty == pytest.approx(0.5)


def test_an_interval_in_another_scale_is_scaled_with_its_uncertainty():
    total = Q_(20.0, "degC") + Q_(9.0, "delta_fahrenheit", uncertainty=1.8)
    assert total.magnitude == pytest.approx(25.0)
    assert total.uncertainty == pytest.approx(1.0)


def test_points_of_an_array_move_elementwise():
    moved = Q_(np.array([20.0, 30.0]), "degC") + Q_(np.array([5.0, 10.0]), "K")
    assert moved.magnitude.tolist() == [25.0, 40.0]


# --- a product takes the temperature as the absolute one it is ----------------


def test_a_plain_number_does_not_scale_a_temperature():
    for operation in (
        lambda: Q_(20.0, "degC") * 2,
        lambda: 2 * Q_(20.0, "degC"),
        lambda: Q_(20.0, "degC") / 2,
        lambda: 2 / Q_(20.0, "degC"),
        lambda: Q_(np.array([20.0, 30.0]), "degC") * 2,
    ):
        with pytest.raises(ValueError, match="absolute temperature"):
            operation()


def test_a_pure_sign_mirrors_the_number_in_its_own_scale():
    assert (-Q_(25.0, "degC")).magnitude == -25.0
    assert (Q_(25.0, "degC") * -1).magnitude == -25.0
    assert (-1 * Q_(25.0, "degC")).unit == Q_(1.0, "degC").unit


def test_a_product_with_a_dimensioned_quantity_uses_the_absolute_temperature():
    energy = Q_(8.314, "J/(mol*K)") * Q_(25.0, "degC")
    assert energy.to("J/mol").magnitude == pytest.approx(8.314 * 298.15)
    ratio = Q_(100.0, "degC") / Q_(0.0, "degC")
    assert ratio.magnitude == pytest.approx(373.15 / 273.15)


def test_a_power_of_a_temperature_is_of_the_absolute_one():
    assert (Q_(25.0, "degC") ** 2).to("K**2").magnitude == pytest.approx(
        298.15**2
    )


# --- the functional module follows the same rules -----------------------------


def test_the_functional_module_moves_a_point_with_the_core_s_plan():
    system = get_current_system()
    unit = system.get_unit
    magnitude, result = add_quantities(
        1.0, unit("centicelsius"), 1.0, unit("K"), system
    )
    assert magnitude == pytest.approx(101.0)
    assert result == unit("centicelsius")
    magnitude, result = sub_quantities(
        25.0, unit("degC"), 20.0, unit("degC"), system
    )
    assert magnitude == pytest.approx(5.0)
    assert result == unit("K")


def test_the_functional_module_adds_levels_through_the_ratio():
    system = get_current_system()
    magnitude, _ = add_quantities(
        10.0, system.get_unit("dB"), 10.0, system.get_unit("dB"), system
    )
    assert magnitude == pytest.approx(10 * math.log10(20))
    with pytest.raises(ValueError, match="level and a plain quantity"):
        add_quantities(
            10.0, system.get_unit("dB"), 1.0, system.get_unit("1"), system
        )
