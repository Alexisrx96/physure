"""Tests for the unit-aware curve fitter."""

try:
    import numpy as np
except (ImportError, ModuleNotFoundError):
    np = None

import pytest

try:
    from physure.application.fit_service import (
        CurveFitResult,
        unit_aware_curve_fit,
    )
except (ImportError, AttributeError):
    # Fit service might fail if numpy/scipy missing
    CurveFitResult = None
    unit_aware_curve_fit = None
from physure.domain.measurement.converters import LinearConverter
from physure.domain.measurement.dimensions import Dimension


@pytest.fixture
def fit_system(system):
    """Set up test fixtures for fit tests."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    return system


@pytest.mark.skipif(
    unit_aware_curve_fit is None, reason="fit dependencies missing"
)
def test_unit_aware_curve_fit_linear_model(fit_system):
    """Fit y = a*x + b, recovering slope/intercept with correct units."""
    a_true, b_true = 2.5, 1.0

    def linear_model(x, a, b):
        return a * x + b

    x_vals = np.linspace(0.0, 10.0, 20)
    xdata = fit_system.Q_(x_vals, "s")
    ydata = fit_system.Q_(a_true * x_vals + b_true, "m")

    p0 = [fit_system.Q_(1.0, "m/s"), fit_system.Q_(0.0, "m")]

    result = unit_aware_curve_fit(linear_model, xdata, ydata, p0)

    assert isinstance(result, CurveFitResult)
    assert len(result.params) == 2

    a_fit, b_fit = result.params
    assert a_fit.unit == fit_system.get_unit("m/s")
    assert b_fit.unit == fit_system.get_unit("m")
    assert np.isclose(a_fit.magnitude, a_true, rtol=1e-6)
    assert np.isclose(b_fit.magnitude, b_true, atol=1e-6)
    assert a_fit.uncertainty >= 0.0
    assert b_fit.uncertainty >= 0.0
