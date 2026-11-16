"""The first `Q_()` must not import the optional heavy packages (CLAUDE.md:
first use stays fast). Deterministic stand-in for the timing gate in
`scripts/benchmark_import.py --check`."""

import subprocess
import sys

HEAVY = ("numpy", "torch", "scipy", "jax", "sympy", "pandas")


def test_scalar_first_use_imports_no_heavy_package():
    code = (
        "import sys, physure; physure.Q_('3 m'); "
        f"print([m for m in {HEAVY!r} if m in sys.modules])"
    )
    out = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        text=True,
        check=True,
    )
    assert out.stdout.strip() == "[]"
