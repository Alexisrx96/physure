# The `interval` flag of the catalog

Date: 2026-11-20. Branch: `docs/post-release-0.4.0`. Approved by the owner on 2026-11-20, with the addition that converting a point to an interval unit is an error. Spec and plan in one document.

## 1. The problem

A temperature interval is recognised by the NAME its unit is written by: `RationalUnit::is_written_interval` (`physure-core/src/units/rational.rs` ~364) tests `display_name` for `delta_` / `_delta`. The catalog declares `delta_celsius = 1.0, O, [delta_degC, degC_delta, delta_celsius]`, which is literally a kelvin under another name, so the name is all that tells them apart. Consequences (ROADMAP section 3):

* `(5 delta_degC * 1 s / 1 s) < 20 degC` and `sqrt((5 delta_degC)^2) < 20 degC` lose the name and are accepted;
* `(1 K + 5 delta_degC) < 20 degC` is accepted while `(5 delta_degC + 1 K)` is refused (the name follows the left operand);
* a user's own interval unit in a custom conf (`dT`) is not caught;
* `(20 degC - 10 degC) < 20 degC` is `True`: a point minus a point is `K`, which counts as either kind;
* `20 degC => delta_degC` is `293.15 delta_degC`: a point converted to an interval by reading its absolute value, the very reading the comparison rule refuses.

## 2. The design

1. **A flag on the unit line**, like `own_kind` (`conf.rs` `is_flag`, the doc in the header of `physure-core/src/units/physure.conf` ~146, the lists in `conf.rs` ~936-956, `physure-python/scripts/generate_units_readme.py` `_FLAGS`): `interval`. `delta_celsius` and `delta_fahrenheit` carry it (`delta_celsius = 1.0, O, interval, [delta_degC, ...]`). A unit system's own definitions may declare it (`dT = 1.0, O, interval, [dT]`). The flag is read by `parse_physure_conf` and `extend_registry`, and is meaningful only on a unit that is not affine and not logarithmic (anything else is a problem reported like any other bad flag usage: refuse it with a clear message).
2. **A field on the unit.** `RationalUnit` gains `interval: bool` (set by the registry when the unit is built from a symbol of a flagged entry, with or without an SI prefix; `false` otherwise). `is_written_interval` reads it; the `delta_` test disappears. Check how `RationalUnit` is built, compared (`PartialEq`/`Hash`: the flag must be part of identity only if the conversion/caching logic needs it; justify the choice and test that `delta_degC` and `K` still convert to each other and compare equal in value), cloned and serialised (frozen conf, `phs conf` witness, `UnitFacts`/`registry.facts`, the Python `RationalUnit` wrapper and `_core.pyi`).
3. **Propagation through arithmetic** (the core's unit algebra for `+`, `-`, `*`, `/`, `^`, `sqrt`, `=>`):
   * an interval times or divided by a plain number (or a dimensionless unit such as `percent`) is an interval;
   * an interval plus or minus an interval is an interval; an interval plus or minus a `K` (neutral) is an interval, in either order (so `(1 K + 5 delta_degC)` and `(5 delta_degC + 1 K)` agree);
   * a point plus an interval is a point, a point minus a point is an interval, a point minus an interval is a point (existing behaviour of the core's temperature arithmetic; do not change the VALUES or the printed units of any of them);
   * **a difference of two points is marked an interval but still prints as `K`** (so no printed output and no golden changes): `(20 degC - 10 degC) < 20 degC` is refused;
   * a unit combined with other dimensions (`delta_degC * s / s`, `delta_degC / m`) is no longer a plain interval: the flag is dropped when the resulting dimension is not exactly the temperature dimension to the first power with a plain-number scale; this is documented (it is today's behaviour by accident of the name);
   * `K` and `degR` declared in the catalog stay neutral (either kind), as now.
4. **A point converted to an interval unit is an error.** `20 degC => delta_degC`, `68 degF => delta_degF`, `(20 degC) => delta_degC` and the same through `Quantity::convert_to`/the registry conversion that `=>` uses: ``cannot convert a temperature point (`20 degC`) to a temperature interval (`delta_degC`): a point says where, an interval says how far; use a difference of two points (`T2 - T1`)`` (reuse `POINT_AGAINST_INTERVAL_ADVICE` for the advice). A neutral `K` and an interval convert to an interval unit as today (`10 K => delta_degC` is `10 delta_degC`; `5 delta_degC => K` is `5 K`: that one stays allowed because `K` is neutral, and its advice is already in the error of the comparison). Converting an interval to a point unit (`5 delta_degC => degC`) is refused too, for the same reason in reverse (check what it does today; if it is accepted with a wrong value, this is the fix).
5. **The consumers read the flag, not the name:** `point_against_interval`, `max`/`min`/`vector`/ranges (they already call the core helper), the Python comparison (`Quantity._compare` through `UnitRegistry.point_against_interval`: the Python domain `CompoundUnit` must carry the flag through `registry.expand`; test it), the shared message, the checker if it consumes `UnitFacts`.
6. **Frozen configuration and witnesses:** the flag is part of what a frozen file records; `phs conf freeze`, `verify`, `diff` and the witness battery read and write it in both directions (tests); a frozen file made before this change (no flag) loads with the shipped catalog's flags for the shipped entries (there is no flagged entry in an old file, so an old file's `delta_celsius` would NOT be an interval: decide, and test, what an old frozen file does: the safe rule is that the shipped names `delta_celsius`/`delta_fahrenheit` keep the flag from the shipped catalog when the frozen file lacks the flag on them; if the frozen format cannot express this, say so and ask in the report rather than guess).

## 3. Tasks (commit each; red tests first)

1. **Tests** (red): a table test in `physure-core` (`tests/`) with every expression of section 2 point 3 and the `=>` cases of point 4, both operand orders; PHS tests (`physure-script/tests/interval_flag.rs`); a conf test that a custom unit with `interval` behaves like `delta_degC` and that `interval` on an affine or logarithmic unit is a problem; pytest for the Python side (a user unit with the flag, `(1 K + 5 delta_degC) < 20 degC`, `20 degC => delta_degC` raises).
2. **Conf + registry + unit field** (points 1-2), then **propagation** (point 3), then **the conversion rule** (point 4): one commit each, mutation check each.
3. **Frozen conf / witness / catalog docs generator** (point 6), `phs doc --save` if the guide text changes (the guide section on temperatures), the units README generated by `generate_units_readme.py` (regenerate it; a test keeps it in sync if one exists).
4. **Consumers and full runs**: Rust suites (`cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast`, `cargo test -p physure --no-fail-fast`), Python (rebuild in this worktree: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`; verify `_core.__file__`; `uv run --no-sync pytest -q`; `ruff check .`; `ruff format --check .`), WASM (`wasm-pack test --node` in `physure-wasm`; golden JS), Java (`cargo build --release -p physure-java`, `mvn -B test -Dphysure.native.dir=D:/Projects/physure/target/release`). Time 3M `phs_compare` calls (release) and the first-use budget (`time python -m physure "500 N / 2 m^2 => kPa"` must stay about 0.5 s) before and after; report.
5. **Docs**: CHANGELOG `[Unreleased]` (Added: the `interval` flag; Changed, breaking: a point converted to an interval unit is an error, with what to write instead; Fixed: the five holes of section 1), `docs/ROADMAP.md` (remove the open line about the name heuristic and the one about `20 degC => delta_degC` if present; fixed-list sentence), the docs that describe `.conf` flags (grep `own_kind`/`noprefix` in `docs/`), `uv run --no-sync mkdocs build --strict`.

## 4. Not in this change

The Python hash vs `==`; `np.mean/dot/cross`; a `delta` function (decided against on 2026-11-20: subtraction is the operation; a sign convention would be a trap); the report cascade item.

## 5. Risks

* The central struct `RationalUnit` changes: construction, `PartialEq`/`Hash`, serialisation, the frozen format and the Python wrapper all see a new field. Tests are table-driven; performance is measured.
* A catalog or frozen file that spells an interval unit without the flag (a user-written conf) loses the rule until the flag is added: documented.
