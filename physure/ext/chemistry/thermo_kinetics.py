"""Kinetics and thermodynamics helpers: Arrhenius, Clausius-Clapeyron, Gibbs (roadmap §6 Phase 4).

Pulls the gas constant from the active UnitSystem (`molar_gas_constant`,
already defined in physure.conf) rather than hardcoding it.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from physure.domain.measurement.quantity import Quantity

# Standard enthalpy of formation (kJ/mol) and molar entropy (J/(mol*K)) at
# 298.15 K, common reference phase per species (starter set, roadmap §6).
STANDARD_ENTHALPY_FORMATION: dict[str, float] = {
    "H2O": -285.8,
    "CO2": -393.5,
    "CH4": -74.8,
    "NH3": -46.1,
    "NaCl": -411.2,
    "O2": 0.0,
    "H2": 0.0,
    "N2": 0.0,
}
STANDARD_ENTROPY: dict[str, float] = {
    "H2O": 69.9,
    "CO2": 213.7,
    "CH4": 186.3,
    "NH3": 192.4,
    "NaCl": 72.1,
    "O2": 205.2,
    "H2": 130.7,
    "N2": 191.6,
}


def _gas_constant() -> Quantity:
    from physure.application.context import get_current_system

    return get_current_system().get_constant("molar_gas_constant")


def arrhenius(
    a: Quantity, ea: Quantity, t: Quantity, r: Quantity | None = None
) -> Quantity:
    """Arrhenius equation: k = A * exp(-Ea / (R*T)).

    Examples:
        >>> from physure import Q_
        >>> a = Q_(1e13, "s^-1")
        >>> ea = Q_(75.0, "kJ/mol")
        >>> t = Q_(298.15, "K")
        >>> round(arrhenius(a, ea, t).magnitude, 2)
        0.73
    """
    exponent = (ea / ((r or _gas_constant()) * t)).to("dimensionless")
    return a * math.exp(-exponent.magnitude)


def clausius_clapeyron(
    delta_h_vap: Quantity,
    t1: Quantity,
    p1: Quantity,
    t2: Quantity,
    r: Quantity | None = None,
) -> Quantity:
    """Clausius-Clapeyron relation: solves for P2 given a known (T1, P1)."""
    gas_r = r or _gas_constant()
    ratio_exponent = (
        (-delta_h_vap / gas_r) * (1 / t2.to("K") - 1 / t1.to("K"))
    ).to("dimensionless")
    return p1 * math.exp(ratio_exponent.magnitude)


def gibbs_free_energy(
    delta_h: Quantity, t: Quantity, delta_s: Quantity
) -> Quantity:
    """Gibbs free energy: dG = dH - T*dS.

    Examples:
        >>> from physure import Q_
        >>> dh = Q_(-285.8, "kJ/mol")
        >>> ds = Q_(-163.2, "J/(mol*K)")
        >>> t = Q_(298.15, "K")
        >>> round(gibbs_free_energy(dh, t, ds).to("kJ/mol").magnitude, 1)
        -237.1
    """
    return delta_h - t * delta_s


def standard_enthalpy(formula: str) -> Quantity:
    """Standard enthalpy of formation at 298.15 K, in kJ/mol."""
    from physure import Q_

    if formula not in STANDARD_ENTHALPY_FORMATION:
        raise KeyError(f"No tabulated standard enthalpy for {formula!r}")
    return Q_(STANDARD_ENTHALPY_FORMATION[formula], "kJ/mol")


def standard_entropy(formula: str) -> Quantity:
    """Standard molar entropy at 298.15 K, in J/(mol*K)."""
    from physure import Q_

    if formula not in STANDARD_ENTROPY:
        raise KeyError(f"No tabulated standard entropy for {formula!r}")
    return Q_(STANDARD_ENTROPY[formula], "J/(mol*K)")
