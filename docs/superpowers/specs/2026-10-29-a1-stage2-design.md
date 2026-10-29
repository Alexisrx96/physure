# A1, stages 2 and 3 — replacing Python's unit classes — design

**Date:** 2026-10-29
**Status:** proposed; the decisions of the last section wait for the maintainer
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

## Decisions for the maintainer

1. **Order.** Registry first (steps 1 and 2, then the seam, then the classes) instead of the spec's classes first and system after. Recommended: the finding above is why.
2. **Where user-visible behaviour changes land.** Each in its own branch after the classes are swapped (step 6), so the swap is judged on "nothing a user sees changed" and each rule on its own diff. The alternative is to fold them into the swap, which is fewer branches and a harder review.
3. **What `Dimension` becomes.** A wrapper of the core's rational dimension (fixes the silent loss of fractional powers) instead of the integer `DimVector` the spec named (a rename, and the wrong answer stays).
