"""A1 stage 2, step 3: what a unit is, and how a number changes unit, is the core's answer."""

import pytest

from physure import Q_
from physure.domain.exceptions import IncompatibleUnitsError


def test_two_temperature_scales_convert_through_their_zero_points():
    assert Q_(100.0, "degC").to("degF").magnitude == 212.0
    assert Q_(25.0, "degC").to("K").magnitude == pytest.approx(298.15)


def test_a_level_converts_through_the_ratio_with_its_uncertainty():
    ratio = Q_(30.0, "dB", uncertainty=1.0).to("1")
    assert ratio.magnitude == pytest.approx(1000.0)
    assert ratio.uncertainty == pytest.approx(1000.0 * 2.302585092994046 / 10)
    assert Q_(1.0, "decibel").to("dB").magnitude == 1.0


def test_an_angle_and_a_solid_angle_are_different_things():
    assert Q_(180.0, "deg").to("rad").magnitude == pytest.approx(
        3.141592653589793
    )
    with pytest.raises(IncompatibleUnitsError):
        Q_(1.0, "arcmin").to("sr")
    with pytest.raises(IncompatibleUnitsError):
        Q_(1.0, "rad").to("sr")
