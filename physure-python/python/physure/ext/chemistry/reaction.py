"""Reaction parsing, balancing, and yield/limiting-reactant calculation (roadmap §3.3).

Balancing solves the element-conservation null space with pure-Python exact
(Fraction-based) Gaussian elimination -- no numpy/scipy/sympy dependency.

# ponytail: assumes exactly one degree of freedom (a single equation set of
# stoichiometric coefficients), true for ordinary homework-scale reactions.
# Multi-reaction networks with several independent solutions are out of
# scope; a Rust solver in physure._core is the upgrade path if that's
# ever needed (roadmap §5).
"""

from __future__ import annotations

import math
import re
from dataclasses import dataclass
from fractions import Fraction
from typing import TYPE_CHECKING

from physure.domain.notation.lexer import subscript_to_ascii
from physure.ext.chemistry.equivalency import mass_to_moles, moles_to_mass
from physure.ext.chemistry.species import Species

if TYPE_CHECKING:
    from physure.domain.measurement.quantity import Quantity

_ARROW_RE = re.compile(r"<=>|->|=|→|⇌")
_TERM_RE = re.compile(r"^\s*\d*\s*([A-Za-z0-9()]+)\s*$")


def _split_terms(side: str) -> list[str]:
    return [t.strip() for t in side.split("+")]


def _parse_species(term: str) -> Species:
    match = _TERM_RE.match(subscript_to_ascii(term))
    if not match:
        raise ValueError(f"Invalid reaction term: {term!r}")
    return Species(match.group(1))


def _rref(
    matrix: list[list[Fraction]],
) -> tuple[list[list[Fraction]], list[int]]:
    """Row-reduces `matrix` to reduced row-echelon form (exact, Fraction)."""
    m = [row[:] for row in matrix]
    rows = len(m)
    cols = len(m[0]) if rows else 0
    pivot_row = 0
    pivots: list[int] = []
    for col in range(cols):
        pivot = next(
            (r for r in range(pivot_row, rows) if m[r][col] != 0), None
        )
        if pivot is None:
            continue
        m[pivot_row], m[pivot] = m[pivot], m[pivot_row]
        pivot_val = m[pivot_row][col]
        m[pivot_row] = [v / pivot_val for v in m[pivot_row]]
        for r in range(rows):
            if r != pivot_row and m[r][col] != 0:
                factor = m[r][col]
                m[r] = [
                    a - factor * b
                    for a, b in zip(m[r], m[pivot_row], strict=True)
                ]
        pivots.append(col)
        pivot_row += 1
        if pivot_row == rows:
            break
    return m, pivots


def _balance(
    reactants: list[Species], products: list[Species]
) -> tuple[list[int], list[int]]:
    """Solves for the smallest positive integer stoichiometric coefficients."""
    species = [*reactants, *products]
    n_react = len(reactants)
    elements = sorted({el for sp in species for el in sp.composition})

    def entry(idx: int, el: str) -> Fraction:
        sign = 1 if idx < n_react else -1
        return Fraction(sign * species[idx].composition.get(el, 0))

    matrix = [[entry(i, el) for i in range(len(species))] for el in elements]
    rref, pivots = _rref(matrix)
    free_cols = [c for c in range(len(species)) if c not in pivots]
    if len(free_cols) != 1:
        raise ValueError(
            "Cannot uniquely balance this reaction "
            "(expected exactly one degree of freedom)"
        )
    free_col = free_cols[0]

    coeffs = [Fraction(0)] * len(species)
    coeffs[free_col] = Fraction(1)
    for row_i, piv_col in enumerate(pivots):
        coeffs[piv_col] = -rref[row_i][free_col]

    if any(c < 0 for c in coeffs):
        coeffs = [-c for c in coeffs]
    if any(c <= 0 for c in coeffs):
        raise ValueError(
            "Cannot balance this reaction with positive integer coefficients"
        )

    denom_lcm = 1
    for c in coeffs:
        denom_lcm = (
            denom_lcm * c.denominator // math.gcd(denom_lcm, c.denominator)
        )
    ints = [int(c * denom_lcm) for c in coeffs]
    gcd_all = 0
    for v in ints:
        gcd_all = math.gcd(gcd_all, v)
    if gcd_all > 1:
        ints = [v // gcd_all for v in ints]

    return ints[:n_react], ints[n_react:]


@dataclass
class ReactionResult:
    """Result of `Reaction.calculate`: limiting reactant and product yields."""

    limiting_reactant: str
    yields: dict[str, Quantity]


class Reaction:
    """A balanced chemical reaction.

    Examples:
        >>> rxn = Reaction.from_string("H2 + O2 -> H2O")
        >>> rxn.reactant_coeffs, rxn.product_coeffs
        ([2, 1], [2])
        >>> Reaction.from_string("N2 + 3 H2 ⇌ 2 NH3").reversible
        True
    """

    __slots__ = (
        "product_coeffs",
        "products",
        "reactant_coeffs",
        "reactants",
        "reversible",
    )

    def __init__(
        self,
        reactants: list[Species],
        products: list[Species],
        reversible: bool = False,
    ) -> None:
        self.reactants = reactants
        self.products = products
        self.reversible = reversible
        self.reactant_coeffs, self.product_coeffs = _balance(
            reactants, products
        )

    @classmethod
    def from_string(cls, equation: str) -> Reaction:
        """Parses e.g. "2 H2 + O2 -> 2 H2O" (coefficients are re-derived).

        Accepts irreversible arrows (``->``, ``=``, ``→``) and equilibrium
        arrows (``⇌``, ``<=>``); the latter set `reversible` to `True`.
        """
        match = _ARROW_RE.search(equation)
        if match is None or _ARROW_RE.search(equation, match.end()):
            raise ValueError(f"Invalid reaction equation: {equation!r}")
        lhs, rhs = equation[: match.start()], equation[match.end() :]
        reversible = match.group() in ("<=>", "⇌")
        reactants = [_parse_species(t) for t in _split_terms(lhs)]
        products = [_parse_species(t) for t in _split_terms(rhs)]
        return cls(reactants, products, reversible=reversible)

    def calculate(self, **inputs: Quantity) -> ReactionResult:
        """Finds the limiting reactant and product yields from input masses."""
        ratios: dict[str, tuple[Quantity, int]] = {}
        for species, coeff in zip(
            self.reactants, self.reactant_coeffs, strict=True
        ):
            mass_q = inputs.get(species.formula)
            if mass_q is None:
                raise ValueError(
                    f"Missing input for reactant {species.formula!r}"
                )
            moles = mass_to_moles(mass_q, species)
            ratios[species.formula] = (moles, coeff)

        limiting_formula = min(
            ratios,
            key=lambda name: ratios[name][0].magnitude / ratios[name][1],
        )
        limiting_moles, limiting_coeff = ratios[limiting_formula]
        extent = limiting_moles / limiting_coeff

        yields: dict[str, Quantity] = {}
        for species, coeff in zip(
            self.products, self.product_coeffs, strict=True
        ):
            product_moles = extent * coeff
            yields[species.formula] = moles_to_mass(product_moles, species)

        return ReactionResult(
            limiting_reactant=limiting_formula, yields=yields
        )
