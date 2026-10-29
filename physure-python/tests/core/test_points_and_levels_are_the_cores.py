"""A1 stage 2, step 4: points, intervals and levels combine as the core says."""

import math

import numpy as np
import pytest

from physure import Q_

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
