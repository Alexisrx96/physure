import math

import pytest

from measurekit import Q_
from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.ext.chemistry.thermo_kinetics import (
    arrhenius,
    clausius_clapeyron,
    gibbs_free_energy,
    standard_enthalpy,
    standard_entropy,
)


def test_arrhenius_matches_worked_example():
    a = Q_(1e13, "s^-1")
    ea = Q_(75.0, "kJ/mol")
    t = Q_(298.15, "K")
    k = arrhenius(a, ea, t)
    assert math.isclose(k.to("s^-1").magnitude, 0.7254, rel_tol=1e-3)


def test_arrhenius_non_dimensionless_exponent_raises():
    a = Q_(1e13, "s^-1")
    ea = Q_(75.0, "kJ")  # missing /mol -> Ea/(R*T) is not dimensionless
    t = Q_(298.15, "K")
    with pytest.raises(IncompatibleUnitsError):
        arrhenius(a, ea, t)


def test_arrhenius_custom_gas_constant_changes_result():
    a = Q_(1e13, "s^-1")
    ea = Q_(75.0, "kJ/mol")
    t = Q_(298.15, "K")
    k_default = arrhenius(a, ea, t)
    k_custom = arrhenius(a, ea, t, r=Q_(8.2, "J/(mol*K)"))
    # a smaller R makes the (negative) exponent more negative -> smaller k
    assert k_custom.to("s^-1").magnitude < k_default.to("s^-1").magnitude
    assert math.isclose(k_custom.to("s^-1").magnitude, 0.4755, rel_tol=1e-3)


def test_clausius_clapeyron_water_vapor_pressure_at_90c():
    # dHvap(water) ~ 40.7 kJ/mol; boils at 373.15 K / 101.325 kPa.
    # Real steam-table pressure at 90 C is ~70.14 kPa -- constant-dHvap
    # approximation puts this within ~1%.
    dh_vap = Q_(40700.0, "J/mol")
    t1 = Q_(373.15, "K")
    p1 = Q_(101325.0, "Pa")
    t2 = Q_(363.15, "K")
    p2 = clausius_clapeyron(dh_vap, t1, p1, t2)
    assert math.isclose(p2.to("kPa").magnitude, 70.14, rel_tol=1e-2)


def test_clausius_clapeyron_water_vapor_pressure_at_80c():
    # Real steam-table pressure at 80 C is ~47.36 kPa.
    dh_vap = Q_(40700.0, "J/mol")
    t1 = Q_(373.15, "K")
    p1 = Q_(101325.0, "Pa")
    t2 = Q_(353.15, "K")
    p2 = clausius_clapeyron(dh_vap, t1, p1, t2)
    assert math.isclose(p2.to("kPa").magnitude, 47.36, rel_tol=2e-2)


def test_clausius_clapeyron_custom_gas_constant_changes_result():
    dh_vap = Q_(40700.0, "J/mol")
    t1 = Q_(373.15, "K")
    p1 = Q_(101325.0, "Pa")
    t2 = Q_(363.15, "K")
    default = clausius_clapeyron(dh_vap, t1, p1, t2)
    custom = clausius_clapeyron(dh_vap, t1, p1, t2, r=Q_(8.2, "J/(mol*K)"))
    assert not math.isclose(
        default.to("kPa").magnitude, custom.to("kPa").magnitude, rel_tol=1e-4
    )


def test_gibbs_free_energy_sign():
    dh = standard_enthalpy("H2O")
    ds = Q_(-163.2, "J/(mol*K)")
    t = Q_(298.15, "K")
    dg = gibbs_free_energy(dh, t, ds)
    assert math.isclose(dg.to("kJ/mol").magnitude, -237.1, rel_tol=1e-3)


def test_gibbs_free_energy_ammonia_formation_matches_published_value():
    # dG_f(NH3, 298.15 K) from the tabulated dH_f/S via
    # 0.5 N2 + 1.5 H2 -> NH3; the real published dG_f(NH3, g) is ~ -16.4 kJ/mol.
    dh = standard_enthalpy("NH3")
    ds_formation = (
        standard_entropy("NH3")
        - 0.5 * standard_entropy("N2")
        - 1.5 * standard_entropy("H2")
    )
    t = Q_(298.15, "K")
    dg = gibbs_free_energy(dh, t, ds_formation)
    assert math.isclose(dg.to("kJ/mol").magnitude, -16.4, rel_tol=2e-2)


@pytest.mark.parametrize(
    ("formula", "expected_kj_per_mol"),
    [
        ("H2O", -285.8),
        ("CO2", -393.5),
        ("CH4", -74.8),
        ("NH3", -46.1),
        ("NaCl", -411.2),
        ("O2", 0.0),
        ("H2", 0.0),
        ("N2", 0.0),
    ],
)
def test_standard_enthalpy_lookup_table(formula, expected_kj_per_mol):
    assert (
        standard_enthalpy(formula).to("kJ/mol").magnitude
        == expected_kj_per_mol
    )


@pytest.mark.parametrize(
    ("formula", "expected_j_per_mol_k"),
    [
        ("H2O", 69.9),
        ("CO2", 213.7),
        ("CH4", 186.3),
        ("NH3", 192.4),
        ("NaCl", 72.1),
        ("O2", 205.2),
        ("H2", 130.7),
        ("N2", 191.6),
    ],
)
def test_standard_entropy_lookup_table(formula, expected_j_per_mol_k):
    assert (
        standard_entropy(formula).to("J/(mol*K)").magnitude
        == expected_j_per_mol_k
    )


def test_unknown_species_enthalpy_raises_key_error():
    with pytest.raises(KeyError):
        standard_enthalpy("XeF99")


def test_unknown_species_entropy_raises_key_error():
    with pytest.raises(KeyError):
        standard_entropy("XeF99")
