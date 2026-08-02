"""Uncertainty propagation checked against GUM / textbook worked examples.

Reference: JCGM 100:2008 "Evaluation of measurement data -- Guide to the
expression of uncertainty in measurement" (GUM), and standard error-propagation
formulas from any intro metrology/physics-lab textbook (e.g. Taylor,
"An Introduction to Error Analysis").

`uncertainty=` on `Q_` uses linear (first-order Taylor / GUM Section 5)
propagation by default -- the same rules a textbook lab report uses.
"""

import math

import pytest

from physure import (
    Q_,
    PhysureContext,
    propagation_mode,
    python_lineage,
)


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
# Scalar uncertainties are lineage-tracked (CovarianceModel) whether or not a
# covariance store is active, matching the Rust core. The engine therefore
# recognizes `x` as correlated with itself and `x - x` collapses to exactly
# zero -- the textbook-correct answer for a perfectly correlated difference.
# Two *separate* measurements of the same value stay independent and still add
# in quadrature, which is what keeps this from being a special case for the
# literal `x - x` shape.
def test_x_minus_x_is_zero_without_an_explicit_context():
    x = Q_(10.0, "m", uncertainty=1.0)
    y = x - x
    assert math.isclose(y.magnitude, 0.0, abs_tol=1e-12)
    assert math.isclose(y.uncertainty, 0.0, abs_tol=1e-12)


def test_uncorrelated_mode_opts_out_of_provenance_tracking():
    # The escape hatch: every input is treated as its own noise source again,
    # so a quantity is independent of itself and x - x adds in quadrature.
    with propagation_mode("uncorrelated"):
        x = Q_(10.0, "m", uncertainty=1.0)
        assert math.isclose((x - x).uncertainty, math.sqrt(2.0), rel_tol=1e-9)
        # Genuinely independent inputs are unaffected by the mode.
        y = Q_(4.0, "m", uncertainty=1.0)
        assert math.isclose((x - y).uncertainty, math.sqrt(2.0), rel_tol=1e-9)


def test_two_measurements_of_the_same_value_still_add_in_quadrature():
    a = Q_(10.0, "m", uncertainty=1.0)
    b = Q_(10.0, "m", uncertainty=1.0)
    d = a - b
    assert math.isclose(d.magnitude, 0.0, abs_tol=1e-12)
    assert math.isclose(
        d.uncertainty, math.sqrt(1.0**2 + 1.0**2), rel_tol=1e-9
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


# --- Provenance lives in the Rust core --------------------------------------
def test_scalar_provenance_is_delegated_to_the_native_core():
    # Python is a thin wrapper here: the merge itself must be the core's, so PHS,
    # the Rust API and Python cannot drift apart on what `x - x` is.
    from physure._core import Lineage
    from physure.domain.measurement.uncertainty import LineageModel

    model = Q_(10.0, "m", uncertainty=1.0)._uncertainty_obj
    assert isinstance(model, LineageModel)
    assert isinstance(model.native, Lineage)


def test_a_traced_uncertainty_refuses_rather_than_answering_differently():
    # A live torch tensor cannot become the f64 the core keys on, and converting it
    # would cut it out of its autograd graph. Refusing names the opt-in instead of
    # quietly answering from the other implementation.
    torch = pytest.importorskip("torch")
    from physure.domain.measurement.uncertainty import (
        CovarianceModel,
        Uncertainty,
        VarianceModel,
    )

    live = torch.tensor(0.1, requires_grad=True)
    with pytest.raises(TypeError, match="python_lineage"):
        Uncertainty.from_standard(live)

    with python_lineage():
        assert isinstance(Uncertainty.from_standard(live), CovarianceModel)

    # KNOWN GAP: a concrete tensor has no graph to lose, but it still goes through
    # the array machinery, which keeps its tensor dtype and drops provenance. So a
    # torch scalar is the one place left where `x - x` disagrees with the core.
    # Routing it to the core would return a plain float instead of a tensor, which
    # is a deliberate call, not something to change silently.
    plain = torch.tensor(0.1)
    assert isinstance(Uncertainty.from_standard(plain), VarianceModel)
    q = Q_(torch.tensor(3.0), "m", uncertainty=plain)
    assert float((q - q).uncertainty) > 0.0


def test_jax_under_jit_agrees_with_the_core_once_opted_in():
    # This is the disparity the opt-in exists to surface: before, tracing silently
    # dropped provenance and `x - x` came back as sqrt(2) under `jit` while every
    # other environment said 0.
    jax = pytest.importorskip("jax")

    @jax.jit
    def x_minus_x(value, sigma):
        x = Q_(value, "m", uncertainty=sigma)
        return (x - x).uncertainty

    with pytest.raises(TypeError, match="python_lineage"):
        x_minus_x(10.0, 1.0)

    with python_lineage():
        assert math.isclose(float(x_minus_x(10.0, 1.0)), 0.0, abs_tol=1e-9)


# --- Model selection ---------------------------------------------------------
def test_the_uncertainty_model_is_chosen_per_scope_and_restored_after():
    import physure

    assert physure.get_uncertainty_model() == "gaussian"
    with physure.uncertainty_model("moments"):
        assert physure.get_uncertainty_model() == "moments"
    assert physure.get_uncertainty_model() == "gaussian"


def test_a_misspelled_model_is_rejected_instead_of_leaving_the_old_one():
    # Silently keeping "gaussian" here would report symmetric answers inside a
    # block entered to say the measurement is not symmetric.
    import physure

    with (
        pytest.raises(ValueError, match="Unknown uncertainty model"),
        physure.uncertainty_model("gausian"),
    ):
        pass


def test_the_moments_model_refuses_rather_than_building_a_gaussian():
    # The core can convert a (sigma-, sigma+) pair to moments and back, but
    # nothing propagates them yet, and Python has no model over them at all.
    import physure

    with (
        physure.uncertainty_model("moments"),
        pytest.raises(NotImplementedError, match="not implemented yet"),
    ):
        Q_(12.3, "pb", uncertainty=0.4)
