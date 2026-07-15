"""End-to-end real-world physics problems solved through Physure's `Q_`.

Every problem is a textbook exercise with a hand-checkable numeric answer.
Each assertion checks BOTH the numeric result (against the published/derived
answer) AND the resulting dimension -- unit correctness is the product, so a
formula that produces the right number in the wrong unit is still a bug.
"""

import math

import pytest

from physure import Q_, get_current_system


@pytest.fixture
def system():
    return get_current_system()


# --- Ideal gas law: PV = nRT --------------------------------------------------
def test_ideal_gas_law_one_mole_at_stp_occupies_22_414_liters(system):
    # Textbook fact: 1 mol of ideal gas at STP (273.15 K, 101325 Pa)
    # occupies 22.414 L (molar_volume_of_ideal_gas_27315_k_101325_kpa in
    # physure.conf independently confirms 0.0224140 m^3).
    n = Q_(1.0, "mol")
    r = system.get_constant("molar_gas_constant")
    t = Q_(273.15, "K")
    p = Q_(101325.0, "Pa")

    v = (n * r * t / p).to("L")
    assert math.isclose(v.magnitude, 22.414, rel_tol=1e-4)
    assert v.unit == system.get_unit("L")


def test_ideal_gas_law_solves_for_pressure_of_compressed_air_tank():
    # 0.5 mol of gas in a 2 L tank at 300 K -> P = nRT/V.
    n = Q_(0.5, "mol")
    r = Q_(8.31446261815324, "J/(mol*K)")
    t = Q_(300.0, "K")
    v = Q_(2.0, "L")

    p = (n * r * t / v).to("kPa")
    assert math.isclose(p.magnitude, 623.585, rel_tol=1e-4)
    assert p.unit == get_current_system().get_unit("kPa")


# --- Kinematics ----------------------------------------------------------------
def test_free_fall_distance_after_2_seconds():
    # s = 1/2 * g * t^2. Dropping an object for 2 s on Earth: s = 19.6133 m
    # (standard textbook free-fall problem, g = 9.80665 m/s^2 by definition).
    g = Q_(9.80665, "m/s^2")
    t = Q_(2.0, "s")
    s = 0.5 * g * t**2
    assert math.isclose(s.to("m").magnitude, 19.6133, rel_tol=1e-6)
    assert s.unit.exponents == {"m": 1}


def test_final_velocity_of_object_dropped_for_3_seconds():
    # v = u + a*t, u = 0. v = 9.80665 * 3 = 29.41995 m/s.
    u = Q_(0.0, "m/s")
    a = Q_(9.80665, "m/s^2")
    t = Q_(3.0, "s")
    v = u + a * t
    assert math.isclose(v.to("m/s").magnitude, 29.41995, rel_tol=1e-6)
    assert v.unit.exponents == {"m": 1, "s": -1}


def test_car_braking_distance_from_100_kmh_to_stop():
    # v^2 = u^2 + 2*a*s -> s = -u^2 / (2a). u=100 km/h=27.7778 m/s,
    # a = -8 m/s^2 (typical hard-braking deceleration) -> s ~ 48.23 m.
    u = Q_(100.0, "km/h").to("m/s")
    a = Q_(-8.0, "m/s^2")
    s = -(u**2) / (2 * a)
    assert math.isclose(s.to("m").magnitude, 48.225, rel_tol=1e-3)


# --- Newton's laws --------------------------------------------------------------
def test_newtons_second_law_force_on_accelerating_mass():
    # F = m*a. A 1200 kg car accelerating at 3 m/s^2 needs 3600 N.
    m = Q_(1200.0, "kg")
    a = Q_(3.0, "m/s^2")
    f = m * a
    assert math.isclose(f.to("N").magnitude, 3600.0, rel_tol=1e-9)
    assert f.unit.exponents == {"kg": 1, "m": 1, "s": -2}


def test_weight_of_70_kg_person_on_earth():
    # W = m*g. A 70 kg person weighs 686.4655 N (~154.3 lbf).
    m = Q_(70.0, "kg")
    g = Q_(9.80665, "m/s^2")
    w = m * g
    assert math.isclose(w.to("N").magnitude, 686.4655, rel_tol=1e-9)
    assert math.isclose(w.to("lbf").magnitude, 154.324, rel_tol=1e-3)


# --- Energy ----------------------------------------------------------------------
def test_kinetic_energy_of_a_car_at_highway_speed():
    # KE = 1/2 m v^2. 1500 kg car at 30 m/s (~108 km/h) -> 675000 J.
    m = Q_(1500.0, "kg")
    v = Q_(30.0, "m/s")
    ke = 0.5 * m * v**2
    assert math.isclose(ke.to("J").magnitude, 675000.0, rel_tol=1e-9)
    assert ke.unit.exponents == {"kg": 1, "m": 2, "s": -2}


def test_potential_energy_of_object_raised_10_meters():
    # PE = mgh. 5 kg raised 10 m -> 490.3325 J.
    m = Q_(5.0, "kg")
    g = Q_(9.80665, "m/s^2")
    h = Q_(10.0, "m")
    pe = m * g * h
    assert math.isclose(pe.to("J").magnitude, 490.3325, rel_tol=1e-9)


def test_mass_energy_equivalence_one_gram_matches_hiroshima_scale_energy():
    # E = m*c^2. 1 g of mass-energy conversion releases 8.98755e13 J
    # (~21.5 kilotons TNT equivalent -- the textbook E=mc^2 example).
    m = Q_(1.0, "g")
    c = get_current_system().get_constant("speed_of_light_in_vacuum")
    e = m * c**2
    assert math.isclose(e.to("J").magnitude, 8.987551787e13, rel_tol=1e-6)
    assert e.to("J").unit.exponents == {"kg": 1, "m": 2, "s": -2}


# --- Electricity: Ohm's law and power --------------------------------------------
def test_ohms_law_voltage_across_resistor():
    # V = I*R. 2 A through a 5 ohm resistor -> 10 V (household-circuit scale).
    i = Q_(2.0, "A")
    r = Q_(5.0, "ohm")
    v = i * r
    assert math.isclose(v.to("V").magnitude, 10.0, rel_tol=1e-9)
    assert v.to("V").unit.exponents == {"kg": 1, "m": 2, "s": -3, "A": -1}


def test_dimensionless_coefficient_does_not_pollute_resulting_unit():
    # Regression: Q_(x, "1") ("unity", physure.conf's documented idiom
    # for a dimensionless coefficient) used to register as the atomic
    # {"1": 1} instead of the truly-empty CompoundUnit({}), because
    # "unity" has no explicit recipe string in its .conf entry -- it fell
    # through to the atomic-unit branch. That "1" key then survived
    # multiplication (kinetic-friction coefficient * normal force * a
    # distance, mirroring Sears & Zemansky ch.6's work-energy example)
    # and made the resulting Joules compare unequal to a clean Joule unit.
    system = get_current_system()
    mu_k = Q_(0.3, "1")
    normal_force = Q_(19.6, "N")
    friction_work = -(mu_k * normal_force) * Q_(2.5, "m")
    assert math.isclose(friction_work.to("J").magnitude, -14.7, rel_tol=1e-9)
    assert friction_work.unit == system.get_unit("J")


def test_ohm_alias_is_dimensionally_equal_to_its_canonical_symbol():
    # Regression: get_unit("ohm") used to return the atomic {"ohm": 1}
    # form instead of the SI-decomposed recipe that get_unit("Ohm")
    # (the canonical alias from physure.conf's [Ohm, ohm, ohms, Ω])
    # correctly returned. That made a resistance arrived at via V / A
    # arithmetic compare unequal to Q_(x, "ohm").unit despite being the
    # same physical unit -- .to() always gave the right number, only the
    # unit-object equality was broken.
    system = get_current_system()
    assert system.get_unit("ohm") == system.get_unit("Ohm")
    assert system.get_unit("ohm").exponents == system.get_unit("Ohm").exponents

    i = Q_(2.0, "A")
    v = Q_(10.0, "V")
    r = (v / i).to("ohm")
    assert r.unit == system.get_unit("ohm")


def test_electrical_power_dissipated_in_a_household_appliance():
    # P = I*V. A 120 V, 5 A appliance (e.g. a toaster) draws 600 W.
    i = Q_(5.0, "A")
    v = Q_(120.0, "V")
    p = i * v
    assert math.isclose(p.to("W").magnitude, 600.0, rel_tol=1e-9)


# --- Gravitation -----------------------------------------------------------------
def test_newtons_law_of_gravitation_earth_surface_recovers_g():
    # F/m = G*M_earth/R_earth^2 should reproduce standard gravity (~9.80665 m/s^2),
    # cross-checking G, Earth's mass, and Earth's radius against a real result.
    system = get_current_system()
    g_const = system.get_constant("newtonian_constant_of_gravitation")
    m_earth = Q_(5.972e24, "kg")  # published mean Earth mass (NASA fact sheet)
    r_earth = Q_(6.371e6, "m")  # published mean Earth radius (NASA fact sheet)
    surface_g = g_const * m_earth / r_earth**2
    assert math.isclose(surface_g.to("m/s^2").magnitude, 9.80665, rel_tol=2e-3)


def test_gravitational_force_between_two_1kg_masses_at_1_meter():
    # F = G*m1*m2/r^2 with m1=m2=1 kg, r=1 m -> F = G = 6.674e-11 N
    # (the classic "how weak is gravity" demonstration).
    system = get_current_system()
    g_const = system.get_constant("newtonian_constant_of_gravitation")
    m1 = Q_(1.0, "kg")
    m2 = Q_(1.0, "kg")
    r = Q_(1.0, "m")
    f = g_const * m1 * m2 / r**2
    assert math.isclose(f.to("N").magnitude, 6.6743e-11, rel_tol=1e-4)


# --- Photon energy and the wave equation -----------------------------------------
def test_photon_energy_of_green_light():
    # E = h*f. Green light ~ 5.45e14 Hz -> E ~ 3.61e-19 J ~ 2.256 eV
    # (matches the well-known visible-spectrum photon-energy range of 1.6-3.3 eV).
    system = get_current_system()
    h = system.get_constant("planck_constant")
    f = Q_(5.45e14, "Hz")
    e = h * f
    assert math.isclose(e.to("J").magnitude, 3.6105e-19, rel_tol=1e-3)
    assert math.isclose(e.to("eV").magnitude, 2.253, rel_tol=1e-3)


def test_wave_equation_speed_equals_frequency_times_wavelength():
    # c = f*lambda. Red laser: lambda=650 nm -> f = c/lambda ~ 4.6122e14 Hz.
    system = get_current_system()
    c = system.get_constant("speed_of_light_in_vacuum")
    wavelength = Q_(650.0, "nm")
    f = (c / wavelength).to("Hz")
    assert math.isclose(f.magnitude, 4.6122e14, rel_tol=1e-4)


def test_fm_radio_wavelength_from_known_broadcast_frequency():
    # A US FM station at 100.0 MHz has wavelength lambda = c/f ~ 2.998 m.
    system = get_current_system()
    c = system.get_constant("speed_of_light_in_vacuum")
    f = Q_(100.0, "MHz")
    wavelength = (c / f).to("m")
    assert math.isclose(wavelength.magnitude, 2.998, rel_tol=1e-4)
