# Angle mode: the radian is dimension one by default (stage A) — design

**Date:** 2026-11-05
**Status:** draft for the maintainer's review. Nothing is implemented.
**Base:** local `main` @ `b27d0f8` (21 commits ahead of `origin/main`); branch `feat/angle-mode`.
**Follows:** finding `F-0001` of `physure-crucible` (`mechanics.rotating-disk-energy`, release blocker, open).
**Stages:** this is stage **A** of three. **B** (decorated blocks in PHS: `@angles(dimension) { ... }`, chained and nested) and **C** (reusable sets of decorators) get their own specs; A only has to leave the seam they need (see "What A leaves for B and C").

## The problem, and why it is a policy and not a bug

`½·I·ω²` for a 4 kg, 0.5 m disk at 10 rad/s is `25 kg·m²·rad²·s⁻²` and cannot be converted to `J`. Reproduced on `main` @ `b27d0f8` with a freshly built `phs` on 2026-11-05, so the A1 program did not change it.

The cause is the catalog, not an algorithm: `physure.conf` declares `[Dimensions] angle = A, noprefix` and `radian = 1.0, A`, and `RationalUnit::same_dimensions` compares the exponents of the base units, so `rad²` never cancels. It is deliberate (A1 stage 2 made angle and solid angle different kinds; `sin` takes "an angle or a dimensionless value"; `lumen = cd*sr`), and it is internally coherent: `(10 N*m/rad) * (2 rad) => J` works. But no textbook writes a torque in `N·m/rad`, so every common angular formula fails:

| Expression (measured on `phs` @ `b27d0f8`) | Result today |
|---|---|
| `(10 rad/s) * (0.5 m) => m/s` | unit mismatch |
| `(2 m) * (0.5 rad) => m` (arc length) | unit mismatch |
| `(10 N*m) * (3 rad/s) => W` | unit mismatch |
| `(377 rad/s) * (0.1 H) => ohm` (reactance) | unit mismatch |
| `(2 rad) => unity`, `(2 rad) + 1` | unit mismatch |
| `(10 rad/s) => Hz` | unit mismatch |

`F-0001` is the only documented finding because the other crucible cases avoid `rad` (`centripetal-force` uses `v`, not `ω·r`).

## Decisions (maintainer, 2026-11-05)

| Question | Decision | Rejected |
|---|---|---|
| Default | **The most common use is the default: the radian is dimension one (SI).** Strict (angle is a dimension) is the opt-in. | Strict default with an opt-in |
| Ways to opt in | A `[Settings]` entry, a scoped override per call or block of code, and PHS syntax. | Only the global setting |
| PHS syntax | **Decorated blocks**, chainable, nestable, with reusable sets of decorators (stages B and C). | A `with` keyword; a file-level directive |
| Order | A (core) first, then B, then C. | One spec for all |

## The design

### The mode

`physure-core/src/units/angle_mode.rs`, a copy of the shape of `uncertainty/mode.rs` (`PropagationMode`):

```rust
pub enum AngleMode { Dimensionless /* default */, Dimension }
// FromStr: "dimensionless" | "dimension"      as_u8 / from_u8
pub fn angle_mode() -> AngleMode              // thread override, else the process value
pub fn set_angle_mode(m: AngleMode) -> AngleMode   // what reading a conf calls
pub fn scoped(m: AngleMode) -> AngleModeGuard      // thread-local; Drop restores the previous one, so guards nest
```

`[Settings] angle_mode = dimensionless` goes in the embedded `physure.conf` (next to `propagation_mode`) and is read by `apply_setting` with the same reporting of a bad value on stderr, keeping the default. The `physure.conf` of the working directory overrides it, as for every setting. Nothing else about the catalog changes: `angle = A` and `radian = 1.0, A` stay, because the strict mode and the steradian need them.

### The rule

In `Dimensionless` mode two units are compatible when their base exponents agree **ignoring the base unit `rad`**. `rad` is still carried through arithmetic and shown (`25 kg·m²·rad²·s⁻²`) until a conversion replaces the unit. The scale is respected: `deg` is π/180, and `90 deg => unity` is `1.5708`.

- `RationalUnit::same_dimensions` becomes mode-aware. The old comparison stays as `same_dimensions_strict`. The key `"rad"` is the base unit that `register_fallback_bases` (`conf.rs`) always registers for the angle dimension, so the literal is safe for the embedded catalog and any conf that extends it. A conf that maps the angle dimension to another base unit would not get the equivalence; that is accepted and stated here rather than made configurable.
- **Levels never take part.** If either unit is logarithmic the comparison is strict, so `1 rad => dB` stays refused (it is in `unit_literal_regression.rs` as a refused program).
- **The steradian never takes part.** Only the key `rad` is ignored, so `deg => sr`, `rad + sr` and `1 deg + 1 sr` stay refused.
- `UnitRegistry::dimension_of` skips the contribution of the base unit `rad` in this mode (it does not skip the dimension symbol `A`, which `sr` shares). Every consumer of `dimension_of`/`facts` then agrees with what converts: Python's `Dimension`, `Quantity.dimension`, and the static checker of `physure-script`.

Which callers use which predicate (the nine callers that CodeGraph lists for `same_dimensions` plus the two in `registry.rs`; the plan re-greps for any other before editing):

| Caller | Predicate | Why |
|---|---|---|
| `Quantity::add`, `sub`, `convert_to`, `approx_eq`, `phs_assert`, `combine_levels`; `UnitRegistry::conversion`, `factor_between`; `physure-script` `comparable` | mode-aware | they ask "can these be combined / converted" |
| `Quantity::as_radians` (trig) | mode-aware | it asks "is this an angle"; `sin(9.81 m/s^2)` is still refused |
| Python `PyQuantity.__eq__` against a bare number | **strict** | equality is not compatibility: `Q(5, 'deg') == 5.0` must not become true |

The sum keeps the unit of the left operand, as today: `(1 m) + (1 m*rad)` is `2 m`.

### Threads

The override is thread-local, like `PropagationMode`'s, and rayon workers do not inherit it. Left alone, `with angle_mode("dimension"): run(program_with_a_parallel_for)` would evaluate the loop body in the process mode: a wrong answer, not an error. So stage A includes the fix: the two rayon sites (`interpreter/expressions.rs` `eval_for`, `builtins/core.rs` `parallel_map`) read `angle_mode()` before the parallel iterator and open `scoped(mode)` inside each task. The mode is `Copy`, so this is one thread-local write per task.

### Surfaces

| Surface | What A adds |
|---|---|
| `physure.conf` | `angle_mode` (everything that reads a conf: PHS, CLI, LSP, Rust API, Python, Java, WASM) |
| Rust | `AngleMode`, `angle_mode()`, `set_angle_mode()`, `scoped()` |
| Python | `physure.angle_mode("dimension")` as a context manager (a `#[pyclass(unsendable)]` holding the guard between `__enter__` and `__exit__`) and a getter; no process-wide setter (a later conf read would undo it, as for `propagation_mode`) |
| PHS | nothing in A (stage B); programs follow the conf and the surrounding scoped mode |
| Java, WASM | the conf only; no runtime setter in A |

The static checker (`check.rs`) needs no change: it reads dimensions through `dimension_of`/`facts`. Its soundness contract (it reports an error only when the interpreter is certain to refuse) holds because both read the same mode.

## Behaviour

The `Today` column is measured for the rows probed above and known from the existing tests for the refusals (`unit_literal_regression.rs`, `units/tests.rs`). The `Dimensionless` column is predicted from the code; the crucible case and the probes above are the acceptance test.

| Expression | Today | `Dimensionless` | `Dimension` |
|---|---|---|---|
| `½·I·ω² => J` | error | `25 J` | error |
| `(10 rad/s) * (0.5 m) => m/s` | error | `5 m/s` | error |
| `(377 rad/s) * (0.1 H) => ohm` | error | `37.7 Ω` | error |
| `(10 rad/s) => Hz` | error | `10 Hz` (the SI reading; the 2π trap) | error |
| `(2 rad) => unity`, `(90 deg) => unity` | error | `2`, `1.5708` | error |
| `5 deg + 50 %` | error | accepted (both are dimension one) | error |
| `deg => sr`, `rad + sr`, `1 deg + 1 sr` | error | error | error |
| `1 rad => dB` | error | error | error |
| `sin(9.81 m/s^2)` | error | error | error |

`Dimension` mode is today's behaviour, and the claim is tested, not argued (see below).

## Compatibility

This changes the default, so it is a breaking change of 0.3.x behaviour. The equivalence only widens what is compatible: no operation that succeeded is refused, and operations that were refused can succeed. Two things can still differ for code that worked: code that relied on the error (a `try`/`except` around a conversion), and code that inspects `Dimension` or `facts`, which no longer reports the angle for a quantity in `rad`. `CHANGELOG.md` gets a `Changed` entry at the top of `[Unreleased]` that says so, names `angle_mode = dimension` as the way back, and says why (the SI reading; the textbook formulas). `docs/UNITS.md`, `PHYSURE_LANGUAGE_GUIDE.md` and the comment above `radian` in `physure.conf` ("Radians are Angles (A)") are updated. The release that carries it is the maintainer's call.

## Testing

- **Strict is today, byte for byte.** Before touching the core, record the answers of `main` @ `b27d0f8` for a corpus (every expression in the tables above, plus the units tests that mention `rad`, `deg`, `arcmin`, `arcsec` and `sr`); after the change, `Dimension` mode must reproduce them exactly. This is the golden-file method used for the large refactors of A1.
- **The rule.** Core tests: each row of the behaviour table, both modes; `scoped` nests and restores, also on unwind; a bad conf value keeps the default and says so.
- **Callers.** One test per row of the callers table, including the strict `__eq__` and the levels/steradian refusals.
- **Threads.** A PHS program with a `for` over more than `parallel_threshold` items, run under `scoped(Dimension)`, gives the strict answer (it would give the default answer without the capture).
- **Surfaces.** Python: the context manager (enter, exit, nested, exception inside), and the existing parity suites (`test_unit_algebra_parity`, `test_system_registry_parity`) run in both modes. Tests that pin the old default (to be found by running the suites, not by guessing) are either moved to `Dimension` mode on purpose or rewritten.
- **Cost.** `same_dimensions` is on the hot path of `add`/`sub`. The benchmarks that exist (`.benchmarks/`) are run before and after; a regression above noise is a blocker. Neither operand carrying `rad` should cost one extra comparison.
- **Gates.** `physure-core` and `physure-script` are under the strict Sonar gate: new coverage ≥ 90%, no new violations. Python 3.11–3.14, ruff, `--doctest-modules`, and the `clean_state` fixture: the process-wide mode is global state and must be reset by it.

## `F-0001` and the crucible

With the default flipped, `mechanics.rotating-disk-energy` should pass with no change to the case. The resolution is a reviewed metadata edit of the finding (`status = "resolved"`, `updated_at`, an addendum to `finding.md` with the run that shows it), made in `physure-crucible` after a new candidate is archived. A case for `Dimension` mode needs a way to select the mode inside the program, which is stage B (`@angles(dimension) { ... }`); until then it is not expressible and is not added.

## What A leaves for B and C

- **A stackable scope.** `scoped` already restores the previous value, so nested blocks work; B's interpreter opens one per decorated block.
- **Safe parallelism.** The capture at the two rayon sites (above) is the model B reuses for every scoped setting.
- **The checker needs the mode per region.** In A it reads the ambient mode; in B it must walk blocks with the mode of each, or `@angles(dimension)` regions would be checked as `Dimensionless`.
- **Open for B:** whether a function called inside a block uses the caller's mode or the one it was defined under (a function that wants its own mode carries `@angles(...)` itself, like `@implicit_units`); which decorators are legal on a block, a function and an assignment (needed for C's sets); how the transpilers (`codegen/`) treat a block (emit, or refuse with a message).

## Not in this stage

- The steradian in `Dimensionless` mode (`lm ↔ cd`); it stays strict until someone has a case that needs it.
- Changing the catalog's `angle = A`; the strict mode and `sr` need it.
- A runtime setter for Java and WASM, and any PHS syntax.
- Special-casing `rad/s ↔ Hz`. It is the SI reading, it follows from the rule, and `Dimension` mode is the protection for whoever wants the 2π caught.
