# Angle mode: a conversion reads the radian and the steradian as 1, as in the SI (stage A) — design

**Date:** 2026-11-05
**Status:** draft for the maintainer's review. Nothing is implemented. Spike S1 and the mechanism of S2 are open and gate the plan; see "Open before implementation".
**Base:** local `main` @ `b27d0f8` (21 commits ahead of `origin/main`); branch `feat/angle-mode`.
**Follows:** finding `F-0001` of `physure-crucible` (`mechanics.rotating-disk-energy`, release blocker, open).
**Stages:** this is stage **A** of three. **B** (decorated blocks in PHS: `@angles(dimension) { ... }`, chained and nested) and **C** (reusable sets of decorators) get their own specs; A only has to leave the seam they need (see "What A leaves for B and C").

## The problem, and why it is a policy and not a bug

`½·I·ω²` for a 4 kg, 0.5 m disk at 10 rad/s is `25 kg·m²·rad²·s⁻²` and cannot be converted to `J`. Reproduced on `main` @ `b27d0f8` with a freshly built `phs` on 2026-11-05, so the A1 program did not change it.

The cause is the catalog, not an algorithm: `physure.conf` declares `[Dimensions] angle = A, noprefix`, `radian = 1.0, A` and `steradian = 1.0, A, own_kind`, and `RationalUnit::same_dimensions` compares the exponents of the base units, so `rad²` never cancels. It is deliberate (A1 stage 2 made angle and solid angle different kinds; `sin` takes "an angle or a dimensionless value"; `lumen = cd*sr`) and internally coherent: `(10 N*m/rad) * (2 rad) => J` works. But the SI writes none of its formulas that way, so the common ones fail:

| Expression (measured on `phs` @ `b27d0f8`) | Result today |
|---|---|
| `(10 rad/s) * (0.5 m) => m/s` | unit mismatch |
| `(2 m) * (0.5 rad) => m` (arc length) | unit mismatch |
| `(10 N*m) * (3 rad/s) => W` | unit mismatch |
| `(377 rad/s) * (0.1 H) => ohm` (reactance) | unit mismatch |
| `(2 rad) => unity` | unit mismatch |
| `(100 cd) / (2 m)^2 => lx` (inverse-square law) | unit mismatch (the message reads `expected 'lx', got 'lx'`) |
| `(5 lm) => cd` | unit mismatch |
| `(10 rad/s) => Hz` | unit mismatch |

`F-0001` is the only documented finding because the other crucible cases avoid `rad` (`centripetal-force` uses `v`, not `ω·r`).

## Decisions (maintainer, 2026-11-05)

| Question | Decision | Rejected |
|---|---|---|
| Governing rule | **Follow the SI.** Where the SI is explicit, Physure does what it says. Where it is silent, behaviour does not change. | Policies of our own that are stricter than the SI without a source |
| Operators | **Only an explicit conversion identifies `rad` and `sr` with 1.** Sums, differences and comparisons keep refusing every angle against a plain number: `2 rad + 1`, `5 deg + 50 %`, `2 m·rad + 1 m` (the SI's Table 4 gives an expression, which licenses a conversion; it says nothing about operators). | Letting a sum or a comparison ignore the markers when the physical dimension agrees |
| Where the policy lives | **A compatibility list**: catalog data that says, for each pair of kinds and each operation, what is allowed. The default list grants conversions only. | Guards hard-coded in the core |
| Default | The conversion default is the SI's. Strict (angle is a dimension) is the opt-in. | Strict default with an opt-in |
| Ways to opt in | A `[Settings]` entry, a scoped override per call or block of code, and PHS syntax. | Only the global setting |
| PHS syntax | **Decorated blocks**, chainable, nestable, with reusable sets of decorators (stages B and C). | A `with` keyword; a file-level directive |
| Order | A (core) first, then B, then C. | One spec for all |

## What the SI says

Source: BIPM, *The International System of Units*, 9th edition, French edition, §2.3.3 and 2.3.4, Tables 4 and 6 and their footnotes, printed pages 24–29 (my translation). The English edition was not readable (the fetched PDF was truncated), and the Brochure has been revised since (the English cover reads V4.01, June 2026): the wording must be re-read in the current English edition before this spec is final.

- **The unit one is a unit.** §2.3.3: for a quantity defined as a ratio of two quantities of the same kind, "the associated unit is the unit one, symbol 1, although the unit one is rarely written explicitly", and "the unit one is necessarily the neutral element of every system of units: it is automatically present". Physure already has it (`unity`, with `%` and `ppm` as scaled versions). Converting to it is a conversion to a unit, not a cast to nothing. It is the unit of the ratios, though, and an angle is not one of them (next bullet), which is why sums stay refused.
- **Table 4.** Plane angle: radian, `rad`, expression in other SI units **1**. Solid angle: steradian, `sr`, **1**. Footnotes (b) and (c): `rad = m/m` and `sr = m²/m²` are suggested, "but this representation is not intrinsic and could be confusing, since angle is not a quantity of the same kind as other length ratios" (and the solid angle not of the same kind as other area ratios). The radian is also the coherent unit of phase angle: "the phase angle increases by 2π rad each period".
- **§2.3.3.** "For historical and conventional reasons, plane and solid angles are considered in the SI as quantities of unit one. When needed, the symbols rad and sr are written explicitly to emphasise that the quantity considered … is, or involves, plane angle or solid angle."
- **Footnote (d) of Table 4.** "The hertz must only be used for periodic phenomena and the becquerel only for random processes linked to the measurement of radionuclide activity."
- **§2.3.4, p. 29.** "The SI unit of frequency is the hertz; the SI unit of angular velocity and angular frequency is the radian per second. … The use of different names serves to emphasise the difference in nature of the quantities in question. It is particularly important to distinguish frequencies from angular frequencies because their numerical values differ by definition by a factor of 2π (see ISO 80000-3). Ignoring this can cause an error of 2π." It recommends always writing frequency, angular frequency and angular velocity in Hz or rad/s, "but not in s⁻¹".
- **Display.** "In photometry the name and symbol of the steradian are generally maintained in the expression of units" (footnote (g), and Table 6 for radiometry). Physure carries the exponent through arithmetic and shows it, so this holds.
- **Same dimension, different quantity.** "The joule is never used to express a torque", although torque and energy have the same dimension (p. 29). Physure does the opposite today (`N*m` is shown as `J`); it is not an angle matter and stays out of this stage (see the end).
- **VIM 1.2** (JCGM 200, verbatim): "quantities of the same dimension are not necessarily of the same kind"; "two quantities of the same kind are mutually comparable".

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

`[Settings] angle_mode = dimensionless` goes in the embedded `physure.conf` (next to `propagation_mode`) and is read by `apply_setting` with the same reporting of a bad value on stderr, keeping the default. The `physure.conf` of the working directory overrides it, as for every setting. Nothing else about the catalog's units changes: `angle = A`, `radian` and `steradian` stay, because the strict mode needs them.

### The rule

The base units `rad` and `sr` are **markers**: in an expression they say what the quantity involves, and the SI gives both the expression 1. In `Dimensionless` mode a **conversion** may read a marker as 1; nothing else may.

- **Strict compatibility is untouched.** `same_dimensions` stays as it is. Two units whose base exponents are identical (markers included) are compatible for every operation, as today: `rad` with `deg`, `kg·m²·rad²·s⁻²` with `g·m²·rad²·s⁻²`.
- **Otherwise the list decides**, and only when the physical parts agree (the exponents of every base unit other than `rad` and `sr`). A pair whose physical parts differ is a dimension mismatch before the list is read.
- The unit shown stays what the arithmetic produced (`25 kg·m²·rad²·s⁻²`) until a conversion replaces it, and the scale is respected (`deg` is π/180, so `90 deg => unity` is `1.5708`).
- `UnitRegistry::dimension_of` and Python's `Dimension` are **not** changed: the angle stays in them, so everything that decides from a dimension (equality, `is_dimensionless`, the sum in Python) keeps today's answer.

### The dimension-one compatibility list

Catalog data in `physure.conf`, read by every binding:

```
[Compatibility]
# kind, kind = operations granted when the physical parts agree and the markers differ
radian, plain     = convert           # `m·rad/s => m/s`, `2 rad => unity`, `25 kg·m²·rad²·s⁻² => J`
steradian, plain  = convert           # `cd/m² => lx`, `5 lm => cd`
```

That is the whole default list. `level` and `frequency` are kinds so that the list can refuse them with a marker; they need no row of their own, because `20 dB => percent` and `s^-1` against `Hz` are already accepted by the strict comparison, which is read first.

- **Kinds.** The kind of a unit is what its markers leave: `radian` (a `rad` component), `steradian` (an `sr` component), `plain` (neither: `m`, `J`, `unity`, `%`, `ppm`, a bare number), `level` (a logarithmic unit), `frequency` (the hertz, a bare named unit: S2), and `mixed` (`rad·sr`, compatible only with the identical combination). `plain` is the absence of a marker, not the unit one. The unit one is a plain unit like any other.
- **Operations:** `convert` (`=>`, `to`, the registry's plan, the binding of an annotated parameter), `add` (sum and difference), `compare` (`==`, `<`, `approx_eq`, `assert`). Products, quotients and powers are not governed; the existing refusal of compound levels stays.
- **Symmetric, and a pair that is not listed allows nothing.** The default grants conversions only, so these stay refused, as they are today:

| Refused | Because |
|---|---|
| `2 rad + 1`, `5 deg + 50 %`, `1 rad == 1`, `2 m·rad + 1 m`, `25 kg·m²·rad²·s⁻² - 25 J`, `rad² + rad` | `radian, plain` grants no `add` and no `compare`, and `radian, radian` with different exponents is not listed. The SI is silent about operators and its footnote (b) says the angle is not of the same kind as other ratios, so today's behaviour stands. The explicit conversion is the way: `(2 rad => unity) + 1`, `(25 kg·m²·rad²·s⁻² => J) - 25 J`. |
| `rad` with `sr`: `deg => sr`, `rad + sr`, `1 deg + 1 sr`, `rad/s => sr/s` | A1 stage 2 decided it (`arcmin => sr` must be rejected) and the tests pin it; footnotes (b) and (c) say they differ in nature. The relation is deliberately not transitive: `rad` converts to `plain`, `plain` to `sr`, but `rad` not to `sr`. |
| `level` with `radian` or `steradian`: `1 rad => dB` | The SI does not treat levels; the VIM kind rule says a level is the logarithm of a ratio of same-kind quantities. Behaviour does not change (`1 rad => dB` is in `unit_literal_regression.rs` as a refused program). |
| `radian` or `steradian` with `frequency`: `10 rad/s => Hz`, `10 rad/s => kHz` | SI footnote (d) and §2.3.4: the hertz is for periodic phenomena and ignoring the 2π "can cause an error of 2π". Explicit, so the SI wins over the `radian, plain` row. `(2π rad) * (50 Hz) => rad/s` and `(10 rad/s) / (6.283185307179586 rad) => Hz` work, because the markers agree or cancel. A user can still go `10 rad/s => 1/s => Hz`: two explicit steps, each allowed, and the second is how a user says "this is a frequency". The SI recommends against writing `s⁻¹`, and the rule does not pretend otherwise. |

**The mode and the list.** `angle_mode = dimension` does not read the list: angles are dimensions, which is today's behaviour byte for byte. So stage B's `@angles(dimension)` means "no list for this block". A project edits the list in its own `physure.conf` and the behaviour table shows what then flips: deleting `steradian, plain` keeps radiometry strict; granting `radian, plain = convert add compare` would bring back `2 rad + 1`, which is the one thing this default exists to prevent, so the spec's tests pin the default (below).

**Where it lives.** A `[Compatibility]` section parsed by `conf.rs`, with problems reported like the other sections (an unknown kind or operation is an error of the conf, not a silent skip), into a process-wide table that `Quantity::add`, `sub`, `convert_to` and the others consult, since they have no registry. Same caveat as the mode: global state, reset by the `clean_state` fixture. The cost is a scan of a unit's short, sorted base exponents for the marker keys, with no allocation, and only when the strict comparison has already failed.

### The predicate and its callers

A new `compatible(op, a, b)` runs the strict comparison first (identical base exponents: yes, as today) and reads the list only when that fails and the mode is `Dimensionless`. It is called where `same_dimensions` decides today (the nine callers that CodeGraph lists plus the two in `registry.rs`; the plan re-greps for any other before editing):

| Caller | Operation | Note |
|---|---|---|
| `Quantity::convert_to`; `UnitRegistry::conversion`, `factor_between` | `convert` | the paths of `=>`, `to`, the plans Python applies, and the binding of an annotated parameter |
| `Quantity::add`, `sub` | `add` | unchanged by default |
| `Quantity::approx_eq`, `phs_assert`; `physure-script` `comparable` | `compare` | unchanged by default |
| `Quantity::combine_levels` | the level rows | unchanged |
| `Quantity::as_radians` (trig) | none | stays strict: it already accepts `rad`, `deg` and a bare number, and refuses `sin(9.81 m/s^2)` |
| `PyQuantity.__eq__`, Python `Quantity.__eq__` | none | unchanged: `dimension_of` is unchanged |

The sum keeps the unit of the left operand, as today.

### Threads

The override is thread-local, like `PropagationMode`'s, and rayon workers do not inherit it. Left alone, `with angle_mode("dimension"): run(program_with_a_parallel_for)` would evaluate the loop body in the process mode: a wrong answer, not an error. So stage A includes the fix: the two rayon sites (`interpreter/expressions.rs` `eval_for`, `builtins/core.rs` `parallel_map`) read `angle_mode()` before the parallel iterator and open `scoped(mode)` inside each task. The mode is `Copy`, so this is one thread-local write per task.

### Surfaces

| Surface | What A adds |
|---|---|
| `physure.conf` | `[Settings] angle_mode` and the `[Compatibility]` section (everything that reads a conf: PHS, CLI, LSP, Rust API, Python, Java, WASM) |
| Rust | `AngleMode`, `angle_mode()`, `set_angle_mode()`, `scoped()` |
| Python | `physure.angle_mode("dimension")` as a context manager (a `#[pyclass(unsendable)]` holding the guard between `__enter__` and `__exit__`) and a getter; no process-wide setter (a later conf read would undo it, as for `propagation_mode`). The mode in force is recorded in the crucible candidate manifest, so a result can be traced to it. |
| PHS | nothing in A (stage B); programs follow the conf and the surrounding scoped mode |
| Java, WASM | the conf only; no runtime setter in A |

The static checker (`check.rs`) reads dimensions through `dimension_of`/`facts`, which do not change, so its sums and comparisons stay as they are. For `=>` and the binding of an annotated parameter it must ask the list, or it would report an error the interpreter does not give; that is part of spike S1.

## Behaviour

The `Today` column is measured for the rows probed above and for `2 rad + 1` and `1 rad == 1`; the other refusals follow from the strict comparison, which compares the exponents of `rad` and `sr` like any other base unit, and from the existing tests (`unit_literal_regression.rs`, `units/tests.rs`). The `Dimensionless` column is predicted from the code; the crucible case and the probes above are the acceptance test.

| Expression | Today | `Dimensionless` | `Dimension` |
|---|---|---|---|
| `½·I·ω² => J` | error | `25 J` | error |
| `(10 rad/s) * (0.5 m) => m/s` | error | `5 m/s` | error |
| `(377 rad/s) * (0.1 H) => ohm` | error | `37.7 Ω` | error |
| `(100 cd) / (2 m)^2 => lx` | error | `25 lx` | error |
| `(5 lm) => cd` | error | `5 cd` | error |
| `(2 rad) => unity`, `(90 deg) => unity` | error | `2`, `1.5708` | error |
| `(2 rad => unity) + 1` | error | `3` | error |
| `(25 kg·m²·rad²·s⁻² => J) - 25 J` | error | `0` | error |
| `(2 rad) + 1` | error | **error** (unchanged) | error |
| `5 deg + 50 %` | error | **error** (unchanged) | error |
| `1 rad == 1` | error | **error** (unchanged) | error |
| `(2 m·rad) + (1 m)` | error | **error** (unchanged) | error |
| `25 kg·m²·rad²·s⁻² - 25 J` | error | **error** (unchanged; convert first) | error |
| `rad² + rad` | error | **error** (unchanged) | error |
| `(10 rad/s) => Hz` | error | **error** (`radian, frequency` is not listed) | error |
| `(10 rad/s) / (6.283185307179586 rad) => Hz` | error | `1.5915 Hz` | error |
| `deg => sr`, `rad + sr`, `1 deg + 1 sr` | error | **error** (`radian, steradian` is not listed) | error |
| `1 rad => dB` | error | **error** (`level, radian` is not listed) | error |
| `sin(9.81 m/s^2)` | error | error | error |

`Dimension` mode is today's behaviour, and the claim is tested, not argued (see below).

One consequence to see: `(30 W) / (4 sr) / (2 m)^2 => W/m^2` (radiance to irradiance) converts to `W/m^2` without a solid angle. It is the SI's "sr is 1" in an expression, numerically exact, and the SI itself recommends keeping `sr` in the expression for radiometry (footnote (g) and Table 6) precisely because of this. Nothing converts implicitly: the user asks for it with `=>`. `Dimension` mode refuses it, and so does a project that deletes `steradian, plain`.

## Compatibility

The default changes, but only what a conversion may do: no operation that succeeded is refused, no sum or comparison that was refused is allowed, and conversions that were refused can succeed. `Dimension` and `facts` are unchanged. `CHANGELOG.md` gets a `Changed` entry at the top of `[Unreleased]` that says so, names `angle_mode = dimension` as the way back, and says why (the SI, Table 4). `docs/UNITS.md`, `PHYSURE_LANGUAGE_GUIDE.md` and the comment above `radian` in `physure.conf` ("Radians are Angles (A)") are updated. The release that carries it is the maintainer's call.

## Testing

- **Strict is today, byte for byte.** Before touching the core, record the answers of `main` @ `b27d0f8` for a corpus (every expression in the tables above, plus the units tests that mention `rad`, `deg`, `arcmin`, `arcsec` and `sr`); after the change, `Dimension` mode must reproduce them exactly. This is the golden-file method used for the large refactors of A1.
- **The prevented operations are pinned, in both modes.** A table-driven negative test with every refusal of the table above (`2 rad + 1`, `5 deg + 50 %`, `1 rad == 1`, `2 m·rad + 1 m`, `25 kg·m²·rad²·s⁻² - 25 J`, `rad² + rad`, and the same with `sr` and `Hz`), and a differential test: over a generated corpus of unit pairs, every `add`, `sub` and comparison in `Dimensionless` mode gives the answer it gives today. This is what proves that the default flip allows no new sum or comparison. A conf that grants `add` for a pair is the only way to change it, and a test shows that too.
- **The conversions.** Core tests: each converting row of the behaviour table, both modes; `scoped` nests and restores, also on unwind; a bad conf value keeps the default and says so.
- **The list.** Table-driven: for every pair of kinds and every operation, the answer equals the embedded list (a golden test, so a change to the list is a visible diff). The refusals involving the hertz under a prefix (`kHz`) and an alias (`hertz`), and the two allowed spellings; `radian` against `steradian` in a product and in a sum; levels refused with a marker. A conf that edits a row changes exactly the behaviour it governs; a conf with an unknown kind or operation is refused with the line. `Dimension` mode ignores the list.
- **Callers.** One test per row of the callers table.
- **Threads.** A PHS program with a `for` over more than `parallel_threshold` items, run under `scoped(Dimension)`, gives the strict answer (it would give the default answer without the capture).
- **Surfaces.** Python: the context manager (enter, exit, nested, exception inside), and the existing parity suites (`test_unit_algebra_parity`, `test_system_registry_parity`) run in both modes. Tests that pin the old default (to be found by running the suites, not by guessing) are either moved to `Dimension` mode on purpose or rewritten.
- **Cost.** `same_dimensions` stays on the hot path of `add`/`sub` and the list is read only after it fails, so the common case costs nothing; the benchmarks that exist (`.benchmarks/`) are run before and after, and a regression above noise is a blocker.
- **Gates.** `physure-core` and `physure-script` are under the strict Sonar gate: new coverage ≥ 90%, no new violations. Python 3.11–3.14, ruff, `--doctest-modules`, and the `clean_state` fixture: the process-wide mode is global state and must be reset by it.

## `F-0001` and the crucible

The Rust consumer of `mechanics.rotating-disk-energy` converts with `to("J")`, which becomes valid, so it passes with no change. **The PHS surface does not pass by itself.** The crucible's wrapper (`validator/case.py`, line 97) builds `_validator_error = abs(<result> - _validator_reference)`, a subtraction of `kg·m²·rad²·s⁻²` and `J`, and this design keeps that refused, on purpose. The conformance claim of the finding is that the energy "cannot be converted to `J`", so the wrapper should convert the result to the unit the case expects, as the Rust consumers already do: `abs((<result> => <expected unit>) - _validator_reference)`. That is a one-line change in `physure-crucible` (and its `tests/test_case.py`), not in any case file, and it is the maintainer's to approve. Until it is made, the PHS surface of `F-0001` keeps failing even with the fix in Physure.

The resolution of `F-0001` is then a reviewed metadata edit of the finding (`status = "resolved"`, `updated_at`, an addendum to `finding.md` with the run that shows it), made after a new candidate is archived. A case for `Dimension` mode needs a way to select the mode inside the program, which is stage B (`@angles(dimension) { ... }`); until then it is not expressible and is not added. Cases for the frequency rule (`rad/s => Hz` refused; `/ (2π rad)` accepted), for the refused sums (`2 rad + 1`) and for the photometry formulas can be added now, since the default is what they exercise.

## What A leaves for B and C

- **A stackable scope.** `scoped` already restores the previous value, so nested blocks work; B's interpreter opens one per decorated block.
- **Safe parallelism.** The capture at the two rayon sites (above) is the model B reuses for every scoped setting.
- **The checker needs the mode per region.** In A it reads the ambient mode; in B it must walk blocks with the mode of each, or `@angles(dimension)` regions would be checked as `Dimensionless`.
- **Open for B:** whether a function called inside a block uses the caller's mode or the one it was defined under (a function that wants its own mode carries `@angles(...)` itself, like `@implicit_units`); which decorators are legal on a block, a function and an assignment (needed for C's sets); how the transpilers (`codegen/`) treat a block (emit, or refuse with a message).

## Open before implementation

- **S1: every path that converts.** The sites where a unit is converted or an argument is bound to a declared unit: PHS `=>` (`binary_ops.rs`), `Quantity::to`/`convert_to`, the binding of an annotated parameter, the plans that Python applies with NumPy and torch, the static checker's `=>`, and the Java and WASM bridges. For each: does it already go through `convert_to`/`conversion`/`factor_between`, so that one change covers it, or does it compare dimensions on its own? Also the nine callers of `same_dimensions` and the two in `registry.rs`, to confirm the table above. Output: a table in this spec, and the tests that would fail. No production code. (The audit of every "is this dimensionless" consumer is no longer needed: `dimension_of` does not change.)
- **S2: how the check sees the `frequency` kind.** The policy is decided by the SI; what is open is how the check sees the hertz. `RationalUnit` carries a `display_name` that the registry sets for named units (and for prefixed ones), and `registry.conversion` sees the symbols before they are expanded, but `Quantity::convert_to` receives an expanded unit. Options: (a) compare the `display_name` of source and target (fragile for aliases and `s^-1` spellings, simple); (b) a flag on the unit, set from the catalog and carried through prefixes and aliases (robust, touches `RationalUnit`). Output: which one, with a prototype that passes the hertz refusals of the list tests above.

## Not in this stage

- Changing the catalog's `angle = A`; the strict mode needs it.
- A runtime setter for Java and WASM, and any PHS syntax.
- **Found while reading the SI and the code, not angle matters, to be filed in the crucible registry or fixed on their own:** `N*m` is shown as `J` (`torque = 10.0 J` in the PHS output of the `F-0001` case) although "the joule is never used to express a torque"; the catalog has no becquerel, gray or sievert, which the SI names precisely so that they are not written as `s⁻¹` and `J/kg`; the message `expected 'lx', got 'lx'` names the same unit on both sides; and Python's `Quantity.__eq__` compares the raw magnitude of a dimensionless quantity with a number, so `Q_(5, '%') == 5.0` is true (by reading the code; the installed extension could not be imported to run it).
