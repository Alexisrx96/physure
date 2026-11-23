# Python: converting an uncertain array keeps its correlation

Date: 2026-11-23. Branch: `docs/post-release-0.4.0`. Approved by the owner on 2026-11-23 ("Cerramos el punto 2 antes del merge"). Spec and plan in one document. Follows `2026-11-22-python-dot-cross-uncertainty-design.md`, whose review found it.

## 1. The problem (measured 2026-11-22 by the dot/cross implementer)

`Quantity.to(unit)` of an UNCERTAIN ARRAY returns a new, independent measurement: the covariance store does not learn that the result is a linear function of the source. For a scalar quantity, `.to()` carries the lineage (the conversion is a linear map of the same source), so the correlation survives. For an array it does not:

| Program (`a = Q_([1,2,3],'m',uncertainty=0.1)`) | Today | Should be |
|---|---|---|
| `a.to('cm') - a` | `± 14.1 cm` | exactly `0 ± 0` (the same measurement in another unit) |
| `a.to('cm') / a` | independent errors | `100`, uncertainty `0` |
| `(a.to('cm') + a).uncertainty` | the independent sum | `2 * 100 * s` style value: fully correlated |
| `t = Q_([20,25],'degC',uncertainty=0.5); t * t` | `± 40.0 K^2` | `± 56.6 K^2` (the product `T*T` with T = 293.15 K: the affine conversion has derivative 1 for degC to K, the same measurement in both factors) |
| `np.sum(a.to('cm')) - np.sum(a) * 100` | non-zero | 0 |

`dot` and `cross` avoid the bug by applying the conversion derivative themselves; `*`, `-`, `+` and the other arithmetic still have it. It is a silent wrong uncertainty (the result looks fine and has a unit).

## 2. The design

1. **The conversion of an array with uncertainty is a linear (affine) map of its source and is propagated through the covariance store**, as `np.sum` and `dot` do: the converted array's store slots are the source's slots scaled by the conversion's derivative (the scale factor; for an affine point conversion the derivative is the scale only, `5/9` for degF to K, `1` for degC to K; the offset does not touch the uncertainty). The mean is converted as today (the core plan); the uncertainty is NOT rebuilt from standard deviations. Find where `.to()` builds the converted array's `uncertainty` (`_convert_by_plan` and the array branch: it must read the core `RationalUnit` conversion plan's scale and offset as the scalar path does) and where the store ("lineage"/`vector_slice`/`update_from_propagation`, `CovarianceModel`) can express `new = f * old`: reuse the same call `dot`/`cross`/`np.sum` use (`update_from_propagation` with a Jacobian `diag(f)` as a sparse/diagonal matrix: do NOT build a dense n x n Jacobian: for 1e5 elements it would need 74 GiB; use a diagonal/sparse form or a scale-the-slot operation if the store has one).
2. The same applies wherever the library converts an uncertain array implicitly: arithmetic between arrays in different units (`a_cm + a_m`), `*` of degC points converted to K (the `t * t` row), comparisons need no uncertainty. Find every internal caller of the conversion that returns an array with uncertainty (`_read_in`, `_add_sub_converting`, `_convert_by_plan`, the temperature arithmetic helpers) and make them keep the correlation; report the list.
3. Scalars are unchanged (they already carry the lineage); arrays WITHOUT uncertainty keep their fast path and their values; arrays in `uncorrelated` mode that only hold variances: a conversion scales the variance by `f^2` (correct for independent errors; same as today's value), no error, but the result is documented as not correlated with the source (as for any uncorrelated-mode array).
4. Logarithmic units/levels: refused or unchanged as today (report which). Non-linear conversions (a level to a ratio): not covered, raise or keep today's behaviour but never a silent zero; say which.
5. Monte Carlo/unscented modes: arrays cannot be created there today (`TypeError`), nothing to do; report if that changed.

## 3. Tasks (commit each, small; red tests first; one mutation check per behaviour; test exact AND uncertain, independent AND correlated, 1-element (inside `physure.PhysureContext()` too) and large arrays, degC/degF/K/mK and a plain scale, and the round trips)

1. **Tests** (red) in `physure-python/tests/measurement_tests/`: every row of section 1; analytic comparisons: `(a.to('cm')).uncertainty == 100 * a.uncertainty` componentwise, `a.to('cm') - a` exactly zero, `a.to('cm') + a` equals `101 * a` in value and uncertainty, `np.sum(a.to('cm'))` has uncertainty `100 * np.sum(a).uncertainty`, `t * t` against the analytic derivative, `a.to('km').to('m') - a` zero, correlated arrays (a shared source `x`: `(b + x).to('cm') - (b + x)`), degF to K derivative `5/9`, temperature intervals; arrays with no uncertainty unchanged (values and speed); a 1e5 array converts in about the time `a.to(unit)` of an exact array takes times a small factor (record the timing; the dense Jacobian must not appear).
2. **Implementation** (point 1, then point 2): one commit each.
3. **Runs**: Python only unless you must touch Rust (if so, stop and report why): `cd physure-python && unset VIRTUAL_ENV && uv run --no-sync pytest -q`, `pytest --doctest-modules physure` (one known failing `SymbolicRegressor` doctest fails on base too), `ruff check .`, `ruff format --check .`, `ty` on touched files (no new errors), `uv run --no-sync mkdocs build --strict`; `_core.__file__` under the worktree.
4. **Docs**: CHANGELOG `[Unreleased]` (Fixed), `docs/ROADMAP.md` (remove the open line about `.to()` of an uncertain array and the one for `t * t`; keep the others).

## 4. Not in this change

The Rust-side rejection of column-major Jacobians; the dense Jacobian of the matrix `dot`; `np.polyfit`; an exact `.to()`; 1-element arrays outside a context.
