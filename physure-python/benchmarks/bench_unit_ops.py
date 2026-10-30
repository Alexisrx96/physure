"""Microbenchmark of the unit and quantity operations (A1 stage 1d onwards).

Run from ``physure-python/``: ``uv run --no-sync python benchmarks/bench_unit_ops.py [out.json]``.
The gate of every step of A1 stage 2 is that no row is more than 10 % slower
than the numbers recorded in the stage 1d section of the design, measured on
the same machine.
"""

import json
import sys
import timeit

import numpy as np

import physure
from physure import Q_, units

m, s = units.m, units.s
q1, q2 = Q_(5.0, "m"), Q_(3.0, "m")
q_speed = Q_(5.0, "m") / Q_(2.0, "s")
array = Q_(np.ones(1000), "m")


def measure(label, fn, results):
    timer = timeit.Timer(fn)
    number, _ = timer.autorange()
    best = min(timer.repeat(repeat=5, number=number)) / number
    results[label] = best * 1e9
    print(f"{label:<42} {best * 1e9:>12,.0f} ns")


def main():
    results = {}
    cases = [
        ("Q_(5.0, 'm')  [string unit]", lambda: Q_(5.0, "m")),
        ("Q_(5.0, units.m)  [unit object]", lambda: Q_(5.0, m)),
        ("5 * units.m  [the sugar]", lambda: 5 * m),
        ("units.m * units.s", lambda: m * s),
        ("units.m / units.s", lambda: m / s),
        ("units.m ** 2", lambda: m**2),
        ("q1 + q2", lambda: q1 + q2),
        ("q1 * q2", lambda: q1 * q2),
        ("q1.to('km')", lambda: q1.to("km")),
        ("q_speed.to('km/h')", lambda: q_speed.to("km/h")),
        ("array(1000).to('km')", lambda: array.to("km")),
    ]
    for label, fn in cases:
        measure(label, fn, results)
    if len(sys.argv) > 1:
        with open(sys.argv[1], "w", encoding="utf-8") as out:
            json.dump(results, out, indent=1)


if __name__ == "__main__":
    print(f"physure {physure.__version__}")
    main()
