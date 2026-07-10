import math

import pytest

from measurekit import Q_
from measurekit.ext.chemistry.reaction import Reaction


def test_balance_water_formation():
    rxn = Reaction.from_string("H2 + O2 -> H2O")
    assert rxn.reactant_coeffs == [2, 1]
    assert rxn.product_coeffs == [2]


def test_balance_iron_oxidation():
    rxn = Reaction.from_string("Fe + O2 -> Fe2O3")
    assert rxn.reactant_coeffs == [4, 3]
    assert rxn.product_coeffs == [2]


def test_calculate_limiting_reactant_and_yield():
    rxn = Reaction.from_string("2 H2 + O2 -> 2 H2O")
    result = rxn.calculate(
        H2=Q_(10.0, "g", uncertainty=0.1),
        O2=Q_(50.0, "g", uncertainty=0.2),
    )
    assert result.limiting_reactant == "O2"
    water_g = result.yields["H2O"].to("g")
    assert math.isclose(water_g.magnitude, 56.31, rel_tol=1e-3)
    assert water_g.uncertainty > 0


def test_unbalanceable_reaction_raises():
    with pytest.raises(ValueError, match="positive integer coefficients"):
        Reaction.from_string("H2 + O2 -> H2O + CO2")


def test_malformed_equation_raises():
    with pytest.raises(ValueError, match="Invalid reaction equation"):
        Reaction.from_string("H2 + O2")
