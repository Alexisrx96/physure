"""Writes tests/golden/uncertainty_dof.tsv: the effective degrees of freedom, coverage factor and expanded
uncertainty that every binding is held to.

A row is a budget (independent measurements summed, each `mean:u:nu`, `nu` the word `inf` when it declares none),
a coverage probability, and what JCGM 100:2008 G.4 says of the sum:

    u_c   = sqrt(sum u_i^2)
    nu_eff = u_c^4 / sum(u_i^4 / nu_i)              (the terms with a finite nu_i)
    k      = t_{(1+p)/2}(floor(nu_eff), at least 1)  (the normal quantile when nu_eff is infinite)
    U      = k u_c

Computed here from the formulas and scipy, not by physure, so that the core, Java and WASM are checked against an
independent reference. scipy is a development tool here, not a dependency of any package:

    uv run --with scipy python scripts/gen_uncertainty_dof_golden.py

Columns: name, budget, p, nu_eff (`inf` for infinity), k, u_c, U; numbers with 17 significant digits.
"""

import math
import pathlib

from scipy import stats

INF = float("inf")

H1 = [(50000623.0, 25.0, 18.0), (215.0, 5.8, 24.0), (0.0, 3.9, 5.0), (0.0, 6.7, 8.0), (0.0, 16.599, 2.0), (0.0, 2.88679, 50.0)]

CASES = [
    ("one_measurement", [(10.0, 0.5, 16.0)], 0.95),
    ("one_measurement_fractional_dof", [(10.0, 0.5, 16.9)], 0.95),
    ("two_measurements", [(1.0, 3.0, 4.0), (2.0, 4.0, 9.0)], 0.95),
    ("one_without_dof", [(1.0, 3.0, 4.0), (2.0, 4.0, INF)], 0.95),
    ("none_with_dof", [(1.0, 3.0, INF), (2.0, 4.0, INF)], 0.95),
    ("dof_below_one", [(1.0, 1.0, 0.4)], 0.95),
    ("four_equal_measurements", [(1.0, 0.1, 6.0)] * 4, 0.683),
    ("median_coverage", [(10.0, 0.5, 16.9)], 0.5),
    ("end_gauge_95", H1, 0.95),
    ("end_gauge_99", H1, 0.99),
    ("large_dof", [(5.0, 0.2, 10000.0), (1.0, 0.3, 300.0)], 0.99),
]

OUT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "golden" / "uncertainty_dof.tsv"


def number(x: float) -> str:
    return "inf" if x == INF else f"{x:.17g}"


def budget_text(budget: list[tuple[float, float, float]]) -> str:
    return ";".join(f"{mean!r}:{u!r}:{'inf' if nu == INF else repr(nu)}" for mean, u, nu in budget)


def reference(budget, p):
    u_c = math.sqrt(sum(u * u for _, u, _ in budget))
    denominator = sum(u**4 / nu for _, u, nu in budget if nu != INF)
    nu_eff = INF if denominator == 0 else u_c**4 / denominator
    k = stats.norm.ppf((1 + p) / 2) if nu_eff == INF else stats.t.ppf((1 + p) / 2, max(1, math.floor(nu_eff)))
    return nu_eff, float(k), u_c, float(k) * u_c


def main() -> None:
    lines = ["# name\tbudget (mean:u:nu;...)\tp\tnu_eff\tk\tu_c\tU (computed with scipy, see scripts/gen_uncertainty_dof_golden.py)"]
    for name, budget, p in CASES:
        nu_eff, k, u_c, big_u = reference(budget, p)
        lines.append("\t".join([name, budget_text(budget), repr(p), number(nu_eff), number(k), number(u_c), number(big_u)]))
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {len(CASES)} rows to {OUT}")


if __name__ == "__main__":
    main()
