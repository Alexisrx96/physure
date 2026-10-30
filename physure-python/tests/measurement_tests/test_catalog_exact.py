"""The numeric factors of physure.conf are the exact value of their definition.

A factor typed with a few digits is a silent error: `1 deg => arcmin` used to
be 60.000043 and `1 atm => Torr` 760.0021. Each reference below is computed
from the definition (exact fractions, or pi where the definition has pi), not
copied from the catalog. The tolerance is a few units in the last place of an
f64; the exact rational scales planned for the unit algebra will remove even
that.
"""

import math
from fractions import Fraction

import pytest

from physure import Q_

LB = Fraction("0.45359237")
G0 = Fraction("9.80665")
FT = Fraction("0.3048")
IN = Fraction("0.0254")
C = 299792458
LBF = LB * G0

# (unit as a user writes it, the SI unit it converts to, exact value)
REFERENCE = [
    ("ly", "m", C * Fraction("365.25") * 86400),
    ("pc", "m", 648000 / math.pi * 149597870700),
    ("slug", "kg", LBF / FT),
    ("acre", "m^2", 43560 * FT**2),
    ("gal", "m^3", 231 * IN**3),
    ("fl_oz", "m^3", 231 * IN**3 / 128),
    ("lbf", "N", LBF),
    ("BTU", "J", Fraction("1055.05585262")),
    ("hp", "W", 550 * FT * LBF),
    ("psi", "Pa", LBF / IN**2),
    ("Torr", "Pa", Fraction(101325, 760)),
    ("mmHg", "Pa", Fraction("13595.1") * G0 / 1000),
    ("deg", "rad", math.pi / 180),
    ("arcmin", "rad", math.pi / 10800),
    ("arcsec", "rad", math.pi / 648000),
    ("Fr", "C", Fraction(1, 10 * C)),
    ("D", "C*m", Fraction(1, 10**21 * C)),
    # CODATA 2022
    ("a0", "m", 5.29177210544e-11),
    ("Eh", "J", 4.359744722206e-18),
    ("me", "kg", 9.1093837139e-31),
    ("tau0", "s", 2.4188843265864e-17),
    ("qp", "C", 1.8755460384151476e-18),
]


@pytest.mark.parametrize(
    ("unit", "si", "exact"), REFERENCE, ids=[r[0] for r in REFERENCE]
)
def test_the_factor_is_the_exact_value_of_its_definition(unit, si, exact):
    got = Q_(1, unit).to(si).magnitude
    assert math.isclose(got, float(exact), rel_tol=1e-15), (unit, got)


# (value, from, to, exact result): relations that hold by definition
IDENTITIES = [
    (1, "deg", "arcmin", 60),
    (1, "arcmin", "arcsec", 60),
    (1, "deg", "arcsec", 3600),
    (180, "deg", "rad", math.pi),
    (1, "mile", "ft", 5280),
    (1, "ft", "in", 12),
    (1, "mile", "yd", 1760),
    (1, "lb", "oz", 16),
    (1, "gal", "fl_oz", 128),
    (1, "gal", "in^3", 231),
    (1, "acre", "ft^2", 43560),
    (1, "atm", "Pa", 101325),
    (1, "atm", "Torr", 760),
    (1, "psi", "lbf/in^2", 1),
    (1, "hp", "ft*lbf/s", 550),
    (1, "slug*ft/s^2", "lbf", 1),
    (1, "pc", "ly", 648000 / math.pi * 149597870700 / (C * 365.25 * 86400)),
    (1, "d", "h", 24),
    (1, "yr", "d", 365.25),
    (212, "degF", "degC", 100),
    (32, "degF", "degC", 0),
]


@pytest.mark.parametrize(
    ("value", "src", "dst", "exact"),
    IDENTITIES,
    ids=[f"{i[0]} {i[1]} in {i[2]}" for i in IDENTITIES],
)
def test_a_conversion_that_is_exact_by_definition_comes_out_exact(
    value, src, dst, exact
):
    got = Q_(value, src).to(dst).magnitude
    assert math.isclose(got, exact, rel_tol=2e-15, abs_tol=1e-12), got
