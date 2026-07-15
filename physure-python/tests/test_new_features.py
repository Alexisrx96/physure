import math

import physure as mk
from physure import Q_, propagation_mode


def test_new_units():
    # 1. CGS units
    # 100 Gal (galileo) = 1 m/s^2
    gal_q = Q_(100.0, "Gal")
    assert math.isclose(gal_q.to("m/s^2").magnitude, 1.0, rel_tol=1e-5)

    # 10000 gauss = 1 tesla
    gauss_q = Q_(10000.0, "gauss")
    assert math.isclose(gauss_q.to("T").magnitude, 1.0, rel_tol=1e-5)

    # poise (dynamic viscosity) to Pa*s
    poise_q = Q_(10.0, "poise")
    assert math.isclose(poise_q.to("Pa*s").magnitude, 1.0, rel_tol=1e-5)

    # stokes (kinematic viscosity) to m^2/s
    stokes_q = Q_(10000.0, "stokes")
    assert math.isclose(stokes_q.to("m^2/s").magnitude, 1.0, rel_tol=1e-5)

    # 2. Atomic units
    # bohr to m
    bohr_q = Q_(1.0, "bohr")
    assert math.isclose(
        bohr_q.to("m").magnitude, 5.29177210903e-11, rel_tol=1e-5
    )

    # hartree to eV
    hartree_q = Q_(1.0, "hartree")
    assert math.isclose(
        hartree_q.to("eV").magnitude, 27.211386245981, rel_tol=1e-5
    )

    # 3. Natural units
    # planck_length to m
    lp_q = Q_(1.0, "planck_length")
    assert math.isclose(lp_q.to("m").magnitude, 1.616255e-35, rel_tol=1e-5)

    # planck_time to s
    tp_q = Q_(1.0, "planck_time")
    assert math.isclose(tp_q.to("s").magnitude, 5.391247e-44, rel_tol=1e-5)


def test_constants():
    system = mk.get_current_system()

    # Check speed of light
    c = system.get_constant("speed_of_light_in_vacuum")
    assert c is not None
    assert math.isclose(c.magnitude, 299792458.0, rel_tol=1e-5)
    assert c.unit == system.get_unit("m/s")

    # Check planck constant
    h = system.get_constant("planck_constant")
    assert h is not None
    assert math.isclose(h.magnitude, 6.626070e-34, rel_tol=1e-5)
    assert h.unit == system.get_unit("J/Hz")

    # Check boltzmann constant
    k_B = system.get_constant("boltzmann_constant")
    assert k_B is not None
    assert math.isclose(k_B.magnitude, 1.380649e-23, rel_tol=1e-5)
    assert k_B.unit == system.get_unit("J/K")


def test_rust_uncertainty_modes():
    # Check that we can run in Monte Carlo mode
    with propagation_mode("monte_carlo"):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2

        # Verify that uncertainty is computed via Monte Carlo non-linearly
        # For y = x^2, with linear propagation uncertainty is 2 * 10 * 1 = 20.0
        # In Monte Carlo propagation, accept a wider range given stochasticity
        assert 18.0 < y.uncertainty < 22.0

    # Check that we can run in Unscented mode
    with propagation_mode("unscented"):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2

        # In Unscented mode, for y = x^2, the exact mean is x^2 + var = 100.0 + 1.0 = 101.0
        assert math.isclose(y.magnitude, 101.0, rel_tol=1e-5)
        # exact std_dev = sqrt(4 * x^2 * var + 2 * var^2) = sqrt(400 + 2) = sqrt(402) = 20.0499...
        assert math.isclose(y.uncertainty, math.sqrt(402.0), rel_tol=1e-5)
