import math

import pytest

from measurekit import Q_
from measurekit.ext.chemistry.reaction import Reaction


@pytest.mark.parametrize(
    ("equation", "reactant_coeffs", "product_coeffs"),
    [
        ("H2 + O2 -> H2O", [2, 1], [2]),
        ("Fe + O2 -> Fe2O3", [4, 3], [2]),  # iron rusting
        ("CH4 + O2 -> CO2 + H2O", [1, 2], [1, 2]),  # methane combustion
        ("C3H8 + O2 -> CO2 + H2O", [1, 5], [3, 4]),  # propane combustion
        ("C2H6 + O2 -> CO2 + H2O", [2, 7], [4, 6]),  # ethane combustion
        ("N2 + H2 -> NH3", [1, 3], [2]),  # Haber process
        ("CO2 + H2O -> C6H12O6 + O2", [6, 6], [1, 6]),  # photosynthesis
        ("NaOH + H2SO4 -> Na2SO4 + H2O", [2, 1], [1, 2]),  # neutralization
        ("Al + O2 -> Al2O3", [4, 3], [2]),  # aluminum oxidation
        ("AgNO3 + NaCl -> AgCl + NaNO3", [1, 1], [1, 1]),  # metathesis
    ],
)
def test_balance_matches_textbook_stoichiometry(
    equation, reactant_coeffs, product_coeffs
):
    rxn = Reaction.from_string(equation)
    assert rxn.reactant_coeffs == reactant_coeffs
    assert rxn.product_coeffs == product_coeffs


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


def test_calculate_methane_combustion_limiting_and_multi_product_yield():
    rxn = Reaction.from_string("CH4 + 2 O2 -> CO2 + 2 H2O")
    result = rxn.calculate(
        CH4=Q_(16.0, "g", uncertainty=0.1),
        O2=Q_(100.0, "g", uncertainty=0.5),
    )
    assert result.limiting_reactant == "CH4"
    co2_g = result.yields["CO2"].to("g").magnitude
    h2o_g = result.yields["H2O"].to("g").magnitude
    assert math.isclose(co2_g, 43.89, rel_tol=2e-3)
    assert math.isclose(h2o_g, 35.93, rel_tol=2e-3)


def test_calculate_haber_process_limiting_and_yield():
    rxn = Reaction.from_string("N2 + 3 H2 -> 2 NH3")
    result = rxn.calculate(
        N2=Q_(28.0, "g", uncertainty=0.2),
        H2=Q_(10.0, "g", uncertainty=0.1),
    )
    assert result.limiting_reactant == "N2"
    nh3_g = result.yields["NH3"].to("g").magnitude
    assert math.isclose(nh3_g, 34.04, rel_tol=2e-3)


def test_calculate_neutralization_multi_product():
    rxn = Reaction.from_string("2 NaOH + H2SO4 -> Na2SO4 + 2 H2O")
    result = rxn.calculate(
        NaOH=Q_(30.0, "g", uncertainty=0.1),
        H2SO4=Q_(49.0, "g", uncertainty=0.2),
    )
    assert result.limiting_reactant == "NaOH"
    na2so4_g = result.yields["Na2SO4"].to("g").magnitude
    h2o_g = result.yields["H2O"].to("g").magnitude
    assert math.isclose(na2so4_g, 53.27, rel_tol=2e-3)
    assert math.isclose(h2o_g, 13.51, rel_tol=2e-3)


def test_unbalanceable_reaction_raises():
    with pytest.raises(ValueError, match="positive integer coefficients"):
        Reaction.from_string("H2 + O2 -> H2O + CO2")


def test_non_unique_balance_raises():
    # H2 and He don't react -- both sides list the same two independent,
    # non-interacting species, so there's no unique stoichiometric ratio.
    with pytest.raises(
        ValueError, match="expected exactly one degree of freedom"
    ):
        Reaction.from_string("H2 + He -> H2 + He")


def test_malformed_equation_raises():
    with pytest.raises(ValueError, match="Invalid reaction equation"):
        Reaction.from_string("H2 + O2")


def test_missing_reactant_input_raises():
    rxn = Reaction.from_string("2 H2 + O2 -> 2 H2O")
    with pytest.raises(ValueError, match="Missing input for reactant 'O2'"):
        rxn.calculate(H2=Q_(10.0, "g"))


def test_malformed_term_raises():
    with pytest.raises(ValueError, match="Invalid formula"):
        Reaction.from_string("2xH2 + O2 -> H2O")


def test_empty_term_raises():
    with pytest.raises(ValueError, match="Invalid reaction term"):
        Reaction.from_string("-> H2O")
