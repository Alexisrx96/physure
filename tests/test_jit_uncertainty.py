import math
import os
import sys

# Add project root and core to sys.path
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
core_path = os.path.join(root, "measurekit_core", "target", "release")
sys.path.insert(0, root)
sys.path.insert(0, core_path)

import measurekit as mk
from measurekit import Q_, jit


@jit
def calculate_energy(mass, velocity):
    return 0.5 * mass * velocity**2


def test_jit_uncertainty():
    print("Testing JIT with Uncertainty Modes...")

    print("\n[JIT + Gaussian]")
    with mk.uncertainty_mode("gaussian"):
        m = Q_(10.0, "kg", uncertainty=0.1)
        v = Q_(2.0, "m/s", uncertainty=0.2)
        e = calculate_energy(m, v)
        # Expected: 0.5 * 10 * 4 = 20.0
        # Uncertainty: sqrt((0.5*v^2 * dm)^2 + (m*v * dv)^2)
        # = sqrt((0.5*4 * 0.1)^2 + (10*2 * 0.2)^2) = sqrt(0.04 + 16) approx 4.005
        print(f"  Energy: {e.magnitude.mean} +/- {e.uncertainty} {e.unit}")
        assert math.isclose(float(e.magnitude.mean), 20.0)
        assert math.isclose(e.uncertainty, math.sqrt(0.04 + 16))

    print("\n[JIT + Monte Carlo]")
    with mk.uncertainty_mode("monte_carlo", samples=10000):
        m = Q_(10.0, "kg", uncertainty=0.1)
        v = Q_(2.0, "m/s", uncertainty=0.2)
        e = calculate_energy(m, v)
        print(f"  Energy: {e.magnitude.mean} +/- {e.uncertainty} {e.unit}")
        # MC should capture non-linearities (if any, though v^2 is quadratic)
        # 0.5 * m * (v +/- dv)^2
        # mean(v^2) = (mean(v))^2 + var(v) = 4 + 0.04 = 4.04
        # Energy = 0.5 * 10 * 4.04 = 20.2
        assert 20.1 < float(e.magnitude.mean) < 20.3

    print("\n[JIT + Unscented]")
    with mk.uncertainty_mode("unscented"):
        m = Q_(10.0, "kg", uncertainty=0.1)
        v = Q_(2.0, "m/s", uncertainty=0.2)
        e = calculate_energy(m, v)
        print(f"  Energy: {e.magnitude.mean} +/- {e.uncertainty} {e.unit}")
        assert math.isclose(float(e.magnitude.mean), 20.2)

    print("\nALL JIT UNCERTAINTY TESTS PASSED")


if __name__ == "__main__":
    test_jit_uncertainty()
