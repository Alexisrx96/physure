"""Chemistry & physical-chemistry extension (see docs/chemistry_roadmap.md)."""

from physure.ext.chemistry.equivalency import (
    mass_to_moles,
    molar_equivalency,
    moles_to_mass,
)
from physure.ext.chemistry.reaction import Reaction, ReactionResult
from physure.ext.chemistry.species import Species
from physure.ext.chemistry.thermo_kinetics import (
    STANDARD_ENTHALPY_FORMATION,
    STANDARD_ENTROPY,
    arrhenius,
    clausius_clapeyron,
    gibbs_free_energy,
    standard_enthalpy,
    standard_entropy,
)

__all__ = [
    "STANDARD_ENTHALPY_FORMATION",
    "STANDARD_ENTROPY",
    "Reaction",
    "ReactionResult",
    "Species",
    "arrhenius",
    "clausius_clapeyron",
    "gibbs_free_energy",
    "mass_to_moles",
    "molar_equivalency",
    "moles_to_mass",
    "standard_enthalpy",
    "standard_entropy",
]
