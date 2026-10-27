# A2 — an enforced layering contract, then honest package names — design

**Date:** 2026-10-27
**Status:** design approved section by section by the maintainer (the naming decision was delegated, see below); spec awaiting review
**Base:** local `main` @ `b026d88` (repo-hygiene PR-1/2/3 and A1 Stage 0 merged, not yet on `origin/main`)
**Follows:** `2026-10-27-repo-hygiene-onboarding-design.md` (this is its deferred item A2) and `2026-10-27-a1-rust-owned-unit-algebra-design.md`

## Goal

A contributor should be able to answer "which package does this code belong in, and what may it import?" from a written rule that a test enforces. Today the package names (`core`, `domain`, `application`) suggest a layering the code does not follow, and a deferred import inside a function is the usual way a cycle is hidden.

## Decisions taken with the maintainer

| Question | Answer |
|---|---|
| How far does A2 go? | Layers made explicit and enforced, misplaced modules moved, **and** package names that tell the truth. |
| When are modules moved or renamed? | **After A1.** A1 deletes a large part of `domain/measurement`, `application` and `core/parsing`; renaming first would move files that are about to disappear. Until then A2 only fixes imports and enforces the contract. Rename and move **once**. |
| Keep the names or rename the packages? | The maintainer asked for whatever causes the least confusion when maintaining and contributing, and delegated the choice. **Chosen: relocate the few misplaced modules so `core < domain < application` becomes true, keep `domain` and `application`, and rename only `core`** (see "A2b"). To be confirmed at the A2b kickoff, and revertible during this spec's review. |

## Non-goals

- Renaming `domain` or `application`, or the neutral layer names (`kernel`, `model`, `services`) as package names.
- Hoisting the 86 other deferred imports that are not layer violations. They are not wrong under the contract; a comment convention may be proposed later.
- Anything A1 already deletes (`system`, `startup`, `dimensions`, ...). Their violations are recorded as temporary exceptions.
- Publishing releases or pushing branches.

## Evidence (measured on `b026d88`)

Method: `ast` over the 92 modules of `physure-python/physure/`. Each import is classified as **top** (runs at import time), **lazy** (inside a function or method) or **typing** (under `if TYPE_CHECKING:`). The compiled extension `physure._core` is external: counting it as the package root, as a first pass did, invented a nine-file cycle that does not exist.

| Fact | Value |
|---|---|
| Import-time cycles between files | **0** |
| Deferred imports of `physure` modules | 113 (89 across packages, 24 inside one package). An earlier count of 213 included standard-library and extension imports and was wrong. |
| `domain` → `application` | 14 non-typing edges (4 top, 10 lazy); `application` → `domain`: 11 (9 top, 2 lazy) |
| `domain` → `core` | 9 (6 top, 3 lazy); `core` → `domain`: 5, all top |
| Deferred imports from `domain` into optional features | `plotting` 6 and `ext` 2 (`Quantity.plot()`, `to_hdf()`): legitimate under the contract |
| Hub modules (non-typing importers) | `domain.measurement.quantity` 29, `domain.measurement.units` 26, `core.dispatcher` 14, `application.context` 14 |
| The layering that fits the code best | `core < application < domain`: `domain` is in practice the **high** layer, the opposite of what the names suggest |
| `application.context` (the active-system `ContextVar`) and `application.tracing.context` | low-level state that `domain` imports, filed under `application` |
| `core.functional`, `core.parsing.*` | high-level logic (they import `CompoundUnit` and the converters) filed under `core` |
| `from physure import default_system` evaluated at import in three modules | The facade attribute resolves the active system **at that moment**, so the value is frozen at import and ignores later context changes |
| Two `print("DEBUG: ...")` left in library code | `backends/torch_backend.py` (`sparse_matmul`) and `backends/torch/autograd_store.py` (`propagate`) |
| Name collisions around `core` | the Rust crate `physure-core`, the extension `physure._core`, the package `physure.core`, and `backends/kernels/` |
| Blast radius of a package rename (references in code and tests) | `domain` 308, `application` 63, `core` 53; README, tutorials and examples use none of them (they import from `physure`) |
| Module-path-sensitive state | `application/io.py` saves state with `pickle`, which embeds the class's module path |

## The contract (A2a)

Layers run from the bottom, which imports nothing above it, to the top. They are assigned **per module**, not per package.

| # | Layer | Modules (initial assignment) |
|---|---|---|
| 1 | `kernel` (11) | `core.{protocols, dispatcher, registry, formatting, autograd}`, `domain.exceptions`, `application.context`, `application.tracing.context`, `_generated_types`, `infrastructure` |
| 2 | `backends` (10) | `backends.*`, `_jit.*` |
| 3 | `model` (27) | `domain.*` (measurement, symbolic), `units.*` |
| 4 | `services` (17) | the rest of `application.*`, `core.functional`, `core.parsing.*`, `functional`, `module`, `symbolic` |
| 5 | `extensions` (21) | `ext.*`, `nn.*`, `plotting`, `pandas_ext` |
| 6 | `facade` (6) | `__init__`, `cli`, `repl`, `__main__`, `constants`, `scripts` |

**Rule.** A module may import from its own layer or any lower one. A deferred import may additionally reach `extensions` (the legitimate case: `Quantity.plot()` or `to_hdf()` with heavy optional dependencies). Imports under `TYPE_CHECKING` are free. The facade may import from anything, but **nothing may import the facade** from inside the package.

**Baseline: 26 violations** (8 at import time, 18 deferred), all recorded as exceptions:

- **8 tagged `a1`**: `system` → `factories`/`parsing`, `startup` ← `context`/`infrastructure`/`quantity`, `dimensions` → `parsing`, `_jit.tracer` → `domain.measurement.units`. A1 deletes the modules involved.
- **18 tagged `a2a`**, fixed by the pull requests below: 10 import the facade, 5 are `backends`/`_jit` reaching `Quantity`, 3 are the model reaching services.

**Enforcement.** One test, `physure-python/tests/core/test_import_layers.py`, reads `physure-python/tests/core/import_layers.toml` (`tomllib`, standard library) and walks the modules with `ast`; no new dependency, under a second. It fails on a new violation, on an exception that no longer occurs (so the list can only shrink), and on a module that has no layer (a new file must be classified). The rule is written in `structure.md` and `CONTRIBUTING.md`.

## Delivery: four pull requests (A2a)

| PR | Content | Violations closed |
|---|---|---|
| **A: the contract** | The test, the layer map, the 26 recorded exceptions, the documentation. No behaviour change. | 0 (fixes the baseline) |
| **B: stop importing the facade** | Import the real internal module (`Q_` from `application.factories`); `default_system` resolved at call time through `get_default_system()` instead of a default argument frozen at import; `__version__` through `importlib.metadata` in `ext/io.py`. | 10 |
| **C: `backends` and `_jit` do not reach `Quantity`** | The model registers its own torch and jax pytrees through a hook the dispatcher calls when a backend loads; `jit` moves out of `_jit/tracer.py` (the primitives stay) to the services layer; `propagate_affine`, which only needs the extension, is evaluated for relocation into `backends`; the two `DEBUG` prints are removed. | 5 |
| **D: the model does not reach services** | The string parsing of `Quantity.from_input` moves down into the model and the factory uses it; `domain.symbolic.*` is classified as services (it is a client of the model); `trace_formulas` is separated from the tracer's `ContextVar`. | 3 |

After B, C and D only the 8 `a1` exceptions remain, and they disappear with A1. Two behaviour changes, both deliberate: `default_system` now follows the active context, and the debug prints are gone.

## A2b: honest names (after A1)

Run after A1's Stage 3, once the modules A1 deletes are gone. Provisional list, confirmed at the kickoff against the contract's layer map:

- `application/context.py` and `application/tracing/context.py` → the lowest package (they are state every layer needs).
- `domain/exceptions.py` → the lowest package.
- `core/functional.py` and whatever of `core/parsing/*` survives A1 → `application`.
- The lowest package is renamed from `core` to a name that collides with none of `physure-core`, `physure._core` and `backends/kernels` (candidates: `runtime`, `foundation`).

Result: `lowest package < backends < domain < application < ext`, which is what the names say. About 6 to 8 modules move and about 40 importers change; the rename of the lowest package touches about 53 references. Old import paths keep working for one release through re-export modules that emit `DeprecationWarning`, and they keep `pickle` files written by older versions loadable (the class is found by module path and name).

## Risks

| Risk | Mitigation |
|---|---|
| The classification of a module is wrong | The map is data in a TOML file, reviewed in PR A; moving a module between layers is a one-line change |
| The test is too strict and annoys contributors | It prints the offending import, the two layers, and the exact line to add to the exceptions file; a new module only needs one line in the map |
| Pytree registration through a hook happens too late or twice | PR C tests registration with torch and with jax (the existing jit and pytree tests), and registration is idempotent |
| Moving string parsing changes `Quantity.from_input` | The existing test suite (904 tests) and the A1 Stage 0 parity tests run on every PR |
| Renaming `core` breaks users' deep imports | One release of re-export modules with `DeprecationWarning`; README and tutorials do not use deep paths |

## Success criteria

- A written rule says what each package may import, and a test fails when it is broken.
- No module imports the package facade; `default_system` is never frozen at import.
- The only exceptions left in the contract are the ones A1 removes.
- After A2b, the package names order themselves the way the contract does, and none of them collides with the Rust crate or the extension.

## Open items resolved during execution

- Whether `propagate_affine` can move into `backends` without taking the global covariance stores with it (PR C).
- Whether the string parsing moves down into the model or is injected through a hook (PR D); the default is to move it.
- The final name of the lowest package, and whether `domain.symbolic` stays under `domain` (A2b kickoff).
