# A1, stages 2 and 3 — replacing Python's unit classes — design

**Date:** 2026-10-29
**Status:** approved by the maintainer on 2026-10-29 (the three decisions of the last section were taken as recommended); step 1 is planned, the others are planned when they come up
**Base:** local `main` @ `fb08c08` (A1 stage 1 complete: stages 0, 1a, 1b, 1c and 1d merged, not yet on `origin/main`)
**Follows:** `2026-10-27-a1-rust-owned-unit-algebra-design.md`, whose stage 2 ("`Dimension` and `CompoundUnit`") and stage 3 ("`UnitSystem`") this refines.

## Goal

Delete the second implementation of unit and dimension algebra in `physure-python/physure/domain/measurement` (`CompoundUnit`, `Dimension`, `BaseExponentEntity`, the converters, `UnitSystem`'s dictionaries, the Python `.conf` parser, the SymPy fallback of the unit-string parser), so that `physure-core` decides what a unit is, what its dimension is and how it converts, and a contributor reads Rust only.

Stage 1 made the core able to answer: exact scales, offsets, levels, the dimension, kind, linearity, alias and simplified form of a unit (`UnitRegistry::facts`), the `5 * unit` sugar. It checked the answers against Python for every unit of the catalog. What is left is to make Python ask the core instead of answering itself.

## What the survey found

| Finding | Evidence | Consequence |
|---|---|---|
| **A unit's facts need a registry, and Python's registry is the `UnitSystem`.** `unit.dimension(system)`, `conversion_factor_to`, `kind`, `is_linear`, `simplify` and `to_string(use_alias)` all take the system. The core answers them from a `UnitRegistry`. | `units.py`; `UnitRegistry::facts` | Swapping the unit class first would leave the logic in Python (the methods would only change owner). The class swap needs a core registry that is the system's source of truth. That is the piece the spec had put in stage 3. **The order changes: the registry comes first.** |
| **The core registry inside a `UnitSystem` is a best-effort mirror.** Every registration is forwarded inside `contextlib.suppress(Exception)` (derived units, aliases, prefixed units), and the mirror knows nothing of the system's settings or of recipes. | `system.py` `_register_unit_recipe_or_base`, `_register_rust_aliases`, `_register_prefixed_rust` | It cannot answer for the system today. It has to be built from the same text as the system, strictly. |
| **Many parallel tables.** `UNIT_SYMBOL_REGISTRY`, `UNIT_REGISTRY`, `UNIT_DIMENSIONS`, `ALIASES`, `ALIAS_TO_EXPONENTS`, `_UNIT_RECIPES`, `PREFIX_REGISTRY`, `_DIMENSION_NAME_REGISTRY`, `_PREFIX_BLOCKLIST`, plus four `*_definitions` dictionaries. | `system.py:40-55` | Each is something the core registry already knows (stage 1c: `symbol_aliases`, `recipes`, `base_dimension`, prefixes). |
| **Surface to migrate.** `CompoundUnit` appears 269 times in 33 files of code and 97 times in 17 files of tests; `Dimension` 106 times in 16 files and 58 in 12 files. What leaks the representation, in lines of code and of tests: `.exponents` (55 and 76), `CompoundUnit(...)` (45 and 63), `.dimension(system)` (25 and 12), `conversion_factor_to` (8 and 10), `.simplify` (5 and 21), `to_string` (7 and 19). | grep | A direct swap touches about 350 places of code at once. The migration has to go behind a seam first. |
| **Python's `Dimension` loses fractional powers silently.** It is a nine-slot integer vector and `__pow__` does `int(power)`, so `Q_(4, "s") ** 0.5` has the unit `s⁰⋅⁵` and the dimension *Dimensionless*: it would add to a plain number. The core keeps rational exponents (`dimension_of` returns them). | measured | Replacing `Dimension` by the core's rational dimension fixes a wrong answer; it is not only a rename. The `DimVector` of the core (`[i8; 9]`, integers) is the wrong target for it. |
| **A second unit-string parser.** `parse_unit_string` tries the core's parser (after a regex that rewrites `°` and implicit products) and falls back to a SymPy-based one. | `application/parsing.py`, `core/parsing/*.py` (~330 lines) | Once the core's parser covers what the fallback was for, it goes. Whatever the fallback accepts and the core rejects has to be found first. |
| **`UnitSystem.register_unit` takes any `UnitConverter`.** The three the library defines (linear, offset, logarithmic) map to the core; a user subclass does not. | `converters.py`, `system.py` | A user converter that is none of the three stops being supported. It is a breaking change and is listed. |
| **Unit operations are cheap; quantity construction is not.** A unit product is about 1 µs in Python and 0.22 µs raw in the core; building a quantity is 15 µs and a conversion 48–85 µs. | stage 1d table | Performance is not the reason for the migration, and a migration that makes a quantity operation slower by more than noise is a failure. |
| **`torch.compile`.** A `CompoundUnit` product compiles with `fullgraph=True`; the raw core operator breaks the graph. `Quantity.__mul__` multiplies units. | stage 1d | The unit operations that `Quantity` performs go through a helper that Dynamo treats as a constant (`torch.compiler.assume_constant_result`), and the break counts must not grow. |

## End state

**The core owns:** `RationalUnit` (symbols with rational exponents, exact scale, offset, level), the `UnitRegistry` of a system (units, aliases, prefixes, recipes, dimensions, settings, constants) built from the catalog text, and every question asked of a unit: dimension (with rational exponents), conversion factor, kind, linearity, alias, simplified form, parsing.

**Python keeps:** `Quantity` and its backends; the active system selected through a `ContextVar`; a `Dimension` value type that is a thin wrapper of the core's dimension (names of dimensions are presentation and stay in Python); the `physure.units` namespace, resolved lazily from the registry; the `.pyi` stub.

**Deleted:** `CompoundUnit` and `Dimension` as classes (deprecated aliases for one release), `BaseExponentEntity`, `converters.py` / `conversions.py`, the dictionaries of `UnitSystem` and the `.conf` parser of `startup.py`, `units/core.py`, `units/_index.py` and `compile_units.py`, the SymPy unit parser, the `wrap_arithmetic` patch of `units.py`.

## Steps

Each step is one branch, verified end to end, that I show before it is merged; the suite, `ruff` and the strict docs build stay green after each; each touches one of the three things it can change (the core, how Python asks, what a user sees) unless it says otherwise. The plan of a step is written when the one before it is merged, from what that step showed; only step 1 is planned now.

1. **The registry answers (core only; new API, nothing consumes it yet).** `PyUnitRegistry` gets what stage 1c built as Rust-level methods: `parse_atomic`, `simplify`, `expand`, `dimension_of`, `kind_of`, `is_linear_of`, `alias_of`, `factor_between`, `facts`, and a registry built from the catalog plus extra conf text. Acceptance: through that object, the facts of every unit equal Python's, as the parity suite already shows for the default registry. *Plan:* `docs/superpowers/plans/2026-10-29-a1-stage2-s1-registry-api.md`.
2. **A system owns its registry, strictly.** `UnitSystem` builds its core registry from the same conf text it parses (the default catalog plus the system file plus what the user passes), registers every later unit in it without `suppress` (the three `contextlib.suppress` sites go; a registration the core refuses is an error), and the three converters become a scale, an offset and a level on the core side. Acceptance: for the built-in systems and for a system with a user-defined unit, a differential test of the registry's facts against the Python system's, over every unit of each.
3. **The seam.** Every use of `.exponents`, of `CompoundUnit(dict)`, of `.dimension(system)`, `conversion_factor_to`, `kind`, `is_linear`, `simplify`, `to_string` outside the three files that define them goes through accessors in one module (`physure.domain.measurement.unit_ops`), implemented over today's classes. No behaviour change. This is the step that shrinks the 350 places to a handful; it also fixes, once, the semantics that callers rely on (what `dimension` returns for a compound, what `kind` is for a product).
4. **`Dimension` on the core.** `Dimension` becomes a thin immutable wrapper of the core's dimension, with rational exponents (so `Q_(4, "s") ** 0.5` has the dimension `T^(1/2)` and refuses to add to a plain number), equality and hash by value, the same `from_string`, `exponents`, properties and names. Acceptance: no operation of `bench_unit_ops.py` slower by more than 10 %.
5. **`CompoundUnit` on the core.** The accessors of step 3 call the registry of step 2 and the unit is a core `RationalUnit`; `CompoundUnit` is a deprecated alias of it (with `.exponents` as a deprecated view); `BaseExponentEntity`, the flyweight cache, `wrap_arithmetic` and the converters go; the sugar factory of stage 1d stops converting. Acceptance: the stage 0 and stage 1 parity tests unchanged; the `torch.compile` break counts of stage 1d not larger (through the constant-result helper); the benchmark gate.
6. **What a user sees changes, one decision per branch**, so each diff is about one rule: angle and solid angle stop sharing a dimension (closes the strict xfail); `noprefix` is enforced; a plain number no longer scales an absolute temperature and a product takes it as kelvin (closes two strict xfails; `Q_(20, "degC") * 2` and `R * Q_(25, "degC")`); a level inside a compound unit (`dB/km`) stays refused with a clear message, or is designed, as the maintainer decides; a unit string the core's parser rejects and the SymPy fallback accepts is either added to the core's parser or becomes an error. Each lands with its changelog line.
7. **The system is a facade (the spec's stage 3).** The dictionaries of `UnitSystem` and the `.conf` parser of `startup.py` are deleted; `UnitSystem` is the registry plus the settings, `physure.units` is resolved from the registry, `units/core.py`, `units/_index.py` and `compile_units.py` are deleted and the stub is still generated. The SymPy unit parser goes with them.
8. **Cleanup and version (the spec's stage 4).** Dead code, `docs/torch_compile_integration.md` corrected to what was measured, the changelog entry as a breaking change, `0.3.0`.

## Gates, measured at every step that can move them

| Gate | How | Limit |
|---|---|---|
| Last bit | stage 0 and stage 1 parity suites (`test_unit_algebra_parity.py`, `unit_facts_golden.rs`) | no change |
| Speed of operations | `physure-python/benchmarks/bench_unit_ops.py` (the table of stage 1d: `Q_`, `5 * m`, unit products, `+`, `*`, `.to`, an array of 1000) | no row more than 10 % slower than the stage 1d numbers on the same machine |
| First use | `python -m physure "500 N / 2 m^2 => kPa"` and `import physure` | under 0.5 s, no torch, one registry |
| `torch.compile` | `torch._dynamo.explain` break counts of stage 1d, and `tests/ext/test_torch_compile.py` | not larger |
| Behaviour | the full Python suite, the Rust workspace, `ruff`, `mkdocs build --strict` | green |

## What a user sees, for the changelog of 0.3.0

- `CompoundUnit` and `Dimension` import with a `DeprecationWarning` for one release; `.exponents` is a deprecated view of `.dimensions`.
- `UnitSystem`'s dictionaries (`UNIT_DIMENSIONS`, `UNIT_SYMBOL_REGISTRY`, `ALIASES`, ...) and `physure.units.core` are removed; `get_definition` goes with them.
- A user `UnitConverter` that is not a linear, offset or logarithmic one is not supported.
- Dimensions have rational exponents; a fractional power of a quantity has the dimension it should.
- Angle and solid angle are different kinds; `noprefix` is enforced; a plain number does not scale an absolute temperature; a level is not allowed inside a compound unit (unless step 6 designs it).
- `physure.UnitDefinition` and `physure.convert_units_inplace` are already gone (stage 1c).

## Not in scope

A Rust `Quantity` that holds arrays (the spec keeps it scalar); publishing branches or releases; the symbolic engine; `A2b` (the relocation of modules and the rename of `core`), which waits for this work because it removes the imports that cause part of the cycle.

## Decisions taken with the maintainer (2026-10-29)

1. **Order: registry first** (steps 1 and 2, then the seam, then the classes) instead of the spec's classes first and system after; the finding above is why.
2. **Where user-visible behaviour changes land: each in its own branch after the classes are swapped** (step 6), so the swap is judged on "nothing a user sees changed" and each rule on its own diff.
3. **What `Dimension` becomes: a wrapper of the core's rational dimension** (fixes the silent loss of fractional powers), not the integer `DimVector` the spec named (a rename that keeps the wrong answer).

## Outcome of step 1 (the registry answers)

- The registry object answers the facts of every unit of the catalog that Python resolves with the same values, and the conversion factor between compound units with exact equality, as the new `tests/core/test_registry_facts.py` shows.
- `UnitRegistry::expand` and `factor_between` are new in the core; a registry can be built from the catalog plus extra text (`build_registry_with`), which a system file or a user conf will use in step 2. `[Settings]` of an extra conf still writes process-wide settings; whether a registry owns its settings is a question of step 2.
- `physure-python/benchmarks/bench_unit_ops.py` is the benchmark the gates cite.

## Step 2, refined (2026-10-29): what a system's registry has to be able to take

Planning step 2 against the code found four things the design did not know, and changes its shape.

| Finding | Evidence | Consequence |
|---|---|---|
| **The current mirror is not only best-effort, it is wrong.** For a unit without a recipe it calls `add_base_unit(symbol)`: `EUR` (scale 1.08 of `USD`) is a base unit of its own in the core registry, and a unit written in a `.conf` as `km` is a base unit `km`. For a prefixed unit it calls `add_derived_unit(prefixed_symbol, base_ru)`, which registers `km` as the metre itself, with no prefix factor. | `system.py` `_register_unit_recipe_or_base`, `_register_prefixed_rust` | Nothing consumes the mirror's answers, which is why it has not shown. A registry that is authoritative has to come from the text, not from forwarding units one at a time. |
| **`get_unit` has a lookup path through the core registry that would drop scales.** After the Python tables and the aliases, it asks `contains` and returns `CompoundUnit(ru.dimensions)`: the *expanded* dimensions of the unit, with no scale. With the mirror it is unreachable; with a registry that knows the catalog, `mile` or `psi` would come back as metres or pascals. | `system.py:425-438`, `__contains__` | The path and the `__contains__` clause are deleted in step 2. |
| **A text parsed after the catalog cannot declare units of a dimension the catalog has no base unit for, nor tell a new base unit from a name for the old one.** `parse_unit_line` resolves a dimension from a map that exists only for the text being parsed (the fallbacks cover `L`, `M`, `T`, `I`, `O`, `N`, `J`, `A`), `$` is not accepted as a dimension symbol, and a second text that says `smoot = 1.0, L` makes `smoot` a base unit. Money (`enable_currency` registers `USD`, `EUR`, `MXN` on `$`) cannot be expressed. | `conf.rs` `parse_conf`, `is_single_ident`; `ext/currency.py` | The core registry keeps the base unit of each dimension and a later text starts from it; `$` is a dimension symbol. |
| **The parser skips what it cannot read and registers its names anyway.** A unit line whose factor is not a number, or whose dimension and recipe do not parse, is dropped silently (the aliases of the second case are registered pointing at nothing). A registry that is the system's source of truth cannot work like that. | `conf.rs` `parse_unit_line` | A strict reading for what a system is made of: all or nothing, every problem reported. The embedded catalog has no unreadable line (probed), and a test says so. |
| **Python registers a logarithmic unit of a quantity.** `test_correctness_fixes.py` registers `dBW` on the dimension of a power with a `LogarithmicConverter`, and converts it to watts with the uncertainty of the derivative. The core's levels (stage 1b-2) are levels of a plain ratio only (`dB`, `pH`). | `tests/measurement_tests/test_correctness_fixes.py`, `conf.rs` | The core models a level of a quantity (`dBW`, `dBm`): `Log, factor, reference, dimension`, the reference in base units. Without it the registry could not say what Python registers. |

**The shape of step 2 changes accordingly.** It splits in two branches so that each is reviewable:

- **2a, the core takes a system's definitions strictly** (core and extension, nothing in Python): `$`, the base unit of each dimension remembered, all-or-nothing extension with the problems reported, `[Settings]` not applied, levels of a quantity, `UnitRegistry.extend_conf`. Plan: `docs/superpowers/plans/2026-10-29-a1-stage2-s2a-strict-registry.md`.
- **2b, a system owns its registry** (Python): a translator from what `register_unit`, `register_prefix` and the builder pass (dimension, converter, aliases, recipe) to conf text; `UnitSystem` builds its registry from the same texts it parses and forwards later registrations as text, strictly; the three `suppress` sites, the mirror methods and the core lookup path of `get_unit` and `__contains__` are deleted; the registry survives pickling by rebuilding from the texts it was made of; a differential test of the registry's facts against the Python system for the default system, the imperial one, a system with a user conf, a system with currency enabled and a bare system with units registered by hand. A user `UnitConverter` that is not linear, offset or logarithmic raises `TypeError` when it is registered. Planned when 2a is merged.

**Decisions taken for step 2 (proposed, to confirm):** split into 2a and 2b; the registry ignores `[Settings]` (a system keeps its settings, and the process-wide ones are applied only where a conf file is read as a file); a level of a quantity is modelled in the core; an unsupported converter is an error at registration (2b).

## Outcome of step 2a (the core takes a system's definitions strictly)

- `extend_registry` is the strict reading of what a system is made of and `parse_physure_conf` stays the lenient one (the catalog and the `physure.conf` of the working directory). The embedded catalog has no line the parser refuses, and a test says so, which is what lets the strict reading be used for the system file as well.
- The registry keeps the base unit of each dimension symbol (`dimension_base`) and a text parsed later starts from it; `$` is a dimension symbol, so `enable_currency` is expressible as text.
- A level can have the dimensions of a quantity (`dBW`, `dBm`), converts to the plain unit of those dimensions through the logarithm, and has no conversion factor between units.
- Any text parsed, strict or lenient, also registers the eight SI base units (`m`, `s`, `kg`, `A`, `K`, `mol`, `cd`, `rad`) and the default dimension symbols of them (`L`, `M`, `T`, `I`, `O`, `N`, `J`, `A`), whether or not the text mentions them. A registry of a bare system therefore knows `kg`; step 2b must not use the registry's membership to answer `in` for a system, which stays the system's own until step 7.
- Step 2b (Python) is planned from here.

## Step 2b, refined (2026-10-29): what a system registers, and how its registry gets it

Planning step 2b against the Python code, with the suite instrumented, found seven things.

| # | Finding | Consequence |
|---|---|---|
| 1 | Nothing outside `system.py` reads a system's registry. The two paths inside it (the "Rust Registry Check" of `get_unit` and the third clause of `__contains__`) were never reached in the 1145 tests of the suite. The mirror is write-only. | Replacing it changes nothing the suite can observe, except the new errors. What 2b buys is a registry that is authoritative and checked, for the seam of step 3 to stand on. |
| 2 | Every `register_unit` and `register_prefix` call of the suite (81 systems, 574 unit names, bare systems with recipes, levels and currencies included) can be written as a conf line the core accepts, and the core then agrees with Python on dimension, kind, linearity and the conversion factor between every pair of linear units. No exception, no difference. | The translator is feasible as it stands, and the differential test is a contract that holds today. |
| 3 | `register_unit(symbol, dimension, converter, name, *aliases)` does not make `name` a resolvable name: a bare system resolves `m` but not `meter` unless `meter` is listed as an alias. The core registers the key of a conf line as a name. | The translator writes the symbol as the key and the aliases as the names, and leaves `name` out, so the two resolve the same set of names. |
| 4 | A prefix factor in a conf must be a decimal, although a unit factor may be a fraction (`5/9`). Python registers prefixes with an exact `Fraction`. | A prefix factor can be a fraction in the core too (Task 1), so nothing is rounded on the way. |
| 5 | A recipe written on a dimensionless unit makes the line `scale, 1, recipe`, which the parser reads as an offset of 1. | The translator never writes a recipe for a dimensionless unit (the core does not use one there). |
| 6 | The builder already holds the merged `[Prefixes]` and `[Units]` sections it registers from. Building the registry from that text takes 0.5 ms for the whole catalog; translating each registration one by one would be slower and a second path for the same data. | A system built from conf gets its registry from the text; the translator is only for what is registered by hand afterwards. |
| 7 | Python's conversion caches are global, not per system. A comparison made after the tests, outside the system that registered the units, reported EUR to USD as 1.1 instead of 1.08. | The differential tests compare inside `use_system(system)`, right after building the system. The leak itself belongs to steps 3 to 5, which remove those caches. |

**Shape of 2b.** A `UnitSystem` keeps the conf texts it has given its registry (`extend_core`), so pickling rebuilds the registry from them. `register_unit` and `register_prefix` write the unit as text and give it to the registry first, so a unit the core cannot take leaves the system as it was. The builder gives the registry the whole merged text once and registers in Python only. The mirror (`add_derived_unit`, `add_base_unit`, `register_alias`, the prefixed copies), its three `contextlib.suppress` sites and the two lookup paths are deleted. `register_dimension` and `register_alias(exponents, ...)` stay Python-only: they are names and display, and the registry derives its own alias from the names of a unit.

**Decisions taken for step 2b (proposed, to confirm):**

- A converter that is not exactly a `LinearConverter`, an `OffsetConverter` or a `LogarithmicConverter` (a custom `UnitConverter`, or a subclass of those three) is refused with `TypeError` at registration. This is what the design of step 2 already said ("user `UnitConverter` subclasses unsupported"); it is a breaking change and goes in the changelog. The alternative is to keep them Python-only until step 5, which leaves units that the registry does not know.
- `create_system` is strict: a line of `[Units]` or `[Prefixes]` (the shipped files, the file named, or the `physure.conf` of the working directory) that the core cannot read raises `ValueError` listing every such line. The two shipped system files have none.
- A system exposes its registry as the read-only property `UnitSystem.core_registry`. Membership (`in`) stays the system's own: the registry also knows the eight SI base units whatever the system registered.
- A kind that contradicts the converter (an `OffsetConverter` registered as `"delta"`, a level with a recipe) is a `ValueError`: the core would otherwise say what Python does not.

## Outcome of step 2b (a system owns its registry)

- The registry of a system is built from what the system registers: the merged `[Prefixes]` and `[Units]` text for `create_system`, and `unit_definition_text` / `prefix_definition_text` for what is registered by hand, which the system gives to the registry before it changes anything of its own. The texts are kept, so pickling rebuilds the registry.
- The five systems of the parity tests (default, imperial, user conf, currency, bare with a prefix, a recipe, a level and a zero point) agree with the registry on dimension, kind, linearity and conversion factor. The differential test is the contract that steps 3 to 5 keep.
- Membership stays the system's own: the registry knows the eight SI base units whatever a system registered.
- Python's conversion caches are global, not per system. The parity tests compare inside the system that registered the units for that reason; the leak itself ends when steps 3 to 5 remove those caches.
- Step 3 (the seam `physure.domain.measurement.unit_ops`) is planned from here.

## Revised order (2026-10-29): Python keeps no implementation of its own

The maintainer's principle after step 2b: the project began in Python and moved to Rust piece by piece, so what is left in Python is a second implementation that drifts from the core. Python delegates; where the two differ, the core is the reference and Python adopts its behaviour. Two decisions follow from it, taken with the maintainer:

- When `CompoundUnit` starts to delegate to the registry, the core's behaviour wins at once. Each rule (angle and solid angle, `noprefix`, scalar times a point, the product with an absolute temperature, ...) is its own commit, with its test and its changelog line, inside the step. This replaces "one branch per rule after the class swap".
- The seam module `physure.domain.measurement.unit_ops` is dropped. The methods of `CompoundUnit` delegate in place and their algorithms are deleted, so nothing is added to be deleted later.

The steps that remain, in order: **3** units and their conversion are the core's (done, below); **4** the point, delta and level arithmetic of `Quantity` (`_affine_*`, `_logarithmic_add_sub`, the functional module) asks the core (done, below); **5** `Dimension` is a unit over the dimension symbols with exact powers and the core's algebra (done, below; `CompoundUnit` was already a subclass of the core's unit, and the exact rational exponent that it still lacks goes with step 6); **6** `UnitSystem` without dictionaries of its own, `register_unit` taking a scale, a zero point or a level instead of a converter object (the converters module goes), the SymPy parser and the generated modules deleted (split in four: 6a the unit parser is the core's, done, below; 6b `UnitSystem` resolves through its registry; 6c the generated unit modules; 6d `register_unit` takes data); **7** cleanup and 0.3.0. Steps 3 and 4 of the earlier list are merged, because delegating what a unit is without converting through the core made Python resolve names (`centicelsius`) whose conversion it still did as a plain factor.

## Outcome of step 3 (units and their conversion are the core's)

- Prototyped first, then rebuilt as clean commits (the code is identical). Delegating `CompoundUnit` alone left one real hazard: Python resolved names that only the core knows, while `Quantity.to` still converted with Python's converters and treated `centicelsius` as linear (0.01 K instead of 273.16 K, with no error). The conversion plan therefore comes first in the history, and the delegation after it.
- The core gained `noprefix` (the registry remembers the names flagged in the catalog), `UnitRegistry::conversion` (the plan `Quantity::convert_to` follows, as data a caller can apply: affine `scale` and `shift`, or the two stages of a level), and an `expand` that returns a single unit with its zero point and level. The extension raises typed errors.
- Python: `conversion_plan.py` applies the plan with the backend of the quantity (the converters are kept as the formulas of a level stage); `Quantity._convert_via_converters` and `_single_named_converter` are gone; `CompoundUnit` lost about 140 lines; `register_alias` forwards a unit to the core.
- Measured in the same checkout: every row of `benchmarks/bench_unit_ops.py` is equal or faster (`q.to('km')` 52 to 19 µs, `q_speed.to('km/h')` 88 to 32 µs, `Q_` 16 to 13 µs, `q1 + q2` 22 to 17 µs); the first use is 0.177 s (0.18 before); `torch.compile` has the same number of graph breaks on six operations and one less on `t.to('K')`; the suite runs in 69 s instead of 95 s.
- Not done, by design: the point and delta arithmetic and the logarithmic addition still read converters from the unit definitions that the system holds, so they do not exist for a unit only the core knows. They refuse with a clear error until step 4. `test_standard_units_do_not_load_sympy` depends on the order of the tests (it passes only if an earlier test imported SymPy) and fails alone on `main` too; it belongs to the deletion of the SymPy parser.
- Step 4 is planned from here.

## Outcome of step 4 (points, intervals and levels combine as the core says)

- No new API in the core: the plan of step 3 (`conversion`), `factor_between` and `expand` are enough. A point moved by an interval is the interval scaled by the factor between the two units, reinterpreted in the unit of the point and added as two quantities of the same unit; two points are both converted to the base unit (K) and subtracted; levels are converted to the plain quantity they stand for, added, and converted back. Each of these goes through the ordinary operations of `Quantity`, so the uncertainty propagates (the arithmetic of points used to set it to zero) and arrays, tensors and levels of any dimension work.
- One commit per rule, as decided: levels add as quantities (and a level does not add to a plain quantity); a point and an interval; products, quotients and powers with an absolute temperature (the two strict xfails of `test_temperature_algebra_parity.py` are plain tests now); the functional module. About 300 lines of Python that read converters beside the core are gone: `_get_converter_if_simple`, `_affine_to_base`, `_affine_result_absolute`, `_affine_result_delta`, `_affine_add_sub` and the nonlinear paths of `functional.py`.
- Measured in the same checkout: every row of `benchmarks/bench_unit_ops.py` is still at least 15 % faster than before step 3; against the end of step 3, `q1 * q2` is about 10 % slower (it asks the registry whether an operand is an absolute temperature, 0.3 µs each; a memo of the answer costs the same, since hashing a `CompoundUnit` is a call into Rust too, so it was not added) and `q1 + q2` is 3 % faster. `torch.compile` has the same graph breaks, the first use is 0.173 s, and the suite runs in 70 s.
- The `UnitDefinition.converter` objects are no longer read for arithmetic or for conversion, but they still exist: the lazy prefix machinery of `UnitSystem` reads their scale, `register_unit` takes them, the translator to conf text reads their type, and the level stages of `conversion_plan.py` reuse their formulas. They leave with step 6, when `register_unit` takes a scale, a zero point or a level instead of a converter object.
- Not done, by design: the functional `mul_quantities`, `truediv_quantities` and `pow_quantities` only add exponents and do not know a point from an interval, and `RationalUnit.mul` still builds `dB * m` (the core refuses it in a `Quantity` and in the parser). Both belong to the cleanup of step 6.
- Step 5 (`Dimension` and `CompoundUnit` wrap the rational types of the core) is planned from here.

## Outcome of step 5 (a dimension is exact and the core does its algebra)

- Found while planning it: `Dimension` cut every power to an integer, so a fractional dimension came out `Dimensionless` (`(4 s) ** 0.5`, `Hz ** 0.5`, `V/sqrt(Hz)`), after step 3 made `CompoundUnit.dimension` hand its answer to `Dimension` as a float. This is the silent loss of a dimension that the project exists to prevent, and it is the reason this step comes before the cleanup.
- `Dimension` keeps one object per set of powers (the flyweight is what the add and subtract paths and the hashing lean on), but its key is a tuple of `(symbol, (numerator, denominator))` and the algebra is `RationalUnit` over dimension symbols, so there is no second implementation of adding and scaling powers. It does not use `Fraction` on the hot path: `Fraction` is pure Python and its ABC metaclass makes every `isinstance` against it slow (an early version made `Q_` 18 % slower and a pure unit product 38 % slower, for that reason alone); the plain `int` and `(n, d)` cases are checked first, and a memo keyed by the items given makes the usual call do no arithmetic.
- The table of base units in `Quantity._base_unit_for_dimension` (L is m, M is kg, ...) is deleted: `UnitRegistry.unit_of_dimension` answers from the base units the registry declares, which is the base unit a conf declares first for a dimension symbol.
- Measured in the same checkout: every row of `benchmarks/bench_unit_ops.py` is within 4 % of the end of step 4 (`units.m * units.s` is back to 1.1 µs), `torch.compile` has the same graph breaks, the first use is 0.175 s, and the suite runs in 70 s.
- Not done, by design. `CompoundUnit` still keeps its exponents twice (a Python dict of floats, and the dimensions of the Rust unit it subclasses), so a non-integer power of a unit is a float in Python and the closest dyadic fraction in Rust (`s^(1/3)` is `0.3333333333333333`, and the rule that `3 * (1/3)` is `1` holds by luck of the float product, not by arithmetic). The exact rational is the next thing to give it, together with the removal of the dual storage, and both belong to step 6, which already rewrites how a unit is made. `DimVector`, the nine-slot integer dimension of the core, is not used by Python any more; it is exported by the crate and by `physure`, so deleting it is a decision for the cleanup of step 7.
- Step 6 (`UnitSystem` without dictionaries of its own, `register_unit` taking a scale, a zero point or a level, and the SymPy parser, the converters and the generated modules deleted) is planned from here.

## Step 6, split (2026-10-29)

Measuring what step 6 has to delete showed that it is four steps, not one:

- The parallel dictionaries of `UnitSystem` (`UNIT_SYMBOL_REGISTRY`, `UNIT_DIMENSIONS`, `UNIT_REGISTRY`, `ALIASES`, `ALIAS_TO_EXPONENTS`, `_UNIT_RECIPES`, `PREFIX_REGISTRY`, ...) have no reader outside `system.py` but the builder of `startup.py` and six test files, since steps 3 and 4 moved every question to the registry. Only `get_unit` and `in` still read them.
- SymPy was the fallback of the unit parser for one reason only: `**` (the suite reached it with nine distinct strings, eight of them `**`).
- The generated modules `physure/units/core.py` and `_index.py` (13,600 lines) map each catalog name to `CompoundUnit({name: 1})`, which the registry can answer.
- `register_unit` takes converter objects, and 71 calls of 16 test files build them, so changing it is an API break of its own.

The steps: **6a** unit strings are read by the core's parser alone (done, below); **6b** `UnitSystem` resolves through its registry (`get_unit`, `in`), and the lazy prefix machinery, the parallel dictionaries and the Python registration of units and prefixes that the builder does are deleted; **6c** the generated unit modules are replaced by a lookup in the catalog registry; **6d** `register_unit` takes a scale, a zero point or a level instead of a converter object (breaking), the converters module goes, and `CompoundUnit` keeps its exponents once and exactly.

## Outcome of step 6a (the unit parser is the core's)

- The lexer of the core treated `**` as a `*` that swallowed the next star, and read integers only. `**` is the power now and a decimal is read as the exact fraction it stands for.
- `application/parsing.py` calls the core's parser and nothing else; `physure/core/parsing/` (the SymPy unit parser, 250 lines) is deleted. SymPy stays an optional extra for the symbolic features. Reading `m/s**2` no longer imports it, and `test_standard_units_do_not_load_sympy`, which passed only when an earlier test had imported SymPy, passes alone.
- Nothing else in the suite needed the fallback: 1,222 tests pass without it.
- Step 6b is planned from here.
