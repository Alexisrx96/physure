import math

import pytest

from physure import Q_
from physure.domain.measurement.equivalencies import equivalencies
from physure.ext.chemistry.equivalency import (
    mass_to_moles,
    molar_equivalency,
    moles_to_mass,
)
from physure.ext.chemistry.species import Species


def test_raw_equivalency_round_trip():
    water = Species("H2O")
    mass = Q_(18.015, "g")
    with equivalencies(molar_equivalency(water)):
        moles = mass.to("mol")
    assert math.isclose(moles.magnitude, 1.0, rel_tol=1e-3)
    with equivalencies(molar_equivalency(water)):
        back = moles.to("g")
    assert math.isclose(back.magnitude, mass.magnitude, rel_tol=1e-6)


@pytest.mark.parametrize(
    ("formula", "mass_g", "expected_mol"),
    [
        ("H2O", 18.015, 1.0),
        ("CO2", 44.009, 1.0),
        ("NaCl", 5.844, 0.1),  # a common lab-bench table-salt weighing
    ],
)
def test_raw_equivalency_matches_known_ratio(formula, mass_g, expected_mol):
    species = Species(formula)
    mass = Q_(mass_g, "g")
    with equivalencies(molar_equivalency(species)):
        moles = mass.to("mol")
    assert math.isclose(moles.magnitude, expected_mol, rel_tol=2e-3)


def test_mass_to_moles_folds_molar_mass_uncertainty():
    water = Species("H2O")
    mass_q = Q_(18.015, "g", uncertainty=0.01)

    with equivalencies(molar_equivalency(water)):
        mass_only = mass_q.to("mol")

    combined = mass_to_moles(mass_q, water)

    assert combined.uncertainty > mass_only.uncertainty
    assert math.isclose(combined.magnitude, 1.0, rel_tol=1e-3)
    assert math.isclose(combined.uncertainty, 0.00055, abs_tol=2e-4)


def test_mass_to_moles_uncertainty_matches_quadrature_formula():
    water = Species("H2O")
    mass_q = Q_(18.015, "g", uncertainty=0.02)

    combined = mass_to_moles(mass_q, water)

    rel_mass = 0.02 / 18.015
    rel_molar_mass = water.molar_mass.uncertainty / water.molar_mass.magnitude
    expected_rel = (rel_mass**2 + rel_molar_mass**2) ** 0.5
    assert math.isclose(
        combined.uncertainty, combined.magnitude * expected_rel, rel_tol=1e-9
    )


def test_mass_to_moles_with_no_input_uncertainty_reflects_only_molar_mass_error():
    water = Species("H2O")
    mass_q = Q_(18.015, "g")  # no measurement uncertainty at all
    combined = mass_to_moles(mass_q, water)

    rel_molar_mass = water.molar_mass.uncertainty / water.molar_mass.magnitude
    assert math.isclose(
        combined.uncertainty, combined.magnitude * rel_molar_mass, rel_tol=1e-9
    )


def test_moles_to_mass_is_inverse():
    water = Species("H2O")
    mol_q = Q_(1.0, "mol", uncertainty=0.0005)
    mass = moles_to_mass(mol_q, water)
    assert math.isclose(mass.magnitude, 18.015, rel_tol=1e-3)
    assert mass.uncertainty > 0


@pytest.mark.parametrize(
    ("formula", "mass_g"),
    [("H2O", 18.015), ("CO2", 44.009), ("NaCl", 58.44)],
)
def test_mass_to_moles_and_back_round_trips(formula, mass_g):
    species = Species(formula)
    original = Q_(mass_g, "g", uncertainty=0.01)
    moles = mass_to_moles(original, species)
    back = moles_to_mass(moles, species)
    assert math.isclose(back.magnitude, mass_g, rel_tol=1e-6)
