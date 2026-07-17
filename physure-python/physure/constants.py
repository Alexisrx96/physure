"""Physical constants as Physure quantities.

Friendly short names (``c``, ``hbar``, ``e``, ...) are resolved lazily from
the active :class:`~physure.domain.measurement.system.UnitSystem`'s CODATA/
SI-2019 constant table (``physure.conf`` ``[Constants]``), so values never
drift out of sync with the canonical table checked by
``tests/measurement_tests/test_constants_codata.py``.

>>> from physure.constants import c
>>> round(c.magnitude)
299792458
"""

from __future__ import annotations

from typing import Any

from physure import get_current_system

# friendly name -> canonical name in physure.conf [Constants]
_ALIASES = {
    "c": "speed_of_light_in_vacuum",
    "h": "planck_constant",
    "hbar": "reduced_planck_constant",
    "k": "boltzmann_constant",
    "k_B": "boltzmann_constant",
    "e": "elementary_charge",
    "G": "newtonian_constant_of_gravitation",
    "N_A": "avogadro_constant",
    "R": "molar_gas_constant",
    "F": "faraday_constant",
    "sigma": "stefan_boltzmann_constant",
    "alpha": "fine_structure_constant",
    "eps0": "vacuum_electric_permittivity",
    "epsilon_0": "vacuum_electric_permittivity",
    "mu0": "vacuum_mag_permeability",
    "mu_0": "vacuum_mag_permeability",
    "m_e": "electron_mass",
    "m_p": "proton_mass",
    "m_n": "neutron_mass",
    "u": "atomic_mass_constant",
    "a0": "bohr_radius",
    "mu_B": "bohr_magneton",
    "Ryd": "rydberg_constant",
    "R_inf": "rydberg_constant",
    "g": "standard_acceleration_of_gravity",
    "atm": "standard_atmosphere",
}

__all__ = sorted(_ALIASES)


def __getattr__(name: str) -> Any:
    canonical = _ALIASES.get(name)
    if canonical is None:
        raise AttributeError(
            f"module 'physure.constants' has no attribute {name!r}"
        )
    value = get_current_system().get_constant(canonical)
    if value is None:
        raise AttributeError(
            f"constant {canonical!r} (aliased as {name!r}) is not registered "
            "in the active UnitSystem"
        )
    return value


def __dir__() -> list[str]:
    return __all__
