"""Uncertainty propagation checked against GUM / textbook worked examples.

Reference: JCGM 100:2008 "Evaluation of measurement data -- Guide to the
expression of uncertainty in measurement" (GUM), and standard error-propagation
formulas from any intro metrology/physics-lab textbook (e.g. Taylor,
"An Introduction to Error Analysis").

`uncertainty=` on `Q_` uses linear (first-order Taylor / GUM Section 5)
propagation by default -- the same rules a textbook lab report uses.
"""

import math

from physure import Q_, PhysureContext


# --- Sum/difference: absolute uncertainties add in quadrature (GUM 5.1.2) -----
def test_sum_of_two_measurements_adds_uncertainty_in_quadrature():
    # x = 10.0 +/- 0.2, y = 5.0 +/- 0.3 -> x+y = 15.0 +/- sqrt(0.2^2+0.3^2)
    x = Q_(10.0, "m", uncertainty=0.2)
    y = Q_(5.0, "m", uncertainty=0.3)
    z = x + y
    assert math.isclose(z.magnitude, 15.0)
    assert math.isclose(
        z.uncertainty, math.sqrt(0.2**2 + 0.3**2), rel_tol=1e-9
    )


def test_difference_of_two_measurements_adds_uncertainty_in_quadrature():
    # Same quadrature rule applies to subtraction (GUM treats +/- identically
    # for independent inputs since only the squared sensitivity matters).
    x = Q_(10.0, "m", uncertainty=0.2)
    y = Q_(5.0, "m", uncertainty=0.3)
    z = x - y
    assert math.isclose(z.magnitude, 5.0)
    assert math.isclose(
        z.uncertainty, math.sqrt(0.2**2 + 0.3**2), rel_tol=1e-9
    )


# --- Product/quotient: relative uncertainties add in quadrature (GUM 5.1.3) ---
def test_product_of_two_measurements_adds_relative_uncertainty_in_quadrature():
    # length = 4.0 +/- 0.1 m, width = 3.0 +/- 0.05 m -> area = 12.0 m^2
    # rel_unc = sqrt((0.1/4.0)^2 + (0.05/3.0)^2)
    length = Q_(4.0, "m", uncertainty=0.1)
    width = Q_(3.0, "m", uncertainty=0.05)
    area = length * width
    assert math.isclose(area.magnitude, 12.0)
    rel_unc = math.sqrt((0.1 / 4.0) ** 2 + (0.05 / 3.0) ** 2)
    assert math.isclose(
        area.uncertainty, area.magnitude * rel_unc, rel_tol=1e-9
    )


def test_quotient_of_two_measurements_adds_relative_uncertainty_in_quadrature():
    # speed = distance/time. d = 100.0 +/- 2.0 m, t = 9.58 +/- 0.02 s
    # (Bolt's 100 m world record, with plausible timing-error bars).
    distance = Q_(100.0, "m", uncertainty=2.0)
    time = Q_(9.58, "s", uncertainty=0.02)
    speed = distance / time
    assert math.isclose(speed.magnitude, 100.0 / 9.58, rel_tol=1e-9)
    rel_unc = math.sqrt((2.0 / 100.0) ** 2 + (0.02 / 9.58) ** 2)
    assert math.isclose(
        speed.uncertainty, speed.magnitude * rel_unc, rel_tol=1e-9
    )


# --- Power law: rel(y) = |n| * rel(x) (GUM 5.1.3, single-variable case) -------
def test_power_law_scales_relative_uncertainty_by_the_exponent():
    # Circle area A = pi*r^2. r = 2.0 +/- 0.05 cm -> rel(A) = 2 * rel(r).
    r = Q_(2.0, "cm", uncertainty=0.05)
    area = math.pi * r**2
    rel_r = 0.05 / 2.0
    expected_rel_area = 2 * rel_r
    assert math.isclose(area.magnitude, math.pi * 4.0, rel_tol=1e-9)
    assert math.isclose(
        area.uncertainty / area.magnitude, expected_rel_area, rel_tol=1e-9
    )


def test_cube_power_law_scales_relative_uncertainty_by_three():
    # Sphere volume V = 4/3 pi r^3. r = 1.0 +/- 0.01 m -> rel(V) = 3 * rel(r).
    r = Q_(1.0, "m", uncertainty=0.01)
    volume = (4.0 / 3.0) * math.pi * r**3
    expected_rel_volume = 3 * (0.01 / 1.0)
    assert math.isclose(
        volume.uncertainty / volume.magnitude,
        expected_rel_volume,
        rel_tol=1e-9,
    )


# --- Multi-variable textbook example: Ohm's law R = V/I -----------------------
def test_ohms_law_multivariable_uncertainty_matches_gum_worked_example():
    # Classic GUM-style lab example: V = 6.00 +/- 0.02 V, I = 2.00 +/- 0.01 A.
    # R = V/I = 3.00 ohm; combined relative uncertainty via quadrature of the
    # two independent relative uncertainties (GUM Eq. 10, uncorrelated inputs).
    v = Q_(6.00, "V", uncertainty=0.02)
    i = Q_(2.00, "A", uncertainty=0.01)
    r = v / i
    assert math.isclose(r.to("ohm").magnitude, 3.00, rel_tol=1e-9)
    rel_unc = math.sqrt((0.02 / 6.00) ** 2 + (0.01 / 2.00) ** 2)
    expected_abs_unc = 3.00 * rel_unc
    assert math.isclose(r.uncertainty, expected_abs_unc, rel_tol=1e-9)


def test_density_from_mass_and_volume_matches_gum_worked_example():
    # rho = m/V. m = 25.0 +/- 0.1 g, V = 3.20 +/- 0.05 cm^3 -> rho ~ 7.8125 g/cm^3
    # (roughly the textbook "identify the metal" density-of-a-cylinder problem).
    m = Q_(25.0, "g", uncertainty=0.1)
    v = Q_(3.20, "cm^3", uncertainty=0.05)
    rho = m / v
    assert math.isclose(rho.magnitude, 25.0 / 3.20, rel_tol=1e-9)
    rel_unc = math.sqrt((0.1 / 25.0) ** 2 + (0.05 / 3.20) ** 2)
    assert math.isclose(rho.uncertainty, rho.magnitude * rel_unc, rel_tol=1e-9)


# --- Correlated vs uncorrelated: x - x --------------------------------------
# Outside an active covariance store, MeasureKit treats every `Q_(...,
# uncertainty=...)` as an independent noise source (VarianceModel), so `x - x`
# adds the uncertainty in quadrature even though the exact answer is 0 +/- 0.
# Inside `PhysureContext()`, scalar uncertainties are lineage-tracked
# (CovarianceModel), so the engine recognizes `x` correlated with itself and
# the propagated uncertainty on `x - x` collapses to exactly zero -- the
# textbook-correct answer for a perfectly correlated difference.
def test_x_minus_x_has_nonzero_uncertainty_without_correlation_tracking():
    x = Q_(10.0, "m", uncertainty=1.0)
    y = x - x
    assert math.isclose(y.magnitude, 0.0, abs_tol=1e-12)
    assert math.isclose(
        y.uncertainty, math.sqrt(1.0**2 + 1.0**2), rel_tol=1e-9
    )


def test_x_minus_x_has_zero_uncertainty_with_correlation_tracking():
    with PhysureContext():
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x - x
        assert math.isclose(y.magnitude, 0.0, abs_tol=1e-12)
        assert math.isclose(y.uncertainty, 0.0, abs_tol=1e-12)


def test_two_x_minus_two_x_has_zero_uncertainty_with_correlation_tracking():
    # A linear combination of a single correlated source (2x - 2x) must also
    # cancel exactly -- proves lineage tracking, not just a special-cased x-x.
    with PhysureContext():
        x = Q_(10.0, "m", uncertainty=1.0)
        w = 2.0 * x - 2.0 * x
        assert math.isclose(w.magnitude, 0.0, abs_tol=1e-12)
        assert math.isclose(w.uncertainty, 0.0, abs_tol=1e-12)
