import measurekit as mk
from measurekit.application.tracing.context import trace_formulas


def test_symbolic_tracing_basic():
    with trace_formulas() as trace:
        m = mk.Q_(10, "kg", symbol="m")
        a = mk.Q_(5, "m/s^2", symbol="a")
        f = m * a

        # Verify the math works
        assert f.magnitude == 50

        # Verify the symbol exists
        latex = trace.get_equation(f)
        assert "m" in latex
        assert "a" in latex
        # Sympy normally renders m*a as 'm a' or 'm \cdot a'
        assert "m" in latex
        assert "a" in latex


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
    f = m * a
    assert f.magnitude == 50


def test_affine_trace():
    # Phase 3 requirement: reflect Affine Logic
    with trace_formulas() as trace:
        t1 = mk.Q_(273.15, "kelvin", symbol="T_1")
        t2 = mk.Q_(373.15, "kelvin", symbol="T_2")
        dt = t2 - t1

        latex = trace.get_equation(dt)
        assert "T" in latex
        assert "1" in latex
        assert "2" in latex
        assert "-" in latex
