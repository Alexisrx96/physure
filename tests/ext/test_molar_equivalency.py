import math

from measurekit import Q_
from measurekit.domain.measurement.equivalencies import equivalencies
from measurekit.ext.chemistry.equivalency import (
    mass_to_moles,
    molar_equivalency,
    moles_to_mass,
)
from measurekit.ext.chemistry.species import Species


def test_raw_equivalency_round_trip():
    water = Species("H2O")
    mass = Q_(18.015, "g")
    with equivalencies(molar_equivalency(water)):
        moles = mass.to("mol")
    assert math.isclose(moles.magnitude, 1.0, rel_tol=1e-3)
    with equivalencies(molar_equivalency(water)):
        back = moles.to("g")
    assert math.isclose(back.magnitude, mass.magnitude, rel_tol=1e-6)


def test_mass_to_moles_folds_molar_mass_uncertainty():
    water = Species("H2O")
    mass_q = Q_(18.015, "g", uncertainty=0.01)

    with equivalencies(molar_equivalency(water)):
        mass_only = mass_q.to("mol")

    combined = mass_to_moles(mass_q, water)

    assert combined.uncertainty > mass_only.uncertainty
    assert math.isclose(combined.magnitude, 1.0, rel_tol=1e-3)
    assert math.isclose(combined.uncertainty, 0.00055, abs_tol=2e-4)


def test_moles_to_mass_is_inverse():
    water = Species("H2O")
    mol_q = Q_(1.0, "mol", uncertainty=0.0005)
    mass = moles_to_mass(mol_q, water)
    assert math.isclose(mass.magnitude, 18.015, rel_tol=1e-3)
    assert mass.uncertainty > 0
