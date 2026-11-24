# Unit holes (`max`, `min`, `vector`, `linspace`, `round`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `max`/`min` of different dimensions, a vector of different dimensions, `linspace` with endpoints of different dimension and `round` with decimals that carry a unit are errors, instead of answers with a confident unit.

**Architecture:** One shared helper in `physure-script` says whether two numeric `PhsValue`s have the same dimension and words the error; `extremum`, the `vector` builtin, `collect_vector` (elementwise results), `linspace` and `round` use it. The checker's `mixed-extremum` warning becomes an error and `check::explains` learns the new runtime words.

**Tech Stack:** Rust (`physure-script`, `physure-cli`), the spec `docs/superpowers/specs/2026-11-17-unit-holes-design.md`.

**Conventions for every task (from this repository):**
* Every cargo command with `CARGO_TARGET_DIR=D:/Projects/physure/target` (bash). NEVER `cargo clean` it, never delete anything under it.
* Repo files are CRLF: edit with the Edit tool (no `sed -i` on sources), chain `&& git commit` so a failed edit cannot commit half of it.
* English in code, docs, commits. Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Nothing is pushed, merged or version-bumped.
* Known Windows lock `Acceso denegado (os error 5)` in some `physure-cli` suites: re-run that suite alone.
* Probe programs with `D:/Projects/physure/target/debug/phs.exe eval -` (stdin; rebuild first: `cargo build -p physure-cli`), or in tests with `physure_script::interpreter::eval_phs` (returns `Vec<PhsValue>`) and `physure_script::report::evaluate`.

Words of the errors (tests assert on these fragments, keep them):
* max/min: `` max of different dimensions: `1 m` is a length and `1 s` a time; compare values of one dimension `` (use `min` for min; name the two values as written, and the dimensions with the words the repository already uses in similar messages, e.g. the `mixed-extremum` message of `check.rs` or the dimension names of `Dimension`; if there is no word for a dimension, show its symbol, e.g. `L`).
* vector: `` a vector holds one dimension: `1 m` is a length and `2 s` a time; put each dimension in its own vector ``.

---

### Task 1: The tests, red

**Files:**
* Create: `physure-script/tests/unit_holes.rs`

- [ ] **Step 1: Write the test file**

Use this skeleton; add the rows named in the spec section 4. Each refused case asserts the message contains the stable fragment (`max of different dimensions`, `a vector holds one dimension`, `linspace`, `round`), each valid neighbour asserts `eval_phs(..).is_ok()` and, where cheap, the exact value text.

```rust
use physure_script::interpreter::eval_phs;

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

fn last(source: &str) -> String {
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).last().unwrap().to_string()
}

#[test]
fn an_extremum_of_different_dimensions_is_refused() {
    for source in ["max(1 m, 1 s)", "min(1 m, 2)", "max(2, 1 m)", "max(1 m, 1 kg, 1 s)", "max(1 rad, 1)"] {
        assert!(refused(source).contains("of different dimensions"), "{source}");
    }
    assert!(refused("max(1 m, 1 s)").contains("`1 m`"));
}

#[test]
fn an_extremum_of_one_dimension_still_works() {
    assert_eq!(last("max(1 m, 3 mm)"), last("1 m"));
    assert!(eval_phs("max(5 degC, 3 degC)").is_ok());
    assert!(eval_phs("max(1 m, 2 +/- 1 m)").is_ok());
    assert!(eval_phs("max(100 percent, 2)").is_ok());
    assert_eq!(last("min(3, 5)"), "3.0");
}

#[test]
fn a_single_argument_extremum_checks_what_it_is_given() {
    assert_eq!(last("max(vector(1 m, 2 m, 3 m))"), last("3 m"));
    assert_eq!(last("min(vector(1 m, 2 m, 3 m))"), last("1 m"));
    assert_eq!(last("max(2 m)"), last("2 m"));
    assert!(refused("max(vector(1 m, 2 s))").contains("a vector holds one dimension"));
    assert!(refused("max(\"a\")").contains("expects numbers or quantities"));
    assert!(refused("max(vector())").contains("max"));
}

#[test]
fn a_vector_holds_one_dimension() {
    for source in ["vector(1 m, 2 s)", "[1 m, 2 s]", "vector(1, 2 m)", "v = [1 m, 2 m, 3 s]\nv"] {
        assert!(refused(source).contains("a vector holds one dimension"), "{source}");
    }
}

#[test]
fn a_vector_of_one_dimension_keeps_working() {
    assert!(eval_phs("vector(1 m, 2 km)").is_ok());
    assert!(eval_phs("vector(1, 2)").is_ok());
    assert!(eval_phs("vector(vector(1 m, 2 m), vector(3 m, 4 m))").is_ok());
    assert!(eval_phs("vector(\"a\", \"b\")").is_ok());
}

#[test]
fn an_elementwise_result_of_mixed_dimensions_is_refused() {
    assert!(refused("vector(1 m, 2 m) * vector(1 s, 2 m)").contains("a vector holds one dimension"));
    assert!(eval_phs("vector(1 m, 2 m) * vector(1 s, 2 s)").is_ok());
    assert!(eval_phs("vector(1 m, 2 m) * 3 s").is_ok());
}

#[test]
fn linspace_needs_endpoints_of_one_dimension_and_a_plain_count() {
    assert!(refused("use * from array\nlinspace(0 m, 1 s, 3)").contains("linspace"));
    assert!(refused("use * from array\nlinspace(0 m, 10, 3)").contains("linspace"));
    assert!(refused("use * from array\nlinspace(0 m, 10 m, 5 s)").contains("linspace"));
    assert!(eval_phs("use * from array\nlinspace(0 m, 10 m, 3)").is_ok());
    assert!(eval_phs("use * from array\nlinspace(0, 10, 3)").is_ok());
    // the points are in the unit of the first endpoint
    assert_eq!(last("use * from array\nlinspace(0 m, 1 km, 3)"), last("vector(0 m, 500 m, 1000 m)"));
}

#[test]
fn round_takes_a_plain_number_of_decimals() {
    assert!(refused("round(1.5 m, 1 s)").contains("round"));
    assert!(refused("round(1.2345 m, 2 m)").contains("round"));
    assert_eq!(last("round(1.2345, 2)"), "1.23");
    assert!(eval_phs("round(1.5 m)").is_ok());
    assert!(eval_phs("round(1.2345 m, 2)").is_ok());
}
```

If a valid-neighbour expectation fails because the CURRENT implementation prints something else (for example `linspace(0 m, 1 km, 3)` may be wrong today: that is the bug the spec asks you to find), read the output first: for `linspace` the expectation in the test is the correct one (0, 500, 1000 m); for the others copy what the current implementation prints for the valid cases and keep it.

- [ ] **Step 2: Run, expect failures**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test unit_holes 2>&1 | grep -E "^test |test result"`
Expected: the refusal tests FAIL (today they are answers); the valid-neighbour tests pass.

- [ ] **Step 3: Commit** (`test(script): the unit holes of max, min, vector, linspace and round, red`). The branch history keeps a red commit; each next task turns its tests green.

---

### Task 2: One helper, then `max`/`min`

**Files:**
* Modify: `physure-script/src/builtins/core.rs` (`extremum`), a new small module `physure-script/src/builtins/dimensions.rs` (or an existing helper file of `builtins/`; read the directory first and follow its pattern), registered in `builtins/mod.rs`

- [ ] **Step 1: The helper**

```rust
//! The one place that says whether two numeric values have the same dimension (unit holes, audit).

use crate::value::PhsValue; // adapt the path to where PhsValue lives

/// The dimension of a number (dimensionless) or a quantity; `None` for anything else.
pub(crate) fn dimension_of(value: &PhsValue) -> Option<physure_core::Dimension> { /* ... */ }

/// `Ok(())` when `a` and `b` are numbers or quantities of one dimension; otherwise the error `phrase` ("max of different
/// dimensions", "a vector holds one dimension", ...) with both values as written and their dimensions in words.
pub(crate) fn same_dimension(a: &PhsValue, b: &PhsValue, error: impl Fn(&str, &str) -> String) -> PhysureResult<()> { /* ... */ }
```

Read how the checker words a dimension (`check.rs`, the `mixed-extremum` message, uses names such as `L`, `dimensionless`) and how `Quantity` exposes its dimension (`q.unit.dimensions`, `Dimension` / `is_plain_number`). Compare dimensions the way the interpreter already compares them for `+` (`Quantity` add refuses `m + s`); reuse that comparison, do not write a second one. A plain number is dimensionless; a `percent` quantity is dimensionless; an angle (`rad`) has its own marker dimension in every angle mode (do not treat it as dimensionless).

- [ ] **Step 2: `extremum`**

Check every argument against the first with the helper; then compare as before. One argument: `Vector(items)` becomes the extremum of `items` under the same rule (empty vector: error `max expects arguments`-style), a Number or Quantity is returned, anything else is `{name} expects numbers or quantities`.

- [ ] **Step 3: Run**

`CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test unit_holes 2>&1 | grep -E "^test "` — the three `max`/`min` tests are green. Run `cargo test -p physure-script --lib builtins` too (the existing `min`/`max` unit tests in `builtins/tests.rs` must stay green).

- [ ] **Step 4: Commit** (`fix(script): max and min refuse values of different dimensions and check a single argument (unit holes)`)

---

### Task 3: Vectors

**Files:**
* Modify: `physure-script/src/builtins/core.rs` (`"vector" => ...`), `physure-script/src/interpreter/binary_ops.rs` (`collect_vector`)

- [ ] **Step 1: The `vector` builtin** checks its numeric items pairwise against the first numeric item with the helper (message: the vector words with the two items as written). Items that are not Number/Quantity are skipped. `[...]` goes through the same builtin (parser line `name: "vector"`): nothing else to change for the literal.
- [ ] **Step 2: Elementwise results.** After `collect_vector` builds the result items of a binary operation, check the numeric ones the same way; the error names the operation (`*`, `/`, `+`...) and the two result elements. A result of one dimension is untouched.
- [ ] **Step 3: Run** the `unit_holes` vector tests, then `cargo test -p physure-script --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"`: read each failure. A test or documented program that built a mixed vector on purpose is fixed in that program (separate vectors); a failure that is not about mixed vectors is a bug in your change.
- [ ] **Step 4: Commit** (`fix(script): a vector holds one dimension, and so does the result of an elementwise operation (unit holes)`)

---

### Task 4: `linspace` and `round`

**Files:**
* Modify: `physure-script/src/builtins/array.rs` (`linspace`), `physure-script/src/builtins/core.rs` (`round_builtin`)

- [ ] **Step 1: `linspace`.** First print what `linspace(0 m, 1 km, 3)` gives today and record it in the commit message. Then: `a` and `b` same dimension (helper; message mentions `linspace`), `b` converted to the unit of `a` before the means are read (use `Quantity::convert_to` as `limit.rs` does), the count must be a plain number (`Number`, or a dimensionless quantity as today), else an error that says `linspace` count must be a plain number. The count bound check stays.
- [ ] **Step 2: `round`.** The decimals argument must be a Number or a dimensionless quantity (`q.unit.is_plain_number()` or the repository's equivalent); a quantity with a unit is an error `round expects a plain number of decimals, got ...`. `round(x)` and `round(x, 2)` unchanged.
- [ ] **Step 3: Run** the `linspace` and `round` tests of `unit_holes`; then the whole `physure-script` suite as in Task 3.
- [ ] **Step 4: Commit** (`fix(script): linspace and round refuse what has the wrong dimension (unit holes)`)

---

### Task 5: The checker and the report

**Files:**
* Modify: `physure-script/src/check.rs` (severity of `mixed-extremum` ~line 998; `explains`; doc comments that say "the one warning"), `physure-script/tests/check.rs`, `tests/report.rs`

- [ ] **Step 1:** `mixed-extremum` severity `error`. Update every doc comment and the module doc that calls it the only warning, and the spec text that quotes it only if it is current documentation (do not edit historical specs).
- [ ] **Step 2:** `explains("mixed-extremum", finding, runtime)`: true when the runtime message contains `of different dimensions` and the same function name (`max`/`min`) as the finding; add the arm in the table of `explains` with a comment, and the unit assertions in `tests/report.rs` (it accepts its own wording, refuses a `min` message for a `max` finding).
- [ ] **Step 3:** Tests: in `tests/check.rs` the existing `mixed-extremum` test now expects severity error AND that the interpreter refuses the program (the file's convention: "each error also checked against the interpreter"); in `tests/report.rs` the guard `a_flagged_line_is_not_said_twice` gets `max(1 m, 1 s)` and `min(1 m, 2)`, and `a_second_problem_on_a_flagged_line_is_reported_too` gets one line mixing a `max` mismatch with another problem (`undefined_fn(1) + max(1 m, 1 s)`).
- [ ] **Step 4: Run** `cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"`; mutation check: make `explains` return false for the new arm, the guard must fail; revert.
- [ ] **Step 5: Commit** (`fix(script): mixed-extremum is an error, and the report says it once (unit holes)`)

---

### Task 6: Every consumer, then the documents

**Files:** whatever a failure points at; `CHANGELOG.md`, `docs/ROADMAP.md` (the open line "Unit holes outside `limit`"), `physure-cli/src/main.rs` + `PHYSURE_LANGUAGE_GUIDE.md` (the guide text that documents `max`, `min`, `vector`, `linspace`, `round`; written only by `phs doc --save`), the audit if it lists these.

- [ ] **Step 1:** Runs, with totals reported: `cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast`; `cargo test -p physure --no-fail-fast`; Python (rebuild in this worktree: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`, verify `_core.__file__` is under the worktree, `uv run --no-sync pytest -q`); WASM (`cd physure-wasm && wasm-pack test --node`; golden JS: `wasm-pack build --target nodejs --out-dir pkg-node` then `node --test tests/js/generated.test.mjs`); Java (`cargo build --release -p physure-java`, then `cd physure-java && mvn -B test -Dphysure.native.dir=D:/Projects/physure/target/release`). A binding test that builds a mixed vector or a mixed `max` is fixed in that test (separate vectors / same dimension) and listed in the report.
- [ ] **Step 2:** The docs guard and corpus guards stay green; a documented program that used a mixed vector or `max` is fixed in the program.
- [ ] **Step 3:** CHANGELOG `[Unreleased]` **Changed** bullet: the breaking change in plain words with what to write instead (one dimension per vector; compare values of one dimension), the single-argument `max`/`min`, `linspace` and `round`, `mixed-extremum` now an error, commits by hash. ROADMAP: close the open unit-holes line (mention the three rows fixed and that the cascade item and the Python `np.mean/dot/cross` remain) and add the item to the fixed-list sentence. Guide: update the generating text and run `phs doc --save`; the sync test `the_committed_language_guide_is_what_phs_doc_prints` stays green. `uv run --no-sync mkdocs build --strict` (3 old `Div ... unclosed` warnings only).
- [ ] **Step 4:** Commit (`docs: the changelog, the roadmap and the guide record the unit holes`).
