import math

import pytest

from measurekit import Q_
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.ext.chemistry.thermo_kinetics import (
    arrhenius,
    gibbs_free_energy,
    standard_enthalpy,
    standard_entropy,
)


def test_arrhenius_matches_worked_example():
    A = Q_(1e13, "s^-1")
    Ea = Q_(75.0, "kJ/mol")
    T = Q_(298.15, "K")
    k = arrhenius(A, Ea, T)
    assert math.isclose(k.to("s^-1").magnitude, 0.72, rel_tol=1e-2)


def test_arrhenius_non_dimensionless_exponent_raises():
    A = Q_(1e13, "s^-1")
    Ea = Q_(75.0, "kJ")  # missing /mol -> Ea/(R*T) is not dimensionless
    T = Q_(298.15, "K")
    with pytest.raises(IncompatibleUnitsError):
        arrhenius(A, Ea, T)


def test_gibbs_free_energy_sign():
    dH = standard_enthalpy("H2O")
    dS = Q_(-163.2, "J/(mol*K)")
    T = Q_(298.15, "K")
    dG = gibbs_free_energy(dH, T, dS)
    assert math.isclose(dG.to("kJ/mol").magnitude, -237.1, rel_tol=1e-3)


def test_standard_entropy_lookup():
    assert standard_entropy("O2").to("J/(mol*K)").magnitude == 205.2


def test_unknown_species_raises_key_error():
    with pytest.raises(KeyError):
        standard_enthalpy("XeF99")
