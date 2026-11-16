"""Measures the cost of `import physure` and of the first `Q_()`.

Each sample is a fresh interpreter, because the cost being measured is the
one-time cost of a new process. The report is the median and the minimum of
`--runs` samples (a single sample on a busy machine is noise).

    python scripts/benchmark_import.py                # report
    python scripts/benchmark_import.py --check        # gate, exit 1 if over

`--check` fails when the median of import + first `Q_()` is over `--budget-ms`
(CLAUDE.md: ~500 ms) or when a heavy optional package got imported by the
first use (they belong to the optional extras and are imported lazily).
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys

# Optional packages that a scalar Q_() must not pull in.
HEAVY = ("numpy", "torch", "scipy", "jax", "sympy", "pandas")

_CHILD = f"""
import json, sys, time
t0 = time.perf_counter()
import physure
t1 = time.perf_counter()
physure.Q_("3 m")
t2 = time.perf_counter()
print(json.dumps({{
    "import_ms": (t1 - t0) * 1e3,
    "first_q_ms": (t2 - t1) * 1e3,
    "heavy": [m for m in {HEAVY!r} if m in sys.modules],
    "modules": len(sys.modules),
}}))
"""


def sample() -> dict:
    out = subprocess.run(
        [sys.executable, "-c", _CHILD],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(out.stdout)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--runs", type=int, default=15)
    ap.add_argument("--budget-ms", type=float, default=500.0)
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    samples = [sample() for _ in range(args.runs)]
    total = [s["import_ms"] + s["first_q_ms"] for s in samples]
    heavy = sorted({m for s in samples for m in s["heavy"]})

    for label, key in (
        ("import physure", "import_ms"),
        ("first Q_()", "first_q_ms"),
    ):
        vals = [s[key] for s in samples]
        print(
            f"{label:15} median {statistics.median(vals):6.0f} ms   min {min(vals):6.0f} ms"
        )
    print(
        f"{'total':15} median {statistics.median(total):6.0f} ms   min {min(total):6.0f} ms"
        f"   (budget {args.budget_ms:.0f} ms)"
    )
    print(f"modules loaded  {samples[0]['modules']}")
    print(f"heavy imported  {', '.join(heavy) or 'none'}")

    if not args.check:
        return 0
    failed = False
    if statistics.median(total) > args.budget_ms:
        print("FAIL: over the first-use budget")
        failed = True
    if heavy:
        print(f"FAIL: first use imported {', '.join(heavy)}")
        failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
