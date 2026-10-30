"""An ambiguous unit has one name per definition; the short symbol is the default.

`cal`, `BTU`, `acre`, `gal`, `fl_oz`, `hp` and `yr` each have several
definitions in use. The short symbol names the default one, and every
definition, the default included, can be written with a qualified name that
nothing else can take.
"""

import math
from fractions import Fraction

import pytest

from physure import Q_

G0 = Fraction("9.80665")
FT = Fraction("0.3048")
US_FT = Fraction(1200, 3937)

# (qualified name, the SI unit it converts to, exact value)
VARIANTS = [
    ("cal_th", "J", Fraction("4.184")),
    ("cal_it", "J", Fraction("4.1868")),
    ("btu_it", "J", Fraction("1055.05585262")),
    ("btu_th", "J", Fraction("453.59237") * 5 * Fraction("4.184") / 9),
    ("btu_iso", "J", Fraction("1055.056")),
    ("acre_international", "m^2", 43560 * FT**2),
    ("acre_us_survey", "m^2", 43560 * US_FT**2),
    ("gal_us", "m^3", Fraction("0.003785411784")),
    ("gallon_imperial", "m^3", Fraction("0.00454609")),
    ("fl_oz_us", "m^3", Fraction("0.003785411784") / 128),
    ("fluid_ounce_imperial", "m^3", Fraction("0.00454609") / 160),
    ("hp_mechanical", "W", 550 * FT * Fraction("0.45359237") * G0),
    ("hp_metric", "W", 75 * G0),
    ("hp_electric", "W", 746),
    ("year_julian", "s", Fraction("365.25") * 86400),
    ("year_gregorian", "s", Fraction("365.2425") * 86400),
]


@pytest.mark.parametrize(
    ("name", "si", "exact"), VARIANTS, ids=[v[0] for v in VARIANTS]
)
def test_each_definition_has_its_exact_value_under_its_own_name(
    name, si, exact
):
    got = Q_(1, name).to(si).magnitude
    assert math.isclose(got, float(exact), rel_tol=1e-15), (name, got)


# (short symbol, the qualified name it stands for, an SI unit to compare in)
DEFAULTS = [
    ("cal", "cal_th", "J"),
    ("calorie", "cal_th", "J"),
    ("BTU", "btu_it", "J"),
    ("btu", "btu_it", "J"),
    ("acre", "acre_international", "m^2"),
    ("gal", "gal_us", "m^3"),
    ("gallon", "gal_us", "m^3"),
    ("fl_oz", "fl_oz_us", "m^3"),
    ("hp", "hp_mechanical", "W"),
    ("yr", "year_julian", "s"),
    ("year", "year_julian", "s"),
]


@pytest.mark.parametrize(
    ("short", "qualified", "si"), DEFAULTS, ids=[d[0] for d in DEFAULTS]
)
def test_a_short_symbol_is_the_documented_default(short, qualified, si):
    assert Q_(1, short).to(si).magnitude == Q_(1, qualified).to(si).magnitude


@pytest.mark.parametrize(
    ("one", "other", "si"),
    [
        ("cal_th", "cal_it", "J"),
        ("btu_it", "btu_th", "J"),
        ("btu_it", "btu_iso", "J"),
        ("acre_international", "acre_us_survey", "m^2"),
        ("gal_us", "gallon_imperial", "m^3"),
        ("hp_mechanical", "hp_metric", "W"),
        ("year_julian", "year_gregorian", "s"),
    ],
)
def test_the_definitions_really_differ(one, other, si):
    a = Q_(1, one).to(si).magnitude
    b = Q_(1, other).to(si).magnitude
    assert not math.isclose(a, b, rel_tol=1e-9)
