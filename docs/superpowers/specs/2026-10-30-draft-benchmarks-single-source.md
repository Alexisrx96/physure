# Draft: one source for the benchmark numbers (F1)

**Status:** DRAFT, not approved. Source: finding F1 of `2026-10-30-post-a1-inventory.md`.

## Problem

The repository publishes the same benchmark page twice and the copies disagree.

- `BENCHMARKS.md` (root, linked from the README: "Full methodology and reproduction steps") and `docs/benchmarks.md` (the published site) differ in 27 changed lines. In the propagation table the N=100 row reads 3.5 ms and 0.8 ms in one and 1.9 ms and 0.9 ms in the other, with a different memory figure.
- Both files were last changed by the same commit, `214252b` (2026-08-04), so the divergence was introduced together, not by one copy going stale.
- Nobody can tell which numbers are current, on which machine, at which commit. The scripts that produce them live in `physure-python/benchmarks/` (`benchmark_propagation.py`, `benchmark_vectorized.py`, `benchmark_comparison.py`, ...). The unit-operation gate used in A1 (`bench_unit_ops.py`) publishes its numbers only inside the design documents.

## Options

| Option | What it is | For | Against |
|---|---|---|---|
| A | Keep `docs/benchmarks.md` as the only copy; the root `BENCHMARKS.md` is deleted and the README links the site page (or its GitHub URL) | one place, nothing to keep in sync | a GitHub reader of the repository lands on the docs folder instead of the root |
| B | Keep both, generate both from one source with a script, as `scripts/generate_units_readme.py` does for `docs/UNITS.md` | both audiences served | a generator to maintain; the two roadmaps already show how easily two files drift |
| C | Keep the root file as a two-line pointer to `docs/benchmarks.md` | GitHub readers still find a file with the usual name | one more file whose only job is a link |

## Recommendation

Option A with C's pointer only if the maintainer wants the root name kept. In either case:

1. Re-measure, because neither set of numbers can be trusted. Record in the page the machine, the Python and Rust versions, the commit, and the date.
2. Add the unit-operation table of `bench_unit_ops.py` as its own section, since the A1 gates quote it and the only record of its numbers is inside design documents.
3. Say in the page how to reproduce each table (the script and its arguments).

## Open questions

- Which of the two sets was measured on the machine the page describes? If neither, both are replaced.
- Does the root file stay (option C) or go (option A)?
- Should CI run the benchmark scripts on a schedule to catch a regression? The A1 experience says run-to-run noise is about 8% on this class of row, so a threshold under about 15% would produce false alarms.

## Acceptance

- One page holds the numbers; the README links it.
- Each table states environment, commit and date, and the command that reproduces it.
- `mkdocs build --strict` passes, with source links written as GitHub blob URLs (the docs link policy).

## Size and dependencies

Size S, risk low. No dependency on the other drafts.
