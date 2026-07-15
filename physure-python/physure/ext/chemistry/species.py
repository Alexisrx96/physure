"""Chemical species: formula parsing and molar mass (roadmap §3.1).

Pure-Python, zero-dependency: a compact IUPAC standard-atomic-weight table
plus a regex/stack formula parser. Molar mass uncertainty combines each
element's tabulated uncertainty in quadrature, weighted by atom count.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

from physure.core.formatting import subscript_to_ascii

if TYPE_CHECKING:
    from physure.domain.measurement.quantity import Quantity

# IUPAC 2021 standard atomic weights: symbol -> (mass g/mol, abs. uncertainty).
# For elements with no stable isotopes (uncertainty not meaningful), the mass
# of the longest-lived known isotope is used with uncertainty 0.0.
ATOMIC_WEIGHTS: dict[str, tuple[float, float]] = {
    "H": (1.008, 0.0002),
    "He": (4.002602, 0.000002),
    "Li": (6.94, 0.02),
    "Be": (9.0121831, 0.0000005),
    "B": (10.81, 0.01),
    "C": (12.011, 0.001),
    "N": (14.007, 0.001),
    "O": (15.999, 0.001),
    "F": (18.998403163, 0.000001),
    "Ne": (20.1797, 0.0006),
    "Na": (22.98976928, 0.00000002),
    "Mg": (24.305, 0.001),
    "Al": (26.9815385, 0.0000007),
    "Si": (28.085, 0.001),
    "P": (30.973761998, 0.000000005),
    "S": (32.06, 0.02),
    "Cl": (35.45, 0.01),
    "Ar": (39.948, 0.001),
    "K": (39.0983, 0.0001),
    "Ca": (40.078, 0.004),
    "Sc": (44.955908, 0.000005),
    "Ti": (47.867, 0.001),
    "V": (50.9415, 0.0001),
    "Cr": (51.9961, 0.0006),
    "Mn": (54.938044, 0.000003),
    "Fe": (55.845, 0.002),
    "Co": (58.933194, 0.000004),
    "Ni": (58.6934, 0.0004),
    "Cu": (63.546, 0.003),
    "Zn": (65.38, 0.02),
    "Ga": (69.723, 0.001),
    "Ge": (72.630, 0.008),
    "As": (74.921595, 0.000006),
    "Se": (78.971, 0.008),
    "Br": (79.904, 0.003),
    "Kr": (83.798, 0.002),
    "Rb": (85.4678, 0.0003),
    "Sr": (87.62, 0.01),
    "Y": (88.90584, 0.00001),
    "Zr": (91.224, 0.002),
    "Nb": (92.90637, 0.00001),
    "Mo": (95.95, 0.01),
    "Tc": (98.0, 0.0),
    "Ru": (101.07, 0.02),
    "Rh": (102.90550, 0.00002),
    "Pd": (106.42, 0.01),
    "Ag": (107.8682, 0.0002),
    "Cd": (112.414, 0.004),
    "In": (114.818, 0.001),
    "Sn": (118.710, 0.007),
    "Sb": (121.760, 0.001),
    "Te": (127.60, 0.03),
    "I": (126.90447, 0.00003),
    "Xe": (131.293, 0.006),
    "Cs": (132.90545196, 0.00000006),
    "Ba": (137.327, 0.007),
    "La": (138.90547, 0.00007),
    "Ce": (140.116, 0.001),
    "Pr": (140.90766, 0.00001),
    "Nd": (144.242, 0.003),
    "Pm": (145.0, 0.0),
    "Sm": (150.36, 0.02),
    "Eu": (151.964, 0.001),
    "Gd": (157.25, 0.03),
    "Tb": (158.925354, 0.000008),
    "Dy": (162.500, 0.001),
    "Ho": (164.930328, 0.000007),
    "Er": (167.259, 0.003),
    "Tm": (168.934218, 0.000006),
    "Yb": (173.045, 0.010),
    "Lu": (174.9668, 0.0001),
    "Hf": (178.49, 0.02),
    "Ta": (180.94788, 0.00002),
    "W": (183.84, 0.01),
    "Re": (186.207, 0.001),
    "Os": (190.23, 0.03),
    "Ir": (192.217, 0.002),
    "Pt": (195.084, 0.009),
    "Au": (196.966569, 0.000005),
    "Hg": (200.592, 0.003),
    "Tl": (204.38, 0.01),
    "Pb": (207.2, 0.1),
    "Bi": (208.98040, 0.00001),
    "Po": (209.0, 0.0),
    "At": (210.0, 0.0),
    "Rn": (222.0, 0.0),
    "Fr": (223.0, 0.0),
    "Ra": (226.0, 0.0),
    "Ac": (227.0, 0.0),
    "Th": (232.0377, 0.0004),
    "Pa": (231.03588, 0.00001),
    "U": (238.02891, 0.00003),
    "Np": (237.0, 0.0),
    "Pu": (244.0, 0.0),
    "Am": (243.0, 0.0),
    "Cm": (247.0, 0.0),
    "Bk": (247.0, 0.0),
    "Cf": (251.0, 0.0),
    "Es": (252.0, 0.0),
    "Fm": (257.0, 0.0),
    "Md": (258.0, 0.0),
    "No": (259.0, 0.0),
    "Lr": (266.0, 0.0),
    "Rf": (267.0, 0.0),
    "Db": (268.0, 0.0),
    "Sg": (269.0, 0.0),
    "Bh": (270.0, 0.0),
    "Hs": (269.0, 0.0),
    "Mt": (278.0, 0.0),
    "Ds": (281.0, 0.0),
    "Rg": (282.0, 0.0),
    "Cn": (285.0, 0.0),
    "Nh": (286.0, 0.0),
    "Fl": (289.0, 0.0),
    "Mc": (290.0, 0.0),
    "Lv": (293.0, 0.0),
    "Ts": (294.0, 0.0),
    "Og": (294.0, 0.0),
}

_TOKEN_RE = re.compile(r"([A-Z][a-z]?)(\d*)|(\()|(\))(\d*)")


def _close_group(
    stack: list[dict[str, int]], group_count: str, formula: str
) -> None:
    if len(stack) < 2:
        raise ValueError(f"Unbalanced parentheses: {formula!r}")
    group = stack.pop()
    mult = int(group_count) if group_count else 1
    for el, n in group.items():
        stack[-1][el] = stack[-1].get(el, 0) + n * mult


def _add_element(
    stack: list[dict[str, int]], element: str, count: str, formula: str
) -> None:
    if element not in ATOMIC_WEIGHTS:
        raise ValueError(f"Unknown element {element!r} in {formula!r}")
    n = int(count) if count else 1
    stack[-1][element] = stack[-1].get(element, 0) + n


def parse_formula(formula: str) -> dict[str, int]:
    """Parses a chemical formula into element -> atom-count.

    Examples:
        >>> parse_formula("H2O")
        {'H': 2, 'O': 1}
        >>> parse_formula("Ca(NO3)2")
        {'Ca': 1, 'N': 2, 'O': 6}
        >>> parse_formula("H₂O")
        {'H': 2, 'O': 1}
    """
    formula = subscript_to_ascii(formula)
    stack: list[dict[str, int]] = [{}]
    pos = 0
    for match in _TOKEN_RE.finditer(formula):
        if match.start() != pos:
            raise ValueError(f"Invalid formula: {formula!r}")
        pos = match.end()
        element, count, open_paren, close_paren, group_count = match.groups()
        if open_paren:
            stack.append({})
        elif close_paren:
            _close_group(stack, group_count, formula)
        elif element:
            _add_element(stack, element, count, formula)

    if pos != len(formula) or len(stack) != 1 or not stack[0]:
        raise ValueError(f"Invalid formula: {formula!r}")
    return stack[0]


class Species:
    """A chemical compound or element, parsed from a formula string.

    Examples:
        >>> Species("H2O").composition
        {'H': 2, 'O': 1}
    """

    __slots__ = ("composition", "formula")

    def __init__(self, formula: str) -> None:
        self.formula = formula
        self.composition = parse_formula(formula)

    @property
    def molar_mass(self) -> Quantity:
        """Molar mass as a Quantity in g/mol, with combined atomic uncertainty."""
        from physure import Q_

        mass = 0.0
        variance = 0.0
        for element, count in self.composition.items():
            elem_mass, elem_std = ATOMIC_WEIGHTS[element]
            mass += count * elem_mass
            variance += (count * elem_std) ** 2
        return Q_(mass, "g/mol", uncertainty=variance**0.5)

    def __repr__(self) -> str:
        return f"Species({self.formula!r}, molar_mass={self.molar_mass})"
