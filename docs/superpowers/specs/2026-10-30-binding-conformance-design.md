# A conformance contract for the Java and WASM bindings (F3) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer on 2026-10-30 (option A, and a path-filtered `bindings.yml`; the two questions of the last section), implemented on `test/binding-conformance`. The CI workflow has **not run on GitHub yet**. Extended the same day (`test/java-lts-matrix`) at the maintainer's request: the Java suite runs on every LTS, 8 to 25.
**Base:** local `main` @ `9a7eef6`
**Follows:** the draft `2026-10-30-draft-binding-conformance-java-wasm.md` (finding F3 of `2026-10-30-post-a1-inventory.md`).

## Problem

After A1 the semantics of units live in one place, the Rust core. Python is checked against it by four parity suites. The other two bindings were checked by nothing that ran.

| | Python | Java (`physure-java`) | WASM (`physure-wasm`) |
|---|---|---|---|
| Tests before | parity suites over the whole catalog, a golden file | **none**: `src/test` did not exist | 43 `wasm_bindgen_test` (14 of them in `registry.rs`); `wasm-pack test --node` passes locally |
| Run by CI before | `tests.yml` | no workflow ran Maven tests | no workflow ran `wasm-pack test` |

Java and WASM call the same core code, so no second implementation of the algebra can drift the way Python's did. What can go wrong is in the binding: how numbers and maps are marshalled, how errors map, a library built from another commit, a release published without a test ever having run against it. Java 0.2.4 and 0.3.0 were built and sent to the registry without one.

## What it found

The first run of the Java contract test failed on four of 44 rows. `UnitRegistry.getUnitExponents` returned only the numerator of each exponent (`(num, _den)` → `num as i32` in `physure-java/src/lib.rs`), so a fractional power came back as a wrong whole number with no error:

| Expression | Java returned | The core says |
|---|---|---|
| `s^0.5` | `{s=1}` | `{s=0.5}` |
| `m^1.5` | `{m=3}` | `{m=1.5}` |
| `Hz^0.5` | `{s=-1}` | `{s=-0.5}` |
| `kg^0.25*m/s^0.5` | `{kg=1, m=1, s=-1}` | `{kg=0.25, m=1, s=-0.5}` |

The WASM binding had the same bug and was fixed in August (`f3fbfc1`); the Java one was not. Every scale and every whole exponent agreed. This is the kind of answer the project forbids ("never silently drop a dimension"), and it is in the Java artifact up to 0.3.0.

A second defect turned up afterwards, not through the contract but while measuring the cost of a call (`2026-10-30`, for the question of per-LTS builds): `get_registry(_handle)` in `physure-java/src/lib.rs` ignored its handle and parsed the embedded catalog on every call (about 0.8 ms per `getUnitScale`), and `initRegistryFromPath` and `initRegistryFromContent` built the custom registry and dropped it, returning a plain `PhsInterpreter::default()`. A unit defined in a custom conf was therefore unknown to its own registry. The handle is now a `RegistryHandle { registry, interpreter }`, the embedded catalog is parsed once per process (a `OnceLock`) and copied into each new registry, and `CustomRegistryTest` (6 tests; 4 of them fail on the old code with "Unknown unit") holds it. `getUnitScale` went from about 800 µs to 2.6 µs and `getUnitPowers` to 11 µs (release native library, JDK 25, this machine). Not fixed: the PHS interpreter behind `evaluate` has a single catalog per process, so a custom conf still does not reach it (a change in `physure-script`, not done and not yet in the inventory), and WASM's `evaluate` has the same limit.

## Decisions (maintainer, 2026-10-30)

| Question | Decision | Rejected |
|---|---|---|
| How far? | **A: the golden file through the calls the bindings already have.** No new call whose only purpose is the test. | B: expose `facts(expr)` in both bindings (a larger public surface on a published artifact). |
| Where does CI run it? | **A new `bindings.yml`, path-filtered**, also called by `java-release.yml` and `wasm-release.yml` before they publish. | Adding both jobs to `tests.yml`, so that every pull request pays for the JNI and wasm builds. |

The fix for the bug above was not asked about; it is the conservative one and is listed under "Open for review".

## What was built

| Piece | Change |
|---|---|
| `tests/golden/unit_dimension_scale.tsv` | 44 rows: expression, scale, base-unit exponents. The 32 expressions of the older golden file plus fractional powers (`s^0.5`, `m^1.5`, `Hz^0.5`, `kg^0.25*m/s^0.5`), a negative power, spelled-out prefixes (`kilometer`, `attoampere`), `unity`, and compound units. Generated from the core's registry; the scale is an `f64` in the shortest form that reads back exactly, each exponent the numerator over the denominator as an `f64`. Line-based on purpose: Java reads it without a JSON library. |
| `physure-core/tests/binding_contract_golden.rs` | Checks the core against the file. `PHYSURE_BLESS_GOLDEN=1` rewrites the rows from the core's current answers, for a change made on purpose; the diff is the review. |
| `physure-java/src/test/java/com/physure/BindingContractTest.java` | JUnit 5. One dynamic test per row and check (88), plus one for whole exponents and one for the refusal below. Scales are compared exactly (`delta` 0). |
| `physure-java/pom.xml` | A `release-8-api` profile (JDK 9+: `maven.compiler.release=8`, so a Java 9+ method cannot compile into the jar; checked with `String.isBlank()`, rejected with the profile and accepted without it) and `-Xlint:-options` (no build warning on any of the five JDKs). `junit-jupiter` 5.10.3 (test scope), `maven-surefire-plugin` 3.2.5 with `-Djava.library.path=${physure.native.dir}` (default `../target/debug`). Nothing changes for a consumer of the jar. |
| `physure-wasm/tests/binding_contract.rs` | One `wasm_bindgen_test` over every row, collecting the disagreements into one message; the golden file comes in with `include_str!`. |
| `tests/golden/unit_semantics.json` | Moved from `physure-python/tests/core/golden/` so that nothing under `physure-core/tests` reaches into the Python tree. The two Python readers and the Rust reader follow. |
| `.github/workflows/bindings.yml` | `native` (Rust, `cargo build -p physure-java`, the library uploaded as an artifact), `java` (needs `native`; a matrix over every LTS, Java 8, 11, 17, 21 and 25, each downloading that library and running `mvn -B test`; `fail-fast` off) and `wasm` (Rust with the wasm32 target, Node 20, `cargo install wasm-pack`, `wasm-pack test --node`). Triggers: push to `main` and pull requests that touch `physure-core`, `physure-script`, `physure-java`, `physure-wasm`, `tests/golden`, `Cargo.toml`, `Cargo.lock` or the workflow; and `workflow_call`. Its concurrency group has its own name, because a called workflow that used `github.workflow` would share the group of its caller and deadlock. |
| `java-release.yml`, `wasm-release.yml` | A `bindings-tests` job (`uses: ./.github/workflows/bindings.yml`, after `check-version`); `publish` needs it. The Java publish runs `mvn ... -DskipTests` because the tests ran in that job. |
| `physure-java` fix | `getUnitExponents` keeps `Map<String, Integer>` and throws `PhysureException` for a unit that has a fractional exponent; the new `getUnitPowers` returns `Map<String, Double>` with the exact powers, as WASM does. |

## What was measured, and what was not

Measured on this machine (Windows x64, Maven 3.9.16, `wasm-pack`, Node):

- The core test and the moved golden test pass; the two Python modules that read the moved file pass (88 tests).
- The WASM contract passes under `wasm-pack test --node`, together with the 43 tests that were already there.
- The Java contract failed on the four rows above before the fix and passes after it (90 tests, 0 failures).
- The Java suite passes, 90 of 90, on every LTS: Temurin 8 (8u504), 11 (11.0.32.1), 17 (17.0.20.1) and 25 (25.0.5), whose SHA-256 was checked against Adoptium's metadata, and Microsoft OpenJDK 21 (21.0.11), each with a clean build. Java 25 warns that source and target 8 are obsolete and, without the `native-access` profile, that `System.loadLibrary` is a restricted method that a later release will block.
- A wrong row (`km` scale off by one ulp, `m^1.5` with the exponent 3) fails all three suites with a message that names the row.

**Not measured:** `bindings.yml` itself (a workflow cannot run from a checkout, so the matrix has run on Windows with a `.dll`, not on the Linux runners with a `.so`), whether Temurin 25 is available to `actions/setup-java` on the runners, the time of `cargo install wasm-pack` on a runner, and the reusable-workflow call from the two release workflows. The first pull request that touches a path filter and the next Java or WASM release are the tests of those.

## What the contract does not do

- It does not make the bindings agree with each other where the core changes on purpose: the file is blessed and the three suites move together. That is the point.
- Python is not in this file. It is checked against the core by its own suites, which read `unit_semantics.json` (dimension by physical symbol, kind, linearity, alias, simplified form) and compare the whole catalog; the bindings return base-unit exponents (`kg`, `m`, `s`), a different shape, so the draft's "one golden file for all four" was not possible and was not forced.
- It checks unit expressions, not quantities, PHS evaluation or the calculus calls.
- Java 25 already calls `-source 8 -target 8` obsolete ("will be removed in a future release"). The day a JDK drops 8 the newest entry of the matrix fails to compile the jar; the choice then is to raise `maven.compiler.target` (ending Java 8 support, a breaking change) or to stop testing that JDK. The matrix is what will show it.
- Java is tested on Linux x64 in CI. The jar also ships natives for macOS, Windows and aarch64 Linux, which no test loads.
- Do not write "bit for bit across Python, Rust, the JVM and WebAssembly" in documentation: what is measured is the scale and the exponents of 44 expressions in three of the four.

## Open for review

- **The Java fix.** The alternative to "refuse and add `getUnitPowers`" is to change `getUnitExponents` to return `Map<String, Double>`, which is what WASM returns but breaks every caller that declares `Map<String, Integer>` (a compile error, or a `ClassCastException` for a caller built against 0.3.0). The refusal changes behaviour only for the inputs that were wrong.
- **A release of the fix.** It is in the changelog under Unreleased; the Java artifact on Maven Central keeps the bug until the next `java-v*` release.

## Acceptance

- A change that alters a unit's scale or base-unit exponents fails the Rust, Java and WASM jobs from the same file. *(Shown locally by the mutation above; the CI jobs are open until they run.)*
- A Java or WASM release cannot start publishing from a commit whose binding tests did not pass. *(`needs:` on `bindings-tests`; open until the next release.)*
