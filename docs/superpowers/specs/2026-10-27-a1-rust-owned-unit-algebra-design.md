# A1 — Rust owns the unit algebra — design

**Date:** 2026-10-27
**Status:** design approved section by section by the maintainer; spec awaiting review
**Base:** local `main` @ `0ac08a3` (PR-1, PR-2 and PR-3 of the repo-hygiene work merged, not yet on `origin/main`)
**Follows:** `2026-10-27-repo-hygiene-onboarding-design.md` (this is its deferred item A1)

## Goal

Remove the second implementation of unit and dimension algebra that lives in
`physure-python/physure/domain/measurement`, so that `physure-core` is the only
place where "what is this unit, what is its dimension, and how do I convert it"
is decided. A contributor should be able to answer those questions by reading
Rust only, and a wrong answer should be fixable in one place.

## Decisions taken with the maintainer

| Question | Answer |
|---|---|
| How much public API may change? | **Replace the Python classes with the Rust types.** `CompoundUnit` and `Dimension` stay importable as deprecated aliases for one release (the project is 0.x beta). |
| How is the work delivered? | **Stages with a safety net**: one PR per stage, suite green after each, characterization tests first. |

## Non-goals

- **A2**, the `application/` / `domain/` / `core/` import cycles. It gets its own spec once A1 has removed the classes that cause part of the cycle (for example `domain → application` imports for `parse_unit_string` and `get_default_system`).
- A Rust `Quantity` that holds arrays. Rust `Quantity` stays scalar-only; arrays, autograd and the NumPy/Torch/JAX backends stay in Python.
- Publishing releases or pushing branches.

## Evidence (measured on `0ac08a3`)

| Fact | Where |
|---|---|
| Python's unit class is a **hybrid**: `class CompoundUnit(BaseExponentEntity, RationalUnit)`. It subclasses the Rust `RationalUnit`, keeps its own `exponents` dict, and adds a flyweight cache. | `domain/measurement/units.py:159` |
| Rust `RationalUnit` only knows a product of unit symbols with rational exponents: `new`, `dimensions` (a symbol → exponent map, not a physical dimension), `id`, `*`, `/`, `**`, `==`, hash, `repr`, `to_string`, pickling. Its `to_string` **ignores** the system and alias arguments (`_system`, `_use_alias`, `_alias_preference`). | `physure-python/src/lib.rs`, `impl PyRationalUnit` |
| Python-only methods on a compound unit: `dimension(system)`, `conversion_factor_to(target, system)`, `kind`, `is_linear`, `simplify`, `to_string(use_alias)`, `to_latex`, and `__rmul__` (the `5 * m` sugar that builds a `Quantity`). | `units.py:166-612` |
| Rust `UnitRegistry` exposes `add_base_unit`, `add_derived_unit`, `register_alias`, `get_unit`, `contains`, `default_si`, `default_imperial`, `get_prefixes`, `from_conf`, `get_categories`, `get_constants_meta`. Rust `UnitDefinition` exposes `scale` (an `f64`), `offset`, `to_base`, `from_base`, `convert_to`, `kind`, `is_linear`. | `lib.rs`, `_core.pyi` |
| Python `UnitSystem` adds `get_definition`, `get_setting`, `get_constant`, `register_prefix`, `register_dimension`, `register_unit`, `resolve_unit`, `get_base_unit_for_dimension`, and mirrors every registration into the Rust registry **best-effort** (`contextlib.suppress(Exception)` for derived units, aliases and prefixed units). | `domain/measurement/system.py` |
| Python converts with **exact `Fraction` scales and a single rounding** (`_exact_scale` in the loader, `_compound_factor_exact`, `conversion_factor_to`), falling back to `float` only when a scale is not exact. Rust stores `scale` as `f64`. | `application/startup.py`, `units.py:286-345` |
| `physure.get_unit()` already returns a Rust `RationalUnit`, while `Q_(...)` stores a Python `CompoundUnit`; `CompoundUnit` is in `__all__`. The public API mixes both representations today. | `physure/__init__.py` |
| Reference counts outside the generated units: `CompoundUnit` ≈ 240 in code (+92 in tests), `Dimension` ≈ 105 (+58), `UnitSystem` ≈ 80 (+17). | grep |
| 6158 generated `CompoundUnit` shortcuts and a 6158-entry index (`units/core.py`, `units/_index.py`, ~11 000 lines) plus `scripts/compile_units.py`. | `physure/units/` |
| The one `torch.compile` test uses `fullgraph=False` and its comment admits Dynamo already graph-breaks on the Rust constructors (`Quantity.__new__`, `RationalUnit.__new__`, `DimVector.from_pairs`). `docs/torch_compile_integration.md` still claims `fullgraph=True` guarantees no graph breaks. | `tests/ext/test_torch_compile.py` |

## End state: who owns what

**Rust (`physure-core`, exposed by thin PyO3 glue in `physure-python/src/lib.rs`) owns:**

- `RationalUnit`, `DimVector`, `UnitRegistry`, `UnitDefinition` as the single implementation.
- New in the core (today only in Python): the physical dimension of a compound unit; the conversion factor between two compound units; affine (offset) and logarithmic conversion; `kind` (absolute vs delta); `is_linear`; `simplify`; `to_string(use_alias)`.

**Python keeps:**

- The `Quantity` container (NumPy/Torch/JAX arrays, autograd, uncertainty containers).
- The active registry selected through a `ContextVar`, and its settings.
- The `5 * m` sugar (see the decision gate below).
- The `physure.units` namespace, resolved lazily from the registry, which makes `units/core.py`, `units/_index.py` and `compile_units.py` unnecessary. The `.pyi` stub that gives IDEs autocompletion is still generated.

**Deleted:** `CompoundUnit` (deprecated alias for one release), `Dimension` (same), `UnitSystem`, `UnitSystemBuilder` (the Python `.conf` parser in `application/startup.py`), `BaseExponentEntity`, the Python converters in `converters.py` / `conversions.py` and the `_UNIT_RECIPES` bookkeeping.

### Requirement: numerical exactness

The Rust implementation must keep the scale of each unit as an exact rational and
produce **the same `float`, bit for bit,** as Python does today (a single rounding
at the boundary). Where an exact value is not representable (overflow of the
rational type, a non-integer exponent) it falls back to `f64` exactly where Python
falls back to `float`. A representation wider than 64 bits is needed, because
scales such as `1e-24` do not fit in an `i64` numerator or denominator.
"Unit correctness is the product": changing a result in the last digit is a bug
even when every test passes.

### Decision gate: the `5 * m` sugar

A bare Rust `RationalUnit` has no `__rmul__` that builds a `Quantity`. Two options:

- **(a)** `RationalUnit.__rmul__` in Rust calls a Python factory registered at import time (`_core.register_quantity_factory(fn)`); it returns `NotImplemented` if none is registered. Recommended.
- **(b)** a tiny Python subclass whose only job is the sugar (and `to_latex`), with no unit logic.

The choice is made by a spike in Stage 1 that measures call overhead and checks
that `m * s` still returns the unit type the rest of the code expects. The
outcome is written down in this spec before Stage 2 starts.

## Stages

Each stage is one pull request. The 904-test suite, `ruff`, the strict docs build
and the lock checks stay green after every stage.

### Stage 0 — safety net (tests only, no behaviour change)

`tests/core/test_unit_algebra_parity.py` compares the Python implementation with
the Rust one over the whole catalog (about 6158 symbols including prefixes and
aliases) and over a set of representative compound units (`kg*m/s^2`, `J/s`,
`km/h`, `degC`, a logarithmic unit): physical dimension, factor to SI with
**exact equality**, `kind` / `is_linear`, and alias formatting. Every divergence
is either fixed (when it is a bug) or recorded as `xfail(strict=True)` with its
cause.

*Exit:* a divergence report and a green suite. This stage has value on its own
even if the rest is cancelled.

### Stage 1 — the core first

In `physure-core`: exact rational scales, `dimension_of`, conversion factor
between compounds, affine/log conversion, `kind`, `is_linear`, `simplify`,
`to_string(use_alias)`. Exposed through thin PyO3 glue. Includes the `5 * m`
spike and the measurements listed under Risks.

*Gate:* the core requires at least 90 % coverage on new code (SonarQube "Physure
Core Strict"), zero new violations and no duplication above 2 %.

*Exit:* every Stage 0 `xfail` flips to a pass; the decision (a)/(b) is recorded.

### Stage 2 — `Dimension` and `CompoundUnit`

`Dimension → DimVector`, `CompoundUnit → RationalUnit` across about 350 code
references (plus about 150 in the tests) in `domain/`, `core/`, `application/`, `nn/`, `ext/`.
Deprecated aliases emit `DeprecationWarning`. `physure.units` becomes lazy and the
generated modules and their generator are deleted.

*Exit:* suite green, and no numeric result changed (the Stage 0 tests are the
referee).

### Stage 3 — `UnitSystem`

`UnitSystem` becomes a thin facade over `UnitRegistry` (settings, constants,
`international` / `imperial` selection). `UnitSystemBuilder` and the Python
`.conf` parsing are deleted; the registry is built by the existing
`build_registry_from_conf`, and the `systems/*.conf` files are read in Rust.

*Exit:* Python has no `.conf` parser; `parse_physure_conf` is the only one.

### Stage 4 — cleanup and version

Remove dead code, correct `docs/torch_compile_integration.md`, write the CHANGELOG
entry marked as a breaking change, and bump the minor version (0.3.0).

## Risks and how each is measured

| Risk | Mitigation |
|---|---|
| Results change in the last bit | Stage 0 asserts exact equality before anything is touched, and Stage 2 reruns it |
| Crossing the FFI boundary slows unit operations | Microbenchmark of `*`, `/`, `**`, conversion and `Q_(...)` construction before Stage 1 and after Stages 1 and 2; report the numbers and decide with data |
| More `torch.compile` graph breaks | Count graph breaks with the existing test (`TORCH_LOGS=graph_breaks`) before and after; the test must keep passing |
| Migration volume (about 350 code references and 150 in tests) | Stages plus deprecated aliases for one release |
| Rust lacks something Python uses | Stage 1 precedes any deletion, and its exit criterion is the Stage 0 suite |
| Windows: a plain `uv run` rebuilds the project while pytest holds `_core.pyd` | Stage plans use `uv run --no-sync`; documented in the contributor notes |

## Success criteria

- A contributor can find the definition of unit and dimension behaviour in one language.
- `domain/measurement` no longer contains unit algebra, a `.conf` parser, or a best-effort mirror; roughly 11 000 generated lines and `compile_units.py` are gone.
- No numeric result differs from today's (bit for bit), and the full suite, `ruff`, the strict docs build and the Sonar gates pass.
- The `5 * m` sugar and `Q_(...)` keep working; `CompoundUnit` and `Dimension` keep importing for one release with a `DeprecationWarning`.

## Open items resolved during execution

- Decision gate (a)/(b) for `5 * m`, by the Stage 1 spike.
- The width of the exact rational type (`i128` versus an arbitrary-precision type), by checking the catalog's smallest and largest scales and the sizes of the products that conversions create.
- Whether `to_latex` stays in Python (presentation) or moves to the core; default: stays in Python unless it needs registry data.

## Outcome of stage 1a (exact scales)

- **Width of the exact type: decided.** `i128` is not enough (quecto times a scale written with 15 digits already needs a denominator of 10^50, and the catalog has such units), `f64` is not exact, so the exact scale has three representations: scale 1 (a variant with no data), a reduced fraction of two `u128` for almost every unit, and `BigRational` for the rest. A value is always held in the smallest one that fits. No new crate is compiled: `num-bigint` was already a dependency of `num-rational`.
- **The `f64` stays.** `RationalUnit.scale` is the exact scale rounded once; the exact scale is an `Option` next to it that is `None` only for a scale that arrived as a bare `f64` or from a fractional power, which is where Python falls back to a float as well. No binding changes.
- **Exit criterion met for this class:** with the exact scales, Python and Rust differ in the last bit for no symbol of the catalog and for no compound case (47 symbols and one compound case before).
- **Cost, measured with the repaired benchmarks** (Windows, a quiet run; on a busy machine the benchmarks that the change cannot touch moved by up to 60 %, so read these as orders of magnitude): a conversion between prefixed units went from about 156 ns to about 233 ns (+31 %), a product of prefixed units from about 117 ns to 147 ns and a quotient from 118 ns to 218 ns, units of scale 1 by +5 % to +15 %, and parsing a unit expression from about 0.53 ms to 0.67 ms (+28 %). The parse is slow to begin with because `Parser::parse_expression` rebuilds the whole registry from the catalog on every call; the extra 0.14 ms is the exact scale built for each of its units. Caching that registry is the remedy; it is not part of this work. With `BigRational` alone the same conversion cost about 770 ns, which is why the `u128` representation exists.
- **Found on the way, not fixed here:** (1) converting between affine scales is not exact either: `100 degC => degF` is `211.99999999999997` in Python and in Rust, because the zero point of Fahrenheit is a double; it needs exact affine arithmetic. (2) `UnitDefinition` / `UnitConverter`, the second unit model that the Python binding exposes, still holds `f64` scales. (3) `core_benchmarks.rs` did not compile; it does now.

### Corrections to stage 1a

- **Affine conversions are exact now.** The inexactness of `100 degC => degF` was in the catalog (5/9 and the Fahrenheit zero point written as rounded decimals), not in the arithmetic: even exact arithmetic on those decimals gives `212.00000000000003`. The catalog writes the fractions (`5/9`, `45967/180`), a conversion is `old * scale + shift` with both rounded once, and Rust and Python compute the same two numbers. Evaluating the affine map exactly on the input would also have been possible for a scalar, but it cannot be vectorised and it turns `273.15 K` into `-2.3e-14 degC`, because the double `273.15` is not the decimal `273.15`.
- **The embedded catalog is parsed once per process** (it was rebuilt, and its settings re-applied, on every `Parser::parse_expression`).
- **Still open, assigned to stage 1c:** `UnitDefinition` / `UnitConverter`, the second unit model that the Python binding uses to convert arrays, holds `f64` scales; it is unified with the model of `RationalUnit` there, so making it exact before that would be done twice.
- **Also open, stage 1c:** `international.conf` in `physure-python` restates celsius, fahrenheit and their deltas on top of the core catalog (the later definition wins), which is why the fractions did not reach Python at first. Only its numbers were corrected, because the first alias it lists is what the frozen unit semantics record; the catalog of the core should be the only place they are written.
- **Found, not fixed, stage 1c with the delta-temperature rule:** in PHS a negative temperature literal is the negation of a positive one, and the positive one is normalised to kelvin first: `-40 degC => K` is `-313.15 K`, not `233.15 K`. Python is not affected (`Q_(-40, "degC")`).

### Outcome of stage 1b-1

- **Delta temperatures.** `delta_celsius` was a new base unit because pass 1 of the catalog parser registered every line at scale 1 with a single dimension symbol as the base unit of that symbol. Only two repeat a symbol: `delta_celsius` (symbol `O`, the same quantity as kelvin) and `steradian` (symbol `A`, not the same quantity as radian). The rule is now: the first one is the base unit, a later one is only another name, unless the catalog flags it `own_kind` (steradian). The flag is a word among the fields of the line, like `noprefix`; Python's loader ignores it until stage 2, when it adopts the core's types and stops filing angle and solid angle under one dimension.
- **Spelled-out prefixes.** The registry keeps the name of each prefix next to its symbol (`prefix_names`) and `split_prefix` returns the symbol either way, so everything downstream (`prefix_exact`, the reverse lookup in `inspect.rs`) is unchanged. The match is the longest prefix that leaves a unit; before, it was the first one a `HashMap` iterated over. Rust also accepts a long prefix in front of a symbol (`kiloA`) and a symbol prefix in front of a long name, a superset of what Python resolves; that is harmless and the stage 2 xfail on names Python cannot resolve is unchanged.
- **Decision (2026-10-28) for the logarithmic units, stage 1b-2.** The spike found that the "kinds merged" report for percent, ppm and unity was an artifact: Rust reads `pH` as picohenry, so the references of the dimensionless group were `pH` and `pct`. In Python, ratios and logarithmic units share the dimensionless dimension and convert into each other (`50 percent` is `-3.01 dB`, `20 dB` is `100`), which ISO 80000-3 supports (levels are dimensionless and related to the ratio by definition) and Pint and Astropy implement; angle never converts to a ratio and only radian/steradian is a real merge. So logarithmic units are scales over plain ratios in Rust, angle and solid angle stay apart, and the earlier decision to reject `percent -> dB` is withdrawn. The "kinds" xfail narrows to angle and solid angle when they land.

### Outcome of stage 1b-2

- **A level is a dimensionless unit with a logarithm.** `RationalUnit.log: Option<LogScale { factor, reference }>`, with `level = factor * log10(ratio / reference)`; the catalog's `Log, factor, reference, 1` lines fill it. It takes part in equality and hashing, so `dB` is not `unity`. A level takes no prefix (Python takes none either), and it survives only alone or times a plain number: a unit expression or a product that would keep the number and lose the logarithm (`dB/km`, `dB*m`, `dB^2`) is an error, and so are `ln`, `exp`, `sqrt` and the trigonometric functions of a level.
- **Same operations as Python, so the same bits.** A conversion is the ratio of the source (`reference * 10^(v / factor)` for a level, `v * scale` for any other unit) and then the target (`factor * log10(x / reference)`, `x / scale`). `log10` and `pow10` became functions of every uncertainty backend instead of being composed from `ln` and `exp`, which would round differently in the last bit; the uncertainty follows the logarithm (`10 dB ± 0.5` is `10 ± 1.1513`).
- **Where Rust is stricter than Python on purpose.** `10 dB + 1 unity` is refused (Python converts the ratio to dB and adds the numbers), and so is any level in a compound unit (Python keeps `dB/km` as a linear compound). `dB/km` is a real attenuation unit: it comes back with the compound-unit API of stage 1c, once `kind` and `is_linear` exist. `dB + dB` also carries the uncertainty now (Python returned `0.0`).
- **`pH` is a level, not a picohenry.** It shadows `p` + `H` exactly as it does in Python (the catalog reserves the symbol on purpose); `picohenry` still works since the prefixes can be spelled out. The parity comparison of the dimensionless group no longer compares everything against a picohenry.
- **The parity suite narrows.** The unknown-unit xfail is gone, and the xfail of the kinds merged by Python is about angle and solid angle only (the percent/ppm/dB part was an artifact of the picohenry).

### Outcome of stage 1c-A (temperatures)

- **What the survey found.** Python and Rust disagreed on temperatures in ways neither was entirely right about. Rust normalised every temperature operation to kelvin, which made `20 degC + 5 degC` answer `571.3 K`, `5 K - 20 degC` answer `-288 K`, `20 degC == 293.15 K` answer `false` (comparisons ignored the zero point) and `-40 degC` answer `-313.15 K` (the minus sign is parsed as `-1 *`). Python adds and subtracts as an affine space (point + interval is a point, point - point is an interval, point + point and interval - point are errors) but scales a point in place (`2 * Q_(20, "degC")` is `40 degC`) and multiplies degC as a plain scale (`R * Q_(25, "degC")` is `207.85 degC·kg·m²/(K·mol·s²)`).
- **The rule (decided 2026-10-28).** Points and intervals follow the affine algebra Python already has. A pure sign (`-1`, `1`) keeps the scale of a point, so `-40 degC` is minus forty degrees Celsius. Any other plain number multiplying or dividing a point is refused, with the hint to convert to kelvin or to use an interval, because the result depends on the scale and the caller has not chosen one. A quantity with dimensions of its own takes the point as the absolute temperature it is, as before (`R * T`, `T^4`, `sqrt(T)`). Comparisons, `min`, `max`, ranges and `assert` use the absolute value (`canonical_magnitude` includes the zero point).
- **Python.** Unchanged until stage 2; `tests/core/test_temperature_algebra_parity.py` pins what agrees and records the two divergences as strict xfail (scaling a point by a number, the product with a dimensioned quantity). Adopting the core's rule is a breaking change for `Q_(20, "degC") * 2` and is listed for the stage 2 changelog.
- **Bare units as numbers.** `5 * degC` in PHS reads the bare unit as `1 degC` and is refused like any scalar times a point; writing the literal `5 degC` works. Whether `number * unit` should build the quantity (the `5 * m` sugar of Python) is the stage 1d spike.

### Outcome of stage 1c-B (what the core says about a unit)

- **Facts are registry operations on a unit written as symbols.** Python's `dimension`, `kind`, `is_linear`, `to_string(use_alias)` and `simplify` all start from the symbols the user wrote (a `CompoundUnit` is a dictionary of symbols), so the core answers them for a unit parsed with `Parser::parse_expression_atomic`: `simplify` replaces the recipe units (`N`, `J`, `Hz`...) by their base symbols, `dimension_of` adds the dimension symbols of what each symbol expands to, `kind_of` and `is_linear_of` look at the offset and the logarithm, and `alias_of` names the result. `UnitRegistry::facts` runs the four on an expression; `RationalUnit::symbolic_repr` is Python's `str(CompoundUnit)`.
- **What the registry had to remember.** The dimension symbol of each base unit, the names of each unit in the order the catalog lists them (the first is the unit's symbol in Python, which is not always its key: `year_gregorian` is `yr_gregorian`), and which units are recipes at scale 1 and under which base symbols. A unit at another scale (`L`, `min`, `cal`, `hp`) is not a recipe and stays what it is when a unit is simplified.
- **The rule for the alias is Python's.** A recipe written out in base symbols is named by the shortest name of the recipe (`kg·m²/s³` is `W`, `N*m` is `J`); a unit written by its symbol is shown by its shortest name (`deg` is `°`), by any other name as written (`gallon`, `percent`), a prefixed unit by the prefix symbol and the symbol of what it prefixes (`kilometer` is `km`, `attoampere` is `aA`), and anything else written out (`ft·lbf`). The unit one (`unity`) is nothing to write.
- **Exit criterion met.** The 32 frozen expressions and the 2681 units of the catalog that Python resolves agree on dimension, kind, linearity and simplified form, and on the alias but for two units: `degC` and `degF`, because Python's `international.conf` declares `°C` and `°F` as their symbols on top of the core catalog and so shows `degC` as written. That is stage 1c-C. `tests/core/test_unit_algebra_parity.py` and `physure-core/tests/unit_facts_golden.rs` pin both.

### Outcome of stage 1c-C (one catalog, no second model)

- **The f64 model was dead.** `UnitDefinition` and `UnitConverter` in the core (a converter per symbol with `f64` scales, a kind, a physical dimension) were only ever built by a Python mirror in `UnitSystem.register_unit` that discarded the result, and `convert_units_inplace` had no caller. They are deleted with the Python mirror and the public names `physure.UnitDefinition` and `physure.convert_units_inplace`; `UnitKind` moved next to `RationalUnit` because the facts of stage 1c-B use it. Nothing was unified: the unified model is the one the registry already is (`RationalUnit` with exact scales and offsets and `LogScale`, the symbol tables of stage 1c-B). The definition per symbol that stage 3 needs (physical dimension, kind, whether a prefix is allowed, name) is designed there, from what `UnitSystem` really asks.
- **One catalog for the temperature scales.** `international.conf` no longer restates them. The reason Python showed `degC` as written while the core showed `°C` was only the order of the names: the first one listed is the symbol of a unit, and the core listed `degC` first where Python's file listed `°C`. The core catalog lists `°C` and `°F` first now, so both sides agree on every unit of the catalog and nothing Python shows changed. In Rust the registry keys the two scales by `°C` and `°F` (`degC` and `degF` are aliases as before, and a unit written `degC` still prints `degC`).
- **`noprefix` is still ignored by the core.** Python refuses a prefix on a unit the catalog marks `noprefix` (`min`, `mi`, `in`, `h`); the core does not. It belongs with the definition per symbol of stage 3.
- **`dB/km` and other levels inside a compound unit are refused** (stage 1b-2) and stay so: nothing in the repository uses them, and they need a decision about what a level per length is. Listed for the stage 2 changelog.

### Outcome of stage 1d (the `5 * m` sugar and the measurements)

**Decision gate: (a).** `RationalUnit.__rmul__` lives in the core and calls the factory that Python registers at import (`_core.register_quantity_factory`); it returns `NotImplemented` when none is registered, and the factory returns it for an operand it does not take. It is implemented now and tested: `5 * RationalUnit({"m": 1})` is a quantity, and so is `3 * (m / s)`. Why not (b), a Python subclass whose only job is the sugar: the Rust operators hand back the base class, so `5 * (a * b)` fails on a subclass unless every operator result is re-wrapped in Python, which is what `units.py` does today (`wrap_arithmetic` re-wraps each result in a `CompoundUnit`) and what stage 2 deletes. Cost of the sugar, measured: calling the factory from the core adds about 130 ns (169 ns against 37 ns for the same trivial function called from Python), on top of the roughly 15 µs that building a `Quantity` takes either way. The factory registered today still builds a `CompoundUnit` from the dimensions of the unit it is given, which costs about 1.7 µs more; stage 2 removes that hop.

**Cost of unit and quantity operations** (best of 5 repeats, ns per call, Windows, one quiet run each; the two columns were measured on different builds, so read differences under 10 % as noise):

| Operation | before stage 1 (`0ac08a3`) | now (stage 1 done) |
|---|---|---|
| `Q_(5.0, "m")` | 16 123 | 15 452 |
| `Q_(5.0, units.m)` | 14 856 | 14 032 |
| `5 * units.m` | 15 937 | 14 400 |
| `units.m * units.s` | 1 124 | 1 050 |
| `units.m / units.s` | 1 134 | 1 066 |
| `units.m ** 2` | 845 | 817 |
| `q1 + q2` | 23 232 | 20 081 |
| `q1 * q2` | 20 424 | 17 335 |
| `q1.to("km")` | 51 693 | 48 295 |
| `Q_(5, "m") / Q_(2, "s")` to `km/h` | 89 199 | 85 129 |
| an array of 1000 to `km` | 63 662 | 59 883 |
| Rust `RationalUnit.__mul__`, raw | 212 | 220 |
| the same, through the Python wrapper of `units.py` | 1 639 | 1 572 |

Stage 1 changed nothing a Python caller can measure. What stage 2 can change is on the last two rows: a unit operation falls from 1.6 µs to 0.2 µs once the wrapper is gone, and a unit operation is under 10 % of any quantity operation, so the effect on quantities is small and in the right direction. The cost of a quantity is in building it (about 15 µs).

**`torch.compile`.** The test (`tests/ext/test_torch_compile.py`) uses `fullgraph=False`, and Dynamo reports these graph breaks (`torch._dynamo.explain`, torch 2.13, CPU): a plain `x.magnitude * 2.0` has none; `(a + b) * 2.0` had 7 before stage 1 and has 6 now, `a * b` had 7 and has 4 (the stray `print("DEBUG")` calls removed in A2 stopped breaking the graph), `a.to("km")` has 2 (a `Tensor.item()` call). What matters for stage 2 is the unit algebra itself: a `CompoundUnit` product next to a tensor operation compiles with `fullgraph=True` (it is Python Dynamo can trace), while the raw Rust `RationalUnit.__mul__` breaks the graph ("Dynamo does not know how to trace ... a third-party C/C++ extension"). `Quantity.__mul__` multiplies units, so replacing `CompoundUnit` by the Rust type would add a break to every product of quantities inside a compiled region. The mitigation was tried and works: wrapping the call in a function decorated with `torch.compiler.assume_constant_result` (or resolving the product outside the traced region) makes the region compile with `fullgraph=True` again. Stage 2 has to route the unit operations that `Quantity` performs through such a helper and must show the counts above not growing: this is its measure for the `torch.compile` risk, and the plan checks it.

**Pre-existing, not changed.** `np.array([1, 2]) * units.m` is an object array of two quantities, not a quantity of an array: numpy answers the product itself. The array goes through `unit.__rmul__(array)` and the `Q_` constructors.

**Stage 1 exit.** The strict xfails left in the parity suite are the two that stage 2 closes by design (angle and solid angle share a dimension in Python; the generated index lists names Python cannot resolve), and the decision for the sugar is recorded here.
