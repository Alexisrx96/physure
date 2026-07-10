"""Molar mass equivalency: links Mass (M) and Amount-of-substance (N) (roadmap §3.2).

`molar_equivalency` is the raw astropy-style equivalency: it lets
`Quantity.to()` cross the Mass <-> Amount dimension boundary and propagates
the *input measurement's* uncertainty (via the equivalency framework's
numerical derivative). The molar mass itself is a constant captured in the
conversion closure, so its own uncertainty is not carried by a bare `.to()`.

`mass_to_moles` / `moles_to_mass` are the species-aware helpers that also
fold the species' molar-mass uncertainty in, in quadrature:
    rel(n) = sqrt(rel(mass)**2 + rel(molar_mass)**2)
This is what the roadmap's worked examples (§4.1) assume.

# ponytail: quadrature combination assumes the mass measurement and the
# tabulated molar mass are independent random variables -- true for a lab
# weighing vs. a periodic-table constant.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from measurekit.domain.measurement.equivalencies import EquivalencyList
    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.ext.chemistry.species import Species


def molar_equivalency(species: Species) -> EquivalencyList:
    """Equivalency between Mass and Amount-of-substance for one species."""
    from measurekit.domain.measurement.dimensions import Dimension

    m_base = species.molar_mass.to("kg/mol").magnitude
    dim_mass = Dimension({"M": 1})
    dim_amount = Dimension({"N": 1})

    return [(dim_mass, dim_amount, lambda m: m / m_base, lambda n: n * m_base)]


def _relative_uncertainty(q: Quantity) -> float:
    magnitude = q.magnitude
    return abs(q.uncertainty / magnitude) if magnitude else 0.0


def mass_to_moles(mass_q: Quantity, species: Species) -> Quantity:
    """Converts a mass Quantity to moles, including molar-mass uncertainty."""
    from measurekit import Q_
    from measurekit.domain.measurement.equivalencies import equivalencies

    with equivalencies(molar_equivalency(species)):
        converted = mass_q.to("mol")

    rel = (
        _relative_uncertainty(mass_q) ** 2
        + _relative_uncertainty(species.molar_mass) ** 2
    ) ** 0.5
    return Q_(
        converted.magnitude,
        converted.unit,
        uncertainty=abs(converted.magnitude) * rel,
    )


def moles_to_mass(mol_q: Quantity, species: Species) -> Quantity:
    """Converts a moles Quantity to mass, including molar-mass uncertainty."""
    from measurekit import Q_
    from measurekit.domain.measurement.equivalencies import equivalencies

    with equivalencies(molar_equivalency(species)):
        converted = mol_q.to("g")

    rel = (
        _relative_uncertainty(mol_q) ** 2
        + _relative_uncertainty(species.molar_mass) ** 2
    ) ** 0.5
    return Q_(
        converted.magnitude,
        converted.unit,
        uncertainty=abs(converted.magnitude) * rel,
    )
