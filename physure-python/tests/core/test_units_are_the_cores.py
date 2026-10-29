"""A1 stage 2, step 3: what a unit is, and how a number changes unit, is the core's answer."""

import pytest

from physure import Q_
from physure.application.context import get_current_system
from physure.application.startup import create_default_system
from physure.domain.exceptions import (
    IncompatibleUnitsError,
    UnknownUnitError,
)
from physure.domain.measurement.system import UnitSystem


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


def test_a_display_alias_is_a_unit_of_scale_one_in_the_core():
    system = create_default_system()
    system.register_alias({"m": 1, "s": -2}, "acceleration", "accel")
    speed = system.Q_(1234.567, "m/s**2")
    # the shortest of the names, as it has always been
    assert speed.unit.to_string(system, use_alias=True) == "accel"
    # the alias also resolves, as a unit
    assert system.Q_(2.0, "accel").to("m/s**2").magnitude == 2.0
    assert system.core_registry.facts("acceleration")["dimension"] == {
        "L": 1,
        "T": -2,
    }
    # and it belongs to this system alone
    assert not get_current_system().core_registry.contains("acceleration")


def test_an_alias_needs_the_units_of_its_exponents():
    system = UnitSystem("bare")
    with pytest.raises(UnknownUnitError):
        system.register_alias({"m": 1, "s": -1}, "velocity")
    assert "velocity" not in system
