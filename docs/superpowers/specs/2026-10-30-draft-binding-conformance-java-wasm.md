# Draft: a conformance contract for the Java and WASM bindings (F3)

**Status:** DRAFT, not approved. Source: finding F3 of `2026-10-30-post-a1-inventory.md`.

## Problem

After A1 the semantics of units live in one place, the Rust core. Python is checked against it by four parity suites and a frozen golden file. The other two bindings are checked by nothing that runs.

| | Python | Java (`physure-java`) | WASM (`physure-wasm`) |
|---|---|---|---|
| Tests in the repository | parity suites over the whole catalog, golden file | **none**: `physure-java/src/test` does not exist | 14 `#[wasm_bindgen_test]` inline in `src/registry.rs` |
| Run by CI | `tests.yml` (Python 3.11 to 3.14) | no workflow runs Maven tests; `java-release.yml` builds the native libraries and deploys | no workflow runs `wasm-pack test`; `wasm-release.yml` builds and publishes |
| Registry API exposed | the full set (`facts`, `knows`, `conversion`, `extend_conf`, ...) | `getUnitExponents`, `getUnitScale`, `evaluate`, `getCategories`, `solve`, `deriv`, `integral` | the same list in `js_name` form |

The golden file that freezes the core's answers is `physure-python/tests/core/golden/unit_semantics.json` (32 expressions: dimension, kind, linearity, alias, simplified form), read by `physure-core/tests/unit_facts_golden.rs`.

What the contract can and cannot catch: Java and WASM call the same core code, so there is no second implementation of the algebra that could drift, the way Python had. What can go wrong is in the binding: marshalling of numbers and maps, error mapping, a native library built from a different commit, a release published without ever being exercised. Java 0.2.4 and 0.3.0 were built and sent to the registry without a single test having run against them.

## Options

- **A. A minimal contract.** One test per binding that feeds the golden expressions through the calls the binding already has (`getUnitExponents`, `getUnitScale`, `evaluate`) and compares dimension and scale with the golden values; wired into CI.
- **B. A contract on the facts.** Add a `facts(expr)` call to both bindings that returns the same JSON the core produces, and compare the full golden record. Larger surface, but it is the one comparison that covers kind, linearity and alias.
- **C. Do nothing.** Argues from "same core, same answer". The record above (a published artifact never exercised) is the argument against.

## Recommendation

A first, in a form that costs little CI: the golden file moves to a neutral place that every consumer reads (for example `tests/golden/` at the repository root, with the Python and Rust readers updated), then:

1. Java: a JUnit test and a Maven `test` phase run by a CI job that builds the JNI library for the runner's platform only.
2. WASM: run the existing 14 tests plus the golden test with `wasm-pack test --node`, in a CI job.
3. B afterwards, if the maintainer wants the bindings to expose `facts`.

Do not state "bit for bit across Python, Rust, the JVM and WebAssembly" in documentation until a test measures it: `getUnitScale` returns a `double` and the comparison would have to be exact.

## Open questions

- Is the CI time acceptable? A Java job needs the Rust toolchain and a native build; the existing `tests.yml` does neither today.
- Should the bindings expose `facts` (option B), or is the current surface the intended one?
- Where should the neutral golden file live, given that `physure-core/tests` currently reaches into `physure-python/tests`?

## Acceptance

- A change that alters a unit's dimension or scale fails the Rust, Python, Java and WASM jobs from the same golden file.
- A release workflow for Java or WASM cannot start from a commit whose binding tests did not pass (a `needs:` on the test job).

## Size and dependencies

Size M, risk low. Independent of the other drafts; it does help N5 (a validation surface exposed to outside callers needs a contract first).
