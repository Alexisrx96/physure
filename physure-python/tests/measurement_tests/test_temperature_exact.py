"""Temperature scales convert exactly at their fixed points.

The catalog writes a degree Fahrenheit as the fractions it is (5/9 K, zero at
45967/180 K) and a conversion is ``old * scale + shift`` with each number
rounded once, the same two numbers the Rust core computes.
"""

from fractions import Fraction

import numpy as np
import pytest

from physure import Q_
from physure.domain.measurement.converters import (
    LinearConverter,
    OffsetConverter,
    affine_coefficients,
)

FIXED_POINTS = [
    (100.0, "degC", "degF", 212.0),
    (212.0, "degF", "degC", 100.0),
    (0.0, "degC", "degF", 32.0),
    (32.0, "degF", "degC", 0.0),
    (-40.0, "degC", "degF", -40.0),
    (-40.0, "degF", "degC", -40.0),
    (0.0, "degC", "K", 273.15),
    (273.15, "K", "degC", 0.0),
    (-459.67, "degF", "K", 0.0),
    (491.67, "degR", "degF", 32.0),
]


@pytest.mark.parametrize(
    ("value", "source", "target", "expected"), FIXED_POINTS
)
def test_fixed_points_convert_exactly(value, source, target, expected):
    assert Q_(value, source).to(target).magnitude == expected


def test_arrays_convert_exactly():
    celsius = Q_(np.array([-40.0, 0.0, 100.0]), "degC")
    assert celsius.to("degF").magnitude.tolist() == [-40.0, 32.0, 212.0]
    fahrenheit = Q_(np.array([-40.0, 32.0, 212.0]), "degF")
    assert fahrenheit.to("degC").magnitude.tolist() == [-40.0, 0.0, 100.0]


def test_uncertainty_scales_by_the_ratio_of_the_degrees():
    converted = Q_(100.0, "degC", uncertainty=1.0).to("degF")
    assert converted.magnitude == 212.0
    assert converted.uncertainty == 1.8


def test_a_conversion_without_zero_points_has_no_shift():
    assert affine_coefficients(LinearConverter(1.0), LinearConverter(0.5)) == (
        2.0,
        0.0,
    )


def test_the_coefficients_are_rounded_once_from_the_exact_values():
    degc = OffsetConverter(
        1.0, 273.15, exact_scale=Fraction(1), exact_offset=Fraction("273.15")
    )
    degf = OffsetConverter(
        5 / 9,
        45967 / 180,
        exact_scale=Fraction(5, 9),
        exact_offset=Fraction(45967, 180),
    )
    assert affine_coefficients(degc, degf) == (1.8, 32.0)
    assert affine_coefficients(degf, degc) == (5 / 9, -160 / 9)


def test_the_coefficients_fall_back_to_the_doubles_without_exact_values():
    celsius = OffsetConverter(1.0, 273.15)
    rounded_fahrenheit = OffsetConverter(0.5555555555555556, 255.3722222222222)
    scale, shift = affine_coefficients(celsius, rounded_fahrenheit)
    assert scale == 1.0 / 0.5555555555555556
    assert shift == (273.15 - 255.3722222222222) / 0.5555555555555556
