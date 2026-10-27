# Repo hygiene PR-3: one unit catalog and coherent build metadata — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Python's default `UnitSystem` is built from the same `physure.conf` the Rust core embeds, so the two can no longer drift, and the stale maturin/Cargo metadata is removed.

**Architecture:** `physure-core` already embeds the catalog (`units::conf::DEFAULT_PHYSURE_CONF`, an `include_str!`). Expose it through one `#[pyfunction] default_conf()`, make `application/startup.py` read that string (with `configparser` interpolation off, because the catalog contains a literal `%`), delete Python's copy, point the two generator scripts at the core file, regenerate the generated modules. Then trim `physure-python/Cargo.toml`. Design: `docs/superpowers/specs/2026-10-27-repo-hygiene-onboarding-design.md`.

**Tech Stack:** Rust (PyO3 0.25), Python 3.11+ (`configparser`), maturin, `uv`, pytest, cargo.

## Why this is riskier than PR-1 and PR-2

Python's copy of the catalog is missing units the core has (`percent`, `ppm`, Spanish aliases such as `julio`) and three `[Settings]` keys. Switching makes Python load them. The cause of the drift is that `configparser`'s default interpolation raises on the `%` in `percent = 0.01, 1, [%, percent, pct, porcentaje]`; with `interpolation=None` the core file parses (98 units, verified). What is **not** known in advance is whether Python's own `.conf` loader (`UnitSystemBuilder`) handles every line the core file contains. The full test suite and the redefinition-warning check in Task 5 are what answer that. If they fail in a way Task 5 does not explain, stop and report; fixing Python's loader belongs to the A1 design, not to this PR.

## Conventions (apply to every task)

- Prerequisite: PR-2 is merged, so `origin/main` has the `Cargo.lock` sync. Check in Setup Step 1.
- Work in a new worktree off `origin/main`, branch `refactor/single-source-conf`. Never commit on `main`.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- `git add <explicit paths>`, never `-A` or `-a`; check `git diff --cached --name-only` before each commit.
- Run `maturin develop` only from this worktree's own `physure-python/`, and confirm afterwards that `physure._core.__file__` is inside this worktree.
- Line length is 79 for Python (ruff).

---

## Setup

- [ ] **Step 1: Create the worktree and confirm the lock is in sync**

```bash
cd /d/Projects/physure
git fetch origin
git worktree add -b refactor/single-source-conf .claude/worktrees/single-conf origin/main
cd .claude/worktrees/single-conf
cargo metadata --locked --format-version 1 > /dev/null; echo "locked-exit=$?"
```

Expected: `locked-exit=0`. If it is `101`, PR-2 (Task 4) has not landed: stop and ask the maintainer, otherwise this PR's `Cargo.lock` diff would mix two unrelated changes.

- [ ] **Step 2: Build this worktree's environment**

```bash
cd physure-python
uv sync --all-extras --dev
echo "VIRTUAL_ENV=${VIRTUAL_ENV:-<unset>}"
uv run maturin develop --release
uv run python -c "import physure._core as c; print(c.__file__)"
```

Expected: the printed path contains `.claude\worktrees\single-conf\physure-python`. Anything else: stop, you built into another checkout's venv. This takes several minutes.

- [ ] **Step 3: Record the baseline**

```bash
uv run python - <<'EOF'
from physure import Q_
for u in ("percent", "ppm", "julio", "celsius"):
    try:
        print(u, "->", Q_(1, u))
    except Exception as e:
        print(u, "-> ERROR", type(e).__name__)
EOF
uv run pytest -q --ignore=tests/core/test_serialization.py 2>&1 | tail -3
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
```

Expected: `percent`, `ppm` and `julio` print `ERROR UnknownUnitError`; `celsius` resolves; pytest ends with a passed count and no failures (the known flake is `tests/test_performance_and_interop.py::test_arrow_speed`; rerun it alone if it is the only failure); the last command prints the converted result (0.25 kPa) and a wall time. **Write down the passed count and the wall time.** If `percent` already resolves, the premise of this PR has changed: stop and report.

---

### Task 1: Write the tests first (they fail now)

**Files:**
- Create: `physure-python/tests/core/test_conf_single_source.py`

- [ ] **Step 1: Create the test file**

```python
"""The default unit catalog is physure-core's embedded physure.conf."""

from pathlib import Path

import pytest

import physure
from physure import Q_
from physure.application.context import get_current_system


def test_percent_converts_to_ppm():
    assert Q_(5, "percent").to("ppm").magnitude == pytest.approx(50_000)


def test_spanish_alias_julio_is_joule():
    assert Q_(1, "julio").to("J").magnitude == pytest.approx(1.0)


def test_core_settings_reach_python():
    settings = get_current_system().settings
    assert settings["parallel_threshold"] == "10000"
    assert settings["max_loop_iterations"] == "1000000"
    assert settings["max_pipeline_steps"] == "1000"


def test_python_package_ships_no_second_catalog():
    config_dir = Path(physure.__file__).parent / "infrastructure" / "config"
    assert not (config_dir / "physure.conf").exists()


def test_native_extension_exposes_the_embedded_catalog():
    from physure._core import default_conf

    text = default_conf()
    assert "[Units]" in text
    assert "percent" in text


def test_temperature_scales_still_convert():
    assert Q_(25, "degC").to("K").magnitude == pytest.approx(298.15)
    assert Q_(77, "degF").to("K").magnitude == pytest.approx(298.15, rel=1e-3)
```

- [ ] **Step 2: Run it and confirm the right tests fail for the right reason**

```bash
uv run pytest tests/core/test_conf_single_source.py -q 2>&1 | tail -15
```

Expected: 5 failed, 1 passed. `test_percent_converts_to_ppm` and `test_spanish_alias_julio_is_joule` fail with `UnknownUnitError`; `test_core_settings_reach_python` with `KeyError`; `test_python_package_ships_no_second_catalog` with an `AssertionError`; `test_native_extension_exposes_the_embedded_catalog` with `ImportError ... default_conf`; `test_temperature_scales_still_convert` passes (it guards behaviour that must not change). If either of the first two passes, stop and report.

Do not commit yet; the file is committed with the Python change in Task 3.

---

### Task 2: Expose the embedded catalog from the native extension

**Files:**
- Modify: `physure-python/src/lib.rs` (before the `// ── Module Registration` comment, and inside `_core`)
- Modify: `physure-python/physure/_core.pyi` (after `dim_vector_from_dict`)

- [ ] **Step 1: Add the function**

In `physure-python/src/lib.rs`, directly after the `max_skewness` function and before the line `// ── Module Registration ──...`, add:

```rust
/// The default unit catalog (`physure.conf`) embedded in `physure-core`: the single source
/// the Python `UnitSystem` is built from.
#[pyfunction]
fn default_conf() -> &'static str {
    ::physure_core::units::conf::DEFAULT_PHYSURE_CONF
}
```

- [ ] **Step 2: Register it**

In the `_core` function, directly after the line:

```rust
    m.add_function(wrap_pyfunction!(max_skewness, m)?)?;
```

add:

```rust
    m.add_function(wrap_pyfunction!(default_conf, m)?)?;
```

- [ ] **Step 3: Add the type stub**

In `physure-python/physure/_core.pyi`, replace:

```
def dim_vector_from_dict(pairs: dict[str, int]) -> DimVector: ...
```

with:

```
def dim_vector_from_dict(pairs: dict[str, int]) -> DimVector: ...
def default_conf() -> str: ...
```

- [ ] **Step 4: Build and check**

```bash
cd physure-python
uv run maturin develop --release
uv run python -c "import physure._core as c; print(c.__file__); t = c.default_conf(); print(len(t), t[:40].replace(chr(10), ' '))"
uv run pytest tests/core/test_conf_single_source.py::test_native_extension_exposes_the_embedded_catalog -q
```

Expected: the path is inside this worktree; a length in the tens of thousands of characters and the start of the catalog header comment; `1 passed`. If the build fails with a missing item, run `grep -n "pub const DEFAULT_PHYSURE_CONF" ../physure-core/src/units/conf.rs` and confirm the path used in Step 1 matches the module that defines it.

- [ ] **Step 5: Commit**

```bash
cd ..
git add physure-python/src/lib.rs physure-python/physure/_core.pyi
git diff --cached --name-only
git commit -m "feat(python): expose the embedded unit catalog as _core.default_conf()" -m "physure-core already embeds physure.conf with include_str!. This hands the same string to Python so the two can share one source. No behaviour change yet." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Build Python's default `UnitSystem` from the core catalog

**Files:**
- Modify: `physure-python/physure/application/startup.py`
- Delete: `physure-python/physure/infrastructure/config/physure.conf`
- Modify: `physure-python/scripts/compile_units.py`
- Modify: `physure-python/scripts/generate_units_readme.py`
- Modify: `.claude/skills/add-unit/SKILL.md`
- Modify: `structure.md`
- Create (already written in Task 1): `physure-python/tests/core/test_conf_single_source.py`

- [ ] **Step 1: Import the function**

In `startup.py`, replace:

```python
from physure.application.context import normalize_propagation_mode, use_system
```

with:

```python
from physure._core import default_conf
from physure.application.context import normalize_propagation_mode, use_system
```

- [ ] **Step 2: Read the base catalog from the extension**

Replace this whole function:

```python
def _load_base_config(
    parser: configparser.ConfigParser, verbose: bool
) -> None:
    """Loads the base physure.conf packaged with the library."""
    try:
        base_config_path = resources.files(
            "physure.infrastructure.config"
        ).joinpath(_CONF_FILE)
        with resources.as_file(base_config_path) as base_config:
            parser.read(str(base_config), encoding="utf-8")
            if verbose:
                print(f"  -> Loaded base config: {base_config}")
    except Exception as e:
        print(f"[WARNING] Could not load base configuration: {e}")
```

with:

```python
def _load_base_config(
    parser: configparser.ConfigParser, verbose: bool
) -> None:
    """Loads the base physure.conf embedded in the native extension."""
    parser.read_string(default_conf())
    if verbose:
        print("  -> Loaded base config: physure._core.default_conf()")
```

- [ ] **Step 3: Turn interpolation off**

In `_get_config_parser`, replace:

```python
    parser = configparser.ConfigParser()
    parser.optionxform = str
```

with:

```python
    # interpolation=None: the catalog contains a literal "%"
    # (percent = 0.01, 1, [%, percent, ...]).
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
```

The `resources` import stays: `_load_extra_config_file` still uses it for the `systems/` files.

- [ ] **Step 4: Delete Python's copy and re-point the generators**

```bash
git rm physure-python/physure/infrastructure/config/physure.conf
```

In `physure-python/scripts/compile_units.py`, replace (inside `load_config`):

```python
    parser = configparser.ConfigParser()
```

with:

```python
    parser = configparser.ConfigParser(interpolation=None)
```

and replace:

```python
    # Point to the physure.conf file
    # We assume CWD is project root d:\physure
    conf_file = "physure/infrastructure/config/physure.conf"
```

with:

```python
    # physure-core owns the catalog. Run this script from physure-python/.
    conf_file = "../physure-core/src/units/physure.conf"
```

In `physure-python/scripts/generate_units_readme.py`, replace:

```python
    parser = configparser.ConfigParser()
    parser.optionxform = str
    parser.read("physure/infrastructure/config/physure.conf", encoding="utf-8")
```

with:

```python
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    # physure-core owns the catalog. Run this script from physure-python/.
    parser.read("../physure-core/src/units/physure.conf", encoding="utf-8")
```

- [ ] **Step 5: Update the `add-unit` skill (it documents this very drift)**

In `.claude/skills/add-unit/SKILL.md`, replace:

```
`physure-core/src/units/registry.rs` (`build_default_si` /
`build_default_imperial`), not read from a `.conf` file. (The pure-Python
fallback does have `physure-python/physure/infrastructure/config/systems/`
— see the known-drift warning below; don't confuse the two.)

**Known issue:** `physure-python/physure/infrastructure/config/physure.conf`
is a second, hand-maintained copy of this catalog that has already
drifted out of sync with the Rust one (missing temperature-scale units and
`%` as of 2026-08). Do not edit it to "keep it in sync" — that's a
standing architecture problem tracked separately, not something to
band-aid per-unit. If your change needs to show up in the pure-Python
fallback path today, say so explicitly rather than silently patching both
files.
```

with:

```
`physure-core/src/units/registry.rs` (`build_default_si` /
`build_default_imperial`), not read from a `.conf` file. The Python package
has only `physure-python/physure/infrastructure/config/systems/` (small
per-system overrides such as `imperial.conf`); its base catalog is this same
file, embedded in the extension and handed to Python by `_core.default_conf()`.
```

Replace:

```
3. **Rebuild the Rust core** (config is compiled in, not read at runtime
   for the Rust-backed path): `cd physure-core && maturin develop && cd ..`
```

with:

```
3. **Rebuild the extension** (the catalog is compiled in with `include_str!`
   and is also what the Python `UnitSystem` is built from):
   `cd physure-python && uv run maturin develop --release && cd ..`
```

Replace:

```
   uv run python -c "
   import logging; logging.basicConfig(level=logging.WARNING)
```

with:

```
   cd physure-python && uv run python -c "
   import logging; logging.basicConfig(level=logging.WARNING)
```

Replace the command block under step 5:

```
   cd physure-python && uv run python scripts/generate_units_readme.py && cd ..
```

with:

```
   cd physure-python
   uv run python scripts/compile_units.py        # physure/units/*.py
   uv run python scripts/generate_units_readme.py # docs/UNITS.md
   uv run python -m physure sync-types            # _generated_types.py, registry.pyi
   cd ..
```

Leave the sentence before the block as it is: it already says the script must run from `physure-python/`, which is now also true of `compile_units.py`.

- [ ] **Step 6: Update the configuration node in `structure.md`**

Replace:

```
  node_infra_conf["infrastructure/config/<br/>physure.conf, international.conf,<br/>imperial.conf<br/>[infrastructure/config/physure.conf]"]
```

with:

```
  node_infra_conf["infrastructure/config/systems/<br/>international.conf, imperial.conf<br/>(base catalog: physure-core's physure.conf,<br/>embedded in _core)<br/>[infrastructure/config/systems/international.conf]"]
```

Replace:

```
click node_infra_conf "https://github.com/Alexisrx96/physure/blob/main/physure-python/physure/infrastructure/config/physure.conf"
```

with:

```
click node_infra_conf "https://github.com/Alexisrx96/physure/blob/main/physure-python/physure/infrastructure/config/systems/international.conf"
```

- [ ] **Step 7: Format, lint, and run the new tests**

```bash
cd physure-python
uv run ruff check --fix physure/application/startup.py scripts/compile_units.py scripts/generate_units_readme.py tests/core/test_conf_single_source.py
uv run ruff format physure/application/startup.py tests/core/test_conf_single_source.py
uv run pytest tests/core/test_conf_single_source.py -q 2>&1 | tail -8
```

Expected: ruff reports no remaining problems; `6 passed`. If `test_percent_converts_to_ppm` or `test_spanish_alias_julio_is_joule` fails, read the error: an `UnknownUnitError` after the switch means Python's loader skipped the line (report which); anything else is covered by Task 5's rule.

- [ ] **Step 8: No stale references remain**

```bash
cd ..
grep -rnE "infrastructure[/.]config" . --include=*.py --include=*.md --include=*.toml --include=*.yml --include=*.rs 2>/dev/null | grep -v "^./docs/superpowers\|^./target\|^./.claude/worktrees"
```

Expected: only lines that mention `systems` (the kept per-system files): in `startup.py` (`physure.infrastructure.config.systems`, the docstring), `.claude/skills/add-unit/SKILL.md`, and `structure.md`. Any other line referring to `config/physure.conf`: update the sentence to name `physure-core/src/units/physure.conf`.

- [ ] **Step 9: Commit**

```bash
git add physure-python/physure/application/startup.py physure-python/scripts/compile_units.py physure-python/scripts/generate_units_readme.py physure-python/tests/core/test_conf_single_source.py .claude/skills/add-unit/SKILL.md structure.md
git diff --cached --name-status
git commit -m "refactor(python): build the default UnitSystem from physure-core's catalog" -m "Python kept a hand-maintained copy of physure.conf that had drifted: it lacked percent, ppm, Spanish aliases such as julio, and the parallel_threshold / max_loop_iterations / max_pipeline_steps settings. The drift had a cause: configparser's default interpolation raises on the literal % in percent's alias list, so the core file could not simply be copied. The parser now runs with interpolation=None and reads the catalog from _core.default_conf()." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `D` for the Python `physure.conf`, `M` for `startup.py`, the two scripts, `SKILL.md` and `structure.md`, `A` for the test file.

---

### Task 4: Regenerate the generated modules from the single catalog

**Files (all generated; expected to change):**
- `physure-python/physure/units/*.py`, `physure-python/physure/units/__init__.pyi`
- `docs/UNITS.md`
- `physure-python/physure/_generated_types.py`, `physure-python/physure/core/registry.pyi`

- [ ] **Step 1: Run the three generators from `physure-python/`**

```bash
cd physure-python
uv run python scripts/compile_units.py
uv run python scripts/generate_units_readme.py
uv run python -m physure sync-types
cd ..
git status --short | grep -v "Cargo.lock"
```

Expected: `compile_units.py` prints `Compilation complete.`; the readme script prints `Successfully generated ...docs/UNITS.md`; `sync-types` prints `Successfully generated _generated_types.py and registry.pyi`. `git status` lists only files under `physure-python/physure/units/`, `physure-python/physure/_generated_types.py`, `physure-python/physure/core/registry.pyi` and `docs/UNITS.md`. Any other modified file: stop and report.

- [ ] **Step 2: Check the new symbols are the ones expected**

```bash
git diff -U0 physure-python/physure/units/_index.py | grep '^+' | grep -v '^+++' | head -40
```

Expected: a non-empty list of added index entries: the units the core catalog defines and Python's copy lacked. Keep it as the record of what Python gained; the CHANGELOG entry in Task 7 names only units that the tests in Task 1 prove.

- [ ] **Step 3: Commit**

```bash
git add physure-python/physure/units physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi docs/UNITS.md
git diff --cached --name-only | head -12
git commit -m "chore(units): regenerate unit modules, types and UNITS.md from the single catalog" -m "Generated output only; review by rerunning scripts/compile_units.py, scripts/generate_units_readme.py and python -m physure sync-types from physure-python/." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Verify behaviour, speed, and collisions

**Files:** none

- [ ] **Step 1: Full Python suite**

```bash
cd physure-python
uv run pytest -q --ignore=tests/core/test_serialization.py 2>&1 | tail -15
```

Expected: the same passed count as the baseline plus the 6 new tests, no failures. Triage rule if something fails: read the failing test. If it asserts the *absence* of a unit that now exists, or a hard-coded count of units, update that assertion in a separate commit explaining why. For any other failure, stop and report with the traceback; do not change `UnitSystemBuilder` here.

- [ ] **Step 2: No alias collisions at bootstrap**

```bash
uv run python - <<'EOF' 2>&1 | grep -i "redefin\|collid\|already" ; echo "collision-lines-exit=$?"
import logging
logging.basicConfig(level=logging.WARNING)
from physure import Q_
Q_(1, "m")
EOF
```

Expected: no lines printed and `collision-lines-exit=1`. A `... is being redefined` line means the catalog and Python's own registration both define a symbol (for example `celsius`): stop and report the symbol; which definition should win is the maintainer's call.

- [ ] **Step 3: First-use time**

```bash
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
```

Expected: the result prints and the wall time is no more than 0.1 s above the baseline recorded in Setup Step 3, and still under the project's ~0.5 s budget for `python -m physure` itself.

- [ ] **Step 4: Lint, format, coverage gate**

```bash
uv run ruff check . && uv run ruff format --check .
uv run pytest -q --ignore=tests/core/test_serialization.py --cov=physure --cov-fail-under=80 2>&1 | tail -4
cd ..
```

Expected: ruff clean; coverage at least 80 %.

- [ ] **Step 5: Docs still build (UNITS.md is in the nav)**

```bash
cd physure-python && uv run mkdocs build --strict; echo "build-exit=$?"; cd ..
```

Expected: `build-exit=0`. If `UNITS.md` breaks the build (for example a `|` inside a table cell from a new alias), fix the generator `generate_units_readme.py` to escape it and rerun Task 4 Step 1.

---

### Task 6: Remove the stale build metadata

**Files:**
- Modify: `physure-python/Cargo.toml`
- Modify: `Cargo.toml` (comment only)
- Modify: `Cargo.lock`

- [ ] **Step 1: Wheel baseline**

```bash
mkdir -p /tmp/physure-wheel-before /tmp/physure-wheel-after
cd physure-python
uv run maturin build --release --out /tmp/physure-wheel-before
cd ..
python -m zipfile -l /tmp/physure-wheel-before/*.whl | awk '{print $1}' | sort > /tmp/physure-wheel-before.txt
wc -l < /tmp/physure-wheel-before.txt
```

Expected: a file count above 100 and a wheel under `/tmp/physure-wheel-before/`.

- [ ] **Step 2: Drop the contradictory maturin table and the unused dependencies**

In `physure-python/Cargo.toml`, delete this table (the authoritative setting is `[tool.maturin]` in `pyproject.toml`, which says `python-source = "."`; there is no `python/` directory):

```toml
[package.metadata.maturin]
python-source = "python"
module-name   = "physure._core"
```

Replace:

```toml
[dependencies]
physure-core.workspace = true
physure-script.workspace = true
pyo3.workspace         = true
numpy.workspace        = true
# Re-export workspace deps that physure-core needs, so they link correctly
num-rational.workspace = true
num-traits.workspace   = true
ndarray.workspace      = true
rand.workspace         = true
rand_distr.workspace   = true
nalgebra.workspace     = true
dyn-clone.workspace    = true
arrow.workspace        = true
chrono.workspace       = true
sprs.workspace         = true
```

with:

```toml
[dependencies]
physure-core.workspace = true
physure-script.workspace = true
pyo3.workspace         = true
numpy.workspace        = true
# Used directly by src/lib.rs (Rational64, FromPrimitive).
num-rational.workspace = true
num-traits.workspace   = true
```

Replace:

```toml
[lib]
name       = "physure_core"  # The compiled .so will be named physure._core
crate-type = ["cdylib"]      # Shared library for Python import
```

with:

```toml
[lib]
# Same lib name as the `physure` crate's rlib (physure-core/); both build today.
# The Python module name comes from `#[pymodule(name = "_core")]` in src/lib.rs
# and `[tool.maturin] module-name` in pyproject.toml, not from this field.
name       = "physure_core"
crate-type = ["cdylib"]      # Shared library for Python import
```

- [ ] **Step 3: Prove the removed dependencies were unused**

```bash
cargo build -p physure-python; echo "build-exit=$?"
```

Expected: `build-exit=0`. If it fails with an unresolved crate (`use of undeclared crate or module X`), restore only that one line (`X.workspace = true`) and rerun; do not restore the others.

- [ ] **Step 4: The lock changed only by removed dependency names**

```bash
git diff -U0 Cargo.lock | grep '^[-+]' | grep -v '^+++\|^---'
cargo metadata --locked --format-version 1 > /dev/null; echo "locked-exit=$?"
```

Expected: only lines starting with `-` (names such as `"arrow",`, `"chrono",`, `"nalgebra",`), none starting with `+`; `locked-exit=0`.

- [ ] **Step 5: Reword the stale profile comment in the root `Cargo.toml`**

Replace:

```toml
# panic must stay "unwind" (the default): both crates build as PyO3 extension
# modules (rlib/cdylib, no standalone binaries), and PyO3's panic -> PanicException
# conversion requires catch_unwind. "abort" would turn any Rust panic into a
# process-killing SIGABRT instead of a catchable Python exception.
```

with:

```toml
# panic must stay "unwind" (the default): physure-python builds as a PyO3 extension
# module, and PyO3's panic -> PanicException conversion requires catch_unwind.
# "abort" would turn any Rust panic into a process-killing SIGABRT instead of a
# catchable Python exception. The profile is workspace-wide, so the phs and
# language-server binaries are built with unwind too.
```

- [ ] **Step 6: The wheel is unchanged, and Rust and Python still pass**

```bash
cd physure-python
uv run maturin build --release --out /tmp/physure-wheel-after
cd ..
python -m zipfile -l /tmp/physure-wheel-after/*.whl | awk '{print $1}' | sort > /tmp/physure-wheel-after.txt
diff /tmp/physure-wheel-before.txt /tmp/physure-wheel-after.txt; echo "wheel-diff-exit=$?"
cargo test -p physure -p physure-script -p physure-cli -p physure-lsp 2>&1 | tail -5
cd physure-python && uv run maturin develop --release && uv run pytest tests/core/test_rust_parity.py tests/core/test_conf_single_source.py -q | tail -3; cd ..
```

Expected: `wheel-diff-exit=0` (identical file lists); cargo tests pass; `2 files` of tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml physure-python/Cargo.toml Cargo.lock
git diff --cached --name-only
git commit -m "build: drop stale maturin metadata and unused physure-python dependencies" -m "[package.metadata.maturin] said python-source = python (there is no such directory) while pyproject.toml, which maturin reads, says '.'. Eight dependencies were declared as 're-exported for physure-core' but are not referenced by src/lib.rs; cargo resolves transitive dependencies itself, and the built wheel's file list is unchanged. The root profile comment still described a two-crate workspace without binaries." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md` (the empty `## [Unreleased]` section)

- [ ] **Step 1: Add the entry**

Replace:

```
## [Unreleased]

## [0.2.4] - 2026-08-28
```

with:

```
## [Unreleased]

### Changed
- **Python and the core now share one unit catalog.** Python used to carry its own hand-maintained copy of `physure.conf` that had drifted from `physure-core`'s: it did not know `percent` / `%`, `ppm` or Spanish aliases such as `julio`, and it never saw `parallel_threshold`, `max_loop_iterations` or `max_pipeline_steps` in `[Settings]`. The copy could not simply be refreshed, because `configparser`'s default interpolation raises on the literal `%` in `percent`'s alias list. Python's `UnitSystem` is now built from the catalog embedded in the native extension (`physure._core.default_conf()`), read with interpolation off, so a unit added to `physure-core/src/units/physure.conf` is available to both. A `physure.conf` in the working directory still overrides the defaults, and is now also read without `%` interpolation, so a `%%` in it is no longer collapsed to `%`.

## [0.2.4] - 2026-08-28
```

- [ ] **Step 2: Check every claim in the entry against a passing test**

```bash
cd physure-python && uv run pytest tests/core/test_conf_single_source.py -q | tail -3; cd ..
```

Expected: `6 passed`. The entry's claims map to tests: `percent` and `ppm` to `test_percent_converts_to_ppm`, `julio` to `test_spanish_alias_julio_is_joule`, the three `[Settings]` keys to `test_core_settings_reach_python`, and the single copy to `test_python_package_ships_no_second_catalog`. Remove any claim that has no passing test.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git diff --cached --name-only
git commit -m "docs(changelog): Python and the core share one unit catalog" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Final check and PR (outward-facing: confirm with the maintainer first)

- [ ] **Step 1: The final diff**

```bash
git log --oneline origin/main..HEAD
git diff --name-only origin/main...HEAD | grep -v "^physure-python/physure/units/\|^docs/UNITS.md"
```

Expected: five commits (expose, refactor, regenerate, build, changelog) and a file list of: `.claude/skills/add-unit/SKILL.md`, `CHANGELOG.md`, `Cargo.lock`, `Cargo.toml`, `physure-python/Cargo.toml`, `physure-python/physure/_core.pyi`, `physure-python/physure/_generated_types.py`, `physure-python/physure/application/startup.py`, `physure-python/physure/core/registry.pyi`, the deleted `physure-python/physure/infrastructure/config/physure.conf`, `physure-python/scripts/compile_units.py`, `physure-python/scripts/generate_units_readme.py`, `physure-python/src/lib.rs`, `physure-python/tests/core/test_conf_single_source.py`, `structure.md`.

- [ ] **Step 2: Ask the maintainer to confirm the push**

The pre-push hook runs a full workspace build and test and is slow. Never use `--no-verify`.

- [ ] **Step 3: Push in the background with the maximum timeout, then open the PR**

```bash
git push -u origin refactor/single-source-conf
gh pr create --base main --title "refactor(python): build the default UnitSystem from physure-core's catalog" --body-file - <<'EOF'
## What
- `_core.default_conf()` exposes the `physure.conf` that `physure-core` already embeds.
- Python's `UnitSystem` is built from it (`configparser` with `interpolation=None`); Python's hand-maintained copy is deleted.
- Generator scripts read the core file; generated modules, `UNITS.md` and types are regenerated (separate commit, generated output only).
- Remove the contradictory `[package.metadata.maturin]` table and eight unused `physure-python` dependencies; the built wheel's file list is unchanged.
- `add-unit` skill and `structure.md` updated: they documented the drift as a known issue.

## Why
The two copies had drifted: Python did not know `percent`, `ppm`, `julio`, or three `[Settings]` keys. The cause was `configparser` interpolation choking on the `%` in `percent`'s alias list, so the file could not just be copied.

## Verification
New tests in `tests/core/test_conf_single_source.py` (red first); full suite; no redefinition warnings at bootstrap; first-use time compared with the baseline; strict docs build; wheel file lists compared before and after the metadata change; `cargo test` for the four CI crates.

## Not in this PR
Python's own `.conf` loader (`UnitSystemBuilder`) still duplicates `parse_physure_conf`; that belongs to the follow-up design for the Python/Rust unit-algebra duplication.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

Expected: the PR URL. Report it to the maintainer.
