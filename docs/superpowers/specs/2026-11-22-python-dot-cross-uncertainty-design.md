# Python `Quantity.dot` and `cross` keep the uncertainty

Date: 2026-11-22. Branch: `docs/post-release-0.4.0`. Approved by the owner on 2026-11-22. Spec and plan in one document. Follows `2026-11-21-python-numpy-units-hash-design.md`.

## 1. Measured (2026-11-22)

| Program (`a = Q_([1,2,3],'m',uncertainty=0.1)`, `b = Q_([4,5,6],'N',uncertainty=0.2)`) | Today | Should be |
|---|---|---|
| `a.dot(b)` | `32.0 J`, uncertainty `0.0` | `32 J` with the propagated uncertainty (`np.sum(a * b)` gives it: the elementwise `a * b` is `[0.447, 0.640, 0.849]` J) |
| `a.cross(b)` | `[-3, 6, -3] J`, uncertainty `[0, 0, 0]` | each component with its propagated uncertainty |
| `np.dot(a, b)`, `np.cross(a, b)`, `a @ b` | `TypeError` | unchanged (or routed to the same code if trivial and correct) |

A silent zero uncertainty from uncertain inputs breaks the project invariant (never silently drop an uncertainty; if it cannot be preserved, raise). `np.polyfit` is NOT in this change (it fails loudly; it needs its own design).

## 2. The design

1. `Quantity.dot(other)` for array-backed quantities with uncertainty is the sum of the elementwise product through the same covariance-store route `np.sum` uses since the previous change (`np.sum(a * b)`): same value, correct uncertainty for independent elements and for lineage-correlated ones the elementwise product already carries. Shape rules: 1-D with 1-D (a scalar); mismatched lengths or dimensions raise as today. Units: the product's unit (`m * N` = `J`, as now). Exact quantities (no uncertainty) keep the current fast path and value.
2. `Quantity.cross(other)` (3-vectors, and 2-vectors if supported today) composes the components with Quantity arithmetic (`a1*b2 - a2*b1`...), so each component has its propagated uncertainty. Known limit, documented: `q[i]` creates a new independent source, so the cross of an array with itself (`a.cross(a)`) gives a non-zero uncertainty where the exact value is 0; say so in the docstring and ROADMAP. If a case cannot be propagated correctly (a model that is neither covariance nor variance, an uncorrelated-mode array that only holds variances, as for `np.sum`), raise `TypeError` naming the method and the reason: never a zero.
3. Where `Quantity.dot`/`cross` take other shapes today (2-D matrices, batches): keep the supported shapes working with the same values and propagate or raise for them too; do not return a zero uncertainty for any uncertain input. Report which shapes exist.
4. `np.dot`/`np.cross`/`@` on array-backed quantities stay `TypeError` unless routing them to the same code is trivial, correct and tested (then do it, listing it in the CHANGELOG).

## 3. Tasks (commit each; red tests first; one mutation check per behaviour; test BOTH exact and uncertain, scalar-component and array-backed, and the correlated case)

1. **Tests** (red) in `physure-python/tests/`: the rows of section 1; `dot` against the analytic value `sqrt(sum((b_i s_a)^2 + (a_i s_b)^2))` for independent errors (and equal to `np.sum(a * b)`), with a shared lineage (`a.dot(a)` with common errors: compare with the exact formula `2 * sqrt(sum (a_i s_i)^2)`-style value or with the reduction through scalar quantities with lineage, as the `np.sum` tests do), units (`m*N` = `J`, dimensionless, temperatures refused as before), exact arrays unchanged, zero-variance mixed, 1-element and 1e5-element arrays (timing), mismatched shapes/dimensions raise, `cross` components against analytic values, the documented `a.cross(a)` limit, the raise for an uncorrelated-mode array.
2. **Implementation** of `dot`, then `cross`: one commit each.
3. **Runs**: Python only (Rust unchanged): `cd physure-python && unset VIRTUAL_ENV && uv run --no-sync pytest -q` (doctests via `--doctest-modules physure`: one known failing `SymbolicRegressor` doctest fails on base too), `ruff check .`, `ruff format --check .`, `ty` on touched files (no new errors), `uv run --no-sync mkdocs build --strict`; `_core.__file__` must be under the worktree.
4. **Docs**: CHANGELOG `[Unreleased]` (Fixed: `dot` and `cross` of uncertain arrays gave zero uncertainty), `docs/ROADMAP.md` (remove the open line for `dot`/`cross`; keep `np.polyfit` listed as unsupported; add the `a.cross(a)` limit), the Python docs that mention `dot`/`cross` (grep `docs/`).

## 4. Not in this change

`np.polyfit`; an exact `.to()`; `np.std` beyond what exists; the other open items of the ROADMAP.
