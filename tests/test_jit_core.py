import sys
from pathlib import Path

import numpy as np

# Add project root to sys.path relative to this test file
_ROOT = Path(__file__).resolve().parent.parent
if str(_ROOT) not in sys.path:
    sys.path.insert(0, str(_ROOT))

# Resolve Rust core path
CORE_PATH = _ROOT / "measurekit_core" / "target" / "release"
if CORE_PATH.exists() and str(CORE_PATH) not in sys.path:
    sys.path.insert(0, str(CORE_PATH))

from measurekit import Q_, jit


@jit
def kinetic_energy(mass, velocity):
    return 0.5 * mass * velocity**2


def test_jit_basic():
    print("Running test_jit_basic...")
    m = Q_(10.0, "kg")
    v = Q_(5.0, "m/s")

    ke = kinetic_energy(m, v)
    print(f"  KE: {ke}")
    assert ke.magnitude == 125.0
    # Check exponents directly instead of string representation
    assert ke.unit.exponents["kg"] == 1
    assert ke.unit.exponents["m"] == 2
    assert ke.unit.exponents["s"] == -2

    ke2 = kinetic_energy(Q_(2.0, "kg"), Q_(10.0, "m/s"))
    print(f"  KE2: {ke2}")
    assert ke2.magnitude == 100.0

    print("  Success: Basic JIT tracing and execution")


def test_jit_dimensional_error():
    print("Running test_jit_dimensional_error...")

    @jit
    def invalid_op(a, b):
        return a + b

    try:
        invalid_op(Q_(1, "m"), Q_(1, "s"))
        assert False, "Should have raised DimensionalError"
    except Exception as e:
        print(f"  Caught expected error: {type(e).__name__}: {e}")
        assert "Incompatible units" in str(e)

    print("  Success: JIT caught dimensional error at trace-time")


def test_jit_numpy():
    print("Running test_jit_numpy...")
    m = Q_(np.array([1.0, 2.0]), "kg")
    v = Q_(np.array([10.0, 20.0]), "m/s")

    ke = kinetic_energy(m, v)
    print(f"  KE (numpy): {ke}")
    expected = 0.5 * np.array([1.0, 2.0]) * np.array([10.0, 20.0]) ** 2
    assert np.allclose(ke.magnitude, expected)

    print("  Success: JIT works with NumPy arrays")


if __name__ == "__main__":
    try:
        test_jit_basic()
        test_jit_dimensional_error()
        test_jit_numpy()
        print("\nALL JIT TESTS PASSED")
    except Exception as e:
        print(f"\nJIT TEST FAILED: {e}")
        import traceback

        traceback.print_exc()
        sys.exit(1)
