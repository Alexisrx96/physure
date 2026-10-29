"""A1 stage 2, step 6e: a unit keeps its powers once, and exactly.

A unit held its powers twice, as a dictionary of floats and as the rationals of
the core, and the float one drifted: ``(m ** 0.1) ** 3`` was ``m^0.30000000000000004``
and reached the core as 1125899906842624/3752999689475413 instead of 3/10.
"""

import pickle
from fractions import Fraction

import pytest

from physure import Q_
from physure.domain.measurement.units import CompoundUnit

METRE = CompoundUnit({"m": 1})


def test_a_power_that_went_through_float_arithmetic_keeps_no_error():
    unit = (METRE**0.1) ** 3
    assert unit.exponents == {"m": Fraction(3, 10)}
    assert unit.dimensions == {"m": (3, 10)}
    assert unit is CompoundUnit({"m": Fraction(3, 10)})


def test_the_same_power_written_four_ways_is_one_unit():
    ways = [0.5, (1, 2), [1, 2], Fraction(1, 2)]
    units = [CompoundUnit({"m": way}) for way in ways]
    assert all(unit is units[0] for unit in units)
    assert hash(units[0]) == hash(units[1])


def test_a_power_is_an_int_or_a_fraction():
    unit = CompoundUnit({"m": 2.0, "s": -0.5, "kg": (1, 3), "K": 0})
    assert unit.exponents == {
        "kg": Fraction(1, 3),
        "m": 2,
        "s": Fraction(-1, 2),
    }
    kinds = {symbol: type(power) for symbol, power in unit.exponents.items()}
    assert kinds == {"kg": Fraction, "m": int, "s": Fraction}


def test_fractional_powers_add_up_exactly():
    third = METRE ** (1 / 3)
    assert third * third * third == METRE
    assert METRE**0.1 * METRE**0.2 == METRE**0.3
    assert CompoundUnit({}) == METRE**0.5 / METRE**0.5
    assert (METRE**2) ** 0.5 == METRE
    assert CompoundUnit({}) == METRE**0.0


def test_the_core_gets_the_exact_power():
    assert (METRE ** (1 / 3)).dimensions == {"m": (1, 3)}
    assert (METRE**-0.75).dimensions == {"m": (-3, 4)}


def test_a_quantity_keeps_the_exact_power_of_its_unit():
    quantity = (Q_(4.0, "m") ** 0.1) ** 3
    assert quantity.unit.exponents == {"m": Fraction(3, 10)}


def test_a_unit_with_a_fractional_power_pickles():
    unit = CompoundUnit({"m": Fraction(1, 3), "s": -2})
    clone = pickle.loads(pickle.dumps(unit))
    assert clone.exponents == unit.exponents
    assert clone.dimensions == unit.dimensions


def test_a_fractional_power_is_shown_as_the_decimal_it_always_was():
    assert str(METRE**0.5) == "m⁰⋅⁵"
    assert str(METRE**-0.5) == "1/m⁰⋅⁵"
    assert str(METRE**2 / CompoundUnit({"s": 1})) == "m²/s"


def test_a_fractional_power_reaches_latex_exactly():
    assert (METRE**0.5).to_latex() == r"\sqrt{m}"
    assert (METRE ** (1 / 3)).to_latex() == r"\sqrt[3]{m}"


def test_a_power_that_is_not_a_number_is_refused():
    with pytest.raises((TypeError, ValueError)):
        CompoundUnit({"m": "square"})
    with pytest.raises((TypeError, ValueError)):
        CompoundUnit({"m": None})
