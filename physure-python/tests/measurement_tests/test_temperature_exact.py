"""Temperature scales convert exactly at their fixed points.

The catalog writes a degree Fahrenheit as the fractions it is (5/9 K, zero at
45967/180 K) and a conversion is ``old * scale + shift`` with each number
rounded once, the same two numbers the Rust core computes.
"""

import numpy as np
import pytest

from physure import Q_
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.system import UnitSystem

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


def _plan(system, source, target):
    registry = system.core_registry
    return registry.conversion(
        registry.parse_atomic(source), registry.parse_atomic(target)
    )


def test_a_conversion_without_zero_points_has_no_shift():
    system = UnitSystem("scales")
    length = Dimension({"L": 1})
    system.register_unit("a", length, 1.0)
    system.register_unit("b", length, 0.5)
    assert _plan(system, "a", "b") == {
        "kind": "affine",
        "scale": 2.0,
        "shift": 0.0,
    }


def test_the_coefficients_are_rounded_once_from_the_exact_values():
    system = UnitSystem("temperatures")
    heat = Dimension({"O": 1})
    system.register_unit("degC", heat, 1, offset="273.15")
    system.register_unit("degF", heat, "5/9", offset="45967/180")
    assert _plan(system, "degC", "degF") == {
        "kind": "affine",
        "scale": 1.8,
        "shift": 32.0,
    }
    assert _plan(system, "degF", "degC") == {
        "kind": "affine",
        "scale": 5 / 9,
        "shift": -160 / 9,
    }


def test_a_float_is_the_decimal_it_prints_as():
    # 0.5555555555555556 is not 5/9: the unit is the decimal it was written as
    system = UnitSystem("rounded")
    heat = Dimension({"O": 1})
    system.register_unit("degC", heat, 1.0, offset=273.15)
    system.register_unit(
        "rounded", heat, 0.5555555555555556, offset=255.3722222222222
    )
    plan = _plan(system, "degC", "rounded")
    assert plan["scale"] == pytest.approx(1.0 / 0.5555555555555556, rel=1e-15)
    assert plan["shift"] == pytest.approx(
        (273.15 - 255.3722222222222) / 0.5555555555555556, rel=1e-12
    )
