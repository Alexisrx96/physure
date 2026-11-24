# `round` overflow and dof, and a temperature point against an interval

Date: 2026-11-20. Branch: `docs/post-release-0.4.0`. Approved by the owner on 2026-11-20 after the measurements below. Spec and plan in one document (the change is small).

## 1. Measured (2026-11-20, `phs eval`)

| Program | Today | Correct |
|---|---|---|
| `round(1.5 m, 400)` | `NaN m` | `1.5 m` |
| `round(123456.789 m, -400)` | `NaN m` | `0 m` |
| `round(1e300 m, 10)` | `inf m` | `1e300 m` |
| `dof(round(from_samples([1 V, 2 V, 4 V]), 1))` | `inf` (the original has `2.0`) | `2.0` |
| `5 delta_degC < 20 degC` | `True` | an error |
| `5 delta_degC > 20 degC` | `False` | an error |
| `5 delta_degC == 5 degC`, `!=` | `False`, `True` | an error |
| `max(5 delta_degC, 20 degC)` | `20 degC` | an error |
| `vector(5 delta_degC, 20 degC)` | accepted | an error |

Still fine and unchanged: `20 degC > 10 K` (point with point, with conversion), `5 delta_degC < 6 delta_degC` (interval with interval), `20 degC + 5 delta_degC` (point), `20 degC - 10 degC` (interval), `vector(20 degC, 300 K)` (both points).

## 2. Rules

1. **`round(x, d)`** (`physure-script/src/builtins/core.rs`, `round_builtin`): the rounding is computed so that it never overflows. For `d >= 0`: if `x * 10^d` is not finite or has magnitude at least 2^53, `x` already has no digit at that place and the answer is `x` itself. For `d < 0`: round `x / 10^-d` and multiply back; if `10^-d` overflows, the answer is `0` (with the sign of `x` irrelevant: `0`). A finite `x` never gives `NaN` or `inf`. A non-finite `x` cannot be built in PHS, but is returned unchanged if it arrives from a host. Ties keep rounding away from zero as today; `round(x)` and `round(x, 2)` are unchanged.
2. **`round` keeps the degrees of freedom** of a quantity (and any other statistical field `Quantity` carries that the rebuilding loses: check what `physure_core::Quantity::new_scalar(.., None, None)` drops: dof, covariance link, provenance). The uncertainty already rides along. Prefer cloning the quantity and replacing only the mean; if the core has no way to do that, add a small method to `Quantity` in `physure-core` (the Rust core comes first) rather than rebuilding in PHS.
3. **A temperature point and a temperature interval are different kinds.** Comparing them (`<`, `>`, `<=`, `>=`, `==`, `!=`) is an error: ``cannot compare a temperature point (`20 degC`) with a temperature interval (`5 delta_degC`)``; the words follow the existing messages of the core for adding two points. The comparison lives in `physure-core` (find where `Quantity` compares: the PHS interpreter, Python and the bindings all call it), so every binding gets the rule. Point with point and interval with interval are unchanged, including `degC` against `K`.
4. `max`/`min` and the elements of a `vector` use the same test: the shared helper `physure-script/src/builtins/dimensions.rs` (`mismatch`) treats a point and an interval as a mismatch, with the words of rule 3 adapted (`max of a temperature point and a temperature interval ...`, `a vector holds one dimension and one kind of temperature ...`).
5. The checker (`check.rs`) may miss all of this (it must not flag a program the interpreter accepts). If it already types temperature points for the `temperature-points` code, extend it only where it is a one-line change; otherwise leave it. `check::explains` is extended only if a checker code can now explain one of the new runtime messages (the report guard `a_flagged_line_is_not_said_twice` decides).

## 3. Tasks (commit each; red tests first where the behaviour is new)

1. **Tests**: `physure-script/tests/round_and_temperature_kinds.rs` with every row of section 1 (refused rows assert the words of rules 3-4; valid neighbours assert their values: `round(1.5 m, 400)` is `1.5 m`, `round(123456.789 m, -400)` is `0 m`, `round(1e300 m, 10)` is `1e300 m`, `round(1.2345 m, 2)` unchanged, `round(2.5)` is `3`, the dof is `2.0`, point-with-point and interval-with-interval comparisons, `max(5 degC, 3 degC)`, `vector(20 degC, 300 K)`); a core test next to the comparison code in `physure-core`; a pytest for the Python comparison (`Q_(5, "delta_degC") < Q_(20, "degC")` raises the repository's incompatible-units error) next to the existing temperature tests of `physure-python/tests`.
2. **`round`** (rules 1-2), red/green, a mutation check each.
3. **The comparison in the core and the helper** (rules 3-4), red/green, mutation check, performance unchanged (the comparison is hot: do not add an allocation).
4. **Consumers and fallout**: `cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast`, `cargo test -p physure --no-fail-fast`, Python (rebuild the extension in this worktree: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`, verify `_core.__file__`, `uv run --no-sync pytest -q`), WASM (`wasm-pack test --node` in `physure-wasm`; golden JS test), Java (`cargo build --release -p physure-java`, then `mvn -B test -Dphysure.native.dir=D:/Projects/physure/target/release` in `physure-java`). Any test that compared a point with an interval is fixed in that test; report each. Check the docs corpus guards stay green.
5. **Docs**: CHANGELOG `[Unreleased]` (Fixed for `round`, Changed for the comparison, a breaking change stated plainly with what to write instead: convert the interval to a point or compare the difference of two points), `docs/ROADMAP.md` (remove the three open lines about `round(1.5 m, 400)`, `round` dropping dof and `max(5 delta_degC, 20 degC)`; mention `round` overflow of `1e300`; add to the fixed-list sentence), the language guide text that documents temperatures and `round` (`physure-cli/src/main.rs`, then `phs doc --save`; the sync test stays green), `uv run --no-sync mkdocs build --strict`.

## 4. Not in this change

`linspace` dropping the uncertainty of an endpoint; the report cascade item; the Python transpile prelude; `np.mean/dot/cross`.

## 5. Risks

* The comparison is in the core: a Python or Java caller that compared an interval with a point now gets an error (intended; no meaningful result existed).
* `round` identity beyond 2^53 changes nothing observable except the former `NaN`/`inf`.
