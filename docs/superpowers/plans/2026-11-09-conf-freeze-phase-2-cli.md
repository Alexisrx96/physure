# Configuration freeze, Phase 2: the `phs conf` commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give people the eight commands of the design (`phs conf`/`status`, `show`, `freeze`, `verify`, `diff`, `update`, `seal`, `history`) over the core API of Phases 1a–1d, so an audited site can freeze, inspect, compare and re-seal a configuration without writing Rust.

**Architecture:** Everything that *decides* lives in the core as pure, tested functions (`physure_core::frozen::report`: `inspect`, `compare`, `history_rows`, `entries_with_origin`); the CLI (`physure-cli/src/conf.rs`) only parses arguments, calls the core, renders text or JSON, asks for confirmation and sets the exit code. Bindings (Phase 5) reuse the same report types. Witnesses are Phase 3: `--witness` and `--run` are not accepted yet.

**Tech Stack:** Rust. `physure-core` stays free of new dependencies (reports are plain structs); `physure-cli` already has `serde_json`, which renders `--json`.

**Spec:** `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` §5.4 (commands), §5.3 (exit codes), §13 (residual risks owned by Phase 2). Read them first. Process rules of this project: tests first; English everywhere; roadmaps (`ROADMAP.md`, `docs/ROADMAP.md`), `CHANGELOG.md` and `structure.md` updated in the same branch; never push; docs under `docs/` follow the docs link policy of `CLAUDE.md`.

---

## Corrections from the adversarial review of this plan (they OVERRIDE anything below that disagrees)

The review (measured and read against the code) found these. Each is a requirement.

**C1 — dispatch first (measured: `phs conf freeze --reason help` printed help and exited 0).** `main()` runs `run_flag_command` before `run_subcommand`, and that function matches `help`, `doc`, `repl`, `daemon`, `-V`, `-d`... anywhere in the arguments. In `main`, *before* `refuse_an_unusable_configuration` and `run_flag_command`: `if args.get(1).map(String::as_str) == Some("conf") { conf::run_conf(&args) }` (never returns). `conf.rs` handles its own `--help`. Tests: `conf freeze --reason help`, `conf show doc`, `conf repl`, `conf daemon`, `conf -V` each do the `conf` action or exit 2 and never run another command. Every other command keeps the pre-command check.

**C2 — `update` shows what it writes.** `plan_update` keeps the overrides, so the prompt must not show the working directory's `physure.conf`. `UpdatePlan` and `SealPlan` gain `old: FrozenFile` (the file as parsed and verified) and `stamp: FileStamp` (from the same read). The difference shown by `update` is `diff(entries(replay_file(&plan.old)), entries(replay_file(&plan.file)))` plus the differing `[Engine]` pairs, computed from memory only: nothing is read twice.

**C3 — what `seal` shows.** The file keeps only the old source-2 marker, so a before/after of the overrides is impossible from the file alone. `seal` shows (a) the full canonical text of the new source 2, and (b) the entry difference between the catalog of sources 1..n without source 2 and with it (what the overrides contribute now). The spec §5.4 wording ("the diff against the last sealed state") is amended to say this. An optional `--against COPY` is not built (YAGNI; a copy of the previously sealed file can be given to `phs conf diff`).

**C4 — `verify --expect`.** `Inspection::seal_matches(expected)` is true only when `problems.is_empty()` **and** the stored seal equals `expected` (case-insensitive). An `--expect` that is not 64 hex characters is misuse (exit 2), a mismatch is exit 1. Test: edit source 2, `--expect <original seal>` gives `"seal_matches": false` and exit 1.

**C5 — `history`** prints the verification problems first and exits with `inspect().exit_code()` (not 0 for "parses"); rows after the first broken link are marked `unverified`.

**C6 — residual risks Phase 2 owns, decided:** 1c-14 is **built**: `UpdatePlan`/`SealPlan` carry the stamp; before `write_atomic` the CLI re-stats the file and refuses with exit 1 ("changed since it was read; nothing written") if length or modification time moved. 1c-15 is closed by reasoning (a single-shot `phs` re-reads on every run; the language server is Phase 5). 1b-3, 1b-4, 1b-5, 1c-2, 1c-10 are documentation items: each is listed by name in Task 10 and closed in §13. 1b-10 (CLI identity) is reassigned to Phase 5 and 1b-11 is decided: the build script covers physure-core only and says so (§7), no change. 1b-12 is only **partially** closed: `engine_block_changed` says that the block changed, not which field (the log keeps only `engine_sha256`).

**C7 — the flag is `--engine-policy`** (the core's refusal text and spec §7 name it), not `--policy`; `update` accepts it too.

**C8 — one replay.** The per-source origins are computed inside `load.rs` (`replay_file_with(file, |index, registry, constants, collected| ...)`, `replay_file` calls it with a no-op), so the trust rule (source 1 trusted only as `embedded`) and the missing-edition check stay in one place. `origin.rs` does not call `replay_source` itself.

**C9 — no replay of a file that does not verify.** `inspect` replays only when `problems` is empty; otherwise `catalog = None` ("not replayed: the file does not verify"). `compare` carries each file side's problems; `diff` exits 1 when a file side does not verify (never "identical").

**C10 — names:** `read_bounded` and `replay_file` are in `load.rs` (`pub(crate)`). The binary hash closure is `|| engine_file_hash(engine_file)` (load.rs; cached; gives the "host did not say" error for `None`), not `sha256_of_file(Option)`.

**Decisions for the underspecified points:**
- **S1** The Unfrozen side's engine is compared field by field from `EngineIdentity::current()` (version, commit, profile, target, features, rustc, flags); `policy` and `binary_sha256` are left out of that side (`engine_pairs` fails for `build` on a build with an unknown commit).
- **S2** Exit codes per case: `OpsError::{AlreadyFrozen, WouldOverwrite, Refused, Engine, Resolve}` = 2; `OpsError::{Conf, Load, Io, Frozen}` = 1; `verify FILE` of a missing file = 1; `--conf FILE` that does not exist = 2 (resolution).
- **S3** `--allow-engine-mismatch` (accepted by `status`, `verify` and `diff`, as the spec says) only lets a different build *inspect*; it never changes the exit code (3 stays 3) and the JSON always carries `"engine_matches"`.
- **S4** `update` and `freeze` always pass `engine_file = current_exe`.
- **S5** Unfrozen `show` and `diff` read `physure.conf` through `try_build_registry_from_conf()` (what the runtime does) and print a warning when the strict reading of it would differ or fail. `report::unfrozen_entries(cwd) -> Result<(Vec<Entry>, Vec<String>), String>` (entries, warnings) is part of Task 3's API.
- **S6** `show` calls `set_engine_file(current_exe)` and then `ensure_valid()`.
- **S7** `--conf` with `freeze` is misuse (exit 2): `freeze` writes to `--out`.
- **S8** `--origin` shows `embedded (restated by overrides)` when a later source sets the same value.
- **S9** The confirmation prompt goes to stderr, so `--json` output on stdout stays clean.
- **S10** A failed replace (Windows sharing violation) is an I/O error, exit 1, nothing written. JSON paths use `to_string_lossy`.
- **S11** Tests that depend on the build naming a commit branch on `EngineIdentity::current().commit == UNKNOWN`.
- **S12** The guide says that after `freeze -> edit source 2 -> seal`, the working directory's `physure.conf` is stale and the default `diff` reports drift until it is updated.

---

## Decisions fixed by this plan

1. **Exit codes (all `phs conf` commands):** `0` ok (identical, for `diff`); `1` a discrepancy (broken seal, modified file, differences, `--expect` mismatch, a catalog that does not replay); `2` misuse, a required frozen file that is missing, a refused operation (`AlreadyFrozen`, `WouldOverwrite`, `Refused`, a declined confirmation, no terminal and no `--yes`); `3` the engine differs from the one the file pins. When several apply the most specific wins in the order 3 (only when the *only* discrepancy is the engine), then 1, then 2 for misuse (misuse is decided before any file is read).
2. **`phs conf` is exempt from the pre-command `ensure_valid()`** in `physure-cli/src/main.rs` (`refuse_an_unusable_configuration`): `status`, `verify`, `diff`, `history` must work on a broken file, `update` must work on a file of another engine, `seal` on a modified file, `freeze` on none. Each command loads with `LoadOptions { check_engine: false }` or reads the file itself and reports. `show` is the exception: it shows the configuration *in force*, so it calls `ensure_valid()` itself first. Every other `phs` command keeps the check.
3. **Flags common to all subcommands:** `--conf PATH` (calls `set_explicit_conf(Some(path))` before anything resolves) and `--json`. Unknown flags and missing values are misuse (exit 2, naming the accepted flags).
4. **`--json` has a stable schema:** every document has `"schema": 1` and `"command": "<name>"`; fields are only ever added. Text output is for people and may change.
5. **Reasons are mandatory:** `freeze`, `update` and `seal` require `--reason "..."` (non-empty after trimming). The actor is `actor_from_env`.
6. **`freeze` policy:** `--policy binary|build|release|record`, default `build` (the spec). A build that does not name its commit (see §7: `PHYSURE_BUILD_COMMIT`) cannot record `build` or `binary`; `freezable` refuses with its message and the command says `--policy release` or `record` is available. For `binary` the engine file is the running `phs` executable.
7. **Confirmation (`update`, `seal`):** show the difference, then ask `Proceed? [y/N]` on stdin. Without `--yes`, if stdin is not a terminal (`std::io::IsTerminal`) the command refuses with exit 2 instead of reading piped input; with `--yes` it never asks. Declining writes nothing (exit 2).
8. **Not here:** witnesses (Phase 3), the bindings (Phase 5), `verify` of an environment beyond reporting differences as warnings.

## File structure

- Create `physure-core/src/frozen/report.rs` — `inspect`, `Inspection`, `EngineState`, `FieldChange`, `compare`, `Side`, `Comparison`, `history_rows`, `HistoryRow`; all pure over the file model and the entries of Phase 1d.
- Create `physure-core/src/frozen/origin.rs` — `entries_with_origin`, `section_and_key`, `select`.
- Modify `physure-core/src/frozen/mod.rs` — declare and re-export the two modules.
- Create `physure-cli/src/conf.rs` — argument parsing (`ConfArgs`, `parse`), one function per command returning a `Outcome { text: String, json: serde_json::Value, code: i32 }`, `run_conf(args)`.
- Modify `physure-cli/src/main.rs` — `mod conf;`, the `"conf"` arm in `run_subcommand`, the exemption in `refuse_an_unusable_configuration`, help text.
- Modify `physure-cli/src/upgrade.rs` — suggest `phs conf diff` when a frozen file is in force.
- Create `physure-core/tests/frozen_report.rs` — integration tests of the report layer on real files.
- Create `physure-cli/tests/conf_commands.rs` — the real `phs` binary in a temporary directory.
- Create `docs/conf_freeze.md` (+ nav entry in `physure-python/mkdocs.yml`) — the user guide.
- Modify `ROADMAP.md`, `docs/ROADMAP.md`, `CHANGELOG.md` (CRLF), `structure.md`, spec §13/§5.4 notes.

Encoding rule for every scripted edit in this repository: files here are UTF-8 with CRLF line endings in many places; use the Edit tool, or Python with `encoding='utf-8'` and `newline=''`, and write backslashes with `chr(92)` — an earlier phase corrupted a spec with cp1252 and with swallowed escapes.

---

## Part A — the core report layer

### Task 1: `inspect` — what is wrong with a file, without refusing to look

**Files:** Create `physure-core/src/frozen/report.rs`; modify `physure-core/src/frozen/mod.rs`; test in `physure-core/tests/frozen_report.rs`.

The one function every command shares. It never fails: every problem becomes data.

```rust
/// How the file's engine pin compares with the running build.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// The engine block could not be read (`read_engine` failed): the message.
    Unreadable(String),
    /// The running build satisfies the pin under the file's policy.
    Matches { policy: EnginePolicy, pinned: String },
    /// It does not: the policy and every difference (`Difference` rendered as text), in the order `check` gives them.
    Differs { policy: EnginePolicy, differences: Vec<String> },
}

/// One field that differs between two sides, or between a file and the running machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange { pub key: String, pub old: String, pub new: String }

#[derive(Debug, Clone)]
pub struct Inspection {
    pub path: PathBuf,
    /// The text could not be read or parsed (size limit, I/O, not a frozen file): the message. Everything below is
    /// then empty.
    pub unreadable: Option<String>,
    pub file: Option<FrozenFile>,
    /// `FrozenFile::verify()` rendered one problem per string. Empty: the seal and the chain hold.
    pub problems: Vec<String>,
    pub engine: Option<EngineState>,
    /// `[Environment]` of the file against `environment_pairs()` of this machine: informational (outside the seal).
    pub environment: Vec<FieldChange>,
    /// The sources replayed strictly: the entries of the catalog, or the problems (`origin: problem`).
    pub catalog: Option<Result<Vec<Entry>, Vec<String>>>,
    /// `atomic_weight_edition()` of the replayed registry.
    pub atomic_weight_edition: Option<String>,
}

pub fn inspect(path: &Path, engine_file: Option<&Path>) -> Inspection;

impl Inspection {
    /// The exit code decision 1 describes for *a file on its own*: 0 when sound; 3 when the only discrepancy is the
    /// engine; 1 otherwise (unreadable, a problem, an unreadable engine block, a catalog that does not replay).
    pub fn exit_code(&self) -> i32;
    /// True when the seal of the file equals `expected` (case-insensitive hex, 64 characters); false otherwise.
    pub fn seal_matches(&self, expected: &str) -> bool;
}
```

Implementation notes (the engineer has no other source for these):
- Read with `ops::read_bounded` (make it `pub(crate)` if it is not) so the 64 MiB bound and the open-handle rule hold; parse with `FrozenFile::parse`.
- `engine` uses `read_engine(file.engine())` then `EngineBlock::check(&EngineIdentity::current(), || sha256_of_file(engine_file))`; when `engine_file` is `None` and the policy is `binary`, the hash closure returns an `io::Error` ("no engine file was given") — that is a `Differs` entry, never a panic.
- `environment` compares `file.environment()` pairs with `environment_pairs()`; a key present on one side only is a change with the empty string on the other.
- `catalog` uses `load::replay_file`; `entries` from Phase 1d; `atomic_weight_edition` from the replayed registry.
- Verification problems are `Problem` values: render with `to_string()`.

- [ ] **Step 1: Write the failing tests** in `physure-core/tests/frozen_report.rs` (helpers: a temp dir, `freeze` + `write_atomic` from Phase 1c as in `tests/frozen_equivalence.rs`; policy `Release`):
  - `a_fresh_file_inspects_clean`: `problems` empty, `engine == Matches{..}`, `catalog` is `Ok`, `exit_code() == 0`, `atomic_weight_edition` starts with `standard-2024`.
  - `an_edited_value_is_a_problem_and_exit_1`: change `"= 8.31446261815324"` in the file text; `problems` non-empty, `exit_code() == 1`.
  - `an_engine_of_another_build_is_exit_3_only`: replace the `[Engine]` `version` with `0.0.1` **and re-seal in the test** with `FrozenFile::reseal`-style helpers from `tests/common` (see `frozen_active.rs` for how files for another engine are made); `engine == Differs`, `problems` empty, `exit_code() == 3`.
  - `a_missing_or_garbage_file_is_unreadable_and_exit_1`.
  - `environment_differences_are_listed_but_do_not_change_the_exit_code`: change `cpus` in `[Environment]` (outside the seal): `environment` has `cpus`, `exit_code() == 0`.
  - `seal_matches_is_case_insensitive_and_exact`: the file's own seal in upper case matches; one digit off does not; a 63-character string does not.
- [ ] **Step 2:** `cargo test -p physure --test frozen_report` — Expected: FAIL (`unresolved import`).
- [ ] **Step 3:** Implement `report.rs` as specified; export from `mod.rs`.
- [ ] **Step 4:** run the test — Expected: PASS. Also `cargo test -p physure --lib` stays green.
- [ ] **Step 5:** Commit: `feat(core): inspect — what is wrong with a frozen file, as data (conf freeze Phase 2)`.

### Task 2: entries with their origin, and `section.key` names

**Files:** Create `physure-core/src/frozen/origin.rs`; test in `physure-core/tests/frozen_report.rs`.

```rust
/// Every entry of the catalog the file's sources build, with the origin of the *last* source that set its value
/// (source 1's origin is `embedded`). Replays the sources cumulatively; an entry that a later source leaves
/// unchanged keeps the earlier origin.
pub fn entries_with_origin(file: &FrozenFile) -> Result<Vec<(Entry, String)>, (String, Vec<String>)>;

/// The `section.key` an entry is shown under: units and their kinds under `units`, `constant` under `constants`,
/// `atomic_weight*` under `atomic_weights`, `setting*` under `settings`, `category` under `categories`, each other
/// kind under its own name; the key is the entry's name (`edition/symbol` for tables).
pub fn section_and_key(entry: &Entry) -> (String, String);

/// Entries whose `section.key`, `section`, or key prefix equals `selector` (`""` selects all).
pub fn select<'a>(entries: &'a [(Entry, String)], selector: &str) -> Vec<&'a (Entry, String)>;
```
Rules: `selector` `constants` selects the section; `constants.molar_gas_constant` one key; anything else matching nothing is an empty list (the command turns that into exit 2 "no such section or key", listing the sections).

- [ ] **Step 1: tests** (in `frozen_report.rs`): freeze with overrides `[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n[Units]\nzz = 2.5, L` → `entries_with_origin` gives `molar_gas_constant` origin `overrides`, `kg` origin `embedded`, `zz` origin `overrides`; a third source `extra` that redefines `zz` gives origin `extra`; `section_and_key` of the constant entry is `("constants","molar_gas_constant")`; `select(.., "constants")` has every constant and nothing else; `select(.., "constants.molar_gas_constant")` has one; `select(.., "nonsense")` is empty.
- [ ] **Step 2:** run — FAIL. **Step 3:** implement (cumulative replay with `replay_source`, comparing `entries` after each source; use maps keyed by `(kind, name)`). **Step 4:** PASS. **Step 5:** commit `feat(core): the origin of every entry and section.key names`.

### Task 3: `compare` — the semantic diff of two sides, engine and environment included

**Files:** extend `report.rs`; tests in `frozen_report.rs`.

```rust
pub enum Side {
    /// A frozen file on disk (it need not verify; its sources are replayed as they are).
    File(PathBuf),
    /// What the running build gives *unfrozen* in `cwd`: the embedded catalog plus `cwd/physure.conf` if any, read
    /// strictly (a `physure.conf` that does not replay is an error naming its problems). Engine = the running build.
    Unfrozen { cwd: PathBuf },
}

pub struct Comparison {
    pub a: String, pub b: String,                    // labels: the path, or "the installed build, unfrozen"
    pub data: Vec<Change>,                           // Phase 1d diff, old = a, new = b
    pub engine: Vec<FieldChange>,                    // the [Engine] pairs that differ (key order of the file); empty when a Side is Unfrozen with the same build
    pub environment: Vec<FieldChange>,
}

pub fn compare(a: &Side, b: &Side, engine_file: Option<&Path>) -> Result<Comparison, String>;

impl Comparison { pub fn is_identical(&self) -> bool; /* data, engine and environment all empty */ }
```
The *default* the CLI uses is `a = File(frozen in force)`, `b = Unfrozen{cwd}`. Note for the CLI text: the data section of that default shows both a library upgrade **and** drift of the working directory's `physure.conf` from the frozen overrides; it says so.

- [ ] **Step 1: tests:** identical sides (a file against `Unfrozen` in a directory whose `physure.conf` equals the frozen overrides) → `is_identical()`; an edited `physure.conf` → `data` names the unit; a file frozen under a doctored embedded text (replace a constant's value in source 1 and re-seal) against `Unfrozen` → a `Changed` for that constant and `engine` empty; two files with different engine pairs → `engine` lists the keys; a `physure.conf` with a syntax error → `Err` that names the file and the problem.
- [ ] **Steps 2–5** as before. Commit `feat(core): compare — data, engine and environment differences of two sides`.

### Task 4: `history_rows`

**Files:** extend `report.rs`; tests.

```rust
pub struct HistoryRow {
    pub n: u32, pub time: String, pub action: String, pub actor: String, pub reason: String, pub engine: String,
    pub policy: String, pub pinned: String, pub diff: String,
    /// `engine_sha256` differs from the previous entry's (the first entry: false). Residual risk 12 of §13: a
    /// moved `binary_sha256` or a changed profile/target/features/rustc/flags shows only as this.
    pub engine_block_changed: bool,
    /// `policy` or `pinned` differs from the previous entry's, in words (`policy: release -> record`), else empty.
    pub pin_change: String,
}
pub fn history_rows(file: &FrozenFile) -> Vec<HistoryRow>;
```
- [ ] **Step 1: tests:** a file frozen then updated (use `plan_update` after doctoring the embedded text) has 2 rows; row 2 `action == "update"`; a policy change `release → record` sets `pin_change == "policy: release -> record"`; row 1 has `engine_block_changed == false`.
- [ ] **Steps 2–5.** Commit `feat(core): history rows with what changed in the engine block`.

---

## Part B — the commands

All CLI tests run the real binary (`env!("CARGO_BIN_EXE_phs")`) in a unique temporary directory with `PHYSURE_CONF` and `PHYSURE_REQUIRE_FROZEN` removed (copy the `Dir` helper of `physure-cli/tests/frozen_exit_codes.rs`). The test build names no commit, so tests freeze with `--policy release` unless the test is about the policy.

### Task 5: argument parsing, dispatch, the exemption, `status`, `history`

**Files:** Create `physure-cli/src/conf.rs`; modify `physure-cli/src/main.rs`; test `physure-cli/tests/conf_commands.rs`.

```rust
pub struct ConfArgs {
    pub command: String,            // "status" when none
    pub positional: Vec<String>,
    pub conf: Option<PathBuf>,      // --conf
    pub json: bool,
    pub reason: Option<String>,     // --reason
    pub policy: Option<EnginePolicy>,
    pub out: Option<PathBuf>,       // --out
    pub expect: Option<String>,     // --expect
    pub yes: bool, pub origin: bool, pub allow_engine_mismatch: bool,
}
pub fn parse(args: &[String]) -> Result<ConfArgs, String>;   // Err: the message, which names the accepted flags
pub struct Outcome { pub text: String, pub json: serde_json::Value, pub code: i32 }
pub fn run_conf(args: &[String]) -> !;   // prints text or json, exits with code
```
`status` (and bare `phs conf`): resolve with `resolve_process()`; if `Frozen`, `inspect(path, Some(current_exe))`; report: the file, why it is in force, frozen or not, seal, `created`, engine state, history length, atomic-weight edition, the verification problems, environment differences; **warnings** (spec §5.4): a `physure.conf` in the working directory that a frozen file ignores; a required frozen file that is missing (`ResolveError::Required` → exit 2, message from the error). Unfrozen: says "no frozen configuration in force" and, if `physure.conf` exists, that it is read as an overlay. Exit code = `Inspection::exit_code()` or 2 for a resolution error. `history`: `history_rows` as a table (text) or the array (json); exit as `status` for the file's soundness? No — `history` exits 0 when the file parses, 1 when unreadable.

- [ ] **Step 1: tests** (binary): `status_without_a_file_says_unfrozen_exit_0`; `status_of_a_sound_file_exit_0_prints_seal_and_edition`; `status_of_an_edited_file_exit_1_and_names_the_problem`; `status_of_a_file_for_another_engine_exit_3`; `status_with_require_frozen_and_no_file_exit_2` (`PHYSURE_REQUIRE_FROZEN=1`); `status_json_has_schema_1_and_command_status`; `a_broken_file_does_not_stop_conf_status_but_stops_other_commands` (`phs eval x` exits 1 with the broken file, `phs conf status` runs); `history_lists_entries_in_order`; `unknown_flag_is_exit_2_and_names_the_flags`.
- [ ] **Steps 2–5.** Implement `parse`, `run_conf`, `status`, `history`; in `main.rs` add `mod conf;`, `"conf" => conf::run_conf(args)` to `run_subcommand`, and skip `refuse_an_unusable_configuration()` when `args.get(1).map(String::as_str) == Some("conf")` (the command sets the explicit path and the engine file itself); add the lines to `print_help`. Commit `feat(cli): phs conf status and history (conf freeze Phase 2)`.

### Task 6: `freeze` and `verify`

**Files:** `physure-cli/src/conf.rs`; tests.

`freeze --reason R [--policy P] [--out PATH]`: build `FreezeOptions { cwd: current_dir, out, reason, policy (default Build), engine_file: Some(current_exe) when policy is Binary, time_unix: now_unix(), actor: actor_from_env(env) }`, call `freeze`, `write_atomic(&path, &text, false)`, print the path, the seal (**"record this seal outside this directory"**), the engine and the policy. Errors: `OpsError` messages verbatim, exit 2 (`AlreadyFrozen` says `phs conf update`). Missing `--reason` is exit 2 before anything is read.

`verify [FILE] [--expect SHA] [--allow-engine-mismatch]`: FILE defaults to the file in force (resolution). `inspect`; print seal, sources, problems, engine, environment warnings; with `--expect` print whether the seal equals it. Exit: `inspect.exit_code()`, but `--expect` mismatch is 1; `--allow-engine-mismatch` turns an engine-only discrepancy (3) into 0 with a printed warning.

- [ ] **Step 1: tests:** `freeze_with_no_physure_conf_writes_a_file_that_verifies` (exit 0; file exists; `phs conf verify` exit 0); `freeze_twice_is_exit_2_and_names_update`; `freeze_without_reason_is_exit_2`; `freeze_prints_the_seal_that_verify_expects` (parse the 64-hex from stdout, `verify --expect <seal>` exit 0, `--expect` of a changed digit exit 1); `freeze_build_policy_on_a_build_without_a_commit_is_exit_2_and_mentions_release` (skip when the test binary names a commit: `option_env!`-free — read `phs --version`? Instead set `PHYSURE_BUILD_COMMIT` is build-time; the test is valid because tests are built without it. Assert on message only if `verify` of a `build` freeze fails.); `freeze_with_overrides_keeps_physure_conf_as_source_2` (`physure.conf` with a constant; `show` later reads it); `verify_names_the_modified_value` (edit the text); `verify_json_schema`.
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf freeze and verify`.

### Task 7: `show`

**Files:** `physure-cli/src/conf.rs`; tests.

`show [SELECTOR] [--origin]`: calls `ensure_valid()` (decision 2; exit code from `ActiveError::exit_code()` on failure), takes the catalog in force (frozen `active()` or the unfrozen build via `embedded_entries`/local overlay — when unfrozen, entries come from `compare`'s unfrozen builder; expose it as `report::unfrozen_entries(cwd) -> Result<Vec<Entry>, String>` in Task 3 if not already public), prints `section.key = value` sorted, with `[origin]` after each when `--origin` (unfrozen: `embedded` or `physure.conf`). A selector that matches nothing is exit 2 and lists the sections. Values are the entry values of Phase 1d; document in the help that unit values include an internal `id`.

- [ ] **Step 1: tests:** `show_constants_lists_only_constants`; `show_one_key`; `show_origin_marks_overrides` (freeze with an override constant; `--origin` prints `overrides`); `show_unknown_selector_is_exit_2`; `show_json`; `show_with_a_broken_file_is_exit_1` (it uses the file in force).
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf show`.

### Task 8: `diff`

**Files:** `physure-cli/src/conf.rs`; tests.

`diff [A] [B]`: zero arguments: `a = File(in force)`, `b = Unfrozen{cwd}`; one argument: that file against `Unfrozen`; two: both files (an argument `unfrozen` stands for `Unfrozen{cwd}`). Print data differences grouped by kind (`+` added, `-` removed, `~` changed with `old -> new`), then engine differences, then environment, then counts and `summary()`; exit 0 identical, 1 different. With no frozen file in force and no arguments: exit 2 "nothing to compare". Text says that for the default the data section covers a library upgrade and a drifted `physure.conf`. JSON: `{"schema":1,"command":"diff","a":..,"b":..,"identical":bool,"data":[{"change":"added|removed|changed","kind":..,"name":..,"old":..,"new":..}],"engine":[{"key","old","new"}],"environment":[..],"summary":".."}`.

- [ ] **Step 1: tests:** `diff_of_a_fresh_freeze_is_identical_exit_0`; `diff_after_editing_physure_conf_exit_1_and_names_the_unit`; `diff_two_files`; `diff_without_a_frozen_file_is_exit_2`; `diff_json_schema_is_stable` (assert the keys of the first change); `diff_does_not_modify_anything` (the file's bytes before and after).
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf diff`.

### Task 9: `update` and `seal`

**Files:** `physure-cli/src/conf.rs`; tests.

`update --reason R [--policy P] [--yes]`: resolve the file in force (error exit 2 if none); `plan_update(path, reason, policy, engine_file, now, actor)`; `Ok(None)` → "nothing to update" exit 0; else print `compare(File(path), Unfrozen{cwd})`'s data/engine summary **and** `plan.summary`, confirm (decision 7), `write_atomic(path, &plan.file.render()?, true)`, print the new seal ("record it"). `seal --reason R [--yes]`: `plan_seal`; `Ok(None)` → "the file already verifies" exit 0; else print what changed (the overrides' entries diff: `compare` of the old sealed overrides vs the edited — use `entries_with_origin` of the plan's file against the last sealed catalog if available, otherwise `plan.summary`), confirm, write, print the seal. A refused plan (`OpsError::Refused`) is exit 2 with its message verbatim. Confirmation helper:

```rust
fn confirm(yes: bool, prompt: &str) -> Result<bool, String> {
    if yes { return Ok(true); }
    if !std::io::stdin().is_terminal() { return Err(format!("{prompt}: refusing to proceed without a terminal; pass --yes in scripts")); }
    // print the prompt, read one line, accept only "y" / "yes" (case-insensitive)
}
```

- [ ] **Step 1: tests:** `update_with_nothing_new_says_so_exit_0`; `update_after_a_doctored_embedded_text` — not reproducible with the real binary (the embedded text is the binary's), so cover the success path at the core level (Phase 1c tests do) and, in the binary test, only `update_without_yes_and_no_terminal_is_exit_2` on a file whose engine block is doctored to force a plan (re-sealed in the test, policy release, version `0.0.1`) — nothing is written (bytes equal); with `--yes` the file is updated, `verify` exit 0, `history` has 2 rows with `update`; `seal_after_editing_overrides`: freeze, append `[Constants]` line to source 2 of the file text, `verify` exit 1, `seal --reason r --yes` exit 0, `verify` exit 0, `history` shows `seal`; `seal_refuses_an_edit_of_source_1_exit_2`; `seal_on_a_sound_file_is_exit_0_nothing_to_do`; `update_and_seal_without_reason_exit_2`.
- [ ] **Steps 2–5.** Commit `feat(cli): phs conf update and seal`.

### Task 10: `phs upgrade` hint, help, user guide, roadmaps, changelog

- [ ] **Step 1:** In `physure-cli/src/upgrade.rs`, at the end of a successful upgrade: if `resolve_process()` is `Ok(Resolution::Frozen{..})`, print `A frozen configuration is in force; review what the new version changes with: phs conf diff`. The upgrade never touches the file. Test: unit test of the message function (the network part is not testable).
- [ ] **Step 2:** `docs/conf_freeze.md`: why (audited sites), the files, every command with an example and its exit codes, the JSON schema, the trust boundary and what is not pinned (copy from spec §7 and §13, do not invent), recording the seal outside the directory, the workflow `freeze → edit overrides → seal → upgrade → diff → update`. Add it to `physure-python/mkdocs.yml` nav under Guides. Links to source files use `https://github.com/Alexisrx96/physure/blob/main/...` only. Run `cd physure-python && uv run mkdocs build --strict` if `uv` is available; if it is not, say so in the commit message and the report.
- [ ] **Step 3:** `ROADMAP.md` + `docs/ROADMAP.md` (Phase 2 done; Phase 3 witnesses next; row 9 percentage), `CHANGELOG.md` (one `Added` entry; CRLF), `structure.md` (the new core modules and `conf.rs`), spec §13 (close the Phase 2 residuals that this phase closes: 1a-5 `verify --expect`, 1b-12 history shows block changes, 1c-4 exemption; list new ones).
- [ ] **Step 4:** full verification: `cargo test -p physure -p physure-cli -p physure-script -p physure-lsp`, `cargo check --workspace`, `cargo clippy -p physure-cli` shows nothing new in files this phase touched.
- [ ] **Step 5:** commit `docs: the phs conf user guide, roadmaps, changelog (conf freeze Phase 2)`.

---

## Self-review against the spec (§5.4)

| Spec item | Task |
|---|---|
| `status`: in force and why, frozen or not, seal and verification, creation date and engine, history count, atomic-weight edition, warnings (ignored `physure.conf`, missing required file, environment differences) | 1, 5 |
| `show [KEY]`, `--origin` | 2, 7 |
| `freeze --reason [--out] [--witness]`; no `physure.conf` still writes; refuses overwrite; prints seal and reminds | 6 (`--witness`: Phase 3) |
| `verify [FILE] [--expect SHA] [--run]`; environment differences | 1, 6 (`--run`: Phase 3) |
| `diff [A] [B]`, JSON stable schema, exit 0/1 | 3, 8 |
| `update --reason [--yes]`: shows diff, nothing if empty, confirmation, atomic, keeps overrides, history | 9 |
| `seal --reason [--yes]`: the only way to change overrides | 9 |
| `history` | 4, 5 |
| `phs upgrade` suggests `phs conf diff` | 10 |
| Exit codes 0/1/2/3 | decisions 1–2, every task |
| Residual risks owned by Phase 2: `verify --expect` (1a-5), history shows block change (1b-12), exempt conf commands (1c-4) | 1, 4, 5 |

Open points deliberately left for later phases: witnesses (Phase 3); code-resident numbers (Phase 4); Python/WASM/Java honouring the file (Phase 5); `plan_*` compare-and-swap (1c-14) — `update`/`seal` here read and write within one command, the window is the confirmation prompt; the plan notes it in the guide.
