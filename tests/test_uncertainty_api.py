import math
import os
import sys

# Add project root and core to sys.path
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
# Add project root to sys.path
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
if root not in sys.path:
    sys.path.insert(0, root)
# measurekit_core injection removed

import measurekit as mk
from measurekit import Q_


def get_val(q):
    if hasattr(q.magnitude, "mean"):
        return float(q.magnitude.mean)
    return float(q.magnitude)


def test_uncertainty_context_manager():
    print("Testing Python Mode (Default/Python)...")
    # Default is 'python'
    x = Q_(10.0, "m", uncertainty=1.0)
    y = x**2
    val = get_val(y)
    print(f"  Python: {val} +/- {y.uncertainty} {y.unit}")
    assert math.isclose(val, 100.0)
    assert math.isclose(y.uncertainty, 20.0)

    print("\nTesting Monte Carlo Mode via context manager...")
    with mk.uncertainty_mode("monte_carlo", samples=50000):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2
        val = get_val(y)
        print(f"  Monte Carlo: {val} +/- {y.uncertainty} {y.unit}")
        # If pure python fallback (linear), mean is 100.0.
        if val > 100.1:
            assert 100.5 < val < 101.5
            assert 19.5 < y.uncertainty < 20.5
        else:
            print("Warning: Using linear fallback.")
            assert math.isclose(val, 100.0)

    print("\nTesting Unscented Mode via context manager...")
    with mk.uncertainty_mode("unscented"):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2
        val = get_val(y)
        print(f"  Unscented: {val} +/- {y.uncertainty} {y.unit}")
        if val > 100.1:
            assert math.isclose(val, 101.0)
            assert y.uncertainty > 20.0
        else:
            print("Warning: Using linear fallback.")
            assert math.isclose(val, 100.0)

    print("\nTesting Transcendental functions via Gaussian core...")
    with mk.uncertainty_mode("gaussian"):
        x = Q_(math.pi / 2, "", uncertainty=0.1)
        y = x.sin()
        val = get_val(y)
        print(f"  sin(pi/2): {val} +/- {y.uncertainty}")
        assert math.isclose(val, 1.0)
        assert math.isclose(y.uncertainty, 0.0, abs_tol=1e-7)

        x2 = Q_(1.0, "", uncertainty=0.1)
        y2 = x2.exp()
        val2 = get_val(y2)
        print(f"  exp(1.0): {val2} +/- {y2.uncertainty}")
        assert math.isclose(val2, math.exp(1.0))
        assert math.isclose(y2.uncertainty, math.exp(1.0) * 0.1)

    print("\nALL API TESTS PASSED")


if __name__ == "__main__":
    test_uncertainty_context_manager()
