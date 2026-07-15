import math
import os
import sys

# Add core to sys.path
core_path = os.path.abspath(
    os.path.join(
        os.path.dirname(__file__), "..", "measurekit_core", "target", "release"
    )
)
sys.path.insert(0, core_path)

import pytest

pytest.importorskip("measurekit_core")
from measurekit_core import Quantity, RationalUnit


def test_uncertainty_modes():
    print("Testing Gaussian Mode...")
    # x = 10 +/- 1
    # We'll use Quantity directly for now to prove the Rust logic
    u = RationalUnit({"m": (1, 1)})
    u = RationalUnit({"m": (1, 1)})
    x_g = Quantity(10.0, u, 1.0, mode="gaussian")
    y_g = x_g**2
    print(f"  Gaussian: {y_g}")
    # y = 100 +/- 20
    assert math.isclose(y_g.mean, 100.0)
    assert math.isclose(y_g.std_dev, 20.0)

    print("Testing Monte Carlo Mode...")
    # Larger sample size for stability
    x_mc = Quantity(10.0, u, 1.0, mode="monte_carlo", samples=100000)
    y_mc = x_mc**2
    print(f"  Monte Carlo: {y_mc}")
    # y_mean should be approx 101.0
    assert 100.8 < y_mc.mean < 101.2
    # std_dev should be approx 20.05
    assert 19.5 < y_mc.std_dev < 20.5

    print("Testing Unscented Mode...")
    x_ut = Quantity(10.0, u, 1.0, mode="unscented")
    y_ut = x_ut**2
    print(
        f"  Unscented: {y_ut.mean:.4f} +/- {y_ut.std_dev:.4f} {y_ut.core_unit}"
    )
    # Unscented should capture exactly 101 for x**2 because it captures 2nd order
    assert math.isclose(y_ut.mean, 101.0)

    print("\nALL RUST BACKEND TESTS PASSED")


if __name__ == "__main__":
    test_uncertainty_modes()
