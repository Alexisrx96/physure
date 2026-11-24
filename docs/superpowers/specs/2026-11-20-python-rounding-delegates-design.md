# Python `round`, `floor`, `ceil`, `trunc` delegate to the core

Date: 2026-11-20. Branch: `docs/post-release-0.4.0`. Standing rule of the project: Python has no implementation of its own; when it and the Rust core differ, Rust wins and the change goes in the changelog. The owner chose this item on 2026-11-20 ("vamos con round"). Spec and plan in one document.

## 1. The problem (measured by the final review of 2026-11-20)

`physure/domain/measurement/quantity.py` (`__round__`, `__floor__`, `__ceil__`, `__trunc__`, ~line 1377) rebuilds the quantity from `self.magnitude` with Python's own functions:

| Program | Python today | The core / PHS |
|---|---|---|
| `round(Q_(2.5, 'm'))` | `2 m` (ties to even) | `3 m` (ties away from zero) |
| `round(Q_(0.125, 'm'), 2)` | `0.12 m` | `0.13 m` |
| `x - round(x, 1)` for `x` with uncertainty | sd 0.174 (a new independent source) | sd 0 (the same source is kept) |
| `dof(round(from_samples(..), 1))` | lost | kept |
| floor / ceil / trunc | rebuilt from `self.uncertainty`: sources, dof and correlation lost | the core's `floor`, `ceil` keep them |
| `round(Q_(1.5, 'm'), 400)`, huge values | `NaN`/`inf` or not overflow-safe | overflow-safe (`round_to_decimal_place`) |

The core has `Quantity::round(decimal_places)`, `floor()` and `ceil()` (`physure-core/src/quantity.rs` ~530-545; all use `shift_mean_to`, which keeps the unit and the uncertainty sources). Whether the core has `trunc` must be checked: if not, add `Quantity::trunc` (`shift_mean_to(mean.trunc())`) in the core, with a core test, rather than keep a Python rule.

## 2. Rules

1. **Scalar magnitudes delegate.** For a `Quantity` whose magnitude is a Python/numpy scalar (the case the core represents), `__round__(ndigits)`, `__floor__`, `__ceil__`, `__trunc__` call the core (`Quantity.round`, `floor`, `ceil`, `trunc`) through a new PyO3 method on `PyQuantity` (`physure-python/src/lib.rs`), and wrap the result back exactly the way the other delegating operations of the same class do (find how `__add__`, `sqrt`, `abs` or `to` obtain and re-wrap the core quantity; follow that pattern, do not invent a new one).
2. `round(q)` (no `ndigits`) is `round(q, 0)`; `ndigits` must be an integer (a float or a unit-carrying value is a `TypeError`, as Python's own `round` says); a negative `ndigits` is legal.
3. **Ties go away from zero** (the core's rule), and the CHANGELOG says so as a behaviour change: `round(Q_(2.5, 'm'))` was `2 m` and is `3 m`.
4. **Array-backed magnitudes** (NumPy, torch, JAX arrays: the core is scalar) keep the existing elementwise implementation of their backend (ties to even, as the backend does). That is documented in the docstring and in the CHANGELOG as a known difference, and recorded as an OPEN line in `docs/ROADMAP.md` section 3 (the principled fix is a vector form of the core operation). Do not write a second rounding rule in Python to "match" the core on arrays.
5. The uncertainty, its sources, the degrees of freedom and the correlation are those of the core's result: `x - round(x, 1)` has sd 0; `dof(round(from_samples(..), 1))` is kept; the unit and the system are unchanged; `round` of a temperature point stays a point.
6. A Python `Quantity` of a foreign system (a registry other than the default) keeps working: the delegation must not assume the default registry.

## 3. Tasks (commit each; red tests first)

1. **Tests**: `physure-python/tests/` (next to the existing measurement/quantity tests): the rows of section 1 for scalars (ties, negative `ndigits`, `ndigits=400`, huge, `x - round(x, 1)` sd 0 in Gaussian and in Monte Carlo/unscented modes if the test fixtures have them, dof kept, floor/ceil/trunc keep the correlation, a point stays a point, a quantity in a prefixed unit (`mm`) rounds in its own unit as PHS does); an array-backed case that still works and keeps its backend rule; `TypeError` for a float `ndigits`. A core test for `trunc` if added.
2. **Core + PyO3**: `Quantity::trunc` if missing; the PyO3 methods; the stub in `physure-python/physure/_core.pyi`.
3. **Python**: the four dunders delegate for scalars; the array path unchanged; docstrings.
4. **Consumers**: Rust suites, Python (rebuild the extension in this worktree: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`; verify `_core.__file__` is under the worktree; `uv run --no-sync pytest -q`; `uv run --no-sync ruff check .` and `ruff format --check .`), WASM and Java unaffected (their totals are re-run only if the core changed: it may, for `trunc`). Doctests run (`--doctest-modules`). Any existing test that pinned ties-to-even or the rebuilt uncertainty is rewritten with the reason, never loosened silently; list each. Time `round` of 1e5 scalar quantities before/after (the delegation must not be slower than the rebuild by more than noise; report).
5. **Docs**: CHANGELOG `[Unreleased]` (Changed: the tie rule and what is now kept; Fixed), `docs/ROADMAP.md` (remove the "Python `Quantity.round` is its own implementation" open line; add the array-backed line), the Python docs that mention rounding (grep `docs/` for `round`), `uv run --no-sync mkdocs build --strict`.

## 4. Not in this change

`np.mean`/`dot`/`cross` zero uncertainty; the Python hash vs `==`; the catalog `interval` flag; `round` of array-backed magnitudes beyond keeping them as they are.
