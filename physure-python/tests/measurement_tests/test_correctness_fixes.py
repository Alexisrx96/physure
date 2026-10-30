"""Regression tests for the correctness batch.

Covers: angle handling in transcendental functions, Jacobian-based
uncertainty propagation through unit conversions (incl. levels),
and the removal of the mul/div inference heuristic.
"""

import math

import pytest

from physure import Q_
from physure.domain.exceptions import DimensionError
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.uncertainty import VarianceModel


class TestTranscendentalAngleHandling:
    def test_sin_of_degrees_converts_to_radians(self):
        assert Q_(90, "deg").sin().magnitude == pytest.approx(1.0)

    def test_cos_of_degrees(self):
        assert Q_(180, "deg").cos().magnitude == pytest.approx(-1.0)

    def test_sin_of_radians_unchanged(self):
        q = Q_(math.pi / 2, "rad").sin()
        assert q.magnitude == pytest.approx(1.0)

    def test_sin_of_dimensionless_unchanged(self):
        assert Q_(0.5, "1").sin().magnitude == pytest.approx(math.sin(0.5))

    def test_sin_rejects_non_angle_units(self):
        with pytest.raises(DimensionError, match="dimensionless or an angle"):
            Q_(5, "m").sin()

    def test_exp_rejects_non_angle_units(self):
        with pytest.raises(DimensionError):
            Q_(2, "kg").exp()

    def test_uncertainty_rescaled_by_angle_conversion(self):
        # d(sin)/dx at 0 rad is 1, so u_out = u_deg * (pi/180)
        q = Q_(0.0, "deg", uncertainty=1.0).sin()
        assert q.uncertainty == pytest.approx(math.pi / 180)


class TestLevelConversionSlopes:
    def test_a_scale_has_a_constant_slope(self):
        converted = Q_(5.0, "km", uncertainty=0.1).to("m")
        assert converted.magnitude == 5000.0
        assert converted.uncertainty == pytest.approx(100.0)

    def test_a_level_slope_matches_the_numeric_derivative(self):
        h = 1e-6
        level = Q_(30.0, "dB", uncertainty=1.0).to("unity")  # 30 dB: 1000x
        assert level.magnitude == pytest.approx(1000.0)
        numeric = (
            Q_(30.0 + h, "dB").to("unity").magnitude
            - Q_(30.0 - h, "dB").to("unity").magnitude
        ) / (2 * h)
        assert level.uncertainty == pytest.approx(abs(numeric), rel=1e-6)

    def test_a_ratio_to_a_level_slope_matches_the_numeric_derivative(self):
        step = 1e-3
        level = Q_(1000.0, "unity", uncertainty=1.0).to("dB")
        assert level.magnitude == pytest.approx(30.0)
        numeric = (
            Q_(1000.0 + step, "unity").to("dB").magnitude
            - Q_(1000.0 - step, "unity").to("dB").magnitude
        ) / (2 * step)
        assert level.uncertainty == pytest.approx(abs(numeric), rel=1e-6)

    def test_a_scalar_level_needs_no_numpy(self):
        # math path for scalars (numpy is an optional extra)
        assert Q_(1000.0, "unity").to("dB").magnitude == pytest.approx(30.0)

    def test_an_array_of_levels_converts_elementwise(self):
        np = pytest.importorskip("numpy")
        levels = Q_(np.array([10.0, 100.0, 1000.0]), "unity").to("dB")
        assert levels.magnitude == pytest.approx([10.0, 20.0, 30.0])


class TestLogUnitUncertaintyPropagation:
    def test_db_to_linear_propagates_jacobian(self, system):
        power_dim = Dimension({"M": 1, "L": 2, "T": -3})
        system.register_unit("W", power_dim, 1.0, "watt")
        system.register_unit(
            "dBW",
            power_dim,
            name="decibel-watt",
            level=(10.0, 1.0),
            allow_prefixes=False,
        )

        q = system.Q_(30.0, "dBW", uncertainty=1.0)
        w = q.to("W")

        assert w.magnitude == pytest.approx(1000.0)
        # dP/d(dB) = P * ln(10) / 10
        expected = 1000.0 * math.log(10) / 10.0
        assert w.uncertainty == pytest.approx(expected)

    def test_offset_conversion_uncertainty_still_scales(self):
        # degF -> K: derivative is 5/9, offset must not contribute
        q = Q_(100.0, "degF", uncertainty=0.9)
        k = q.to("K")
        assert k.uncertainty == pytest.approx(0.9 * 5.0 / 9.0)


class TestPropagateMulDivExplicitJacobians:
    def test_missing_jacobians_raise(self):
        u1 = VarianceModel(variance=0.04)
        u2 = VarianceModel(variance=0.09)
        with pytest.raises(ValueError, match="jac_self"):
            u1.propagate_mul_div(u2, 3.0, 1.0, 3.0)

    def test_multiplication_by_one_uses_product_rule(self):
        # Regression guard: with val2 == 1, result == val1/val2 as well,
        # and the removed heuristic misclassified mul as div.
        a = Q_(3.0, "m", uncertainty=0.1)
        b = Q_(1.0, "s", uncertainty=0.5)
        r = a * b
        expected = math.sqrt((1.0 * 0.1) ** 2 + (3.0 * 0.5) ** 2)
        assert r.uncertainty == pytest.approx(expected)
