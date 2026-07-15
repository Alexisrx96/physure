"""Regression tests for the correctness batch.

Covers: angle handling in transcendental functions, Jacobian-based
uncertainty propagation through unit converters (incl. logarithmic),
and the removal of the mul/div inference heuristic.
"""

import math

import pytest

from physure import Q_
from physure.domain.exceptions import DimensionError
from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)
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


class TestConverterDerivatives:
    def test_linear_derivatives_are_exact(self):
        c = LinearConverter(1000.0)
        assert c.to_base_derivative(5.0) == 1000.0
        assert c.from_base_derivative(5.0) == pytest.approx(1e-3)

    def test_offset_derivatives_ignore_offset(self):
        c = OffsetConverter(scale=5.0 / 9.0, offset=255.372)
        assert c.to_base_derivative(100.0) == pytest.approx(5.0 / 9.0)
        assert c.from_base_derivative(300.0) == pytest.approx(9.0 / 5.0)

    def test_log_derivatives_match_numeric(self):
        c = LogarithmicConverter(factor=10.0)
        v = 30.0  # 30 dB -> 1000x in base
        h = 1e-6
        numeric = (c.to_base(v + h) - c.to_base(v - h)) / (2 * h)
        assert c.to_base_derivative(v) == pytest.approx(numeric, rel=1e-6)

        base = c.to_base(v)
        numeric = (c.from_base(base + h) - c.from_base(base - h)) / (2 * h)
        assert c.from_base_derivative(base) == pytest.approx(numeric, rel=1e-6)

    def test_log_from_base_scalar_needs_no_numpy(self):
        # math path for scalars (numpy is an optional extra)
        c = LogarithmicConverter(factor=10.0)
        assert c.from_base(1000.0) == pytest.approx(30.0)

    def test_custom_converter_numeric_fallback(self):
        from physure.domain.measurement.converters import UnitConverter

        class SquareConverter(UnitConverter):
            @property
            def is_linear(self):
                return False

            def to_base(self, value):
                return value**2

            def from_base(self, value):
                return value**0.5

        c = SquareConverter()
        assert c.to_base_derivative(3.0) == pytest.approx(6.0, rel=1e-4)
        assert c.from_base_derivative(9.0) == pytest.approx(
            1.0 / 6.0, rel=1e-4
        )


class TestLogUnitUncertaintyPropagation:
    def test_db_to_linear_propagates_jacobian(self, system):
        power_dim = Dimension({"M": 1, "L": 2, "T": -3})
        system.register_unit("W", power_dim, LinearConverter(1.0), "watt")
        system.register_unit(
            "dBW",
            power_dim,
            LogarithmicConverter(factor=10.0),
            "decibel-watt",
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
