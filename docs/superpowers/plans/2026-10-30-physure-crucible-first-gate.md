# Physure Crucible First Scientific Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the private `D:\Projects\physure-crucible` repository and deliver a reproducible black-box release gate that validates PHS and the packaged Rust API with an initial university-mechanics case set.

**Architecture:** Python 3.12 coordinates immutable candidate artifacts, clean subprocesses, per-case scientific references, JSON evidence, and release-gate exit codes. PHS scripts self-check through public language assertions, while isolated Rust consumer binaries emit machine-readable observations from the packaged `physure` crate; neither path imports the Physure workspace.

**Tech Stack:** Python 3.12, pytest, standard-library `dataclasses`/`decimal`/`fractions`/`hashlib`/`json`/`subprocess`/`tarfile`/`tomllib`, mpmath 1.x, Cargo/Rust 2021, TOML, GitHub Actions.

**Spec:** `D:\Projects\physure\docs\superpowers\specs\2026-10-30-external-scientific-validator-design.md`

## Global Constraints

- The new repository is private and lives at `D:\Projects\physure-crucible`; it is not a Physure workspace member.
- Validate only public PHS CLI behavior and public items from the packaged Rust crate `physure` (`physure_core` library name).
- The Rust candidate is the `.crate` produced by `cargo package -p physure`, never `D:\Projects\physure\physure-core` as a path dependency.
- PHS runs with its default configuration and no personal config; the default uncertainty mode under test is `correlated`.
- A `conformance` failure always returns a failing gate status; `experimental` findings are reported but never hide or create conformance failures.
- No `xfail`, retries, network access, automatically regenerated scientific snapshots, SciPy, custom test framework, or internal Physure fixture reuse.
- Numeric expectations and tolerances are decimal strings in TOML and are parsed with `Decimal`; no global tolerance exists.
- Every executed command has a finite timeout and preserves sanitized command, exit status, duration, stdout, and stderr.
- Current required release platforms are `linux-x86_64`, `linux-aarch64`, `macos-x86_64`, `macos-aarch64`, and `windows-x86_64`.
- Local commands use the repository's Python 3.12 `.venv` (`.venv\Scripts\python.exe` on Windows); CI obtains Python 3.12 through its setup action. The current shell has no global `python` command.

## Review Focus

- An artifact whose bytes do not match its declared SHA-256 must be rejected before any child process starts (Task 1 test).
- A malicious or malformed `.crate` archive must not extract outside the scratch directory (Task 3 test).
- A child that exceeds its timeout must be terminated and reported as a failure without hanging the suite (Task 1 test).
- A release missing even one required platform report must not aggregate to conforming (Task 8 test).
- A reference at or near zero must be judged by its absolute tolerance instead of accidentally receiving no usable tolerance (Task 4 test).

---

## File Map

All paths below are relative to `D:\Projects\physure-crucible`.

- `pyproject.toml` — Python floor, the `mpmath` runtime dependency, pytest development dependency, and `crucible` console entry point.
- `validator/candidate.py` — immutable candidate manifest types, TOML loading, path resolution, and SHA-256 verification.
- `validator/process.py` — sanitized environment and timeout-bounded subprocess execution.
- `validator/run_phs.py` — PHS invocation and mandatory public-contract/sentinel probes.
- `validator/run_rust.py` — safe `.crate` extraction, isolated consumer materialization, Cargo execution, and observation parsing.
- `validator/case.py` — scientific-case schema, decimal tolerance comparison, and PHS assertion rendering.
- `validator/report.py` — per-case evidence, JSON serialization, and gate status.
- `validator/gate.py`, `validator/__main__.py` — case discovery/orchestration and CLI.
- `validator/aggregate.py` — cross-platform report completeness and release decision.
- `probes/` — minimal PHS and Rust public-contract probes.
- `cases/conformance/mechanics/` — initial scientific cases and their derivations.
- `cases/conformance/mechanics/rust-consumer/` — one Cargo package with one binary per mechanics case.
- `references/sources.toml`, `references/mechanics.py` — versioned sources and independently evaluated mechanics references.
- `scripts/prepare_candidate.py` — build/package helper that creates a hashed candidate manifest without becoming part of validation.
- `tests/` — validator unit, integration, oracle, mutant, and aggregation tests.
- `.github/workflows/tests.yml` — validator self-tests on Windows, Linux, and macOS; actual Physure release-artifact handoff is a later integration plan.

### Task 1: Repository Bootstrap, Candidate Identity, and Clean Processes

**Files:**
- Create: `.gitignore`
- Create: `README.md`
- Create: `pyproject.toml`
- Create: `validator/__init__.py`
- Create: `validator/candidate.py`
- Create: `validator/process.py`
- Create: `tests/test_candidate.py`
- Create: `tests/test_process.py`

**Interfaces:**
- Produces: `Artifact(path: Path, sha256: str)`, `Candidate(version: str, commit: str, platform: str, required_platforms: tuple[str, ...], phs: Artifact, rust_crate: Artifact, cargo_home: Path)`, `load_candidate(path: Path) -> Candidate`, and `verify_artifact(artifact: Artifact) -> None`.
- Produces: `Execution(command: tuple[str, ...], returncode: int | None, stdout: str, stderr: str, duration_s: float, timed_out: bool)` and `run_process(command: Sequence[str], cwd: Path, timeout_s: float, extra_env: Mapping[str, str] | None = None) -> Execution`.
- Produces: `clean_environment(scratch: Path, inherited: Mapping[str, str]) -> dict[str, str]`; it keeps only platform toolchain essentials, removes arbitrary user variables, and points temp/config/cache locations at `scratch`.

- [ ] **Step 1: Initialize only the approved sibling repository**

Create `D:\Projects\physure-crucible`, run `git init`, and confirm `git rev-parse --show-toplevel` returns exactly that path before writing project files. Do not add it to the Physure workspace.

- [ ] **Step 2: Create project metadata and the local Python environment**

Create the metadata-only `pyproject.toml`, `.gitignore`, `README.md`, and empty `validator/__init__.py`. Set `requires-python = ">=3.12"`, runtime dependency `mpmath>=1.3,<2`, and development dependency `pytest>=8,<9`; create `.venv` from the installed CPython 3.12 distribution and install the project with its development dependencies. Do not add `candidate.py` or `process.py` yet.

```powershell
& 'C:\Users\Irvin\AppData\Roaming\uv\python\cpython-3.12-windows-x86_64-none\python.exe' -m venv .venv
.\.venv\Scripts\python.exe -m pip install -e '.[dev]'
```

- [ ] **Step 3: Write failing candidate and process tests**

Add tests named:

```python
def test_load_candidate_resolves_relative_artifact_paths(tmp_path): ...
def test_verify_artifact_rejects_wrong_sha256_before_execution(tmp_path): ...
def test_candidate_requires_all_five_unique_platforms(tmp_path): ...
def test_clean_environment_drops_unlisted_user_variable(tmp_path): ...
def test_run_process_captures_stdout_stderr_and_status(tmp_path): ...
def test_run_process_terminates_and_marks_timeout(tmp_path): ...
```

The manifest fixture must use `[phs] path/sha256`, `[rust] crate_path/sha256/cargo_home`, and top-level `version`, `commit`, `platform`, and `required_platforms`.

```toml
version = "0.3.0"
commit = "<full-git-sha>"
platform = "windows-x86_64"
required_platforms = ["linux-x86_64", "linux-aarch64", "macos-x86_64", "macos-aarch64", "windows-x86_64"]

[phs]
path = "artifacts/phs.exe"
sha256 = "<64-lowercase-hex>"

[rust]
crate_path = "artifacts/physure-0.3.0.crate"
sha256 = "<64-lowercase-hex>"
cargo_home = "cache/cargo-home"
```

- [ ] **Step 4: Run the new tests and verify the red state**

Run: `python -m pytest tests/test_candidate.py tests/test_process.py -q`  
Expected: collection/import failure because `validator.candidate` and `validator.process` do not exist.

- [ ] **Step 5: Implement candidate loading and artifact verification**

`verify_artifact` streams SHA-256 reads.

- [ ] **Step 6: Run the candidate tests**

Run: `python -m pytest tests/test_candidate.py -q`  
Expected: all candidate tests pass while process tests still fail because `validator.process` is missing.

- [ ] **Step 7: Implement sanitized, timeout-bounded process execution**

`run_process` uses text UTF-8 with replacement for invalid child bytes and kills a timed-out process before returning evidence.

- [ ] **Step 8: Verify Task 1**

Run: `python -m pytest tests/test_candidate.py tests/test_process.py -q`  
Expected: all Task 1 tests pass.

- [ ] **Step 9: Commit Task 1**

```bash
git add .gitignore README.md pyproject.toml validator tests/test_candidate.py tests/test_process.py
git commit -m "chore: bootstrap scientific validator"
```

### Task 2: PHS Black-Box Runner and Language Sentinels

**Files:**
- Create: `validator/run_phs.py`
- Create: `probes/phs/success.phs`
- Create: `probes/phs/assertion_must_fail.phs`
- Create: `probes/phs/dimension_error.phs`
- Create: `tests/test_run_phs.py`

**Interfaces:**
- Consumes: `Candidate`, `Execution`, `verify_artifact`, and `run_process` from Task 1.
- Produces: `run_phs(candidate: Candidate, script: Path, timeout_s: float = 10.0) -> Execution`.
- Produces: `probe_phs(candidate: Candidate) -> tuple[Execution, ...]`; success must exit zero, while `assert(false, "validator-sentinel")` and `1.0 m + 1.0 s` must both exit nonzero with a diagnostic and without `panicked at` or a backtrace.

- [ ] **Step 1: Write failing PHS runner tests**

```python
def test_run_phs_uses_only_binary_and_script_arguments(monkeypatch, candidate): ...
def test_run_phs_preserves_nonzero_status_and_diagnostic(monkeypatch, candidate): ...
def test_probe_rejects_an_always_green_fake_interpreter(monkeypatch, candidate): ...
def test_probe_rejects_panic_text_as_normal_diagnostic(monkeypatch, candidate): ...
```

Assert the successful command is exactly `(str(candidate.phs.path), str(script))`; no flag may change default configuration.

- [ ] **Step 2: Verify the PHS tests fail**

Run: `python -m pytest tests/test_run_phs.py -q`  
Expected: FAIL because `validator.run_phs` is missing.

- [ ] **Step 3: Implement the PHS process runner**

Implement `run_phs` with the exact command and timeout/evidence behavior above.

- [ ] **Step 4: Add the three public-contract probe scripts and probe evaluation**

`success.phs` proves quantity literals, compound units, conversion, arithmetic, comparison, and `assert` with `assert(1.0 km, 1000.0 m)`. The two negative probes establish that assertions and dimensional diagnostics cannot silently pass.

- [ ] **Step 5: Verify Task 2**

Run: `python -m pytest tests/test_run_phs.py -q`  
Expected: all Task 2 tests pass.

- [ ] **Step 6: Commit Task 2**

```bash
git add validator/run_phs.py probes/phs tests/test_run_phs.py
git commit -m "feat: validate PHS as a black box"
```

### Task 3: Packaged Rust Consumer Runner

**Files:**
- Create: `validator/run_rust.py`
- Create: `probes/rust-public-api/Cargo.toml.in`
- Create: `probes/rust-public-api/src/main.rs`
- Create: `tests/test_run_rust.py`

**Interfaces:**
- Consumes: `Candidate`, `Execution`, `verify_artifact`, and `run_process` from Task 1.
- Produces: `ObservedQuantity(value: Decimal, std_dev: Decimal, unit: str)`, `ObservedError(category: str, message: str)`, and `Observed = ObservedQuantity | ObservedError`.
- Produces: `unpack_crate(crate_file: Path, destination: Path) -> Path`, `materialize_consumer(template: Path, crate_root: Path, destination: Path) -> Path`, `run_rust_consumer(candidate: Candidate, template: Path, binary: str, timeout_s: float = 120.0) -> tuple[Execution, Observed | None]`, and `parse_observation(stdout: str) -> Observed`.
- Observation protocol: one line exactly `PHYSURE_RESULT\t<value>\t<std_dev>\t<unit>` or `PHYSURE_ERROR\t<category>\t<message>`; zero or multiple markers are errors.

- [ ] **Step 1: Write failing archive, materialization, and parser tests**

```python
def test_unpack_crate_returns_single_packaged_root(tmp_path): ...
def test_unpack_crate_rejects_parent_path_member(tmp_path): ...
def test_materialize_consumer_points_only_to_extracted_crate(tmp_path): ...
def test_parse_observation_reads_decimal_fields(): ...
def test_parse_observation_reads_expected_public_error(): ...
def test_parse_observation_rejects_missing_or_duplicate_markers(): ...
def test_cargo_timeout_is_preserved_as_evidence(monkeypatch, candidate): ...
```

- [ ] **Step 2: Verify the Rust runner tests fail**

Run: `python -m pytest tests/test_run_rust.py -q`  
Expected: FAIL because `validator.run_rust` is missing.

- [ ] **Step 3: Implement safe extraction and consumer materialization**

Use Python 3.12 `tarfile` data filtering plus an explicit resolved-path containment check. Copy the consumer template to scratch and replace exactly one `__PHYSURE_CRATE_PATH__` placeholder in `Cargo.toml.in`.

- [ ] **Step 4: Implement observation parsing and isolated Cargo execution**

Parse exactly one result/error marker, run `cargo generate-lockfile --offline`, then run `cargo run --offline --locked --bin <binary>` with only `CARGO_HOME` added from the verified candidate manifest.

- [ ] **Step 5: Add the public Rust probe**

The probe must compile calls to `Quantity::new`, `Quantity::with_uncertainty`, `add`, `sub`, `mul`, `div`, `pow`, `to`, `value.mean()`, `value.std_dev()`, and `unit.__repr__()`. It must also prove an unknown unit returns `Err` rather than panicking, then emit one known observation.

- [ ] **Step 6: Verify Task 3**

Run: `python -m pytest tests/test_run_rust.py -q`  
Expected: all Task 3 tests pass.

- [ ] **Step 7: Commit Task 3**

```bash
git add validator/run_rust.py probes/rust-public-api tests/test_run_rust.py
git commit -m "feat: exercise packaged Rust API"
```

### Task 4: Scientific Case Contract, Comparator, Report, and Gate

**Files:**
- Create: `validator/case.py`
- Create: `validator/report.py`
- Create: `validator/gate.py`
- Create: `validator/__main__.py`
- Create: `tests/test_case.py`
- Create: `tests/test_report.py`
- Create: `tests/test_gate.py`

**Interfaces:**
- Consumes: both runners and `Observed` from Tasks 2–3.
- Produces: `ExpectedQuantity(value: Decimal, std_dev: Decimal, unit: str, rel_tol: Decimal, abs_tol: Decimal)`, `ExpectedError(contains: tuple[str, ...], excludes: tuple[str, ...])`, and `ScientificCase(id: str, classification: Literal["conformance", "experimental"], domain: str, surfaces: tuple[str, ...], source_id: str, phs_result: str | None, rust_binary: str | None, expected: ExpectedQuantity | None, expected_errors: Mapping[str, ExpectedError], directory: Path)`.
- Produces: `load_case(path: Path) -> ScientificCase`, `within_tolerance(actual: Decimal, reference: Decimal, rel_tol: Decimal, abs_tol: Decimal) -> bool`, and `render_checked_phs(case: ScientificCase, source: str) -> str`.
- Produces: `CaseOutcome`, `GateReport`, `run_gate(candidate: Candidate, cases_root: Path) -> GateReport`, and `write_report(report: GateReport, path: Path) -> None`.
- CLI: `python -m validator --candidate candidate.toml --cases cases --report reports/result.json`; exit `0` only when every conformance case passes and all probes pass.

- [ ] **Step 1: Write failing schema and comparison tests**

```python
def test_load_case_parses_numeric_fields_as_decimal_strings(tmp_path): ...
def test_load_case_rejects_unknown_classification_or_surface(tmp_path): ...
def test_within_tolerance_uses_maximum_relative_or_absolute_bound(): ...
def test_within_tolerance_handles_zero_reference_with_absolute_tolerance(): ...
def test_render_checked_phs_appends_case_id_and_case_tolerance(tmp_path): ...
```

The generated PHS check uses the case's result variable and asserts that the absolute error is within either its absolute bound or its relative bound; it must never use PHS's built-in fixed `assert(actual, expected)` tolerance as the scientific case tolerance.

A numeric case uses this exact TOML shape (all numeric fields remain strings):

```toml
id = "mechanics.constant-velocity"
classification = "conformance"
domain = "mechanics"
surfaces = ["phs", "rust"]
source = "openstax-university-physics-v1"

[phs]
result = "result"

[rust]
binary = "constant-velocity"

[expected]
value = "600"
std_dev = "0"
unit = "m"
rel_tol = "1e-12"
abs_tol = "1e-12"
```

An error case omits `[expected]` and uses surface-specific expectations:

```toml
[expected_error.phs]
contains = ["incompatible"]
excludes = ["panicked at", "stack backtrace"]

[expected_error.rust]
contains = ["UnitMismatch"]
excludes = ["panicked at", "stack backtrace"]
```

- [ ] **Step 2: Write failing report and gate tests**

```python
def test_conformance_failure_makes_gate_fail(): ...
def test_experimental_failure_is_reported_without_failing_gate(): ...
def test_probe_failure_always_fails_gate(): ...
def test_report_preserves_reference_tolerance_command_and_raw_streams(tmp_path): ...
def test_cli_returns_nonzero_for_mutant_observation(tmp_path): ...
```

- [ ] **Step 3: Run Task 4 tests and verify the red state**

Run: `python -m pytest tests/test_case.py tests/test_report.py tests/test_gate.py -q`  
Expected: FAIL because the case/report/gate modules are missing.

- [ ] **Step 4: Implement case loading, tolerance comparison, and PHS assertion rendering**

Run: `python -m pytest tests/test_case.py -q`  
Expected: all case schema, zero-reference tolerance, and generated-assertion tests pass.

- [ ] **Step 5: Implement JSON reporting, gate orchestration, and CLI**

`run_gate` verifies both artifact hashes, runs both public-contract probes once, discovers `case.toml` files in sorted order, and records each surface separately plus the combined case decision. Do not add plugins, registries, inheritance, or a generic adapter base class.

- [ ] **Step 6: Verify Task 4**

Run: `python -m pytest tests/test_case.py tests/test_report.py tests/test_gate.py -q`  
Expected: all Task 4 tests pass, including the deliberate mutant failure.

- [ ] **Step 7: Commit Task 4**

```bash
git add validator/case.py validator/report.py validator/gate.py validator/__main__.py tests/test_case.py tests/test_report.py tests/test_gate.py
git commit -m "feat: add scientific conformance gate"
```

### Task 5: Mechanics Sources, Kinematics, and Newtonian Dynamics

**Files:**
- Create: `references/sources.toml`
- Create: `references/mechanics.py`
- Create: `tests/test_mechanics_references.py`
- Create: `cases/conformance/mechanics/{constant-velocity,constant-acceleration,free-fall,projectile-range,newton-second-law}/{case.toml,model.md,program.phs}`
- Create: `cases/conformance/mechanics/rust-consumer/Cargo.toml.in`
- Create: `cases/conformance/mechanics/rust-consumer/src/bin/{constant-velocity,constant-acceleration,free-fall,projectile-range,newton-second-law}.rs`

**Interfaces:**
- Consumes: the Task 4 case schema and Task 3 observation protocol.
- Produces: `reference_for(case_id: str) -> ExpectedQuantity` for numeric case IDs in `references/mechanics.py`; Tasks 6–7 extend the same function without changing its signature. Error cases are checked by their explicit `ExpectedError` manifests rather than this numeric oracle.
- Every `program.phs` assigns its final quantity to the variable named by `phs_result`; the gate appends the assertion.

- [ ] **Step 1: Add versioned scientific sources**

Record the SI Brochure for SI definitions and standard gravity `g0 = 9.80665 m/s^2`, plus an edition-pinned university-mechanics text. Include title, edition, chapter/section, publisher, URL or identifier, and access/snapshot date; do not cite Physure.

- [ ] **Step 2: Write failing reference tests for the first five cases**

Use `Decimal`/`Fraction` for algebraic cases and `mpmath` with 50 decimal digits for trigonometry. Assert these snapshots are absent before implementation:

| Case ID | Inputs | Expected result | Tolerance |
|---|---|---|---|
| `constant-velocity` | `72 km/h`, `30 s` | `600 m` | abs `1e-12`, rel `1e-12` |
| `constant-acceleration` | `x0=5 m`, `v0=3 m/s`, `a=2 m/s^2`, `t=4 s` | `33 m` | abs `1e-12`, rel `1e-12` |
| `free-fall` | `g0`, `t=2 s` | `19.6133 m` | abs `1e-12`, rel `1e-12` |
| `projectile-range` | `v0=20 m/s`, `theta=30 deg`, `g0` | `35.324005803589955 m` | abs `1e-12`, rel `1e-12` |
| `newton-second-law` | `1200 kg * 2.5 m/s^2`; variant `1e20 kg * 1e-10 m/s^2` | `3000 N`; variant `1e10 N` | abs `1e-12`, rel `1e-12`; variant abs `1e-2` |

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: FAIL because the five case snapshots and reference functions are missing.

- [ ] **Step 3: Implement the five independent reference calculations**

Keep equations in `references/mechanics.py`; do not call Physure or copy its constants catalog.

- [ ] **Step 4: Add the five complete PHS/Rust case fixtures**

Each `model.md` records equation, substitutions, assumptions, domain, source ID, and tolerance rationale. Each Rust binary emits the Task 3 observation marker.

- [ ] **Step 5: Verify Task 5**

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: all five source/reference/manifest checks pass.

- [ ] **Step 6: Commit Task 5**

```bash
git add references cases/conformance/mechanics tests/test_mechanics_references.py
git commit -m "test: validate mechanics kinematics and forces"
```

### Task 6: Energy, Momentum, Rotation, and Oscillation Cases

**Files:**
- Modify: `references/mechanics.py`
- Modify: `tests/test_mechanics_references.py`
- Create: `cases/conformance/mechanics/{kinetic-energy,inelastic-collision,centripetal-force,rotating-disk-energy,spring-period,non-si-energy-invariance}/{case.toml,model.md,program.phs}`
- Create: `cases/conformance/mechanics/rust-consumer/src/bin/{kinetic-energy,inelastic-collision,centripetal-force,rotating-disk-energy,spring-period,non-si-energy-invariance}.rs`

**Interfaces:**
- Consumes and extends `reference_for(case_id)` from Task 5.
- Produces six additional numeric cases through the existing Task 4 schema.

- [ ] **Step 1: Add failing reference tests for the six cases**

| Case ID | Inputs | Expected result | Tolerance |
|---|---|---|---|
| `kinetic-energy` | `m=2 kg`, `v=3 m/s` | `9 J` | abs/rel `1e-12` |
| `inelastic-collision` | `m1=2 kg`, `v1=3 m/s`, `m2=1 kg`, `v2=-1 m/s` | `5/3 m/s` | abs/rel `1e-12` |
| `centripetal-force` | `m=0.5 kg`, `v=4 m/s`, `r=2 m` | `4 N` | abs/rel `1e-12` |
| `rotating-disk-energy` | disk `M=4 kg`, `R=0.5 m`, `omega=10 rad/s` | `25 J` | abs/rel `1e-12` |
| `spring-period` | `m=2 kg`, `k=200 N/m` | `0.6283185307179586 s` | abs/rel `1e-12` |
| `non-si-energy-invariance` | `1000 g`, `36 km/h` | `50 J`, equal to SI form | abs/rel `1e-12` |

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: six new missing-case failures while Task 5 cases remain green.

- [ ] **Step 2: Extend the independent reference calculations**

Use `Fraction` for exact rational values and 50-digit `mpmath` for the spring period.

- [ ] **Step 3: Add the six complete PHS/Rust case fixtures**

The non-SI case includes both formulations in the same PHS/Rust fixture and proves their equality as a metamorphic property.

- [ ] **Step 4: Verify Task 6**

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: all eleven accumulated mechanics cases pass reference checks.

- [ ] **Step 5: Commit Task 6**

```bash
git add references/mechanics.py cases/conformance/mechanics tests/test_mechanics_references.py
git commit -m "test: validate mechanical energy and rotation"
```

### Task 7: Default Uncertainty and Adversarial Mechanics Cases

**Files:**
- Modify: `references/mechanics.py`
- Modify: `tests/test_mechanics_references.py`
- Create: `cases/conformance/mechanics/{kinetic-energy-uncertainty,correlated-cancellation,dimension-mismatch,negative-square-root}/{case.toml,model.md,program.phs}`
- Create: `cases/conformance/mechanics/rust-consumer/src/bin/{kinetic-energy-uncertainty,correlated-cancellation,dimension-mismatch,negative-square-root}.rs`
- Create: `cases/experimental/physics/phs-structured-uncertainty/{case.toml,model.md,program.phs}`

**Interfaces:**
- Consumes and extends `reference_for(case_id)` from Task 5.
- Produces the last four conformance cases (15 total) plus one non-blocking DX finding.

- [ ] **Step 1: Add failing uncertainty and error-contract tests**

| Case ID | Inputs | Expected result | Tolerance |
|---|---|---|---|
| `kinetic-energy-uncertainty` | `m=2 +/- 0.1 kg`, `v=3 +/- 0.2 m/s` | mean `9 J`, std dev `1.2816005617976298 J` | abs/rel `1e-12` |
| `correlated-cancellation` | reuse `x=10 +/- 0.5 m` in `x-x` | `0 +/- 0 m` | abs `1e-12`, rel `0` |
| `dimension-mismatch` | `1 m + 1 s` | public error, no panic | discrete |
| `negative-square-root` | `sqrt(-1 m^2)` | public domain error, no panic | discrete |

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: four new missing-case failures while the first eleven remain green.

- [ ] **Step 2: Extend references with analytic first-order uncertainty**

For kinetic energy use `sigma_E^2 = (0.5*v^2*sigma_m)^2 + (m*v*sigma_v)^2`. For cancellation require the correlated default to preserve identity and return zero variance.

- [ ] **Step 3: Add the four conformance fixtures and the experimental PHS DX case**

Stored standard-deviation conformance is Rust. PHS evaluates the same expression and checks mean/unit; because PHS currently lacks a public structured unrounded uncertainty readout, record that limitation in `experimental` rather than parsing hidden precision from formatted text. Error fixtures must match the surface-specific `ExpectedError` contract.

- [ ] **Step 4: Verify all 15 mechanics cases**

Run: `python -m pytest tests/test_mechanics_references.py -q`  
Expected: all 15 conformance cases and the experimental DX manifest pass reference/schema checks.

- [ ] **Step 5: Commit Task 7**

```bash
git add references/mechanics.py cases tests/test_mechanics_references.py
git commit -m "test: validate uncertainty and invalid mechanics"
```

### Task 8: Local Candidate Preparation, Cross-Platform Aggregation, and Self-Test CI

**Files:**
- Create: `validator/aggregate.py`
- Create: `scripts/prepare_candidate.py`
- Create: `tests/test_aggregate.py`
- Create: `tests/test_prepare_candidate.py`
- Create: `tests/test_end_to_end.py`
- Create: `.github/workflows/tests.yml`
- Modify: `README.md`

**Interfaces:**
- Consumes: candidate/report/gate types from Tasks 1 and 4.
- Produces: `prepare_candidate(physure_root: Path, phs_binary: Path, platform: str, output: Path) -> Path`; it runs `cargo package -p physure --allow-dirty`, finds the resulting `.crate`, hashes both artifacts, and writes the manifest.
- Produces: `AggregateReport(required_platforms: tuple[str, ...], received_platforms: tuple[str, ...], conforming: bool)` and `aggregate_reports(paths: Sequence[Path]) -> AggregateReport`.

- [ ] **Step 1: Write failing preparation and aggregation tests**

```python
def test_prepare_candidate_records_real_hashes_and_five_platforms(monkeypatch, tmp_path): ...
def test_aggregate_rejects_missing_required_platform(tmp_path): ...
def test_aggregate_rejects_duplicate_or_mismatched_release_identity(tmp_path): ...
def test_aggregate_fails_when_any_platform_has_conformance_failure(tmp_path): ...
def test_aggregate_passes_only_one_report_per_required_platform(tmp_path): ...
```

- [ ] **Step 2: Write the end-to-end mutant test**

`test_end_to_end.py` must drive a temporary case through the same comparator/report/CLI path with a deliberately wrong Rust observation and assert exit code nonzero plus a report containing actual, reference, tolerance, and case ID. Add a second test proving an experimental mutant remains visible but does not change a clean conformance exit code.

- [ ] **Step 3: Verify Task 8 tests fail**

Run: `python -m pytest tests/test_prepare_candidate.py tests/test_aggregate.py tests/test_end_to_end.py -q`  
Expected: FAIL because candidate preparation and aggregation are missing.

- [ ] **Step 4: Implement local candidate preparation**

Hash the exact `.crate` and PHS binary, preserve the candidate commit/version/platform, and write the Task 1 manifest shape.

- [ ] **Step 5: Implement cross-platform report aggregation**

Reject missing, duplicate, or identity-mismatched platform reports before evaluating their conformance decisions.

- [ ] **Step 6: Add README commands and validator self-test CI**

The workflow runs the validator's own tests on `windows-latest`, `ubuntu-latest`, and `macos-latest`; it does not download unpublished Physure artifacts or publish reports. Document that release-workflow handoff and private-repo credentials require a separate plan.

- [ ] **Step 7: Run the full validator self-test suite**

Run: `python -m pytest -q`  
Expected: all tests pass, including archive containment, timeout, always-green PHS sentinel, numeric mutant, experimental classification, and incomplete-platform aggregation.

- [ ] **Step 8: Prepare and run the current local Physure candidate**

Run from `D:\Projects\physure`:

```powershell
cargo build --release -p physure-cli --bin phs
```

Then run from `D:\Projects\physure-crucible`:

```powershell
python scripts/prepare_candidate.py --physure-root D:\Projects\physure --phs D:\Projects\physure\target\release\phs.exe --platform windows-x86_64 --output candidate.toml
python -m validator --candidate candidate.toml --cases cases/conformance/mechanics --report reports/windows-x86_64.json
```

Expected: a complete JSON report is produced. Exit `0` means the current candidate conforms; exit nonzero is a real release-blocking baseline finding and must be reported, not waived or rewritten as `experimental`.

- [ ] **Step 9: Commit Task 8**

```bash
git add validator/aggregate.py scripts tests .github README.md
git commit -m "ci: complete local scientific release gate"
```

## Final Verification

- [ ] Run `python -m pytest -q` and record the exact pass/fail count.
- [ ] Run `git status --short` and confirm only intentionally ignored local candidate/report artifacts remain untracked.
- [ ] Inspect `git log --oneline --decorate -8` and confirm one focused commit per task.
- [ ] Confirm `cargo metadata` from `D:\Projects\physure` does not list `physure-crucible`.
- [ ] Confirm the local baseline report records artifact hashes, platform, commands, probes, references, tolerances, stdout/stderr, and any blockers.
- [ ] Do not create a remote private repository, push code, or modify Physure's release workflows without separate authorization.
