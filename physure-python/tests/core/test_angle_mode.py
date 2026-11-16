"""A conversion reads the radian and the steradian as 1; a sum or a comparison never does."""

import pytest

import physure
from physure import Q_


def _speed():
    return Q_(10, "rad/s") * Q_(0.5, "m")


def test_the_default_mode_is_the_si_one():
    assert physure.get_angle_mode() == "dimensionless"


def test_a_conversion_reads_the_radian_as_one():
    assert _speed().to("m/s").magnitude == pytest.approx(5.0)


def test_the_energy_of_a_rotating_disk_converts_to_joules():
    mass, radius, omega = Q_(4, "kg"), Q_(0.5, "m"), Q_(10, "rad/s")
    inertia = 0.5 * mass * radius**2
    energy = 0.5 * inertia * omega**2
    assert energy.to("J").magnitude == pytest.approx(25.0)


def test_a_rate_of_angle_converts_to_per_second_but_not_to_hertz():
    assert Q_(10, "rad/s").to("s^-1").magnitude == pytest.approx(10.0)
    with pytest.raises(physure.PhysureError):
        Q_(10, "rad/s").to("Hz")


def test_the_strict_mode_is_the_old_behaviour():
    with physure.angle_mode("dimension"):
        assert physure.get_angle_mode() == "dimension"
        with pytest.raises(physure.PhysureError):
            _speed().to("m/s")
    assert physure.get_angle_mode() == "dimensionless"


def test_scopes_nest_and_restore_after_an_exception():
    with physure.angle_mode("dimension"):
        with physure.angle_mode("dimensionless"):
            assert physure.get_angle_mode() == "dimensionless"
        assert physure.get_angle_mode() == "dimension"
    with pytest.raises(RuntimeError), physure.angle_mode("dimension"):
        raise RuntimeError
    assert physure.get_angle_mode() == "dimensionless"


def test_an_unknown_mode_is_refused():
    with pytest.raises(ValueError, match="dimensionless, dimension"):
        physure.angle_mode("degrees")


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ("rad", "unity"),
        ("deg", "percent"),
        ("m*rad", "m"),
        ("kg*m^2*rad^2/s^2", "J"),
        ("rad", "sr"),
    ],
)
def test_a_sum_never_reads_a_marker_as_one(left, right):
    for mode in ("dimensionless", "dimension"):
        with physure.angle_mode(mode), pytest.raises(physure.PhysureError):
            Q_(2, left) + Q_(1, right)


def test_ordering_against_a_bare_number_is_not_defined_for_an_angle():
    with pytest.raises(TypeError):
        Q_(1, "rad") < 1  # noqa: B015
