"""Tests for the functional core (physure.core.functional)."""

import math

import numpy as np
import pytest

from physure import Q_
from physure.core.functional import (
    add_quantities,
    get_xp,
    mul_quantities,
    pow_quantities,
    sub_quantities,
    truediv_quantities,
)
from physure.domain.exceptions import IncompatibleUnitsError


@pytest.fixture
def system():
    return Q_(1, "m").system


def unit(_system, name):
    return Q_(1, name).unit


def test_get_xp_arrays_and_scalars():
    xp = get_xp(np.array([1.0]), np.array([2.0]))
    assert xp.add(1, 2) == 3
    # Pure Python scalars fall back to numpy
    xp = get_xp(1.0, 2.0)
    assert xp.multiply(2, 3) == 6


def test_add_same_unit(system):
    mag, u = add_quantities(
        1.0, unit(system, "m"), 2.0, unit(system, "m"), system
    )
    assert math.isclose(mag, 3.0)
    assert u == unit(system, "m")


def test_add_converts_second_operand(system):
    mag, u = add_quantities(
        10.0, unit(system, "m"), 1.0, unit(system, "km"), system
    )
    assert math.isclose(mag, 1010.0)
    assert u == unit(system, "m")


def test_add_incompatible_raises(system):
    with pytest.raises(IncompatibleUnitsError):
        add_quantities(1.0, unit(system, "m"), 1.0, unit(system, "s"), system)


def test_sub_with_conversion(system):
    mag, u = sub_quantities(
        1.0, unit(system, "km"), 200.0, unit(system, "m"), system
    )
    assert math.isclose(mag, 0.8)
    assert u == unit(system, "km")


def test_sub_incompatible_raises(system):
    with pytest.raises(IncompatibleUnitsError):
        sub_quantities(1.0, unit(system, "m"), 1.0, unit(system, "s"), system)


# --- Affine (offset) arithmetic ------------------------------------------------


def test_add_two_temperatures_is_an_error(system):
    degc = unit(system, "degC")
    with pytest.raises(ValueError, match="affine"):
        add_quantities(20.0, degc, 30.0, degc, system)


def test_add_delta_to_temperature(system):
    # 5 K + 20 degC = 25 degC
    mag, u = add_quantities(
        5.0, unit(system, "K"), 20.0, unit(system, "degC"), system
    )
    assert math.isclose(mag, 25.0)
    assert u == unit(system, "degC")


def test_add_temperature_plus_delta(system):
    mag, u = add_quantities(
        20.0, unit(system, "degC"), 5.0, unit(system, "K"), system
    )
    assert math.isclose(mag, 25.0)
    assert u == unit(system, "degC")


def test_sub_two_temperatures_gives_linear_delta(system):
    # 25 degC - 20 degC = 5 (in a linear unit of the same dimension)
    mag, u = sub_quantities(
        25.0, unit(system, "degC"), 20.0, unit(system, "degC"), system
    )
    assert math.isclose(mag, 5.0)
    assert u.dimension(system) == unit(system, "K").dimension(system)


def test_sub_delta_from_temperature(system):
    # 25 degC - 5 K = 20 degC
    mag, u = sub_quantities(
        25.0, unit(system, "degC"), 5.0, unit(system, "K"), system
    )
    assert math.isclose(mag, 20.0)
    assert u == unit(system, "degC")


def test_sub_temperature_from_delta(system):
    # 300 K - 20 degC: subtract in base, express in the offset unit
    mag, u = sub_quantities(
        300.0, unit(system, "K"), 20.0, unit(system, "degC"), system
    )
    assert math.isclose(mag, 300.0 - 293.15 - 273.15)
    assert u == unit(system, "degC")


# --- Logarithmic arithmetic -----------------------------------------------------


def test_add_log_quantities_power_sum(system):
    # pH is a -log10 unit: adding concentrations, result back in pH
    ph = unit(system, "pH")
    mag, u = add_quantities(7.0, ph, 7.0, ph, system)
    # Two equal concentrations double: pH drops by log10(2)
    assert math.isclose(mag, 7.0 - math.log10(2), rel_tol=1e-9)
    assert u == ph


def test_sub_log_quantities(system):
    ph = unit(system, "pH")
    big, small = 6.0, 7.0  # pH 6 is 10x the concentration of pH 7
    mag, u = sub_quantities(big, ph, small, ph, system)
    expected = -math.log10(10**-6.0 - 10**-7.0)
    assert math.isclose(mag, expected, rel_tol=1e-9)
    assert u == ph


# --- mul / div / pow --------------------------------------------------------------


def test_mul_combines_exponents(system):
    mag, u = mul_quantities(
        3.0, unit(system, "m"), 4.0, unit(system, "s"), system
    )
    assert math.isclose(mag, 12.0)
    assert u == Q_(1, "m*s").unit


def test_mul_cancels_exponents(system):
    _, u = mul_quantities(
        1.0, unit(system, "m"), 1.0, Q_(1, "m^-1").unit, system
    )
    assert u.dimension(system).is_dimensionless


def test_truediv_combines_exponents(system):
    mag, u = truediv_quantities(
        6.0, unit(system, "m"), 2.0, unit(system, "s"), system
    )
    assert math.isclose(mag, 3.0)
    assert u == Q_(1, "m/s").unit


def test_pow_scales_exponents(system):
    mag, u = pow_quantities(3.0, unit(system, "m"), 2, system)
    assert math.isclose(mag, 9.0)
    assert u == Q_(1, "m^2").unit


class _DynamicExponent:
    """Stand-in for a traced (non-floatable) exponent."""

    def __rpow__(self, base):
        return base**2.0

    def __float__(self):
        raise TypeError("dynamic exponent cannot be concretized")


def test_pow_dynamic_exponent_dimensionless_ok(system):
    dimensionless = Q_(1, "").unit
    mag, u = pow_quantities(3.0, dimensionless, _DynamicExponent(), system)
    assert math.isclose(float(mag), 9.0)
    assert u.dimension(system).is_dimensionless


def test_pow_dynamic_exponent_with_dimension_raises(system):
    with pytest.raises(TypeError, match="concretized"):
        pow_quantities(3.0, unit(system, "m"), _DynamicExponent(), system)
