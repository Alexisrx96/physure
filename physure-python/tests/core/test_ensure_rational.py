"""The conversion of an object with powers to the core's unit keeps every power."""

from fractions import Fraction

from physure._jit.tracer import _ensure_rational
from physure.domain.measurement.dimensions import Dimension


def test_a_fractional_power_is_not_truncated_to_zero():
    # a power of 1/2 became int(Fraction(1, 2)) == 0: the dimension vanished
    dimension = Dimension({"T": Fraction(1, 2)})
    assert _ensure_rational(dimension).dimensions == {"T": (1, 2)}


def test_integer_and_fractional_powers_mix():
    dimension = Dimension({"L": 2, "T": Fraction(-3, 2)})
    assert _ensure_rational(dimension).dimensions == {
        "L": (2, 1),
        "T": (-3, 2),
    }
