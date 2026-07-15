"""Real-world unit-conversion checks across every unit system MeasureKit ships.

Every expected value below is a published, independently verifiable conversion
factor (NIST SP 811 / SP 330, exact SI-2019 definitions, or the international
yard-and-pound agreement of 1959). Exact-by-definition factors are checked
bit-for-bit (`==`); measured/rounded factors use `math.isclose` at the
precision the .conf table stores them.
"""

import math

import pytest

from physure import Q_

# --- Imperial / US customary length -----------------------------------------
# International yard and pound agreement (1959): 1 yd = 0.9144 m exactly,
# from which 1 ft = 0.3048 m and 1 in = 0.0254 m follow exactly.
EXACT_LENGTH = [
    ("in", "m", 0.0254),
    ("ft", "m", 0.3048),
    ("yd", "m", 0.9144),
    ("mi", "m", 1609.344),  # 1 mi = 5280 ft exactly
]


@pytest.mark.parametrize(("unit", "target", "expected"), EXACT_LENGTH)
def test_imperial_length_exact_factors(unit, target, expected):
    assert Q_(1.0, unit).to(target).magnitude == expected


# Exact integer chains within the imperial system itself.
EXACT_LENGTH_CHAINS = [
    ("ft", "in", 12.0),
    ("mi", "ft", 5280.0),
    ("mi", "yd", 1760.0),
    ("yd", "ft", 3.0),
]


@pytest.mark.parametrize(("unit", "target", "expected"), EXACT_LENGTH_CHAINS)
def test_imperial_length_exact_chains(unit, target, expected):
    assert Q_(1.0, unit).to(target).magnitude == expected


# --- Mass --------------------------------------------------------------------
# 1959 agreement: 1 lb = 0.45359237 kg exactly.
def test_pound_to_kilogram_exact():
    assert Q_(1.0, "lb").to("kg").magnitude == 0.45359237


def test_pound_to_ounce_exact_chain():
    assert Q_(1.0, "lb").to("oz").magnitude == 16.0


def test_ounce_to_kilogram_exact():
    # oz = lb / 16 = 0.45359237 / 16
    assert math.isclose(
        Q_(1.0, "oz").to("kg").magnitude, 0.45359237 / 16, rel_tol=1e-12
    )


def test_slug_to_kilogram_matches_published_value():
    # 1 slug = 1 lbf / (1 ft/s^2) = 14.5939 kg (NIST SP 811 Appendix B).
    assert math.isclose(
        Q_(1.0, "slug").to("kg").magnitude, 14.5939, rel_tol=1e-6
    )


# --- Volume / area -------------------------------------------------------------
def test_us_gallon_to_liter_matches_published_value():
    # US liquid gallon = 231 in^3 = 3.785411784 L exactly (federal register 1893/1959);
    # the .conf stores it rounded to 6 sig figs.
    assert math.isclose(
        Q_(1.0, "gal").to("L").magnitude, 3.78541, rel_tol=1e-6
    )


def test_fluid_ounce_to_liter_matches_published_value():
    # 1 US fl oz = 29.5735 mL = 0.0295735 L (NIST handbook 44).
    assert math.isclose(
        Q_(1.0, "fl_oz").to("L").magnitude, 0.0295735, rel_tol=1e-6
    )


def test_hectare_to_square_meter_exact():
    assert Q_(1.0, "ha").to("m^2").magnitude == 10000.0


def test_acre_to_square_meter_matches_published_value():
    # 1 acre = 4046.8564224 m^2 (US survey-independent international acre).
    assert math.isclose(
        Q_(1.0, "acre").to("m^2").magnitude, 4046.856, rel_tol=1e-6
    )


def test_acre_is_about_43560_square_feet():
    # Textbook fact: 1 acre = 43560 ft^2, to the precision the stored acre allows.
    acre_ft2 = Q_(1.0, "acre").to("m^2").magnitude / (
        Q_(1.0, "ft").to("m").magnitude ** 2
    )
    assert math.isclose(acre_ft2, 43560.0, rel_tol=1e-6)


# --- Astronomical --------------------------------------------------------------
def test_light_year_matches_published_value():
    # IAU: 1 ly = 9.4607e15 m (Julian year * c).
    assert math.isclose(
        Q_(1.0, "ly").to("m").magnitude, 9.4607e15, rel_tol=1e-5
    )


def test_parsec_matches_published_value():
    # IAU 2015: 1 pc = 3.0857e16 m.
    assert math.isclose(
        Q_(1.0, "pc").to("m").magnitude, 3.0857e16, rel_tol=1e-5
    )


def test_parsec_is_about_3_26_light_years():
    # Well-known astronomy fact used as a cross-check between two units.
    ratio = Q_(1.0, "pc").to("m").magnitude / Q_(1.0, "ly").to("m").magnitude
    assert math.isclose(ratio, 3.2616, rel_tol=1e-3)


# --- Energy ---------------------------------------------------------------------
def test_electron_volt_to_joule_exact():
    # SI-2019: eV is defined from the now-exact elementary charge.
    assert Q_(1.0, "eV").to("J").magnitude == 1.602176634e-19


def test_calorie_to_joule_exact():
    # Thermochemical calorie, exact by definition since 1948 (IT calorie): 4.184 J.
    assert Q_(1.0, "cal").to("J").magnitude == 4.184


def test_btu_to_joule_matches_published_value():
    # International Table BTU = 1055.05585262 J; .conf stores 1055.06 (6 sig figs).
    assert math.isclose(
        Q_(1.0, "BTU").to("J").magnitude, 1055.06, rel_tol=1e-6
    )


def test_erg_to_joule_exact():
    assert Q_(1.0, "erg").to("J").magnitude == 1e-7


def test_watt_hour_to_joule_exact():
    assert Q_(1.0, "Wh").to("J").magnitude == 3600.0


def test_kilowatt_hour_to_joule_exact():
    assert Q_(1.0, "kWh").to("J").magnitude == 3.6e6


# --- Pressure --------------------------------------------------------------------
def test_standard_atmosphere_to_pascal_exact():
    # CIPM 1954 definition: 1 atm = 101325 Pa exactly.
    assert Q_(1.0, "atm").to("Pa").magnitude == 101325.0


def test_bar_to_pascal_exact():
    assert Q_(1.0, "bar").to("Pa").magnitude == 100000.0


def test_psi_to_pascal_matches_published_value():
    # 1 psi = 1 lbf / in^2 = 6894.757 Pa (NIST SP 811).
    assert math.isclose(
        Q_(1.0, "psi").to("Pa").magnitude, 6894.757, rel_tol=1e-6
    )


def test_torr_to_pascal_matches_published_value():
    # 1 torr = 101325/760 Pa = 133.3223684...
    assert math.isclose(
        Q_(1.0, "torr").to("Pa").magnitude, 101325.0 / 760.0, rel_tol=1e-5
    )


def test_mmhg_to_pascal_matches_published_value():
    assert math.isclose(
        Q_(1.0, "mmHg").to("Pa").magnitude, 133.322, rel_tol=1e-6
    )


def test_standard_atmosphere_is_about_760_mmhg():
    ratio = (
        Q_(1.0, "atm").to("Pa").magnitude / Q_(1.0, "mmHg").to("Pa").magnitude
    )
    assert math.isclose(ratio, 760.0, rel_tol=1e-4)


# --- Power -----------------------------------------------------------------------
def test_horsepower_to_watt_matches_published_value():
    # Mechanical horsepower = 550 ft*lbf/s = 745.6998715822702 W (exact from ft, lbf).
    assert math.isclose(Q_(1.0, "hp").to("W").magnitude, 745.7, rel_tol=1e-5)


# --- CGS system ---------------------------------------------------------------
def test_dyne_to_newton_exact():
    assert Q_(1.0, "dyn").to("N").magnitude == 1e-5


def test_erg_to_joule_cgs_exact():
    assert Q_(1.0, "erg").to("J").magnitude == 1e-7


def test_poise_to_pascal_second_exact():
    # Water's viscosity at 20 C is ~1 cP = 0.01 P -- sanity-anchors the unit.
    assert Q_(1.0, "P").to("Pa*s").magnitude == 0.1


def test_stokes_to_square_meter_per_second_exact():
    assert Q_(1.0, "St").to("m^2/s").magnitude == 1e-4


def test_gauss_to_tesla_exact():
    # Earth's magnetic field is ~0.5 G = 5e-5 T -- this factor anchors that fact.
    assert Q_(1.0, "G").to("T").magnitude == 1e-4


def test_kayser_to_inverse_meter_exact():
    assert Q_(1.0, "kayser").to("1/m").magnitude == 100.0


# --- Atomic units ------------------------------------------------------------
def test_bohr_radius_to_meter_matches_published_value():
    assert math.isclose(
        Q_(1.0, "bohr").to("m").magnitude, 5.29177210903e-11, rel_tol=1e-9
    )


def test_hartree_to_joule_matches_published_value():
    assert math.isclose(
        Q_(1.0, "Eh").to("J").magnitude, 4.3597447222071e-18, rel_tol=1e-9
    )


# --- Temperature (affine, not multiplicative) ---------------------------------
CELSIUS_TO_KELVIN = [
    (0.0, 273.15),
    (100.0, 373.15),
    (-40.0, 233.15),
    (37.0, 310.15),  # human body temperature
    (20.0, 293.15),  # standard room temperature (NIST/IUPAC)
]


@pytest.mark.parametrize(("celsius", "kelvin"), CELSIUS_TO_KELVIN)
def test_celsius_to_kelvin_affine_conversion(celsius, kelvin):
    assert math.isclose(
        Q_(celsius, "degC").to("K").magnitude, kelvin, rel_tol=1e-9
    )


FAHRENHEIT_TO_KELVIN = [
    (32.0, 273.15),  # freezing point of water
    (212.0, 373.15),  # boiling point of water
    (98.6, 310.15),  # human body temperature (approx.)
    (-40.0, 233.15),  # the crossover point, same number in both scales
]


@pytest.mark.parametrize(("fahrenheit", "kelvin"), FAHRENHEIT_TO_KELVIN)
def test_fahrenheit_to_kelvin_affine_conversion(fahrenheit, kelvin):
    assert math.isclose(
        Q_(fahrenheit, "degF").to("K").magnitude, kelvin, rel_tol=1e-3
    )


def test_negative_40_is_the_celsius_fahrenheit_crossover():
    c = Q_(-40.0, "degC").to("degF").magnitude
    assert math.isclose(c, -40.0, rel_tol=1e-9)


# --- Round-trip invariants -----------------------------------------------------
ROUND_TRIP_PAIRS = [
    (12.34, "m", "ft"),
    (98.6, "lb", "kg"),
    (2.5, "atm", "psi"),
    (310.15, "K", "degF"),
    (5.0, "gal", "L"),
    (42.0, "eV", "J"),
]


@pytest.mark.parametrize(("value", "unit", "other"), ROUND_TRIP_PAIRS)
def test_round_trip_conversion_recovers_original_value(value, unit, other):
    round_tripped = Q_(value, unit).to(other).to(unit).magnitude
    assert math.isclose(round_tripped, value, rel_tol=1e-9)
