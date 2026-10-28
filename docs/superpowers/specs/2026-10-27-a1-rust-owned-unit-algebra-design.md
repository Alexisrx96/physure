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
