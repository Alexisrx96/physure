# A2 PR B: stop importing the package facade — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove nine of the ten imports of the package facade (`from physure import ...`) from inside the package, fixing a latent bug on the way, and keep the tenth as a documented exception. The layering contract from PR A proves it: its exception list shrinks by exactly these imports.

**Architecture:** Two kinds of change, in two commits. (1) `default_system` was imported at module level in three modules and used as a default argument, so the unit system active when physure was first imported was baked in forever and a `use_system(...)` block was silently ignored; the three now resolve `get_current_system()` when the object is created. (2) `Q_` is an instance of `QuantityFactory`; six deferred imports took it from the facade, and now build it from `physure.application.factories`, where it is defined. The one import that stays is `__version__` in `ext/io.py`, recorded as a deliberate exception (see Task 5). Design: `docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md`.

**Tech Stack:** Python 3.11+, pytest, ruff. No Rust changes.

## What was measured while planning (use it to check your results)

The change was implemented and verified on a prototype branch before this plan was written.

| Measure | Before | After |
|---|---|---|
| Imports of the facade from modules below it | 10 | 1 (`ext/io.py`, documented) |
| Imports recorded as layering exceptions | 26, in 23 records | 17, in 16 records (8 `a1`, 8 `a2a`, 1 `release`, counted in imports) |
| Behaviour test `test_active_system_default.py` | 3 failed, 1 passed | 4 passed |
| Full suite | 914 passed, 6 xfailed | **918 passed, 6 xfailed** |
| First use (`python -m physure "500 N / 2 m^2 => kPa"`) | n/a | 0.11 s, prints `0.25 kPa` |
| Exception records after each commit | 23 | 20 after the first, 16 after the second, 16 after the third |

`physure/constants.py` also does `from physure import get_current_system`; it belongs to the facade layer, so it is allowed and is not touched.

## Deviation from the design (read this)

The spec proposed reading `__version__` through `importlib.metadata` in `ext/io.py`. That was dropped, for two reasons found while planning: `tests/ext/test_io.py` requires the saved `physure_version` to equal `physure.__version__` exactly, and the version in the installed metadata can lag behind (an editable install keeps the old version until it is reinstalled, which happens after every release bump); and moving `__version__` to a leaf module would require editing the release workflows, which grep and `sed` it in `physure/__init__.py` and cannot be tested locally. So `ext/io.py` keeps its import and the exception is recorded with the cause `release`, explaining the way out.

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `refactor/a2-import-the-real-module` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/`, and always use `uv run --no-sync ...`. Never run `maturin develop` or `uv sync`.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files with non-ASCII text.
- The edits are made by one script, in parts, so that each commit leaves the contract consistent with the code. Do not edit the sources by hand.

---

### Task 1: Create the branch and extract the helper script

**Files:** none tracked

- [ ] **Step 1: Start from a clean tree at the tip of `main`**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
git status --short
git log --oneline -1
git log --oneline -1 main
```

Expected: `git status --short` prints nothing (or only ` M Cargo.lock`); the two `log` lines show the same commit. If `physure-python/tests/symbolic/test_active_system_default.py` shows as untracked, it is a leftover of the planning session: confirm it is the only entry, then delete exactly that path (Task 2 recreates it). Any other untracked or modified file: stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c refactor/a2-import-the-real-module main
git status -sb | head -1
```

Expected: `## refactor/a2-import-the-real-module`.

- [ ] **Step 3: Extract the edit script from this plan (do not retype it)**

```bash
python -X utf8 - <<'EOF'
import pathlib, re
FENCE = chr(96) * 3
plan = pathlib.Path(r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a2-pr-b-stop-importing-the-facade.md").read_text(encoding="utf-8")
m = re.search(re.escape("Create `apply_pr_b.py` with exactly this content:") + r"\n\n" + FENCE + r"python\n(.*?)\n" + FENCE + r"\n", plan, re.S)
assert m, "script block not found in the plan"
out = pathlib.Path(r"C:\Users\Irvin\AppData\Local\Temp\apply_pr_b.py")
out.write_text(m.group(1) + "\n", encoding="utf-8", newline="\n")
print("script written |", m.group(1).count("\n") + 1, "lines")
EOF
```

Expected: `script written | ... lines`. The script block is defined in Task 3, Step 2; it is extracted now so that later steps can run it.

---

### Task 2: Write the behaviour test first (it must fail)

**Files:**
- Create: `physure-python/tests/symbolic/test_active_system_default.py`

- [ ] **Step 1: Create the test**

Extract it from this plan instead of retyping it:

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 - <<'EOF'
import pathlib, re
FENCE = chr(96) * 3
plan = pathlib.Path(r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a2-pr-b-stop-importing-the-facade.md").read_text(encoding="utf-8")
anchor = "Create `physure-python/tests/symbolic/test_active_system_default.py` with exactly this content:"
m = re.search(re.escape(anchor) + r"\n\n" + FENCE + r"python\n(.*?)\n" + FENCE + r"\n", plan, re.S)
assert m, "test block not found in the plan"
out = pathlib.Path("physure-python/tests/symbolic/test_active_system_default.py")
out.write_text(m.group(1) + "\n", encoding="utf-8", newline="\n")
print("test written |", m.group(1).count("\n") + 1, "lines")
EOF
```

Create `physure-python/tests/symbolic/test_active_system_default.py` with exactly this content:

```python
"""Symbolic helpers follow the unit system that is active when they are created.

Their default used to be the system that was active when physure was first
imported, so a ``use_system`` block was silently ignored.
"""

import pytest

from physure.application.context import use_system
from physure.application.startup import create_system


@pytest.fixture
def imperial():
    return create_system("imperial.conf")


def test_symbolic_quantity_uses_the_active_system(imperial):
    from physure.domain.symbolic.quantity import SymbolicQuantity

    with use_system(imperial):
        x = SymbolicQuantity("x", "m")
    assert x.system is imperial


def test_symbolic_expression_uses_the_active_system(imperial):
    sp = pytest.importorskip("sympy")
    from physure.domain.measurement.units import CompoundUnit
    from physure.domain.symbolic.expression import SymbolicExpression

    with use_system(imperial):
        expression = SymbolicExpression(sp.Symbol("x"), CompoundUnit({"m": 1}))
    assert expression.system is imperial


def test_function_uses_the_active_system(imperial):
    sp = pytest.importorskip("sympy")
    from physure.application.functions.functions import Function
    from physure.domain.measurement.units import CompoundUnit

    meter = CompoundUnit({"m": 1})
    with use_system(imperial):
        function = Function(
            parameters={"x": meter},
            output_unit=meter,
            symbolic_func=sp.Symbol("x"),
        )
    assert function.system is imperial


def test_an_explicit_system_still_wins(imperial):
    from physure.domain.symbolic.quantity import SymbolicQuantity

    x = SymbolicQuantity("x", "m", system=imperial)
    assert x.system is imperial
```

- [ ] **Step 2: Lint it**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/symbolic/test_active_system_default.py
uv run --no-sync ruff format --check tests/symbolic/test_active_system_default.py
```

Expected: `All checks passed!` and `1 file already formatted`.

- [ ] **Step 3: Run it and confirm the bug is real**

```bash
uv run --no-sync pytest tests/symbolic/test_active_system_default.py -q -p no:cacheprovider 2>&1 | grep -E "^FAILED|passed|failed"
```

Expected: `3 failed, 1 passed`. The three failures are `test_symbolic_quantity_uses_the_active_system`, `test_symbolic_expression_uses_the_active_system` and `test_function_uses_the_active_system`, each an `AssertionError` on `.system is imperial`; `test_an_explicit_system_still_wins` passes. If any of the first three passes, stop and report: the premise changed.

---

### Task 3: Resolve the active system when the object is created

**Files:**
- Modify: `physure-python/physure/application/functions/functions.py`
- Modify: `physure-python/physure/domain/symbolic/expression.py`
- Modify: `physure-python/physure/domain/symbolic/quantity.py`
- Modify: `physure-python/tests/core/import_layers.toml` (three records removed)
- Modify: `CHANGELOG.md`
- Test: `physure-python/tests/symbolic/test_active_system_default.py`

- [ ] **Step 1: Apply the `defaults` and `changelog` parts**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_b.py defaults
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_b.py changelog
```

Expected: `edited` lines for the three source files, `edited ...import_layers.toml (dropped 3 records)` and `edited CHANGELOG.md`. Any `AssertionError` means an anchor changed: stop and report.

- [ ] **Step 2: Lint, format and run the checks**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/symbolic/test_active_system_default.py tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean; `8 passed` (the 4 behaviour tests and the 4 contract tests); `20` exception records. If the contract test fails with "recorded N, found M", the script and the code disagree: stop and report.

- [ ] **Step 3: Run the symbolic and function tests**

```bash
uv run --no-sync pytest tests/symbolic tests/functions -q -p no:cacheprovider 2>&1 | tail -2
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/application/functions/functions.py physure-python/physure/domain/symbolic/expression.py physure-python/physure/domain/symbolic/quantity.py physure-python/tests/symbolic/test_active_system_default.py physure-python/tests/core/import_layers.toml CHANGELOG.md
git diff --cached --name-status
git commit -m "fix(symbolic): use the unit system that is active when the object is created" -m "SymbolicQuantity, SymbolicExpression and Function took 'from physure import default_system' as a default argument. The facade attribute resolves the active system at that moment, so the value was frozen when physure was first imported and a use_system(...) block was silently ignored. They now resolve get_current_system() when the object is created; an explicit system still wins. This also removes three imports of the package facade from inside the package, and their exception records." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `M` for the three sources, `A` for the test, `M` for the TOML and `M` for `CHANGELOG.md`; nothing else.

---

### Task 4: Import `Q_` from where it is defined

**Files:**
- Modify: `physure-python/physure/ext/chemistry/equivalency.py`
- Modify: `physure-python/physure/ext/chemistry/species.py`
- Modify: `physure-python/physure/ext/chemistry/thermo_kinetics.py`
- Modify: `physure-python/physure/module.py`
- Modify: `physure-python/tests/core/import_layers.toml` (four records removed)

- [ ] **Step 1: Apply the `factory` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_b.py factory
```

Expected: `edited` lines for the four sources and `edited ...import_layers.toml (dropped 4 records)`.

- [ ] **Step 2: Lint, format and run the checks**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_import_layers.py tests/ext -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean; all pass (the contract tests and the chemistry/extension tests, which exercise the changed functions); `16` exception records.

- [ ] **Step 3: Commit**

```bash
cd ..
git add physure-python/physure/ext/chemistry/equivalency.py physure-python/physure/ext/chemistry/species.py physure-python/physure/ext/chemistry/thermo_kinetics.py physure-python/physure/module.py physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "refactor: build Q_ from application.factories instead of importing the facade" -m "Q_ is a QuantityFactory(). Six deferred imports in ext/chemistry and module.py took it from the package facade; they now import QuantityFactory from the module that defines it. No behaviour change." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: five `M` entries.

---

### Task 5: Record the one import that stays

**Files:**
- Modify: `physure-python/tests/core/import_layers.toml`

- [ ] **Step 1: Apply the `release` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_b.py release
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -n 'cause = "release"' -B4 tests/core/import_layers.toml
```

Expected: `4 passed`; the record shown is `physure.ext.io` importing `physure` (top) with `cause = "release"`, and the header comment of the file now explains the `release` cause.

- [ ] **Step 2: Commit**

```bash
cd ..
git add physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "test(layers): keep ext/io's __version__ import as a documented exception" -m "tests/ext/test_io.py requires the saved physure_version to equal physure.__version__ exactly, and the installed metadata can lag behind after a release bump; moving __version__ to a leaf module means editing the release workflows, which grep and sed it in physure/__init__.py. The record says why and how to remove it." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: one `M` entry.

---

### Task 6: Verify the whole project

**Files:** none

- [ ] **Step 1: Lint and format**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
```

Expected: `All checks passed!` and every file formatted.

- [ ] **Step 2: Run the full suite, as CI does**

```bash
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
```

Expected: `918 passed, 6 xfailed`. If any other test fails, run it alone and report; do not change application code.

- [ ] **Step 3: First use stays fast, and no facade import is left but the documented one**

```bash
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
grep -rnE --include="*.py" "^\s*(from physure import|import physure$)" physure | grep -v "^physure/__init__.py"
```

Expected: `0.25 kPa` in about 0.1 to 0.2 s; the grep prints exactly two lines: `physure/constants.py:18:from physure import get_current_system` (a facade-layer module, allowed) and `physure/ext/io.py:25:from physure import __version__` (the documented exception). Any other line is a facade import this plan missed.

- [ ] **Step 4: Confirm the branch**

```bash
cd ..
git status --short
git log --oneline -4
```

Expected: `git status --short` empty (or only ` M Cargo.lock`); three new commits on `refactor/a2-import-the-real-module` on top of `main`.

- [ ] **Step 5: Hand-off**

Report the branch name, the three commit SHAs and the measured numbers (they should match the table above). Nothing is pushed. Integrating the branch is the maintainer's decision. The remaining `a2a` exceptions (8 imports) are the ones A2 PRs C and D close.

---

## Reference: the files this plan embeds

Create `apply_pr_b.py` with exactly this content:

```python
"""A2 PR B: stop importing the package facade from inside the package.

Usage (from the repository root of the worktree):
    python -X utf8 apply_pr_b.py defaults | factory | release | changelog

Every replacement asserts that its anchor occurs exactly as many times as
expected, and every file keeps its own line endings.
"""

import pathlib
import re
import sys

SRC = pathlib.Path("physure-python/physure")
TOML = pathlib.Path("physure-python/tests/core/import_layers.toml")
CHANGELOG = pathlib.Path("CHANGELOG.md")


def edit(path, replacements):
    raw = path.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    for old, new, count in replacements:
        old, new = old.replace("\n", eol), new.replace("\n", eol)
        assert raw.count(old) == count, (
            f"{path}: {old[:60]!r} found {raw.count(old)}, expected {count}"
        )
        raw = raw.replace(old, new)
    path.write_bytes(raw.encode("utf-8"))
    print("edited", path)


def drop_exceptions(records):
    """Removes the recorded exceptions (importer, imported, kind) that are fixed."""
    raw = TOML.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    text = raw.replace(eol, "\n")
    for importer, imported, kind in records:
        pattern = re.compile(
            r"\[\[exception\]\]\nimporter = \"" + re.escape(importer)
            + r"\"\nimported = \"" + re.escape(imported)
            + r"\"\nkind = \"" + kind + r"\"\n(?:count = \d+\n)?cause = \"a2a\"\n\n?"
        )
        text, n = pattern.subn("", text)
        assert n == 1, (importer, imported, kind, n)
    TOML.write_bytes(text.replace("\n", eol).encode("utf-8"))
    print("edited", TOML, f"(dropped {len(records)} records)")


GET = "from physure.application.context import get_current_system"
FACTORY = "from physure.application.factories import QuantityFactory"
part = sys.argv[1]

if part == "defaults":
    # default_system was frozen when physure was first imported: resolve the
    # active system when the object is created
    edit(SRC / "application/functions/functions.py", [
        ("from physure import default_system\n", GET + "\n", 1),
        ("    system: UnitSystem = field(default=default_system, repr=False)\n",
         "    system: UnitSystem = field(default_factory=get_current_system, repr=False)\n", 1),
    ])
    edit(SRC / "domain/symbolic/expression.py", [
        ("from physure import default_system\nfrom physure.application.functions.functions import Function\n",
         GET + "\nfrom physure.application.functions.functions import Function\n", 1),
        ("        system: UnitSystem = default_system,\n        variables: set[SymbolicQuantity] | None = None,\n",
         "        system: UnitSystem | None = None,\n        variables: set[SymbolicQuantity] | None = None,\n", 1),
        ("        self.system = system\n",
         "        self.system = system if system is not None else get_current_system()\n", 1),
    ])
    edit(SRC / "domain/symbolic/quantity.py", [
        ("from physure import default_system\nfrom physure.domain.exceptions import IncompatibleUnitsError\n",
         GET + "\nfrom physure.domain.exceptions import IncompatibleUnitsError\n", 1),
        ("        system: UnitSystem = default_system,\n    ):\n        \"\"\"Initializes a SymbolicQuantity.\"\"\"\n",
         "        system: UnitSystem | None = None,\n    ):\n        \"\"\"Initializes a SymbolicQuantity.\"\"\"\n        if system is None:\n            system = get_current_system()\n", 1),
    ])
    drop_exceptions([
        ("physure.application.functions.functions", "physure", "top"),
        ("physure.domain.symbolic.expression", "physure", "top"),
        ("physure.domain.symbolic.quantity", "physure", "top"),
    ])

elif part == "factory":
    # Q_ is a QuantityFactory(); import it from where it is defined, not from the facade
    lazy_q = "    from physure import Q_\n"
    new_q = "    " + FACTORY + "\n\n    Q_ = QuantityFactory()\n"
    edit(SRC / "ext/chemistry/equivalency.py", [
        (lazy_q + "    from physure.domain.measurement.equivalencies import equivalencies\n",
         new_q + "    from physure.domain.measurement.equivalencies import equivalencies\n", 2),
    ])
    edit(SRC / "ext/chemistry/species.py", [
        ("        from physure import Q_\n\n        mass = 0.0\n",
         "        " + FACTORY + "\n\n        Q_ = QuantityFactory()\n        mass = 0.0\n", 1),
    ])
    edit(SRC / "ext/chemistry/thermo_kinetics.py", [
        (lazy_q + "\n    if formula not in STANDARD_ENTHALPY_FORMATION:\n",
         new_q + "\n    if formula not in STANDARD_ENTHALPY_FORMATION:\n", 1),
        (lazy_q + "\n    if formula not in STANDARD_ENTROPY:\n",
         new_q + "\n    if formula not in STANDARD_ENTROPY:\n", 1),
    ])
    edit(SRC / "module.py", [
        ("                from physure import Q_\n\n                return Q_(float(mag_str), unit_str)\n",
         "                " + FACTORY + "\n\n                return QuantityFactory()(float(mag_str), unit_str)\n", 1),
    ])
    drop_exceptions([
        ("physure.ext.chemistry.equivalency", "physure", "lazy"),
        ("physure.ext.chemistry.species", "physure", "lazy"),
        ("physure.ext.chemistry.thermo_kinetics", "physure", "lazy"),
        ("physure.module", "physure", "lazy"),
    ])

elif part == "release":
    # the one facade import that stays, on purpose: say why
    raw = TOML.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    text = raw.replace(eol, "\n")
    old = 'importer = "physure.ext.io"\nimported = "physure"\nkind = "top"\ncause = "a2a"\n'
    assert text.count(old) == 1
    text = text.replace(old, old.replace('cause = "a2a"', 'cause = "release"'))
    old = "#   a2a - fixed by the A2 pull requests that clean up the imports\n"
    assert text.count(old) == 1
    text = text.replace(
        old,
        old
        + "#   release - kept on purpose: `__version__` must stay in physure/__init__.py\n"
        + "#         because the release workflows grep and sed it there. Move it to a\n"
        + "#         leaf module together with those workflows.\n",
    )
    TOML.write_bytes(text.replace("\n", eol).encode("utf-8"))
    print("edited", TOML)

elif part == "changelog":
    edit(CHANGELOG, [(
        "\n## [0.2.4] - 2026-08-28\n",
        "\n### Fixed\n"
        "- **`SymbolicQuantity`, `SymbolicExpression` and `Function` use the unit system that is active when they are created.** Their default was bound once, when physure was first imported, so a `physure.use_system(...)` block was silently ignored and the object came out in the original system.\n"
        "\n## [0.2.4] - 2026-08-28\n",
        1,
    )])

else:
    raise SystemExit(f"unknown part: {part}")
```
