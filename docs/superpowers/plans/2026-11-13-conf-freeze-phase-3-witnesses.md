# Configuration freeze, Phase 3: witnesses — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A frozen file also pins *behaviour*: a built-in battery of about 40 reference calculations and any number of user scripts are evaluated at `freeze`, their results stored bit for bit in the sealed `[Witness]` block, and re-evaluated by `phs conf verify --run`. A change the engine pin cannot see (another CPU or C library, a library change the version string does not reflect) becomes a reported, bit-exact difference. `update` and `seal` can no longer change a validated result silently.

**Architecture:** The model already carries `[Witness]` pairs and `WitnessScript`s inside the seal (Phase 1a; the rule "every `user.*` key names a script and vice versa" is `Problem::WitnessKeys`). This phase adds (a) `physure_core::frozen::witness`: the result type, the `[Witness]` value format, the built-in battery (core APIs only, no script dependency), the comparison and its report; (b) the CLI glue that evaluates user `.phs` scripts with the machinery of `phs eval` (`physure-script` is above the core, so the core takes the evaluator as an argument and never depends on it); (c) the commands: `freeze --witness`, `verify --run`, the witness-drift rule of `update`/`seal`, `status`.

**Tech Stack:** Rust. No new dependencies (SHA-256 is the core's own; `serde_json` is already in the CLI).

**Spec:** `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` §8 (witnesses), §4 (`[Witness]`, scripts), §5.4, §9, §13 (Phase 3 owners: 1a-3 "the value of a `user.<name>` key is tied to nothing until witnesses are evaluated"). Read them first. Process rules: tests first; English; roadmaps (`ROADMAP.md`, `docs/ROADMAP.md`), `CHANGELOG.md` (CRLF), `structure.md`, the guide `docs/conf_freeze.md` and spec §13 in the same branch; never push; docs link policy of `CLAUDE.md` (`mkdocs build --strict`). Scripted edits: UTF-8, CRLF-aware, backslashes via `chr(92)`.

---

## Decisions fixed by this plan

1. **What a witness records.**
   - *Built-in* `builtin.<id>` (id: lowercase `[a-z0-9_.-]`, stable forever once released): value `<mean_bits>:<u_bits>:<unit>` — the IEEE-754 bit patterns of the mean and of the standard uncertainty as 16 lowercase hex digits each, and the unit text the interpreter prints (may be empty). A case with no uncertainty stores `u_bits = 0000000000000000`. A case whose result is not a number (a balancing, a boolean) stores its canonical text in the unit position as `text=<single line>` and zero bits; ids say so.
   - *User* `user.<name>`: value `sha256:<64 hex>`: the SHA-256 of the canonical result (decision 4). The script itself is the sealed `@@ witness <name>` block; `name` is the file stem restricted to the `check_name` alphabet (a stem that does not fit is refused with exit 2 naming the rule), unique.
2. **The battery is versioned by its ids.** A release may add cases; it may never change what an id computes. `verify --run` reports (a) recorded cases that differ, (b) recorded cases the running build no longer has (**missing**, a discrepancy), and (c) cases the running build has that the file does not record (**new**, informational: they come with a newer library and are recorded by `update --accept-witness-changes`).
3. **Evaluation context.** Witnesses always run under the configuration they are about to describe or that is in force: `freeze` runs them under the unfrozen catalog that the new file will contain (a frozen file cannot be in force at that moment, `freeze` refuses `AlreadyFrozen`); `verify --run [FILE]` runs under FILE (the CLI sets it as the explicit configuration for this single-shot process, resets the cache, and requires it to load: a file that does not verify exits 1, one for another engine exits 3 — evaluating under an engine the file does not pin proves nothing); `update`/`seal` previews run under the planned file (decision 6). Built-in cases obtain units, constants, atomic weights and settings only through the hooked catalog (`with_conf_registry` and the `try_*` forms), never through a registry they build themselves, so a changed override changes the witnesses that depend on it.
4. **User scripts** are evaluated like `phs eval` with seed `0` and the sandbox limits of `Limits::sandboxed`; the result is the `physure_script::report::Report`; a witness must evaluate with `ok == true` (otherwise `freeze` refuses it, exit 2, with the findings). Canonical result = the compact `serde_json::to_string(&report)` (finite `f64` prints in its shortest round-trip form, so it is bit-exact; non-finite values print as `null` and are a *recorded* result). The hash covers `results` and `diagnostics`.
5. **Tolerance: none.** Comparison is bit for bit (spec §8). A difference prints both bit patterns, both decimal values and the distance in ulps, so a person can tell a last-digit CPU difference from a changed formula. Environment differences stay warnings (§9), but `verify --run` makes the *consequence* visible.
6. **Drift rule for `update` and `seal`.** Both previews end with the witness comparison *recorded in the old file vs recomputed under the planned file*. If any witness differs (or is missing) the command refuses with **exit 2 and writes nothing** unless `--accept-witness-changes` is given; with it, the written file re-records **all** witnesses from the recomputation, the history `diff` line ends with `; witnesses: N changed, M new, K missing`, and the new seal is printed as usual. If nothing differs, nothing is re-recorded except cases that are *new* (recorded without needing the flag, and counted in the diff line). Evaluation under the planned file works by writing the planned file to a temporary sibling (`<name>.witness-tmp-<pid>`), setting it as the explicit configuration, evaluating, and removing it on every path; the planned file's engine is the running build, so it loads.
7. **No tool to add a user witness to an existing file** in this phase (YAGNI): witnesses are chosen at `freeze`. Existing scripts are kept by `update` and `seal`.
8. **`--allow-engine-mismatch` is removed** (it is accepted and does nothing; Phase 2 residual 4): `status`, `verify` and `diff` no longer accept it (exit 2 "unknown flag").
9. **Exit codes of the new paths:** `verify --run`: 0 every recorded witness matches; 1 a witness differs, is missing, errors, or the file does not verify; 3 the engine differs (nothing is evaluated); 2 misuse. `freeze --witness`: 2 for a script that does not read, evaluate cleanly or has an unusable name.
10. **Determinism rules for the battery:** no clock, no environment, no threads whose scheduling can reorder a floating-point reduction, a fixed seed wherever a pseudo-random generator is used; a case whose result could depend on the thread count (`parallel_threshold`) must pin it by running below it. Each case runs on its own, isolated by `catch_unwind`; a panic is that case's `Error`, never an abort.

## File structure

- Create `physure-core/src/frozen/witness.rs` — `WitnessValue`, `format_value`/`parse_value`, `Case`, `battery()`, `run_battery()`, `WitnessReport`, `compare()`, `ulp_distance`.
- Create `physure-core/src/frozen/battery/` (`units.rs`, `uncertainty.rs`, `chemistry.rs`, `mod.rs`) — the cases, one function each, grouped.
- Modify `physure-core/src/frozen/ops.rs` — `FreezeOptions.witnesses` (pre-evaluated inputs), `UpdatePlan`/`SealPlan` witness handling hooks.
- Modify `physure-core/src/frozen/mod.rs`; `physure-core/src/frozen/seal.rs` only if a new `Problem` is needed (it is not expected to be).
- Create `physure-cli/src/conf_witness.rs` — user-script evaluation, name rules, evaluation under a planned file, the drift preview.
- Modify `physure-cli/src/conf.rs` — flags and commands.
- Tests: `physure-core/tests/frozen_witness.rs`, `physure-core/tests/battery_reference.rs` (independent reference values), `physure-cli/tests/conf_witness.rs`.
- Docs: `docs/conf_freeze.md`, `docs/conf_coverage.md` is Phase 4's (do not create), ROADMAPs, CHANGELOG, structure.md, spec §13.

---

## Part A — core

### Task 1: the value, its format, the report

**Files:** Create `physure-core/src/frozen/witness.rs`; test in `physure-core/tests/frozen_witness.rs`.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessValue {
    /// mean and standard uncertainty as bit patterns, and the unit text.
    Number { mean: u64, uncertainty: u64, unit: String },
    /// A result that is not a number (a balancing): one line of text.
    Text(String),
    /// `sha256:<hex>` of a user script's canonical result.
    Hash(String),
}
pub fn format_value(v: &WitnessValue) -> String;           // the `[Witness]` value
pub fn parse_value(s: &str) -> Result<WitnessValue, String>; // strict: 16 lowercase hex digits, unit without ':'-ambiguity (split at the first two ':'), `text=` prefix, `sha256:` prefix

pub fn ulp_distance(a: f64, b: f64) -> Option<u64>;  // None when either is non-finite or the signs differ and neither is zero

#[derive(Debug, Clone, PartialEq)]
pub enum WitnessStatus { Match, Differs { recorded: String, running: String, ulps: Option<u64> }, Missing, New, Error(String) }
pub struct WitnessRow { pub key: String, pub status: WitnessStatus }
pub struct WitnessReport { pub rows: Vec<WitnessRow> }
impl WitnessReport { pub fn all_match(&self) -> bool /* Match and New only */; pub fn counts(&self) -> (usize /*differ*/, usize /*new*/, usize /*missing*/, usize /*error*/) }

/// recorded = the file's `[Witness]` pairs; running = freshly computed (key, Result<WitnessValue,String>).
pub fn compare(recorded: &Pairs, running: &[(String, Result<WitnessValue, String>)]) -> WitnessReport;
```
Rules: a recorded key whose value does not parse is a row `Error("recorded value does not read: ...")`; keys not starting with `builtin.` or `user.` are `Error`; ordering of rows: recorded order then new ones.

- [ ] **Step 1: tests** (`frozen_witness.rs`): `format_then_parse_roundtrips_for_every_variant`; `parse_refuses_uppercase_hex_short_hex_and_missing_parts`; `a_unit_with_a_colon_survives` (`m:s` unit text); `-0.0_and_nan_bit_patterns_are_preserved` (NaN payload, -0.0 differ from 0.0 by bits); `ulp_distance_of_adjacent_floats_is_1`, `across_zero`, `non_finite_is_none`; `compare_reports_match_differs_missing_new_and_error`; `differs_carries_both_bit_patterns_and_ulps`.
- [ ] **Steps 2–5** (fail, implement, pass). Commit `feat(core): witness values, their [Witness] format and the comparison report (conf freeze Phase 3)`.

### Task 2: the battery framework and the units/constants cases

**Files:** Create `physure-core/src/frozen/battery/{mod,units}.rs`; test `physure-core/tests/battery_reference.rs`.

```rust
pub struct Case { pub id: &'static str, pub area: &'static str, pub run: fn() -> Result<WitnessValue, String> }
pub fn battery() -> Vec<Case>;                         // all cases, stable order
pub fn run_battery() -> Vec<(String, Result<WitnessValue, String>)>;   // key "builtin.<id>", each case under catch_unwind
```
Cases (ids and what each computes; the implementer writes the closures with core APIs through the hooks): `units.foot_to_metre`, `units.kmh_to_ms`, `units.degc_to_k_25`, `units.degf_to_degc_212`, `units.db_20_to_ratio`, `units.psi_to_pa`, `units.kwh_to_j`, `units.ev_to_j`, `units.atm_to_kpa`, `units.cal_th_to_j`, `units.knot_to_ms`, `units.ph_7_to_ratio` (only if pH exists in the catalog; otherwise another log unit), and the catalog constants `const.molar_gas_constant`, `const.boltzmann`, `const.planck`, `const.avogadro`, `const.speed_of_light`, `const.elementary_charge` (read through the hooked catalog, so an override shows).

- [ ] **Step 1: tests in `battery_reference.rs`:** for every units case an **independent reference** written in the test as a literal from a primary source (NIST SP 811 / SI Brochure / CODATA 2022 as cited in the catalog), compared with an explicit tolerance (≤ 4 ulps, or exact where the factor is exact in the SI definition); `battery_ids_are_unique_stable_lowercase_and_every_case_runs`; `the_battery_is_deterministic_run_twice_bit_for_bit`; `a_panicking_case_is_an_error_row_not_an_abort` (inject through a test-only case list); `changing_an_override_changes_the_dependent_witness` (activate a frozen file whose overrides redefine `molar_gas_constant` and see `const.molar_gas_constant` change — use the activation pattern of `tests/frozen_equivalence.rs`).
- [ ] **Steps 2–5.** Commit `feat(core): the witness battery framework and the units and constants cases`.

### Task 3: uncertainty and statistics cases

**Files:** `physure-core/src/frozen/battery/uncertainty.rs`; tests.

Ids: `unc.sum_correlated`, `unc.sum_uncorrelated`, `unc.product_first_order`, `unc.quotient_first_order`, `unc.power_first_order`, `unc.montecarlo_seed_1` (fixed seed; below the parallel threshold; document the generator), `unc.unscented_product`, `unc.gum_h1_end_gauge` (GUM H.1: combined standard uncertainty and effective degrees of freedom of the end-gauge calibration, from the GUM's own inputs), `unc.welch_satterthwaite_example` (GUM G.4 example), `unc.expanded_k_p95`, `stat.t_p95_nu1`, `…nu2`, `…nu5`, `…nu10`, `…nu30`, `…nu100`, `stat.t_p99_nu10`, `stat.t_p99_nu100` (`math::stats::student_t_two_sided`).

- [ ] **Step 1: tests:** references from the literature, independent of the engine: Student-t two-sided quantiles from a published table (t₀.₉₇₅: ν=1 12.7062, 2 4.3027, 5 2.5706, 10 2.2281, 30 2.0423, 100 1.9840; t₀.₉₉₅ ν=10 3.1693, 100 2.6259; tolerance 5e-5 relative as the table has 4–6 digits) — each case's mean within that tolerance, **and** the bit pattern is only what the engine says; GUM H.1: combined standard uncertainty 32 nm and ν_eff ≈ 16 (GUM H.1.6) within the GUM's rounding; Welch–Satterthwaite per GUM G.4.1 example; first-order propagation cases against hand-derived closed forms (sum: √(u₁²+u₂²) uncorrelated, u₁+u₂ fully correlated, etc.) to ≤ 2 ulps; Monte Carlo and unscented within their stated statistical/analytical accuracy of the first-order value (assert the tolerance the implementation documents, and that the *same seed gives the same bits*).
- [ ] **Steps 2–5.** Commit `feat(core): the uncertainty and Student-t witness cases`.

### Task 4: chemistry cases

**Files:** `physure-core/src/frozen/battery/chemistry.rs`; tests.

Ids: `chem.molar_mass_h2o`, `chem.molar_mass_glucose`, `chem.molar_mass_calcium_hydroxide`, `chem.molar_mass_iron_sulfate`, `chem.mass_to_moles_nacl`, `chem.balance_propane_combustion` (`Text`), `chem.balance_iron_oxide_reduction` (`Text`), `chem.arrhenius`, `chem.clausius_clapeyron_water`, `chem.gibbs_298`. Atomic weights come through the hooked registry (so the selected edition and the user's definitions show).

- [ ] **Step 1: tests:** molar masses against the CIAAW/IUPAC values with the standard weights of the selected edition computed **by hand in the test** (sum of integer counts × the weights in `tests/golden/ciaaw_*.tsv`), balancing against the textbook coefficients (C₃H₈ + 5 O₂ → 3 CO₂ + 4 H₂O; Fe₂O₃ + 3 CO → 2 Fe + 3 CO₂), Arrhenius/Clausius–Clapeyron/Gibbs against closed forms evaluated in the test with the catalog's constants.
- [ ] **Steps 2–5.** Commit `feat(core): the chemistry witness cases`. After this task `battery().len()` is between 38 and 46: assert `>= 38` and document the final count in the guide.

### Task 5: `freeze` records witnesses; `verify` knows the rule

**Files:** modify `physure-core/src/frozen/ops.rs`; tests in `physure-core/tests/frozen_witness.rs`.

```rust
pub struct WitnessInput { pub name: String, pub text: String, pub canonical_sha256: String }   // user script + the hash of its result (the caller evaluated it)
// FreezeOptions gains: pub witnesses: &'a [WitnessInput], pub battery: Vec<(String, Result<WitnessValue,String>)>  // already computed under the unfrozen catalog
```
`freeze` refuses (`OpsError::Refused`) when any battery row is an `Error` (naming the case) or two witnesses share a name; otherwise `[Witness]` = every battery row as `builtin.<id>` followed by every user witness as `user.<name>`, `witness_scripts` = the scripts in the order given. `UpdatePlan`/`SealPlan` gain nothing here (Task 9 does).

- [ ] **Step 1: tests:** a frozen file made with a battery and two user witnesses verifies; its `[Witness]` keys are `builtin.*` then `user.*`; a battery with an `Error` row is refused naming the case; a duplicate user name is refused; deleting a `user.*` key or a script is `Problem::WitnessKeys`; editing a `builtin.*` value breaks the seal; `witnesses` empty (no battery) still freezes (the core is policy-free; the CLI always passes the battery).
- [ ] **Steps 2–5.** Commit `feat(core): freeze records the witness battery and user witnesses`.

---

## Part B — the commands

### Task 6: user scripts, names, `freeze --witness`

**Files:** create `physure-cli/src/conf_witness.rs`; modify `conf.rs`; tests `physure-cli/tests/conf_witness.rs`.

```rust
pub fn witness_name(path: &Path) -> Result<String, String>;           // file stem in the check_name alphabet, else Err naming the rule
pub fn evaluate_user_witness(text: &str) -> Result<String, String>;  // seed 0, Limits::sandboxed, report.ok required; Ok(sha256 hex of the compact JSON of the Report)
```
`freeze --witness FILE.phs` (repeatable): reads each (UTF-8, ≤ 1 MiB, else exit 2), evaluates under the **unfrozen** catalog, passes `WitnessInput`s plus `run_battery()` to `freeze`. Missing file / not evaluable / bad name / duplicate: exit 2 before anything is written.

- [ ] **Step 1: tests (binary):** `freeze_embeds_the_battery_and_a_user_witness_and_verify_passes`; `the_user_script_is_byte_exact_in_the_file`; `a_script_with_an_error_is_refused_exit_2_with_the_findings`; `a_stem_that_does_not_fit_is_refused`; `two_scripts_with_the_same_stem_are_refused`; `freeze_without_witness_still_records_the_battery`; `status_shows_the_witness_counts`.
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf freeze --witness`.

### Task 7: `verify --run`

**Files:** `conf.rs`, `conf_witness.rs`; tests.

`verify [FILE] --run`: after the usual verification, if the file does not verify → exit 1 (nothing run); engine differs → exit 3 (nothing run); else set the file as explicit configuration (a relative FILE is made absolute first), reset the cache, `ensure_valid()`, run `run_battery()` and every user witness script embedded in the file (evaluated like `freeze` does), `witness::compare`, print rows grouped (differs first, with both bit patterns, decimals and ulps; missing; new; error; then the count of matches) and exit 0/1 per decision 9. JSON adds `"witnesses": {"checked", "match", "differs": [...], "missing": [...], "new": [...], "errors": [...]}` and `"witnesses_ok"`.

- [ ] **Step 1: tests:** a fresh file → exit 0 and `match` equals the number recorded; edit a recorded `builtin.*` value **and re-seal in the test** (core API) → exit 1 naming the case with both bit patterns; a recorded `user.*` hash changed and re-sealed → exit 1; a recorded case id this build does not have (add a `builtin.zz_removed` pair, re-seal) → `missing`, exit 1; a case the file lacks (remove one pair, re-seal) → `new`, exit 0 and listed; `verify --run` on a file for another engine → exit 3, nothing evaluated (assert the output says so); `--run` on a tampered file → exit 1; JSON shape; `verify --run FILE` of a file not in force evaluates under FILE (override a constant in FILE's overrides, freeze it elsewhere with that override, run from another directory: the `const.*` witness matches).
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf verify --run`.

### Task 8: remove `--allow-engine-mismatch`

- [ ] **Step 1:** test that `status`, `verify`, `diff` reject `--allow-engine-mismatch` with exit 2 and the accepted flags; **Step 2–5:** remove it from `shape()`, `ConfArgs`, HELP, tests that used it, the guide, spec §5.4 and §13 (Phase 2 residual 4 closed). Commit `refactor(cli): remove --allow-engine-mismatch, which never changed anything`.

### Task 9: `update` and `seal` witness drift

**Files:** `conf_witness.rs`, `conf.rs`; tests.

After `plan_update`/`plan_seal` succeed and the usual preview is built, evaluate the witnesses under the planned file (decision 6: temporary sibling, explicit configuration, always removed — a `Drop` guard), `compare(plan.old.witnesses(), running)`, and print the witness section of the preview. If `!all_match` (any differ or missing) and `--accept-witness-changes` is absent: exit 2, message names the first differing witnesses and the flag, nothing written. With the flag (or with only *new* cases): rewrite `[Witness]` from the recomputation (`file.set_witnesses`, user scripts kept), append the counts to the history diff line, `reseal` — implemented in core as `ops::rewitness(plan: &mut UpdatePlan/SealPlan, results, user_results)` so the history entry is created exactly once (the plan's own `reseal` already ran: restructure so the witness re-recording happens **before** the single `reseal`; the plan functions take the recomputed witnesses as an optional input `Option<&[(String, Result<WitnessValue,String>)]>`).

- [ ] **Step 1: tests:** (core) a plan made with recomputed witnesses that differ carries them and the history diff ends with `witnesses: 1 changed, 0 new, 0 missing`; (binary, using a file whose `[Engine]` is doctored to 0.0.1 and re-sealed as in Phase 2 tests so `update` has something to do) `update` with a doctored recorded witness value and no flag → exit 2, file bytes unchanged, output names the witness; the same with `--accept-witness-changes` → exit 0, `verify --run` exit 0 afterwards, `history` shows the counts; `seal` after editing a source-2 constant that a `const.*` witness depends on: without the flag exit 2, with it exit 0; `update` that changes no witness needs no flag; the temporary file is never left behind (assert the directory listing) even when the command fails or is declined; `--json` without `--yes` carries the witness section.
- [ ] **Steps 2–5.** Commit `feat(cli): update and seal refuse silent witness changes`.

### Task 10: docs

- [ ] **Step 1:** `docs/conf_freeze.md`: witnesses chapter (what is recorded and how, the battery list by area with the final count, user witnesses, `verify --run` output and exit codes, the drift rule and `--accept-witness-changes`, that comparison is bit for bit and what an ulp distance means, that environment differences are warnings and witnesses are how their consequence is seen, the determinism rules, what witnesses do **not** prove (they sample behaviour; they are not a proof of correctness of the rest of the library)); remove `--allow-engine-mismatch` mentions; update JSON schemas. Do not create `docs/conf_coverage.md` (Phase 4). Links per the docs link policy. Run `cd physure-python && uv run mkdocs build --strict`.
- [ ] **Step 2:** `ROADMAP.md`, `docs/ROADMAP.md` (Phase 3 done, Phase 4 next, row 9 percentage), `CHANGELOG.md` (one Added entry, once, CRLF), `structure.md` (witness module, battery, `conf_witness.rs`), spec §13 (close 1a-3 and Phase 2 residual 4; add new residuals found while implementing, with owners: e.g. battery growth policy, ulp differences across CPUs expected and what to do, user scripts that read files or the clock are refused by the sandbox — verify that statement against `Limits::sandboxed` before writing it).
- [ ] **Step 3:** full verification: `cargo test -p physure -p physure-cli -p physure-script -p physure-lsp`, `cargo check --workspace`, `cargo clippy -p physure -p physure-cli --all-targets` (nothing new in touched files), mkdocs strict. Commit `docs: witnesses — guide, roadmaps, changelog, spec (conf freeze Phase 3)`.

---

## Self-review against the spec

| Spec item | Task |
|---|---|
| Built-in battery (~40): conversions incl. affine and logarithmic, uncertainty per mode with fixed seed, Welch–Satterthwaite (GUM), Student-t at several ν, molar masses, balancing, Arrhenius, gas constant from the catalog; bit patterns + unit text | 2, 3, 4 (+ format in 1) |
| User witnesses: `freeze --witness`, embedded script, evaluated like `phs eval`, canonical result hashed, `verify --run` re-evaluates the embedded text | 5, 6, 7 |
| `verify --run` re-executes the witnesses under the frozen configuration | 7 |
| Witnesses detect what an engine pin cannot (CPU / C library last digits) | decision 5, task 7 |
| Residual 1a-3 (`user.<name>` value tied to nothing) | 5, 7 |
| Phase 2 residual 4 (`--allow-engine-mismatch`) | 8 |
| An update must not change validated results silently (the point of the whole feature) | 9 |

Left for later: the coverage table and code-resident numbers (Phase 4); bindings honouring frozen files and running witnesses from Python (Phase 5); adding a user witness to an existing file (not needed yet).
