"""`5 * unit` builds a quantity for the units of the core too (A1 stage 1d)."""

import numpy as np
import pytest

from physure import Q_
from physure._core import RationalUnit


def test_a_number_times_a_core_unit_is_a_quantity():
    quantity = 5 * RationalUnit({"m": 1})
    assert quantity == Q_(5.0, "m")


def test_the_sugar_works_on_the_result_of_unit_algebra():
    speed = RationalUnit({"m": 1}) / RationalUnit({"s": 1})
    assert 3 * speed == Q_(3.0, "m/s")


def test_an_array_is_taken_by_the_reflected_product():
    # numpy answers `array * unit` itself (an object array), so the array goes in directly
    quantity = RationalUnit({"m": 1}).__rmul__(np.array([1.0, 2.0]))
    assert quantity.to("km").magnitude.tolist() == pytest.approx(
        [0.001, 0.002]
    )


def test_other_operands_are_not_defined():
    with pytest.raises(TypeError):
        _ = "5" * RationalUnit({"m": 1})
    with pytest.raises(TypeError):
        _ = None * RationalUnit({"m": 1})
