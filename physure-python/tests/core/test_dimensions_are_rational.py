"""A1 stage 2, step 5: a dimension keeps its powers exact, and the core does its algebra."""

from fractions import Fraction

import pytest

from physure import Q_
from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.dimensions import Dimension, get_dimension

LENGTH = Dimension({"L": 1})
TIME = Dimension({"T": 1})

# --- a fractional power is not truncated -------------------------------------


def test_the_dimension_of_a_square_root_keeps_its_half():
    assert (Q_(4.0, "s") ** 0.5).dimension == Dimension({"T": Fraction(1, 2)})
    assert Q_(1.0, "Hz**0.5").dimension == Dimension({"T": Fraction(-1, 2)})
    assert str(Dimension({"T": 0.5})) == "T^(1/2)"
    assert Dimension({"T": 0.5}).exponents == {"T": Fraction(1, 2)}


def test_a_spectral_density_is_not_dimensionless():
    # a noise density in V/sqrt(Hz): the old dimension lost the power of the time
    density = Q_(1.0, "V/Hz**0.5")
    assert density.dimension == Dimension(
        {"L": 2, "M": 1, "T": Fraction(-5, 2), "I": -1}
    )
    with pytest.raises(IncompatibleUnitsError):
        density + Q_(1.0, "V")
    with pytest.raises(IncompatibleUnitsError):
        Q_(1.0, "s**0.5") + Q_(1.0, "1")


def test_a_unit_with_a_fractional_power_parses_without_sympy():
    assert Q_(9.0, "m^(1/2)").dimension == Dimension({"L": Fraction(1, 2)})
    assert get_dimension("L^(1/2)") == Dimension({"L": Fraction(1, 2)})


def test_to_base_units_of_a_fractional_dimension():
    base = Q_(2.0, "V/Hz**0.5").to_base_units()
    assert base.magnitude == pytest.approx(2.0)
    assert base.dimension == Q_(1.0, "V/Hz**0.5").dimension


# --- the algebra of the core --------------------------------------------------


def test_dimensions_multiply_divide_and_take_powers():
    assert Dimension({"L": 2, "T": -2}) == (LENGTH / TIME) ** 2
    assert TIME**0.5 * TIME**0.5 == TIME
    assert (Dimension({"L": 2}) ** 0.5) == LENGTH
    assert TIME ** Fraction(-1, 3) * TIME ** Fraction(1, 3) == Dimension({})
    assert (LENGTH * TIME).exponents == {"L": 1, "T": 1}


def test_a_dimension_is_one_object_per_set_of_powers():
    assert Dimension({"L": 1, "T": -1}) is LENGTH / TIME
    assert hash(Dimension({"T": 0.5})) == hash(TIME**0.5)
    assert Dimension({}).is_dimensionless
    assert not TIME.is_dimensionless


def test_any_symbol_is_a_dimension_after_the_si_ones():
    custom = Dimension({"X": 2, "L": 1})
    assert list(custom.exponents) == ["L", "X"]
    assert str(custom) == "L·X²"
