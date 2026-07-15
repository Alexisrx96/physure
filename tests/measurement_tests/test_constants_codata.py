"""CODATA / SI-2019 physical-constant checks.

Every value below is a published, independently verifiable reference figure
(CODATA 2018 recommended values -- the vintage the [Units] "ATOMIC UNITS"
section of measurekit.conf already matches exactly, e.g. bohr = 5.29177210903e-11 m,
electron_mass = 9.1093837015e-31 kg). The 2019 SI redefinition fixes h, e,
k_B, N_A, and c *exactly* -- those have zero measurement uncertainty, so they
are checked bit-for-bit, not "close enough".

The derived-relation cross-checks at the bottom recompute a constant from the
engine's *other* stored constants (e.g. R = N_A * k_B) -- proving the table is
internally self-consistent, not just individually plausible.
"""

import math

import pytest

from measurekit import get_current_system


@pytest.fixture
def system():
    return get_current_system()


# name -> exact published value (SI-2019 defining constants; zero uncertainty)
EXACT_CONSTANTS = [
    ("speed_of_light_in_vacuum", 299792458.0),  # defines the metre
    ("planck_constant", 6.62607015e-34),  # defines the kilogram
    ("elementary_charge", 1.602176634e-19),  # defines the ampere
    ("boltzmann_constant", 1.380649e-23),  # defines the kelvin
    ("avogadro_constant", 6.02214076e23),  # defines the mole
    ("standard_acceleration_of_gravity", 9.80665),  # CGPM 1901 definition
    ("standard_atmosphere", 101325.0),  # CIPM 1954 definition
]


@pytest.mark.parametrize(("name", "exact_value"), EXACT_CONSTANTS)
def test_exact_by_definition_constants_match_si_2019(
    system, name, exact_value
):
    c = system.get_constant(name)
    assert c is not None
    assert c.magnitude == exact_value


# name -> (CODATA-2018 published value, relative tolerance)
MEASURED_CONSTANTS = [
    ("vacuum_electric_permittivity", 8.8541878128e-12, 1e-6),
    ("vacuum_mag_permeability", 1.25663706212e-6, 1e-6),
    ("fine_structure_constant", 7.2973525693e-3, 1e-6),
    ("bohr_radius", 5.29177210903e-11, 1e-6),
    ("rydberg_constant", 10973731.568160, 1e-6),
    ("bohr_magneton", 9.2740100783e-24, 1e-6),
    ("electron_mass", 9.1093837015e-31, 1e-6),
    ("proton_mass", 1.67262192369e-27, 1e-6),
    ("neutron_mass", 1.67492749804e-27, 1e-6),
    ("atomic_mass_constant", 1.66053906660e-27, 1e-6),
    (
        "newtonian_constant_of_gravitation",
        6.67430e-11,
        1e-4,
    ),  # G: only ~5 sig figs known
    ("stefan_boltzmann_constant", 5.670374419e-8, 1e-6),
]

# Derived-exact constants: computed purely from other SI-2019-exact constants,
# so they carry zero measurement uncertainty and are checked bit-for-bit.
DERIVED_EXACT_CONSTANTS = [
    ("molar_gas_constant", 8.31446261815324),  # = N_A * k_B
    ("faraday_constant", 96485.33212331001),  # = N_A * e
    ("reduced_planck_constant", 1.0545718176461565e-34),  # = h / (2*pi)
]


@pytest.mark.parametrize(("name", "exact_value"), DERIVED_EXACT_CONSTANTS)
def test_derived_exact_constants_match_full_precision(
    system, name, exact_value
):
    c = system.get_constant(name)
    assert c is not None
    assert c.magnitude == exact_value


@pytest.mark.parametrize(
    ("name", "published_value", "rel_tol"), MEASURED_CONSTANTS
)
def test_constants_match_codata_within_tolerance(
    system, name, published_value, rel_tol
):
    c = system.get_constant(name)
    assert c is not None
    assert math.isclose(c.magnitude, published_value, rel_tol=rel_tol)


@pytest.mark.parametrize(
    ("name", "expected_unit"),
    [
        ("speed_of_light_in_vacuum", "m/s"),
        ("planck_constant", "J/Hz"),
        ("reduced_planck_constant", "J*s"),
        ("elementary_charge", "C"),
        ("boltzmann_constant", "J/K"),
        ("avogadro_constant", "1/mol"),
        ("molar_gas_constant", "J/(mol*K)"),
        ("faraday_constant", "C/mol"),
        ("stefan_boltzmann_constant", "W/(m^2*K^4)"),
        ("bohr_magneton", "J/T"),
        ("bohr_radius", "m"),
        ("rydberg_constant", "1/m"),
        ("standard_acceleration_of_gravity", "m/s^2"),
        ("standard_atmosphere", "Pa"),
        ("newtonian_constant_of_gravitation", "m^3/(kg*s^2)"),
        ("electron_mass", "kg"),
        ("proton_mass", "kg"),
        ("neutron_mass", "kg"),
        ("atomic_mass_constant", "kg"),
    ],
)
def test_constant_dimensions_are_physically_correct(
    system, name, expected_unit
):
    c = system.get_constant(name)
    assert c is not None
    assert c.unit == system.get_unit(expected_unit)


# --- derived-relation cross-checks ------------------------------------------
# Each identity is recomputed from the engine's *own* stored constants, so a
# failure here means the table is internally inconsistent -- a stronger
# credibility signal than any single spot-check.


def test_molar_gas_constant_equals_avogadro_times_boltzmann(system):
    r = system.get_constant("molar_gas_constant")
    n_a = system.get_constant("avogadro_constant")
    k_b = system.get_constant("boltzmann_constant")
    assert math.isclose(
        r.magnitude, n_a.magnitude * k_b.magnitude, rel_tol=1e-9
    )


def test_faraday_constant_equals_avogadro_times_elementary_charge(system):
    f = system.get_constant("faraday_constant")
    n_a = system.get_constant("avogadro_constant")
    e = system.get_constant("elementary_charge")
    assert math.isclose(f.magnitude, n_a.magnitude * e.magnitude, rel_tol=1e-6)


def test_stefan_boltzmann_constant_derived_from_h_c_kb(system):
    sigma = system.get_constant("stefan_boltzmann_constant")
    h = system.get_constant("planck_constant")
    c = system.get_constant("speed_of_light_in_vacuum")
    k_b = system.get_constant("boltzmann_constant")
    derived = (2 * math.pi**5 * k_b.magnitude**4) / (
        15 * h.magnitude**3 * c.magnitude**2
    )
    assert math.isclose(sigma.magnitude, derived, rel_tol=1e-6)


def test_bohr_magneton_derived_from_e_hbar_me(system):
    mu_b = system.get_constant("bohr_magneton")
    e = system.get_constant("elementary_charge")
    hbar = system.get_constant("reduced_planck_constant")
    m_e = system.get_constant("electron_mass")
    derived = e.magnitude * hbar.magnitude / (2 * m_e.magnitude)
    assert math.isclose(mu_b.magnitude, derived, rel_tol=1e-6)


def test_speed_of_light_derived_from_vacuum_permittivity_and_permeability(
    system,
):
    c = system.get_constant("speed_of_light_in_vacuum")
    eps_0 = system.get_constant("vacuum_electric_permittivity")
    mu_0 = system.get_constant("vacuum_mag_permeability")
    derived = 1.0 / math.sqrt(mu_0.magnitude * eps_0.magnitude)
    assert math.isclose(c.magnitude, derived, rel_tol=1e-6)


def test_rydberg_constant_derived_from_alpha_me_c_h(system):
    r_inf = system.get_constant("rydberg_constant")
    alpha = system.get_constant("fine_structure_constant")
    m_e = system.get_constant("electron_mass")
    c = system.get_constant("speed_of_light_in_vacuum")
    h = system.get_constant("planck_constant")
    derived = (
        alpha.magnitude**2 * m_e.magnitude * c.magnitude / (2 * h.magnitude)
    )
    assert math.isclose(r_inf.magnitude, derived, rel_tol=1e-6)
