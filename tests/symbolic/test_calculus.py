import pytest
import sympy as sp

from measurekit.domain.symbolic.quantity import SymbolicQuantity


def test_calculus_kinematics():
    """
    Verifies symbolic differentiation and integration with unit tracking
    using a classic kinematics scenario: x = 1/2 * a * t^2.
    """
    # 1. Setup
    # t (Time) = SymbolicQuantity("t", "s")
    t = SymbolicQuantity("t", "s")
    # a (Acceleration) = SymbolicQuantity("a", "m/s^2")
    a = SymbolicQuantity("a", "m/s^2")

    # x (Position) = 0.5 * a * t**2
    # Note: 0.5 is a float, which works with SymbolicExpression arithmetic
    x = 0.5 * a * t**2

    # Verify setup units (Position should be meters)
    # m/s^2 * s^2 = m
    assert str(x.unit) == "m"

    # 2. Test 1 (Velocity)
    # Calculate v = x.diff(t)
    v = x.diff(t)

    # Assert v.expr equals 1.0 * a * t
    # SymPy differentiation of 0.5 * a * t**2 wrt t is 1.0 * a * t
    assert v.expr == 1.0 * a.expr * t.expr

    # Assert v.unit equals m/s
    assert str(v.unit) == "m/s"

    # 3. Test 2 (Acceleration - Second Derivative)
    # Calculate acc = x.diff(t, 2)
    acc = x.diff(t, 2)

    # Assert acc.expr equals 1.0 * a (SymPy keeps the float multiplier usually)
    # diff(1.0 * a * t, t) -> 1.0 * a
    assert acc.expr == 1.0 * a.expr

    # Assert acc.unit equals m/s^2
    assert str(acc.unit) == "m/s²"

    # 4. Test 3 (Integration - Back to Position)
    # Calculate dist = v.integrate(t)
    # Integrate 1.0 * a * t dt -> 0.5 * a * t^2
    dist = v.integrate(t)

    # Assert dist.unit equals m
    assert str(dist.unit) == "m"

    # Check expression structure (might differ slightly, e.g. 0.5 vs 1/2)
    # But semantically equal
    assert sp.simplify(dist.expr - x.expr) == 0


def test_differentiation_invalid_variable():
    """Ensures TypeError is raised by a non-SymbolicQuantity."""
    t = SymbolicQuantity("t", "s")
    x = t**2

    # Differentiating by raw symbol should fail
    with pytest.raises(
        TypeError, match="Differentiation variable must be a SymbolicQuantity"
    ):
        x.diff(sp.Symbol("x"))

    # Differentiating by string should fail
    with pytest.raises(
        TypeError, match="Differentiation variable must be a SymbolicQuantity"
    ):
        x.diff("x")


def test_integration_invalid_variable():
    """Ensures TypeError is raised by a non-SymbolicQuantity."""
    t = SymbolicQuantity("t", "s")
    v = t

    with pytest.raises(
        TypeError, match="Integration variable must be a SymbolicQuantity"
    ):
        v.integrate(sp.Symbol("x"))
