# N5b: `phs eval`, `phs unit`, `phs check -` and the JSON report — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** an agent can run `phs eval program.phs` (or `-` for stdin) and get one JSON document: the value, unit, uncertainty and dimension of every result, plus the checker's findings, the syntax errors and the runtime errors, with the sandbox of N5a around it.

**Architecture:** `physure_script::report` owns the data shapes (`Report`, `Value`, `Finding`, `UnitDescription`) and `evaluate(source, limits, seed)`, which parses, runs the static checker, evaluates statement by statement on a sandbox thread under `with_seed`, and renders results with `Quantity::display_parts()` and `UnitRegistry::facts`. `physure-cli` adds three thin commands over it and a watchdog that ends the process if the deadline is not honoured.

**Tech Stack:** Rust, `serde`/`serde_json` (already dependencies of both crates).

Design: `docs/superpowers/specs/2026-10-30-agent-validation-surface-design.md` (section "N5b"). Stage N5a (merged) provides `Limits`, `run_sandboxed`, `with_seed`.

---

### Task 1: the report shapes and `evaluate`

**Files:** Create `physure-script/src/report.rs`; modify `physure-script/src/lib.rs`; test `physure-script/tests/report.rs`.

Public surface:

```rust
pub struct Report { pub ok: bool, pub results: Vec<Item>, pub diagnostics: Vec<Finding> }
pub struct Item { pub line: usize, pub name: Option<String>, pub value: Value }      // `value` flattened in JSON
pub struct Value { pub kind: &'static str, pub text: String, pub value: Option<f64>, pub uncertainty: Option<f64>,
                   pub unit: Option<String>, pub dimension: Option<String>, pub items: Option<Vec<Value>> }
pub struct Finding { pub line: usize, pub severity: &'static str, pub code: String, pub message: String }
pub const DEFAULT_SEED: u64 = 0;
pub fn evaluate(source: &str, limits: Limits, seed: u64) -> Report
```

Rules (each is a test in `tests/report.rs`):
- `x = 9.81 m/s^2 +/- 0.02 m/s^2` then `x * 2 s` gives items for both lines, the second a `quantity` with `unit` `m/s`, `dimension` `L·T⁻¹`, `value` 19.62, `uncertainty` 0.04.
- A number, a boolean, a string and a vector (`items`, all elements, not the display's first three) have `kind` `number`, `bool`, `string`, `vector`.
- An assignment's item carries `name`.
- Dimension mismatch (`1 m + 1 s`): `ok` false; a `dimension-mismatch` finding at line 1 from the checker; the interpreter's own error for that line is dropped (same line already has an error finding).
- Unknown unit: `5 fooz`: finding `unknown-name`.
- Syntax error: `ok` false, no results, one finding `syntax` at the line pest names (0 when the message has none).
- A runtime error that the checker did not see (`1 / 0 m`, or a call to an undefined function): finding `runtime`, evaluation goes on to the next statement.
- A limit error (`fn f(x) = f(x)` then `f(1)`; `Limits` with a past deadline): finding `limit`, evaluation stops.
- `export3d` to a file: finding `forbidden`, no file written.
- Output cap: with `max_output_bytes = Some(50)` and five long results, results stop before the cap is passed, one `limit` finding says so, `ok` false.
- Same seed, same JSON: two runs of the same program with the same seed serialize to identical strings. (Monte Carlo cannot be selected from inside `evaluate`: the mode is a host setting, covered by the N5a test.)

- [ ] Write the tests, see them fail to compile, implement, run `cargo test -p physure-script --test report`, commit `feat(script): report::evaluate, the structured answer of a sandboxed run`.

### Task 2: `describe_unit`

**Files:** `physure-script/src/report.rs`, test `physure-script/tests/report.rs`.

```rust
pub struct UnitDescription { pub unit: String, pub dimension: String, pub kind: &'static str, pub linear: bool, pub alias: String, pub simplified: String }
pub fn describe_unit(symbol: &str) -> Result<UnitDescription, String>
```

- `km` is dimension `L`, kind `delta`, linear; `N` has `simplified` in base units and dimension `L·M·T⁻²`; `degC` is kind `point`, not linear; an unknown symbol is `Err("unknown unit 'fooz'")`.
- [ ] Tests, implement, commit `feat(script): report::describe_unit`.

### Task 3: the CLI commands

**Files:** Create `physure-cli/src/eval.rs`; modify `physure-cli/src/main.rs` (dispatch, help), `physure-cli/src/check.rs` (`-` reads stdin), `physure-cli/README.md`.

- `phs eval <file|-> [--seed N] [--timeout-ms N]`: prints the `Report` as JSON on stdout; exit 0 when `ok`, 1 when not, 2 for a usage error or an unreadable file. Default timeout 5000 ms. A watchdog thread ends the process with a JSON `limit` report and exit 1 if the evaluation has not returned 2 s after the deadline.
- `phs unit <symbol>`: prints `UnitDescription` JSON, exit 0; an unknown unit prints `{"error": "..."}` and exits 1.
- `phs check -` reads the program from stdin.
- Tests in `eval.rs`: `evaluate_text` returns `(json, exit_code)` for success, a dimension error, a syntax error and an unknown unit (the draft's acceptance list), parsed back with `serde_json`; `unit_json` for `km` and an unknown symbol; the watchdog is tested through a function that takes the grace period, with a native sleep standing in for a hang.
- [ ] Implement, `cargo test -p physure-cli`, run the real binary once for each command, commit `feat(cli): phs eval, phs unit, and phs check - for agents`.

### Task 4: docs

- [ ] README section "Agents" (JSON shape, exit codes, limits), `CHANGELOG.md` Added entry, spec status line (N5b done), commit `docs: N5b`.
