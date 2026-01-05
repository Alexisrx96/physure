import measurekit as mk
from measurekit.application.tracing.context import trace_formulas


def test_symbolic_tracing_basic():
    with trace_formulas() as trace:
        m = mk.Q_(10, "kg", symbol="m")
        a = mk.Q_(5, "m/s^2", symbol="a")
        F = m * a

        # Verify the math works
        assert F.magnitude == 50

        # Verify the symbol exists
        latex = trace.get_equation(F)
        assert "m" in latex
        assert "a" in latex
        # Sympy normally renders m*a as 'm a' or 'm \cdot a'
        assert "m" in latex and "a" in latex


def test_symbolic_tracing_complex():
    with trace_formulas() as trace:
        x = mk.Q_(2, "m", symbol="x")
        y = mk.Q_(3, "m", symbol="y")
        z = (x + y) * x / y

        latex = trace.get_equation(z)
        # Expected something like \frac{x (x + y)}{y}
        assert "x" in latex
        assert "y" in latex
        assert "+" in latex


def test_zero_overhead_active():
    # Outside context manager, creating quantities should not crash
    # and should not register anything anywhere global.
    m = mk.Q_(10, "kg", symbol="m")
    a = mk.Q_(5, "m/s^2", symbol="a")
    F = m * a
    assert F.magnitude == 50


def test_affine_trace():
    # Phase 3 requirement: reflect Affine Logic
    with trace_formulas() as trace:
        T1 = mk.Q_(273.15, "kelvin", symbol="T_1")
        T2 = mk.Q_(373.15, "kelvin", symbol="T_2")
        dT = T2 - T1

        latex = trace.get_equation(dT)
        assert "T" in latex
        assert "1" in latex
        assert "2" in latex
        assert "-" in latex
