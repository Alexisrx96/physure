"""Unit tests for non-linear units (Temperature, Logarithmic) and calculus."""

import pytest
import sympy as sp

from physure import Q_

# --- Temperature (Offset Converter) Tests ---


def test_temperature_conversions():
    """Test standard conversion between Offset (C, F) and Linear (K) units."""
    # 100 C -> 212 F
    t_c = Q_(100, "degC")
    t_f = t_c.to("degF")
    assert t_f.magnitude == pytest.approx(212, abs=1e-9)
    # Checking unit symbol or string
    assert "F" in t_f.unit.to_string() or "fahrenheit" in t_f.unit.to_string()

    # 100 C -> 373.15 K
    t_k = t_c.to("K")
    assert t_k.magnitude == pytest.approx(373.15, abs=1e-9)
    assert "K" in t_k.unit.to_string()


def test_temperature_subtraction_results_in_delta():
    """T (Offset) - T (Offset) should result in a Delta (Linear) Quantity."""
    t1 = Q_(100, "degC")
    t2 = Q_(90, "degC")
    diff = t1 - t2

    # 100 - 90 = 10 degC diff -> 10 K (Interval)
    assert diff.magnitude == pytest.approx(10.0, abs=1e-9)

    # The system should return this in Base Units of the dimension (Kelvin)
    # Kelvin corresponds to LinearConverter(1.0)
    assert diff.unit.to_string() == "K"


def test_temperature_addition_raises_error():
    """T (Offset) + T (Offset) is ambiguous and should raise ValueError."""
    t1 = Q_(100, "degC")
    t2 = Q_(90, "degC")
    with pytest.raises(ValueError, match="Cannot add two absolute quantities"):
        _ = t1 + t2


def test_temperature_plus_delta():
    """T (Offset) + Delta (Linear) should result in T (Offset)."""
    # 100 degC + 5 K = 105 degC
    t = Q_(100, "degC")
    delta = Q_(5, "K")
    res = t + delta

    assert res.magnitude == pytest.approx(105.0)
    assert "C" in res.unit.to_string()

    # Commutative: Delta + T
    res2 = delta + t
    assert res2.magnitude == pytest.approx(105.0)
    assert "C" in res2.unit.to_string()


def test_temperature_minus_delta():
    """T (Offset) - Delta (Linear) should result in T (Offset)."""
    # 100 degC - 5 K = 95 degC
    t = Q_(100, "degC")
    delta = Q_(5, "K")
    res = t - delta

    assert res.magnitude == pytest.approx(95.0)
    assert "C" in res.unit.to_string()


def test_delta_minus_temperature_raises_error():
    """Delta (Linear) - T (Offset) is undefined and should raise ValueError."""
    delta = Q_(5, "K")
    t = Q_(100, "degC")

    with pytest.raises(
        ValueError, match="Cannot subtract an absolute quantity"
    ):
        _ = delta - t


# --- Logarithmic (dB) Tests ---


def test_logarithmic_addition():
    """dB + dB should sum their linear powers."""
    # 10 dB + 20 dB
    # 10 dB -> 10^1 = 10
    # 20 dB -> 10^2 = 100
    # Sum = 110
    # 10 log10(110) approx 20.4139

    u1 = Q_(10, "dB")
    u2 = Q_(20, "dB")
    res = u1 + u2

    import math

    expected = 10 * math.log10(110)

    assert res.magnitude == pytest.approx(expected, abs=1e-5)
    assert "dB" in res.unit.to_string()


def test_logarithmic_subtraction():
    """dB - dB should subtract their linear powers."""
    # 20 dB - 10 dB
    # 100 - 10 = 90
    # 10 log10(90) approx 19.542

    u1 = Q_(20, "dB")
    u2 = Q_(10, "dB")
    res = u1 - u2

    import math

    expected = 10 * math.log10(90)

    assert res.magnitude == pytest.approx(expected, abs=1e-5)
    assert "dB" in res.unit.to_string()


# --- Calculus / Differentiation Tests ---


def test_symbolic_differentiation():
    """Test derivatives of a simple position function."""
    t = sp.Symbol("t")
    # x(t) = 5 t^2 meters
    expression = 5 * t**2
    x = Q_(expression, "m")

    # Variable 't' with unit 's'
    q_t = Q_(t, "s")

    # v = dx/dt -> 10t m/s
    v = x.diff(q_t)

    assert str(v.magnitude) == str(10 * t)
    # Unit should be m/s
    # Physure might format as 'm/s' or 'm s^-1' depending on implementation
    u_str = v.unit.to_string()
    assert "m" in u_str
    assert "s" in u_str

    # a = dv/dt -> 10 m/s^2
    a = v.diff(q_t)
    assert str(a.magnitude) == "10"
    # u_str_a = a.unit.to_string()
    # Check if s has exponent -2
    # Simple check:
    assert v.unit.exponents.get("m", 0) == 1
    assert v.unit.exponents.get("s", 0) == -1

    assert a.unit.exponents.get("m", 0) == 1
    assert a.unit.exponents.get("s", 0) == -2


def test_differentiation_wrt_string():
    """Test differentiation with respect to a dimensionless symbol string."""
    t = sp.Symbol("t")
    x = Q_(5 * t**2, "m")

    # diff wrt "t" -> treated as dimensionless
    # Result unit should still be 'm', magnitude differentiated
    res = x.diff("t")

    assert str(res.magnitude) == str(10 * t)
    assert res.unit.to_string() == "m" or res.unit.to_string() == "meter"
