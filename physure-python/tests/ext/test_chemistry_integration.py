"""Cross-module scientific-rigor checks: Hess's law and a full
mass-in -> balanced-reaction -> yield -> reaction-enthalpy pipeline,
verified against real published thermochemical data.
"""

import math

from physure import Q_
from physure.ext.chemistry import Reaction, standard_enthalpy


def _reaction_enthalpy_kj(rxn: Reaction) -> float:
    reactants_h = sum(
        coeff * standard_enthalpy(sp.formula).to("kJ/mol").magnitude
        for sp, coeff in zip(rxn.reactants, rxn.reactant_coeffs, strict=True)
    )
    products_h = sum(
        coeff * standard_enthalpy(sp.formula).to("kJ/mol").magnitude
        for sp, coeff in zip(rxn.products, rxn.product_coeffs, strict=True)
    )
    return products_h - reactants_h


def test_hess_law_methane_combustion_matches_published_value():
    # CH4 + 2 O2 -> CO2 + 2 H2O; real dH_combustion(CH4) ~ -890.3 kJ/mol.
    rxn = Reaction.from_string("CH4 + 2 O2 -> CO2 + 2 H2O")
    assert math.isclose(_reaction_enthalpy_kj(rxn), -890.3, rel_tol=1e-3)


def test_hess_law_haber_process_matches_published_value():
    # N2 + 3 H2 -> 2 NH3; real dH_rxn ~ -92.2 kJ/mol (2 * dH_f(NH3)).
    rxn = Reaction.from_string("N2 + 3 H2 -> 2 NH3")
    assert math.isclose(_reaction_enthalpy_kj(rxn), -92.2, rel_tol=1e-3)


def test_full_pipeline_mass_in_to_yield_and_reaction_enthalpy():
    # Burn 16 g CH4 with excess O2: combine the stoichiometric yield
    # calculation with Hess's-law reaction enthalpy so every layer
    # (species -> equivalency -> reaction -> thermo) agrees with real
    # combustion data in one consistent check.
    rxn = Reaction.from_string("CH4 + 2 O2 -> CO2 + 2 H2O")
    result = rxn.calculate(
        CH4=Q_(16.0, "g", uncertainty=0.1),
        O2=Q_(200.0, "g", uncertainty=1.0),
    )
    assert result.limiting_reactant == "CH4"
    co2_g = result.yields["CO2"].to("g").magnitude
    assert math.isclose(co2_g, 43.89, rel_tol=2e-3)
    assert math.isclose(_reaction_enthalpy_kj(rxn), -890.3, rel_tol=1e-3)
