# The third state of a temperature: point, interval, unknown

Date: 2026-11-20. Branch: `docs/post-release-0.4.0`. Owner's rule (2026-11-20): follow what is physically correct. Follows `2026-11-20-interval-flag-design.md`. Spec and plan in one document.

## 1. The problem

After the interval flag, the core knows two kinds of temperature by the unit (`degC`, `degF`: points, always, `is_affine`; `delta_degC`...: intervals, `RationalUnit.interval`) and treats a plain `K` or `degR` as of UNKNOWN kind, which is right for `300 K` written by hand (the SI uses the kelvin for both). But a `K` that comes out of `20 degC => K` is a KNOWN absolute temperature, and today it is forgotten:

* `((20 degC) => K) < 5 delta_degC` is accepted (an absolute temperature against an interval);
* `T = 20 degC => K; (T + 5 delta_degC) < 5 delta_degC` is accepted;
* `T = 20 degC => K; T => delta_degC` is `293.15 delta_degC`;
* the printed unit of `5 delta_degC + 1 K` (`delta_degC`) and the flag disagree (ROADMAP).

## 2. The design: a kind on the unit

1. **`RationalUnit` carries a temperature kind** with three values: `Unknown` (the default of every unit, including a plain `K`/`degR` and everything that is not a temperature), `Point` and `Interval`. The `interval: bool` field of the previous change becomes this kind (`Interval` is what `interval == true` is); the catalog flag `interval` of a `.conf` line and its meaning do not change. An affine unit (`degC`, `degF`) is a point by its offset whatever its kind field says (keep `is_affine` as the source of truth for those; the kind field matters for units without an offset). Like `interval` and `periodic`, the kind is NOT part of `PartialEq`/`Hash`.
2. **Where a `Point` of K comes from:** converting an affine point to a unit without offset (`20 degC => K`, `68 degF => degR`; `Quantity::convert_to` and `UnitRegistry::conversion`, the same routes that refuse point<->interval today) marks the result `Point`; so does `Point +/- Interval` and `Interval + Point` when the result is in a unit without offset. A `K` written by hand, a constant, a `K` read from a host (Java, a Python float with `'K'`) stay `Unknown`.
3. **The algebra** (the core's unit algebra for `+`, `-`, `*`, `/`, `^`, `sqrt`, `=>`, `Quantity::sum_unit` and its Python mirror):

| Operation | Result kind |
|---|---|
| Point + Interval, Interval + Point, Point - Interval | Point |
| Point - Point | Interval (prints as before: `K`) |
| Interval +/- Interval | Interval |
| Interval or Point +/- Unknown | Unknown |
| Unknown +/- Unknown, Unknown - Point | Unknown |
| Point + Point (both without offset) | Unknown (no refusal: `(T1 + T2) / 2` is an average, not a sum of points; two affine points still refuse as today) |
| Interval * or / a plain number or ratio | Interval |
| Point * or / a plain number or ratio | Unknown (a scaled absolute temperature is not a place on the scale any more; conservative: no false refusal) |
| anything combined with another dimension (`J/(kg*K)`, `delta_degC / m`, `K * s / s`) | Unknown (the kind is dropped, as the flag was) |

4. **The consumers read the kind:** comparison and `max`/`min`/`vector`/ranges (`point_against_interval`, the shared helper `dimensions::mismatch`): a Point (affine or marked) against an Interval is refused, an Unknown is compatible with both; conversion (`convert_to`, `conversion`, Python `.to()`): a Point (affine or marked) does not convert to an Interval unit and an Interval does not convert to a point unit (a marked `Point` K to `delta_degC` is refused; an Unknown K converts both ways as today). The messages are those of the core (`POINT_AGAINST_INTERVAL_ADVICE` etc.); the comparison message names the K point as a temperature point.
5. **Python** mirrors it (`CompoundUnit` carries the kind through `as_interval`/`as_point`, `registry.sum_is_interval` becomes the kind algebra asked of the core, `.to()`, pickling, flyweights: the kind is in the cache key like the flag is today). **Java** carries units as text: unchanged (an open ROADMAP line already says so). WASM keeps core objects. The frozen-file/witness formats do not change (the kind is run-time, not catalog).
6. The checker (`check.rs`) is unchanged (it never flags the pair).

## 3. Tasks (commit each; red tests first; one mutation check per behaviour)

1. **Tests** (red): a core table test with every row of section 2.3 (both operand orders) and the programs of section 1; PHS tests (`physure-script/tests/temperature_kind.rs`): the gas-law and heat programs that must keep working (`T1 = 293.15 K; T1 + 5 delta_degC => degC` is 25 degC; `T = 20 degC => K; T - 5 delta_degC > 10 degC` is True; `Q = m*c*(T2 - T1)`; `(T2 - T1)/(1 s)`; `R*T` in both forms; `T2 - T1 > 5 K`; `(T1 + T2) / 2` for K points is accepted), and the new refusals (`(20 degC => K) < 5 delta_degC`; `T + 5 delta_degC < 5 delta_degC`; `T => delta_degC`); pytest for the Python side.
2. **The field and the conf link**: replace `interval: bool` with the kind where it lives (`rational.rs`, the registry building units from flagged entries, `UnitFacts` keeps `interval` as the catalog fact, the Python `RationalUnit` getter/constructor keyword, stubs); do not change what `is_interval()` answers. Then **the marking and the algebra** (points 2-3), then **the consumers** (point 4), one commit each.
3. **Python** (point 5).
4. **Consumers and full runs**: Rust (`cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast`, `cargo test -p physure --no-fail-fast`), Python (rebuild in this worktree: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`; verify `_core.__file__`; `uv run --no-sync pytest -q`; `ruff check .`; `ruff format --check .`), WASM (`wasm-pack test --node` in `physure-wasm`; golden JS), Java (`cargo build --release -p physure-java`, `mvn -B test -Dphysure.native.dir=D:/Projects/physure/target/release`). Compare with HEAD-before (build `060eb44` in the scratchpad with its own target dir, or reason from `git show`) every expression of the previous change's propagation table: no VALUE and no printed unit may change; list every acceptance that changes and classify it. Size of `RationalUnit` must stay 352 bytes (the kind is one byte in the padding); time 3M `phs_compare` and the first-use budget (`time python -m physure "500 N / 2 m^2 => kPa"`).
5. **Docs**: CHANGELOG `[Unreleased]` (Changed: the kinds and the algebra; Fixed: the holes of section 1), `docs/ROADMAP.md` (remove the open lines this closes: the three-state follow-up and the `delta_degC`-printed-unit inconsistency if closed; keep the others), the guide (`physure-cli/src/main.rs`, then `phs doc --save`), `uv run --no-sync mkdocs build --strict`.

## 4. Not in this change

Java and the text bridges; `(5 delta_degC * 1 s / 1 s)` and `sqrt((5 delta_degC)^2)` (the kind is dropped across dimensions, as documented); a user's own interval unit without the flag; a misspelt flag; the Python hash vs `==`; `np.mean/dot/cross`.

## 5. Risks

* A false refusal if a `Point` is marked where the value is not an absolute temperature: the marking happens only from an affine conversion or from Point +/- Interval, and every scaling or combination with another dimension drops it. Every program of the previous change's propagation table and the heat/gas programs above are the guard.
* The central struct changes again (one byte); `PartialEq`/`Hash` do not include the kind: test that two units equal by `==` with different kinds never leak the wrong kind through a cache (Python flyweights, `get_cached_unit`, conversion plans).
