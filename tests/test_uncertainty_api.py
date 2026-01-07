import math
import os
import sys

# Add project root and core to sys.path
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
core_path = os.path.join(root, "measurekit_core", "target", "release")
if root not in sys.path:
    sys.path.insert(0, root)
if core_path not in sys.path:
    sys.path.insert(0, core_path)

import measurekit as mk
from measurekit import Q_


def test_uncertainty_context_manager():
    print("Testing Python Mode (Default/Python)...")
    # Default is 'python'
    x = Q_(10.0, "m", uncertainty=1.0)
    y = x**2
    print(f"  Python: {y.magnitude} +/- {y.uncertainty} {y.unit}")
    assert math.isclose(float(y.magnitude), 100.0)
    assert math.isclose(y.uncertainty, 20.0)

    print("\nTesting Monte Carlo Mode via context manager...")
    with mk.uncertainty_mode("monte_carlo", samples=50000):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2
        print(f"  Monte Carlo: {y.magnitude} +/- {y.uncertainty} {y.unit}")
        assert 100.8 < float(y.magnitude.mean) < 101.2
        assert 19.5 < y.uncertainty < 20.5

    print("\nTesting Unscented Mode via context manager...")
    with mk.uncertainty_mode("unscented"):
        x = Q_(10.0, "m", uncertainty=1.0)
        y = x**2
        print(f"  Unscented: {y.magnitude} +/- {y.uncertainty} {y.unit}")
        assert math.isclose(float(y.magnitude.mean), 101.0)
        assert y.uncertainty > 20.0

    print("\nTesting Transcendental functions via Gaussian core...")
    with mk.uncertainty_mode("gaussian"):
        x = Q_(math.pi / 2, "", uncertainty=0.1)
        y = x.sin()
        print(f"  sin(pi/2): {y.magnitude.mean} +/- {y.uncertainty}")
        assert math.isclose(float(y.magnitude.mean), 1.0)
        assert math.isclose(y.uncertainty, 0.0, abs_tol=1e-7)

        x2 = Q_(1.0, "", uncertainty=0.1)
        y2 = x2.exp()
        print(f"  exp(1.0): {y2.magnitude.mean} +/- {y2.uncertainty}")
        assert math.isclose(float(y2.magnitude.mean), math.exp(1.0))
        assert math.isclose(y2.uncertainty, math.exp(1.0) * 0.1)

    print("\nALL API TESTS PASSED")


if __name__ == "__main__":
    test_uncertainty_context_manager()
