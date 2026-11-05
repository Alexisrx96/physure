# Angle mode: the radian and the steradian are unit one by default, as in the SI (stage A) — design

**Date:** 2026-11-05
**Status:** draft for the maintainer's review. Nothing is implemented. Spike S1 and the mechanism of S2 are open and gate the plan; see "Open before implementation".
**Base:** local `main` @ `b27d0f8` (21 commits ahead of `origin/main`); branch `feat/angle-mode`.
**Follows:** finding `F-0001` of `physure-crucible` (`mechanics.rotating-disk-energy`, release blocker, open).
**Stages:** this is stage **A** of three. **B** (decorated blocks in PHS: `@angles(dimension) { ... }`, chained and nested) and **C** (reusable sets of decorators) get their own specs; A only has to leave the seam they need (see "What A leaves for B and C").

## The problem, and why it is a policy and not a bug

`½·I·ω²` for a 4 kg, 0.5 m disk at 10 rad/s is `25 kg·m²·rad²·s⁻²` and cannot be converted to `J`, nor subtracted from one (the failing PHS statement of the archived report is `abs(result - _validator_reference)`, a subtraction). Reproduced on `main` @ `b27d0f8` with a freshly built `phs` on 2026-11-05, so the A1 program did not change it.

The cause is the catalog, not an algorithm: `physure.conf` declares `[Dimensions] angle = A, noprefix`, `radian = 1.0, A` and `steradian = 1.0, A, own_kind`, and `RationalUnit::same_dimensions` compares the exponents of the base units, so `rad²` never cancels. It is deliberate (A1 stage 2 made angle and solid angle different kinds; `sin` takes "an angle or a dimensionless value"; `lumen = cd*sr`) and internally coherent: `(10 N*m/rad) * (2 rad) => J` works. But the SI writes none of its formulas that way, so the common ones fail:

| Expression (measured on `phs` @ `b27d0f8`) | Result today |
|---|---|
| `(10 rad/s) * (0.5 m) => m/s` | unit mismatch |
| `(2 m) * (0.5 rad) => m` (arc length) | unit mismatch |
| `(10 N*m) * (3 rad/s) => W` | unit mismatch |
| `(377 rad/s) * (0.1 H) => ohm` (reactance) | unit mismatch |
| `(2 rad) => unity`, `(2 rad) + 1` | unit mismatch |
| `(100 cd) / (2 m)^2 => lx` (inverse-square law) | unit mismatch (the message reads `expected 'lx', got 'lx'`) |
| `(5 lm) => cd` | unit mismatch |
| `(10 rad/s) => Hz` | unit mismatch |

`F-0001` is the only documented finding because the other crucible cases avoid `rad` (`centripetal-force` uses `v`, not `ω·r`).

## Decisions (maintainer, 2026-11-05)

| Question | Decision | Rejected |
|---|---|---|
| Governing rule | **Follow the SI.** Where the SI is explicit, Physure does what it says. Where it is silent, behaviour does not change. | Policies of our own that are stricter than the SI without a source |
| Where the policy lives | **A compatibility list**: catalog data that says which kinds of pure quantity of unit one may be converted, added or compared. Proposed by the maintainer; refined after the maintainer's objection that a unit one is not something to convert to (see "Two layers"). | Guards hard-coded in the core |
| Default | The radian and the steradian are unit one (SI Table 4). Strict (angle is a dimension) is the opt-in. | Strict default with an opt-in |
| Ways to opt in | A `[Settings]` entry, a scoped override per call or block of code, and PHS syntax. | Only the global setting |
| PHS syntax | **Decorated blocks**, chainable, nestable, with reusable sets of decorators (stages B and C). | A `with` keyword; a file-level directive |
| Order | A (core) first, then B, then C. | One spec for all |

## What the SI says

Source: BIPM, *The International System of Units*, 9th edition, French edition, §2.3.3 and 2.3.4, Tables 4 and 6 and their footnotes, printed pages 24–29 (my translation). The English edition was not readable (the fetched PDF was truncated), and the Brochure has been revised since (the English cover reads V4.01, June 2026): the wording must be re-read in the current English edition before this spec is final.

- **The unit one is a unit.** §2.3.3: for a quantity defined as a ratio of two quantities of the same kind, "the associated unit is the unit one, symbol 1, although the unit one is rarely written explicitly", and "the unit one is necessarily the neutral element of every system of units: it is automatically present". Physure already has it (`unity`, with `%` and `ppm` as scaled versions). Converting to it is a conversion to a unit, not a cast to nothing; but it is the unit of the ratios, and an angle is not one of them (next bullet).
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

### Two layers

A first version of this spec had one rule and a list of kinds in which "plain" was called `one` and covered every unit without an angle component, `m` and `J` included. That made a row such as `radian, one = convert add compare` read as "convert to the unit one" and licensed `2 rad + 1` and `5 deg + 50 %`, which the SI never licenses and today's behaviour refuses. The maintainer objected that a one is not something to convert to. The unit one is a real unit (above), but the objection found the real flaw: two different situations were in one rule. They are now two layers.

**Layer 1: a marker in an expression.** When a unit has a physical dimension (some base exponent other than `rad` and `sr`), `rad` and `sr` are **markers** that say what the quantity involves (§2.3.3: "the symbols rad and sr are written explicitly to emphasise…"). Table 4 gives both the expression 1, so two such units are compatible when their physical parts agree, whatever their markers, for every operation: convert, add and compare. `kg·m²·rad²·s⁻²` and `J` are compatible; so are `m·rad/s` and `m/s`, and `cd/m²` and `lx`. This is the layer that fixes `F-0001`, including its subtraction. Markers are still carried through arithmetic and shown (`25 kg·m²·rad²·s⁻²`) until a conversion replaces the unit, and the scale is respected (`deg` is π/180).

**Layer 2: kinds of pure unit one.** When both units have no physical dimension, the quantities are pure ratios, and the SI's own footnotes (b) and (c) say an angle is not of the same kind as other ratios. Here the **compatibility list** decides, and its default is today's behaviour plus the one thing Table 4 licenses, an explicit conversion into the plain unit: same kind, anything (`rad` with `deg`, `rad²` with `rad`, `%` with `unity`); different kinds, only what the list names.

### The dimension-one compatibility list

Catalog data in `physure.conf`, read by every binding:

```
[Compatibility]
markers  = radian, steradian        # base units that are 1 in an expression (SI Table 4)
periodic = hertz                    # SI Table 4 (d): not identified with a unit that has another marker
# between pure quantities of unit one: kind, kind = operations
radian, ratio     = convert         # `2 rad => unity` is 2; `2 rad + 1` is not allowed
steradian, ratio  = convert
level, ratio      = convert         # as today: `20 dB => percent`
```

- **Kinds** of a pure unit one, read from its base exponents: `radian` (a `rad` component), `steradian` (an `sr` component), `ratio` (neither: `unity`, `%`, `ppm`, a bare number), `level` (a logarithmic unit), and `mixed` (`rad·sr`), compatible only with the identical combination.
- **Operations:** `convert` (`=>`, `to`, the registry's plan), `add` (sum and difference), `compare` (`==`, `<`, `approx_eq`, `assert`). Products, quotients and powers are not governed; the existing refusal of compound levels stays.
- **Symmetric, and a pair that is not listed allows nothing.** Not listed, so refused:

| Pair or case | Refused because |
|---|---|
| `radian` with `ratio` in a sum or a comparison: `2 rad + 1`, `5 deg + 50 %`, `1 rad == 1` | The SI is silent about operators and its footnote (b) says the angle is not of the same kind as other ratios, so today's behaviour stands. The explicit conversion is the way: `(2 rad => unity) + 1`. |
| `radian` with `steradian` | A1 stage 2 decided it (`arcmin => sr` must be rejected) and the tests pin it; footnotes (b) and (c) say they differ in nature. The relation is deliberately not transitive: `rad` converts to `ratio`, `ratio` to `sr`, but `rad` not to `sr`. |
| `level` with `radian` or `steradian` | The SI does not treat levels; the VIM kind rule says a level is the logarithm of a ratio of same-kind quantities. Behaviour does not change (`1 rad => dB` is in `unit_literal_regression.rs` as a refused program). |
| `10 rad/s => Hz`, `10 rad/s => kHz` (the `periodic` unit against a unit with a different marker) | SI footnote (d) and §2.3.4: the hertz is for periodic phenomena and ignoring the 2π "can cause an error of 2π". Explicit, so the SI wins over layer 1. `(2π rad) * (50 Hz) => rad/s` and `(10 rad/s) / (6.283185307179586 rad) => Hz` work, because the markers agree or cancel. |

**The mode and the list.** `angle_mode = dimension` consults neither layer: angles are dimensions, which is today's behaviour byte for byte. So stage B's `@angles(dimension)` means "no markers and no list for this block". A project edits the data in its own `physure.conf`, and the behaviour table shows what then flips. For example, removing `steradian` from `markers` keeps radiometry strict: `(100 cd) / (2 m)^2 => lx` and radiance to irradiance go back to refused; adding `radian, ratio = convert add compare` would bring back `2 rad + 1`.

**Where it lives.** A `[Compatibility]` section parsed by `conf.rs`, with problems reported like the other sections (an unknown kind, operation or unit is an error of the conf, not a silent skip), into a process-wide table that `Quantity::add`, `sub`, `convert_to` and the others consult, since they have no registry. Same caveat as the mode: global state, reset by the `clean_state` fixture. The cost is a scan of a unit's short, sorted base exponents for the marker keys, with no allocation.

### The predicate and its callers

`RationalUnit::same_dimensions` stays as it is and is renamed `same_dimensions_strict`; a new mode-aware predicate applies the two layers and takes the operation. `UnitRegistry::dimension_of` skips the marker base units in `Dimensionless` mode, so the dimension symbol `A` never appears for them, and every consumer of `dimension_of`/`facts` (Python's `Dimension` and `Quantity.dimension`, the static checker) agrees with layer 1. Which callers use which predicate (the nine callers that CodeGraph lists for `same_dimensions` plus the two in `registry.rs`; the plan re-greps for any other before editing):

| Caller | Predicate | Why |
|---|---|---|
| `Quantity::add`, `sub`, `convert_to`, `approx_eq`, `phs_assert`, `combine_levels`; `UnitRegistry::conversion`, `factor_between`; `physure-script` `comparable` | mode-aware, with the operation: `add` for `add`/`sub`; `convert` for `convert_to`, `conversion`, `factor_between`; `compare` for `approx_eq`, `phs_assert`, `comparable`; `combine_levels` follows the level rows | they ask "can these be combined / converted" |
| `Quantity::as_radians` (trig) | mode-aware | it asks "is this an angle"; `sin(9.81 m/s^2)` is still refused |
| `PyQuantity.__eq__` (core wrapper, `physure-python/src/lib.rs`) against a bare number | **strict** | equality is not compatibility |
| Python `Quantity.__eq__` (`quantity.py`, the class users hold) | **rewritten**, see below | it tests `dimension.is_dimensionless` and then compares the raw magnitude |

The sum keeps the unit of the left operand, as today: `(1 m) + (1 m*rad)` is `2 m`.

**The Python equality is wrong today and the default flip would widen it.** `Quantity.__eq__` returns `self.magnitude == other` for a dimensionless quantity, ignoring the unit's scale, so `Q_(5, '%') == 5.0` is already true (5 % is 0.05). By reading the code, not by running it: the compiled extension in the main checkout predates `unit_text` and cannot be imported. Once `dimension_of` drops `rad`, `Q_(5, 'deg') == 5.0` would become true as well. The fix is to compare after converting to the unit one (`self.to(unity).magnitude == other`), so `Q_(5, 'deg') == 5.0` is false, `Q_(1, 'rad') == 1.0` is true and `Q_(5, '%') == 0.05` is true. Between two quantities the same method converts with `other.to(self.unit)`, which is the `convert` operation, so `Q_(1, 'rad') == Q_(1, 'unity')` would be true while the core's `compare` refuses the pair; equality has to ask `compare`. This is a behaviour fix of its own and goes in the changelog as such. The wider question, every other place that asks "is this dimensionless", is spike S1.

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

The static checker (`check.rs`) needs no change for layer 1: it reads dimensions through `dimension_of`/`facts`. It does need layer 2 and the `periodic` rule (it must not report an error the interpreter accepts, and must report the ones it refuses); that is part of spike S1.

## Behaviour

The `Today` column is measured for the rows probed above and known from the existing tests for the refusals (`unit_literal_regression.rs`, `units/tests.rs`). The `Dimensionless` column is predicted from the code; the crucible case and the probes above are the acceptance test.

| Expression | Today | `Dimensionless` | `Dimension` |
|---|---|---|---|
| `½·I·ω² => J` | error | `25 J` (layer 1) | error |
| `25 kg·m²·rad²·s⁻² - 25 J` (the PHS check of `F-0001`) | error | `0` (layer 1) | error |
| `(10 rad/s) * (0.5 m) => m/s` | error | `5 m/s` (layer 1) | error |
| `(377 rad/s) * (0.1 H) => ohm` | error | `37.7 Ω` (layer 1) | error |
| `(100 cd) / (2 m)^2 => lx` | error | `25 lx` (layer 1) | error |
| `(5 lm) => cd` | error | `5 cd` (layer 1) | error |
| `(2 rad) => unity`, `(90 deg) => unity` | error | `2`, `1.5708` (list: `radian, ratio = convert`) | error |
| `(2 rad) + 1`, `5 deg + 50 %`, `1 rad == 1` | error | **error** (unchanged: only `convert` is listed) | error |
| `(2 rad => unity) + 1` | error | `3` | error |
| `(10 rad/s) => Hz` | error | **error** (`periodic`) | error |
| `(10 rad/s) / (6.283185307179586 rad) => Hz` | error | `1.5915 Hz` | error |
| `deg => sr`, `rad + sr`, `1 deg + 1 sr` | error | **error** (`radian, steradian` is not listed) | error |
| `1 rad => dB` | error | **error** (`level, radian` is not listed) | error |
| `sin(9.81 m/s^2)` | error | error | error |

`Dimension` mode is today's behaviour, and the claim is tested, not argued (see below).

One consequence to see: `(30 W) / (4 sr) / (2 m)^2 => W/m^2` (radiance to irradiance) becomes `W/m^2` without a solid angle. It is the SI's "sr is 1" in an expression, numerically exact, and the SI itself recommends keeping `sr` in the expression for radiometry (footnote (g) and Table 6) precisely because of this. The mode keeps it in the unit and the display; `Dimension` mode refuses the conversion, and so does a project that removes `steradian` from `markers`.

## Compatibility

This changes the default, so it is a breaking change of 0.3.x behaviour. The equivalence only widens what is compatible: no operation that succeeded is refused, and operations that were refused can succeed (a quantity with a physical dimension and a marker, and an explicit conversion of a pure angle to a plain ratio). Sums and comparisons of a pure angle with a plain number stay refused. Two things can still differ for code that worked: code that relied on the error (a `try`/`except` around a conversion), and code that inspects `Dimension` or `facts`, which no longer reports the angle for a quantity in `rad` or `sr`. `CHANGELOG.md` gets a `Changed` entry at the top of `[Unreleased]` that says so, names `angle_mode = dimension` as the way back, says why (the SI, Table 4), and a `Fixed` entry for the equality of a dimensionless quantity with a number. `docs/UNITS.md`, `PHYSURE_LANGUAGE_GUIDE.md` and the comment above `radian` in `physure.conf` ("Radians are Angles (A)") are updated. The release that carries it is the maintainer's call.

## Testing

- **Strict is today, byte for byte.** Before touching the core, record the answers of `main` @ `b27d0f8` for a corpus (every expression in the tables above, plus the units tests that mention `rad`, `deg`, `arcmin`, `arcsec` and `sr`); after the change, `Dimension` mode must reproduce them exactly. This is the golden-file method used for the large refactors of A1.
- **The layers.** Core tests: each row of the behaviour table, both modes; `scoped` nests and restores, also on unwind; a bad conf value keeps the default and says so.
- **The list.** Table-driven: for every pair of kinds and every operation, the answer equals the embedded list (a golden test, so a change to the list is a visible diff). The `periodic` rule with the hertz under a prefix (`kHz`) and an alias (`hertz`), and the two allowed spellings; `radian` against `steradian` in a product and in a sum; the level rows unchanged. A conf that edits `markers` or a row changes exactly the behaviour it governs; a conf with an unknown kind, operation or unit is refused with the line. `Dimension` mode ignores both.
- **Callers.** One test per row of the callers table, including the strict `PyQuantity.__eq__` and the rewritten Python equality (both with a number and between quantities).
- **Threads.** A PHS program with a `for` over more than `parallel_threshold` items, run under `scoped(Dimension)`, gives the strict answer (it would give the default answer without the capture).
- **Surfaces.** Python: the context manager (enter, exit, nested, exception inside), and the existing parity suites (`test_unit_algebra_parity`, `test_system_registry_parity`) run in both modes. Tests that pin the old default (to be found by running the suites, not by guessing) are either moved to `Dimension` mode on purpose or rewritten.
- **Cost.** `same_dimensions` is on the hot path of `add`/`sub`. The benchmarks that exist (`.benchmarks/`) are run before and after; a regression above noise is a blocker. Neither operand carrying a marker should cost one extra comparison.
- **Gates.** `physure-core` and `physure-script` are under the strict Sonar gate: new coverage ≥ 90%, no new violations. Python 3.11–3.14, ruff, `--doctest-modules`, and the `clean_state` fixture: the process-wide mode is global state and must be reset by it.

## `F-0001` and the crucible

With the default flipped, `mechanics.rotating-disk-energy` should pass with no change to the case, on both surfaces: the Rust consumer converts with `to("J")` and the PHS program subtracts, and both are layer 1. The resolution is a reviewed metadata edit of the finding (`status = "resolved"`, `updated_at`, an addendum to `finding.md` with the run that shows it), made in `physure-crucible` after a new candidate is archived. A case for `Dimension` mode needs a way to select the mode inside the program, which is stage B (`@angles(dimension) { ... }`); until then it is not expressible and is not added. Cases for the `periodic` rule (`rad/s => Hz` refused; `/ (2π rad)` accepted), for the refused sums (`2 rad + 1`) and for the photometry formulas can be added now, since the default is what they exercise.

## What A leaves for B and C

- **A stackable scope.** `scoped` already restores the previous value, so nested blocks work; B's interpreter opens one per decorated block.
- **Safe parallelism.** The capture at the two rayon sites (above) is the model B reuses for every scoped setting.
- **The checker needs the mode per region.** In A it reads the ambient mode; in B it must walk blocks with the mode of each, or `@angles(dimension)` regions would be checked as `Dimensionless`.
- **Open for B:** whether a function called inside a block uses the caller's mode or the one it was defined under (a function that wants its own mode carries `@angles(...)` itself, like `@implicit_units`); which decorators are legal on a block, a function and an assignment (needed for C's sets); how the transpilers (`codegen/`) treat a block (emit, or refuse with a message).

## Open before implementation

- **S1: every consumer of "is this dimensionless".** About 10 sites in `physure-python` (`is_dimensionless`) and about 25 production sites in `physure-core` and `physure-script` (`dimensions.is_empty()`, `is_plain_number()`, `dimensionless()`). For each: does it follow the mode (it asks about compatibility) or stay strict (it asks about identity or about a pure number: equality with a bare number, `exp`/`ln` arguments, `float(q)`, a `pow` exponent)? Where does the list have to be consulted (the checker, the registry plan, `convert_to`, the two Python bridges), and does any Python decision made from `Dimension` equality bypass it (a sum of `2 rad` and `1` must still be refused)? Output: a table in this spec, and the tests that would fail. No production code.
- **S2: how the check sees the `periodic` unit.** The policy is decided by the SI; what is open is how the check sees the hertz. `RationalUnit` carries a `display_name` that the registry sets for named units (and for prefixed ones), and `registry.conversion` sees the symbols before they are expanded, but `Quantity::convert_to` receives an expanded unit. Options: (a) compare the `display_name` of source and target (fragile for aliases and `s^-1` spellings, simple); (b) a flag on the unit, set from `periodic =` in the catalog and carried through prefixes and aliases (robust, touches `RationalUnit`). Output: which one, with a prototype that passes the `periodic` rows of the list tests above.

## Not in this stage

- Changing the catalog's `angle = A`; the strict mode needs it.
- A runtime setter for Java and WASM, and any PHS syntax.
- **Found while reading the SI, not angle matters, to be filed in the crucible registry:** `N*m` is shown as `J` (`torque = 10.0 J` in the PHS output of the `F-0001` case) although "the joule is never used to express a torque"; the catalog has no becquerel, gray or sievert, which the SI names precisely so that they are not written as `s⁻¹` and `J/kg`; and the message `expected 'lx', got 'lx'` names the same unit on both sides.
