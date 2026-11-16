"""Writes tests/golden/student_t_two_sided.tsv: the two-sided Student-t coverage factors that
`physure_core::math::stats::student_t_two_sided` is checked against.

For a coverage probability p and nu degrees of freedom the factor is k with P(|T_nu| < k) = p, that is
`t.ppf((1 + p) / 2, nu)`; for infinite nu it is the normal quantile. scipy is the independent reference
and is a development tool here, not a dependency of any package:

    uv run --with scipy python scripts/gen_student_t_golden.py

Columns: nu (the word `inf` for infinity), p, k with 17 significant digits.
"""

import pathlib

from scipy import stats

NUS = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 30, 40, 60, 100, 300, 1000, 10000, float("inf")]
PS = [0.5, 0.683, 0.9, 0.95, 0.954, 0.99, 0.997, 0.999]

OUT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "golden" / "student_t_two_sided.tsv"


def main() -> None:
    lines = ["# nu\tp\tk (two-sided Student-t coverage factor, scipy.stats)"]
    for nu in NUS:
        for p in PS:
            quantile = (1 + p) / 2
            k = stats.norm.ppf(quantile) if nu == float("inf") else stats.t.ppf(quantile, nu)
            lines.append(f"{'inf' if nu == float('inf') else int(nu)}\t{p}\t{k:.17g}")
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {len(lines) - 1} rows to {OUT}")


if __name__ == "__main__":
    main()
