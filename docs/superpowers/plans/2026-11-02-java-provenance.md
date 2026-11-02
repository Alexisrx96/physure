# Java provenance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Java `Quantity` objects carry the sources of their uncertainty across JNI, so correlation, degrees of freedom and expanded uncertainty work in Java as in the core, and function and equation calls bind quantities by value.

**Architecture:** The core owns the lineage; Java carries `(id, contribution)` pairs and hands them back. Four small core pieces (`from_sources`, `sources`/`display_sources`, `new_source_id`, `PhsInterpreter::run_program_with`), then the JNI layer, then the Java classes. Spec: `docs/superpowers/specs/2026-11-02-java-provenance-design.md`.

**Tech Stack:** Rust (physure-core, physure-script, physure-java with `jni`), Java 8-compatible sources, JUnit 5 via Maven (JDK 21 locally).

Commit trailer for every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Rust target dir is shared with the welch-satterthwaite worktree to avoid a cold build: `export CARGO_TARGET_DIR=D:/Projects/physure/.claude/worktrees/welch-satterthwaite/target`.

---

### Task 1: Core: sources as plain data

**Files:**
- Create: `physure-core/src/uncertainty/sources.rs`, `physure-core/tests/sources.rs`
- Modify: `physure-core/src/uncertainty/mod.rs` (module + re-export), `physure-core/src/quantity.rs` (`folded_scale` helper used by `display_parts`)

- [ ] **Step 1: Write the failing test** (`physure-core/tests/sources.rs`): a measured quantity's `sources()` is one `(id, u)` term; `Quantity::from_sources(mean, sources, unit)` of it equals it and `x.sub(&rebuilt)` is exactly zero (same id); two `with_uncertainty` quantities do not cancel; `2 m * 3 km` has `display_sources` equal to `sources` times 1000 and their quadrature sum equals `display_parts().1`; `sources()` is `None` for a Monte Carlo quantity (`Quantity::new_scalar(.., Some("monte_carlo"), Some(100))`); `new_source_id()` is above every id handed out before it and `from_sources` with a larger id reserves it.
- [ ] **Step 2: Run it:** `cargo test -p physure --test sources` → FAIL (items do not exist).
- [ ] **Step 3: Implement** `sources.rs`: `pub fn new_source_id() -> u32 { lineage::fresh_id() }`; `impl Quantity { pub fn from_sources(mean, sources: Vec<(u32, f64)>, unit: RationalUnit) -> Quantity` (Gaussian over `Lineage::from_terms`), `pub fn sources(&self) -> Option<Vec<(u32, f64)>>` (Gaussian only), `pub fn display_sources(&self)` (terms times `folded_scale()`) }`. In `quantity.rs` extract `fn folded_scale(&self) -> Option<f64>` (the `display_name.is_none() && (scale - 1).abs() > 1e-9` test) and use it in `display_parts` without changing its output.
- [ ] **Step 4: Run** `cargo test -p physure` → all pass (the doctests too).
- [ ] **Step 5: Commit** `feat(core): the sources of a quantity as plain data, and a quantity rebuilt from them`.

### Task 2: Script: evaluate with bindings, leaving nothing behind

**Files:**
- Modify: `physure-script/src/interpreter/statements.rs` (new `run_program_with`)
- Create: `physure-script/tests/run_program_with.rs`

- [ ] **Step 1: Write the failing test:** `f(a, b) = a - b` defined in an interpreter; `run_program_with(parse("f(_arg0, _arg1)"), [("_arg0", x), ("_arg1", x)])` with the same measured `x` is an exact zero; with two separate measurements it is `sqrt(2) u`; afterwards `interpreter.env` has no `_arg0`; a binding named like an existing variable shadows it during the call only (the variable is intact afterwards); an assignment in the program does not leak; an error in the program leaves the interpreter unchanged.
- [ ] **Step 2: Run** `cargo test -p physure-script --test run_program_with` → FAIL (no such method).
- [ ] **Step 3: Implement** `pub fn run_program_with(&self, program: &Program, bindings: Vec<(String, PhsValue)>) -> PhysureResult<PhsValue>`: clone `self.env`, insert the bindings, run each statement with `eval_statement_with_env`, return the last value (the variable an assignment set when it yields nothing).
- [ ] **Step 4: Run** `cargo test -p physure-script` → pass.
- [ ] **Step 5: Commit** `feat(script): run a program with values bound by name, leaving the interpreter untouched`.

### Task 3: JNI: sources across the boundary

**Files:**
- Modify: `physure-java/src/lib.rs`, `physure-java/src/main/java/com/physure/Quantity.java`, `NativeEngine.java`
- Test: `physure-java/src/test/java/com/physure/ProvenanceTest.java`

- [ ] **Step 1: Write the failing Java tests** (`ProvenanceTest`): `x.subtract(x)` is exactly `0` with uncertainty `0`; `x.multiply(x)` has uncertainty `2 |v| u`; `x.add(y).subtract(y)` equals `x`; two separate `new Quantity(10, 0.5, "m")` subtract to `0.5 * sqrt(2)`; `x.to("cm")` then `x.subtract(that.to("m"))` is `0`; `2 m * 3 km` result subtract itself is `0` (folded scale); `x.pow(2).divide(x.pow(2))` is `1` with uncertainty `0`; the first-use id is the same when two threads race (`new Quantity(...)` shared by 8 threads each doing `x.subtract(x)`).
- [ ] **Step 2: Run** `mvn -f physure-java/pom.xml test -Dtest=ProvenanceTest` (build the native first: `cargo build -p physure-java`) → FAIL.
- [ ] **Step 3: Implement.** Java: `Quantity` gets the `Sources` volatile holder (resolved lazily by `NativeEngine.newSourceId()`), the package-private constructor `(double, double, String, long[], double[])`, and package-private `sourceIds()` / `sourceCoefficients()`; `round` scales the coefficients. Rust: `get_rust_quantity` reads those two arrays and calls `Quantity::from_sources` (keeping the unit validation and message); one helper `java_quantity(env, mean, std, unit, sources)` builds the object for `make_java_quantity` (with `display_sources`) and for `convertQuantity` (with `sources()` of the converted quantity); `None` sources pass `null` arrays, which the Java side treats as pending.
- [ ] **Step 4: Run** `mvn -f physure-java/pom.xml test` → the whole suite passes (existing strict tests included; fix any test that encoded the old wrong uncertainty and say so in the commit).
- [ ] **Step 5: Commit** `feat(java): a Quantity keeps the sources of its uncertainty across the native calls`.

### Task 4: Trigonometry and `abs` in the core

**Files:** `physure-java/src/lib.rs`, `Quantity.java`, `NativeEngine.java`; test in `ProvenanceTest.java`.

- [ ] **Step 1: Failing tests:** `x.sin().subtract(x.sin())` is `0`; `x.sin()` of `30 deg` uses the degree unit; `sin()` of a length is a `PhysureException`; `x.abs()` of a negative keeps its sources (`x.abs().add(x)` of a negative `x`... `x` is `-3 +/- 0.1`, `x.abs().subtract(x.multiply(-1))` is `0`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** native `mathFunction(Quantity q, String name)` calling `Quantity::{sin,cos,tan,abs}`; Java `sin/cos/tan/abs` call it; delete `trigonometric`, `DoubleUnaryOperator`.
- [ ] **Step 4: Run** `mvn test` → pass (the existing trigonometry tests stay green: the values come from the core now).
- [ ] **Step 5: Commit** `feat(java): sin, cos, tan and abs are the core's, so they keep their sources`.

### Task 5: Degrees of freedom API

**Files:** `lib.rs`, `Quantity.java`, `NativeEngine.java`, create `ExpandedUncertainty.java`, `physure-java/src/test/java/com/physure/UncertaintyDofGoldenTest.java`.

- [ ] **Step 1: Failing tests:** `UncertaintyDofGoldenTest` reads `../tests/golden/uncertainty_dof.tsv` (11 rows), builds each budget (`new Quantity(mean, u, "m").withDof(nu)` summed) and checks `u_c` (1e-12 relative), `effectiveDof` (1e-12), `coverageFactor(p)` (1e-9) and `expanded(p).getExpandedUncertainty()` (1e-9); plus: `withDof` on an exact quantity, on a sum, with `0` or a conflicting value throws `PhysureException`; a conversion keeps the degrees of freedom; `fromSamples(new double[]{10,10.2,9.9,10.1,10.3}, "V")` is mean `10.1`, uncertainty `sqrt(0.025/5)`, `effectiveDof` `4`; `ExpandedUncertainty.toString()` rounds by the GUM rule.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** natives `declareDof`, `effectiveDof`, `expanded` (returns `k`, `U`, `nu_eff`), `fromSamples`, `gumRound` and the Java methods and `ExpandedUncertainty` (immutable; getters `getValue`, `getStandardUncertainty`, `getExpandedUncertainty`, `getCoverageFactor`, `getCoverageProbability`, `getEffectiveDof`, `getUnit`).
- [ ] **Step 4: Run** `mvn test` → pass.
- [ ] **Step 5: Commit** `feat(java): degrees of freedom, coverage factor and expanded uncertainty`.

### Task 6: Function and equation calls by value

**Files:** `lib.rs`, `NativeEngine.java`, `UnitRegistry.java`, `PhyFunction.java`, `PhyEquation.java`; tests in `ProvenanceTest.java`.

- [ ] **Step 1: Failing tests:** a registry function `f(a, b) = a - b`: `f.call(x, x)` is exactly `0`, `f.call(x, y)` with two measurements is `sqrt(u_x^2 + u_y^2)`; after the calls `registry.evaluate("_arg0")` throws (nothing left behind); `PhyEquation("V = R * I").call("R", r, "I", r)`-style reuse of one measurement under two names is exact where it should be; a `String` argument still works; `callMap` unchanged.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** native `evaluateQuantityWith(handle, expr, names, values)` through `run_program_with` inside `with_catalog`; `UnitRegistry.evaluateWith(String, String[], Quantity[])` (synchronized, closed check); `PhyFunction.call(Quantity...)` binds `_arg0...`; `PhyEquation.call(Object...)` binds `Quantity` values by name and substitutes the rest as text.
- [ ] **Step 4: Run** `mvn test` → pass.
- [ ] **Step 5: Commit** `feat(java): function and equation calls bind quantities by value, with their sources`.

### Task 7: Remove the workaround, update the generator

**Files:** `QuantityVector.java`, `physure-script/src/codegen/java.rs`, `physure-script/tests/codegen_dof.rs`.

- [ ] **Step 1: Failing tests:** generator test expects Java output to contain `.withDof(9.0)` (and `Double.POSITIVE_INFINITY` for `dof inf`) and no refusal; a `QuantityVector` test that `v.dot(v)` of vectors built from distinct but equal quantities is not the reused-measurement answer, and `v.dot(v)` of the same vector object is (no `a == b` branch needed).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement:** delete `a == b ? a.pow(2) : a.mul(b)` in `product`; java.rs emits `.withDof(n)`; drop `dof_refusal` from java.rs.
- [ ] **Step 4: Run** `cargo test -p physure-script` and `mvn test` → pass.
- [ ] **Step 5: Commit** `feat(java): the generator writes the degrees of freedom; the vector product needs no workaround`.

### Task 8: Docs and changelog

**Files:** `CHANGELOG.md`, `ROADMAP.md`, `docs/ROADMAP.md`, `docs/scientific_gaps_and_rigor.md`, the Welch–Satterthwaite spec (Java section), `physure-java/README.md`, `structure.md` if it lists modules.

- [ ] **Step 1:** Update every statement that Java is left out (grep `left out`, `Java after`, `WASM and Java follow`); add the changelog entries: Added (Java provenance and degrees of freedom API, `run_program_with`, core `from_sources`/`sources`), Changed (`x.subtract(x)` is `0`, `x.multiply(x)` is `x^2`, results that reuse a measurement are smaller and correct).
- [ ] **Step 2: Docs build:** `cd physure-python && uv run mkdocs build --strict` (no relative source links).
- [ ] **Step 3: Commit** `docs: Java quantities keep their sources`.

### Task 9: Final verification

- [ ] `cargo test --workspace` (excluding what needs a Python venv), `cargo clippy -p physure -p physure-script -p physure-java`, `mvn test` on JDK 21, `uv run ruff check .` untouched, Sonar not run unless asked (the local server scan is the user's gate before the push).
- [ ] Report to the user; they fast-forward `main`. No push.
