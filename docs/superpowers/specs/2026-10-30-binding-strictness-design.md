# A stricter test of the Java and WASM bindings — findings and design

**Date:** 2026-10-30
**Status:** done on `test/bindings-strict`, at the maintainer's request ("increase the tests to identify any bug or issue; be more strict with Java and WASM"). Nothing here has run in CI yet.
**Base:** local `main` @ `5d57444` plus `test/java-lts-matrix` and `fix/java-registry-handle` (this branch is stacked on them).
**Follows:** `2026-10-30-binding-conformance-design.md` (F3), whose contract tested unit exponents and scales only.

## Method

The F3 contract checks one thing: what `getUnitExponents` and `getUnitScale` return. Everything else a binding exposes (quantity arithmetic, evaluate, the calculus calls, functions, equations, vectors, the lifecycle of the native handle, threads) was untested. The surface was read first (`Quantity.java` and the other Java wrappers, the JNI functions, the WASM classes), then probed with throwaway programs that printed what the binding did for several hundred inputs, then turned into tests that state what a correct binding does: the answers the core gives, with the uncertainty, scale and dimension intact, an error where there is no answer, and no crash.

The yardstick is the project's own rule: unit correctness is the product, so a wrong number with a confident unit is worse than an exception, and an uncertainty is never dropped.

## What it found

Severity: **A** silent wrong answer or loss of data, **B** a crash, a hang or a feature that never worked, **C** an inconsistency or a rough edge.

| # | Sev. | Where | What happened | Fixed in |
|---|---|---|---|---|
| 1 | A | core, both bindings | `2 m * 3 km` was handed over as 6 next to the scale-less symbol `m^2`: 1000 times too small, no error (`value.mean()` and `unit.__repr__()` read separately; `Display` folded the scale in, the accessors did not). Also `3 km / 2 m`, vector dot products across scales. | `Quantity::display_parts()`; JNI results and WASM getters read it |
| 2 | A | Java | the native operations never received the uncertainty: `(10 ± 0.5 m) + (5 ± 0.3 m)` had none; every conversion and power too | `lib.rs`: the field goes in, the triple comes out |
| 3 | A | Java | `1 m + 5` was `6 m` and `5 + 1 m` was `6`: the wrapper caught every native error and added the raw numbers | `Quantity` is the native operation |
| 4 | A | Java | `20 degC - 5 K` was `288.15 K`, `20 degC + 5 K` was refused (the wrapper converted before adding); `20 degC * 2` was `40 degC` (the core refuses it) | same |
| 5 | A | Java | `sin(90 deg)` was 0.894 and `sin(1 m)` a number (the unit was ignored) | angle converted natively, uncertainty propagated |
| 6 | A | Java | `evaluate("a = 15 m; b = 5 m; a * b")`, the README's own example, returned 15 of the unit `m\n5.0 m\n75.0 m^2`; an uncertain result was rounded for display (0.2236 read 0.2) and read as the unit `± 0.10 m` | native `evaluateQuantity` returns the last value exactly |
| 7 | A | Java | `norm()` of `(3 ± 0.1, 4 ± 0.1) m` had uncertainty 0.071 for a first-order 0.1: `x * x` went through the core as two independent factors (identity is lost crossing JNI) | a component times itself is its square |
| 8 | A | core | `Quantity::with_uncertainty(1, NaN, "m")` came out exact: NaN and infinity were read as "no uncertainty" | refused |
| 9 | A | Java | `round(1e300, 20)` was 0.092 (a `long` overflow); `1 m / 0` was `Infinity ± NaN`; `Quantity.parse("1,5 m")` was 1 of the unit `1,5 m` | `BigDecimal`; delegated to the core; refused |
| 10 | A | Java | `PhyFunction.add/subtract/multiply/divide/compose` returned the left operand unchanged for functions defined in PHS; a call dropped the uncertainty of its arguments | built from the definitions; refused for mixed kinds |
| 11 | B | Java | eight threads calling `evaluate` on one `UnitRegistry` ended the JVM with heap corruption (`0xC0000374`); `close()` could free the handle under a running call | every call, `close()` included, holds the registry's lock |
| 12 | B | script | 24 nested parentheses took 53 s (18 took 19 s in WebAssembly); every level parsed its contents twice. 100 levels overflowed the stack and ended the process | one rule per primary; brackets nested past 64 levels are a parse error |
| 13 | B | Java | `solve`, `deriv`, `integral` and everything built on them failed with "Undefined function": the interpreter never unlocked the calculus builtins (WASM does) | unlocked |
| 14 | B | Java | `PhyEquation.call` failed for every equation ("solve expects equation string and target string") | solves for the one variable not given |
| 15 | B | core | the unit of `sqrt(4 m)` was written `m^1/2`, which the parser refuses, so a result could not be used in a second operation in any binding | written `m^(1/2)` |
| 16 | B | catalog | the `mass` category listed `u`, which is not a unit (only the prefix micro) | removed; a test parses every unit of every category |
| 17 | C | Java, WASM | a custom conf line that could not be read was dropped silently, and the conf's `[Settings]`, which are process-wide, changed the whole process | read strictly (`extend_registry`): `Invalid unit definitions: ...` |
| 18 | C | Java | a double quote in an argument of `solve`, `deriv` or `integral` ended the PHS string it is put in and the rest ran as a statement | `IllegalArgumentException` |
| 19 | C | Java | `PhyEquation` split on the `=` of `<=` and `==`; `substitute` took `$` and `\` in the value as regex syntax; `det` stopped at 2x2; a `null` was a native "Invalid JNI string" | fixed |

Findings 1, 8, 12, 15 and 16 are in the shared crates, not in a binding: the CLI and the language server share the parser of 12, and every caller of the core's unit text shares 15.

## What was verified

- Java: 179 tests on Java 8, 11, 17, 21 and 25 (the contract suite, `CustomRegistryTest`, and four strict classes: `QuantityStrictTest`, `RegistryStrictTest`, `LinearAlgebraStrictTest`, `FunctionStrictTest`), no warning on any of them. Before the fixes 25 of the first 62 strict tests failed, one concurrency probe killed the JVM, and the deep-nesting probe hung or killed it.
- WASM: `wasm-pack test --node`, 84 tests (43 existing, the 1 contract test, 40 new in `tests/strict.rs`). Before the fixes 5 of the first 39 failed.
- Core: `display_parts.rs` (8) and `binding_inputs.rs` (3). `physure-script`: `tests/nesting.rs` (5) and the 413 existing tests, unchanged. The workspace suite (`physure`, `physure-script`, `physure-cli`, `physure-lsp`: 790 tests) and the Python suite (1274 tests) pass; one old Python test that exercised the Java fallback `combineUncertainty` is deleted with the fallback.
- The new tests were run against the old code first and failed for the reasons above.

## Decisions taken without asking (to overrule)

1. **The nesting limit is 64.** In a release build 100 levels still parse on a 1 MB stack (a JVM thread, WebAssembly) and 150 overflow it, so 64 leaves headroom for the evaluation that follows the parse. It is a constant, `MAX_NESTING_DEPTH`.
2. **`UnitRegistry.evaluate` is strict**: a result that is not a quantity (a string, nothing) is a `PhysureException`; it used to be a quantity of 1 with the text as its unit. `Quantity.parse` stays lenient for a bare unit (`"m/s"` is 1 m/s) but refuses text that starts like a number and is not one.
3. **Java arguments**: `null` is a `NullPointerException`; a `Quantity` with a `NaN` or infinite uncertainty is an `IllegalArgumentException`; a negative uncertainty is its magnitude (as in the core).
4. **WASM `UnitRegistry.fromContent` throws** on a definition it cannot read (it returns a `Result`); it never could fail before.
5. **`approxEquals` is unchanged** (absolute tolerance 1e-6 in this quantity's unit: 1e-9 m and 5e-9 m are "equal"); its Javadoc says so.

These change behaviour for callers whose code relied on the old answers, which were wrong; the changelog lists each.

## Not fixed

- **The PHS interpreter has one catalog per process**, so a custom conf never reaches `evaluate` (Java and WASM): the registry of a binding answers `getUnitScale` and `getUnitExponents` from its own catalog and `evaluate` from the default one. A change in `physure-script`.
- **`evaluate` costs about 0.4 ms per call** in Java; it is the interpreter, not the binding.
- **Unit text is not idempotent in its spacing**: a unit the core builds reads `m * s^-1`, the same unit typed reads `m*s^-1`. Cosmetic; both are the same unit.
- **A conf with text before its first section is accepted** (`this is not a conf [[[` is ignored), as are blank and comment lines; an unreadable line inside a section is an error.
- **Java is tested on Linux x64 in CI and on Windows here**; the natives for macOS and aarch64 that the jar ships are not loaded by any test.
- **CI has not run**: `bindings.yml` with the new tests, and the Maven build on a runner.
