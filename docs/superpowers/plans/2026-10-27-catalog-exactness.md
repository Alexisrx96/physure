# Catalog fix: CODATA 2022, exact unit factors, one name per definition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every numeric value of `physure-core/src/units/physure.conf` the exact value of its definition, in one CODATA edition (2022), and give every unit that has several definitions in use (calorie, BTU, acre, gallon, fluid ounce, horsepower, year) one name per definition, with a test that no name can ever mean two units. Diagnosis and numbers: `docs/superpowers/specs/2026-10-27-catalog-numeric-audit.md` (branch `docs/catalog-numeric-audit`).

**Architecture:** Three commits, each leaving the repository green. (1) `[Constants]` is regenerated with `repr` instead of `f"{val:e}"` (which kept 7 significant digits) from CODATA 2022, and the atomic and Planck units of `[Units]` follow. (2) The 15 truncated factors, `franklin` and `debye` become the exact value of their definition. (3) Each ambiguous unit gets one entry per definition under a qualified name (`cal_it`, `btu_th`, `acre_us_survey`, `gallon_imperial`, `hp_metric`, `year_gregorian`, …); the short symbol stays the documented default. The catalog is compiled into the extension (`include_str!`) and read by Python, PHS and the Rust API alike, so there is one place to fix. The Rust unit parser is not touched.

**Tech Stack:** Rust (tests only), Python 3.11+, pytest, ruff, scipy (reference for CODATA 2022). The extension must be rebuilt after each catalog change.

## What was measured while planning (use it to check your results)

The change was implemented and verified on a prototype branch before this plan was written.

| Measure | Before | After |
|---|---|---|
| Unit factors off by more than a couple of ulps | 22 of 69 checkable | 0 |
| `[Constants]` entries cut to 7 digits | 189 of 411 | 0 |
| CODATA editions in the file | 2018 and 2022 | 2022 |
| `1 deg => arcmin`, `1 atm => Torr`, `1 gal => in^3` | 60.000043, 760.0021, 230.99989 | 60, 760, 231 (within 2e-15) |
| `ε₀ μ₀ c² − 1` from the stored constants | −2.8e-08 | about 1e-12 |
| New tests | — | 81 (1 + 43 + 37, see each task) and 1 Rust test |
| Python suite | 940 passed, 6 xfailed | **1021 passed, 6 xfailed** |
| Exception records of the layering contract | 7 | 7 (no Python import changes) |

## Decisions taken while planning

- **Default meanings** (the short symbol keeps naming exactly one definition, written in a comment above the group in the catalog): `cal` = thermochemical calorie 4.184 J; `BTU`/`btu` = International Table BTU 1055.05585262 J (the old value `1055.06` matched no definition); `acre` = international acre; `gal`, `gallon` and `fl_oz` = US liquid gallon and fluid ounce; `hp` = mechanical horsepower; `yr`, `year` = Julian year. `torr` is exactly atm/760 and `mmHg` is the conventional millimetre of mercury, so they differ by 1.4e-7 as they do in reality.
- **How to be unambiguous, including where the short symbol is taken.** Every entry name already resolves on its own (`gallon_us` works today) and no name can be shared. So each definition gets its own entry named `<unit>_<qualifier>`, and the default also gets a qualified alias (`cal_th`, `btu_it`, `acre_international`, `gal_us`, `fl_oz_us`, `hp_mechanical`, `year_julian`). `tests/core/test_catalog_namespace.py` turns the registry's "log a warning, the later definition wins" into a failure: no two units may claim a name, the only short symbols that also read as prefix + another unit are the five known ones (`cd`, `ft`, `kg`, `pH`, `yd`; the exact symbol wins), and every entry name resolves.
- **Not done here, on purpose.** The five shadowed readings (centi-day, femto-tonne, pico-henry, yocto-day) need long prefixed names (`centiday`), which the Rust unit parser does not accept yet: that is A1 stage 1. Exact rational scales (A1 stage 1) will make the identities exact to the last bit; until then they hold to about 2e-15, which is the tolerance of the new tests. Troy ounce, US survey foot and mile, tropical year and the 59 °F BTU are not added: nothing uses them yet.
- **CODATA 2022 is enforced.** The generator refuses to run on a scipy whose `electron mass` is not the 2022 value, and the constants test is skipped on such a scipy instead of failing.

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `fix/catalog-exactness` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/` (or with `--project physure-python`), and use `uv run --no-sync ...`. Never run `uv sync`. The one command that builds is `uv run --no-sync maturin develop --release` from `physure-python/`; after it, `uv run --no-sync python -X utf8 -c "import physure._core as c; print(c.__file__)"` must print a path inside `D:\Projects\physure\.claude\worktrees\single-conf\physure-python\physure\`. If it prints any other path, stop and report.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it. The generated files (list below) are staged with `git add -u -- <those paths>`, which stages only modifications of tracked files.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files.
- The catalog and test-support edits are made by one script, `apply_catalog.py`, in parts; the new test files are extracted from this plan by `plan_block.py` (Task 1). Do not edit those files by hand and do not retype the tests.
- **Generated files**, rewritten by the three commands in "Regenerate" below and committed with the change that caused them: `docs/UNITS.md`, `physure-python/physure/_generated_types.py`, `physure-python/physure/core/registry.pyi`, `physure-python/physure/units/__init__.pyi`, `physure-python/physure/units/_index.py`, `physure-python/physure/units/core.py`.
- **Rebuild** (after every catalog change, before regenerating or running Python tests; about 1 to 2 minutes):

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync maturin develop --release 2>&1 | tail -2
uv run --no-sync python -X utf8 -c "import physure._core as c; print(c.__file__)"
```

- **Regenerate** (after the rebuild):

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 scripts/compile_units.py 2>&1 | tail -1
uv run --no-sync python -X utf8 scripts/generate_units_readme.py 2>&1 | tail -1
uv run --no-sync python -X utf8 -m physure.cli sync-types 2>&1 | tail -1
```

Expected: `Compilation complete.`, `Successfully generated ...UNITS.md`, `Successfully generated _generated_types.py and registry.pyi`.
- **The Python suite rewrites three generated files.** A test adds `mock_unit` (a unit that `tests/core/test_registry.py` mocks) back to `_generated_types.py`, `registry.pyi` and `units/__init__.pyi`; the leak is already in `main` and this change does not fix it. So the order in every task is: rebuild, regenerate, run the tests, **regenerate again**, stage and commit, and no Python test runs between the last regeneration and the commit. The first commit therefore also removes `mock_unit` from those three files.
- Rust tests: the core crate's package name is `physure` (`cargo test -p physure`), the language crate's is `physure-script`. Both are run from the worktree root.

---

### Task 1: Create the branch and the helper scripts

**Files:** none tracked

- [ ] **Step 1: Start from a clean tree at the tip of `main`**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
git status -sb | head -3
git log --oneline -1
git log --oneline -1 main
```

Expected: no changed files (a modified `Cargo.lock` alone is fine); the two `log` lines show the same commit. If the worktree is on another branch with no uncommitted changes, `git switch --detach main` first. Any other uncommitted change: stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c fix/catalog-exactness main
git status -sb | head -1
```

Expected: `## fix/catalog-exactness`.

- [ ] **Step 3: Rebuild the extension at the state of `main`**

Python reads the catalog from the compiled extension, not from the file, so a stale build would make the red tests of the next tasks meaningless. Run the **Rebuild** block of the Conventions.

Expected: `Installed physure-0.2.4` and a `_core.pyd` path inside the worktree.

- [ ] **Step 4: Write `plan_block.py`, which extracts a fenced block from this plan**

```bash
cat > /c/Users/Irvin/AppData/Local/Temp/plan_block.py <<'EOF'
"""Writes the fenced block that follows an anchor line of the plan to a file.

Usage: python -X utf8 plan_block.py "<anchor line>" <language> <destination>
"""
import pathlib
import re
import sys

PLAN = pathlib.Path(
    r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-catalog-exactness.md"
)
FENCE = chr(96) * 3
anchor, lang, dest = sys.argv[1:4]
text = PLAN.read_text(encoding="utf-8").replace("\r\n", "\n")
m = re.search(
    re.escape(anchor) + r"\n\n" + FENCE + lang + r"\n(.*?)\n" + FENCE + r"\n",
    text,
    re.S,
)
assert m, f"block not found after: {anchor}"
out = pathlib.Path(dest)
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(m.group(1) + "\n", encoding="utf-8", newline="\n")
print("written", dest, "|", m.group(1).count("\n") + 1, "lines")
EOF
```

Expected: no output.

- [ ] **Step 5: Extract `apply_catalog.py` (defined at the end of this plan)**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `apply_catalog.py` with exactly this content:' python /c/Users/Irvin/AppData/Local/Temp/apply_catalog.py
```

Expected: `written ... | 341 lines`. An `AssertionError: block not found` means the plan file is not at the path inside `plan_block.py`: stop and report.

---

### Task 2: CODATA 2022 at full precision

**Files:**
- Create: `physure-python/tests/measurement_tests/test_catalog_constants.py`
- Modify: `physure-python/scripts/generate_constants_config.py`
- Modify: `physure-core/src/units/physure.conf` (`[Constants]`, and five atomic and Planck units of `[Units]`)
- Modify: `physure-python/tests/measurement_tests/test_constants_codata.py`
- Modify: `physure-script/tests/unit_literal_regression.rs`
- Modify: `physure-python/tests/core/test_unit_algebra_parity.py` (`KNOWN_LAST_BIT`)
- Modify: `CHANGELOG.md`
- Regenerated: the generated files

- [ ] **Step 1: Move the tests first and watch them fail**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/measurement_tests/test_catalog_constants.py` with exactly this content:' python physure-python/tests/measurement_tests/test_catalog_constants.py
unset VIRTUAL_ENV
uv run --no-sync --project physure-python python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_catalog.py constants_tests
cd physure-python
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/measurement_tests/test_catalog_constants.py tests/measurement_tests/test_constants_codata.py -q -p no:cacheprovider 2>&1 | tail -1
```

The test file written by the first command is:

Create `physure-python/tests/measurement_tests/test_catalog_constants.py` with exactly this content:

```python
"""[Constants] is CODATA 2022 at full precision: what scipy.constants says.

The table used to be written with `f"{value:e}"`, which keeps 7 significant
digits, so 189 of its 411 values were wrong in the 7th digit. The generator
now writes `repr`, and this test compares the stored values with what the
generator produces from scipy.constants.
"""

import importlib.util
import pathlib
import re

import pytest

from physure._core import default_conf

scipy_constants = pytest.importorskip("scipy.constants")
if scipy_constants.physical_constants["electron mass"][0] != 9.1093837139e-31:
    pytest.skip("scipy.constants is not CODATA 2022", allow_module_level=True)

GENERATOR = (
    pathlib.Path(__file__).resolve().parents[2]
    / "scripts"
    / "generate_constants_config.py"
)


def _values(section):
    return {
        m.group(1): float(m.group(2))
        for m in re.finditer(r"^(\w+) = (\S+) ", section, re.M)
    }


def test_every_constant_is_the_codata_2022_value_at_full_precision():
    spec = importlib.util.spec_from_file_location("generator", GENERATOR)
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)
    generated = _values(generator.generate_constants_section())
    stored = _values(
        default_conf().split("[Constants]")[1].split("[Categories]")[0]
    )
    assert len(stored) == len(generated) > 400
    assert stored == generated
```

Expected: `written ... | 44 lines`; `edited` lines for `test_constants_codata.py` and `unit_literal_regression.rs`; ruff clean; `15 failed, 33 passed` (the new catalog test and 14 of the CODATA checks, which now ask for the 2022 values at 1e-12). If the result is different, stop and report.

- [ ] **Step 2: Apply the `constants` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
unset VIRTUAL_ENV
uv run --no-sync --project physure-python python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_catalog.py constants
git diff --stat | tail -8
```

Expected: `edited` lines for the generator, `physure.conf` (twice: the `[Constants]` section, then five factors of `[Units]`) and `CHANGELOG.md`. `git diff --stat` lists the conf with about 400 changed lines (the 213 constants that change, the header, five units). Any `AssertionError` means an anchor changed: stop and report.

- [ ] **Step 3: Rebuild and regenerate** (see Conventions)

Expected: `Installed physure-0.2.4`, a `_core.pyd` path inside the worktree, then the three generator lines.

- [ ] **Step 4: Check**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/measurement_tests/test_catalog_constants.py tests/measurement_tests/test_constants_codata.py -q -p no:cacheprovider 2>&1 | tail -1
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
cargo test -p physure-script --test unit_literal_regression 2>&1 | grep -E "^test result"
```

Expected: ruff clean; `48 passed`; `941 passed, 6 xfailed` (the 940 of `main` plus the new constants test); `test result: ok. 39 passed`.

- [ ] **Step 5: Regenerate once more, then commit**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 scripts/compile_units.py 2>&1 | tail -1
uv run --no-sync python -X utf8 scripts/generate_units_readme.py 2>&1 | tail -1
uv run --no-sync python -X utf8 -m physure.cli sync-types 2>&1 | tail -1
cd ..
git add physure-core/src/units/physure.conf physure-python/scripts/generate_constants_config.py physure-python/tests/measurement_tests/test_catalog_constants.py physure-python/tests/measurement_tests/test_constants_codata.py physure-python/tests/core/test_unit_algebra_parity.py physure-script/tests/unit_literal_regression.rs CHANGELOG.md
git add -u -- docs/UNITS.md physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi physure-python/physure/units/_index.py physure-python/physure/units/core.py
git diff --cached --name-status
git commit -m "fix(units): the constants table keeps every digit, in one CODATA edition" -m "generate_constants_config.py wrote every constant above 1e4 or below 1e-4 with f'{val:e}', which keeps 7 significant digits: 189 of the 411 constants were wrong in the seventh digit (eps0 * mu0 * c^2 was off by 3e-8), and the atomic and Planck units of [Units] were CODATA 2018 while the table said 2022. The generator now writes repr and refuses any scipy that is not CODATA 2022; [Constants] is regenerated and the five affected units follow. The constants test compares with CODATA 2022 at 1e-12 and the derived relations at 1e-10 or better, where they were 1e-6. The generated types lose mock_unit, a test artifact that had leaked into them." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: 

```
M	CHANGELOG.md
M	docs/UNITS.md
M	physure-core/src/units/physure.conf
M	physure-python/physure/_generated_types.py
M	physure-python/physure/core/registry.pyi
M	physure-python/physure/units/__init__.pyi
M	physure-python/scripts/generate_constants_config.py
M	physure-python/tests/core/test_unit_algebra_parity.py
A	physure-python/tests/measurement_tests/test_catalog_constants.py
M	physure-python/tests/measurement_tests/test_constants_codata.py
M	physure-script/tests/unit_literal_regression.rs
```

Nothing else.

---

### Task 3: The exact value of each definition

**Files:**
- Create: `physure-python/tests/measurement_tests/test_catalog_exact.py`
- Modify: `physure-core/src/units/physure.conf` (17 factors of `[Units]`)
- Modify: `physure-core/src/units/tests.rs`
- Modify: `physure-script/tests/unit_literal_regression.rs`
- Modify: `physure-python/tests/measurement_tests/test_conversions_realworld.py`
- Modify: `physure-python/tests/core/test_unit_algebra_parity.py` (`KNOWN_LAST_BIT`)
- Modify: `CHANGELOG.md`
- Regenerated: the generated files

- [ ] **Step 1: Write the test and watch it fail**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/measurement_tests/test_catalog_exact.py` with exactly this content:' python physure-python/tests/measurement_tests/test_catalog_exact.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/measurement_tests/test_catalog_exact.py && uv run --no-sync ruff format --check tests/measurement_tests/test_catalog_exact.py
uv run --no-sync pytest tests/measurement_tests/test_catalog_exact.py -q -p no:cacheprovider 2>&1 | tail -1
```

The block written by the first command is:

Create `physure-python/tests/measurement_tests/test_catalog_exact.py` with exactly this content:

```python
"""The numeric factors of physure.conf are the exact value of their definition.

A factor typed with a few digits is a silent error: `1 deg => arcmin` used to
be 60.000043 and `1 atm => Torr` 760.0021. Each reference below is computed
from the definition (exact fractions, or pi where the definition has pi), not
copied from the catalog. The tolerance is a few units in the last place of an
f64; the exact rational scales planned for the unit algebra will remove even
that.
"""

import math
from fractions import Fraction

import pytest

from physure import Q_

LB = Fraction("0.45359237")
G0 = Fraction("9.80665")
FT = Fraction("0.3048")
IN = Fraction("0.0254")
C = 299792458
LBF = LB * G0

# (unit as a user writes it, the SI unit it converts to, exact value)
REFERENCE = [
    ("ly", "m", C * Fraction("365.25") * 86400),
    ("pc", "m", 648000 / math.pi * 149597870700),
    ("slug", "kg", LBF / FT),
    ("acre", "m^2", 43560 * FT**2),
    ("gal", "m^3", 231 * IN**3),
    ("fl_oz", "m^3", 231 * IN**3 / 128),
    ("lbf", "N", LBF),
    ("BTU", "J", Fraction("1055.05585262")),
    ("hp", "W", 550 * FT * LBF),
    ("psi", "Pa", LBF / IN**2),
    ("Torr", "Pa", Fraction(101325, 760)),
    ("mmHg", "Pa", Fraction("13595.1") * G0 / 1000),
    ("deg", "rad", math.pi / 180),
    ("arcmin", "rad", math.pi / 10800),
    ("arcsec", "rad", math.pi / 648000),
    ("Fr", "C", Fraction(1, 10 * C)),
    ("D", "C*m", Fraction(1, 10**21 * C)),
    # CODATA 2022
    ("a0", "m", 5.29177210544e-11),
    ("Eh", "J", 4.359744722206e-18),
    ("me", "kg", 9.1093837139e-31),
    ("tau0", "s", 2.4188843265864e-17),
    ("qp", "C", 1.8755460384151476e-18),
]


@pytest.mark.parametrize(
    ("unit", "si", "exact"), REFERENCE, ids=[r[0] for r in REFERENCE]
)
def test_the_factor_is_the_exact_value_of_its_definition(unit, si, exact):
    got = Q_(1, unit).to(si).magnitude
    assert math.isclose(got, float(exact), rel_tol=1e-15), (unit, got)


# (value, from, to, exact result): relations that hold by definition
IDENTITIES = [
    (1, "deg", "arcmin", 60),
    (1, "arcmin", "arcsec", 60),
    (1, "deg", "arcsec", 3600),
    (180, "deg", "rad", math.pi),
    (1, "mile", "ft", 5280),
    (1, "ft", "in", 12),
    (1, "mile", "yd", 1760),
    (1, "lb", "oz", 16),
    (1, "gal", "fl_oz", 128),
    (1, "gal", "in^3", 231),
    (1, "acre", "ft^2", 43560),
    (1, "atm", "Pa", 101325),
    (1, "atm", "Torr", 760),
    (1, "psi", "lbf/in^2", 1),
    (1, "hp", "ft*lbf/s", 550),
    (1, "slug*ft/s^2", "lbf", 1),
    (1, "pc", "ly", 648000 / math.pi * 149597870700 / (C * 365.25 * 86400)),
    (1, "d", "h", 24),
    (1, "yr", "d", 365.25),
    (212, "degF", "degC", 100),
    (32, "degF", "degC", 0),
]


@pytest.mark.parametrize(
    ("value", "src", "dst", "exact"),
    IDENTITIES,
    ids=[f"{i[0]} {i[1]} in {i[2]}" for i in IDENTITIES],
)
def test_a_conversion_that_is_exact_by_definition_comes_out_exact(
    value, src, dst, exact
):
    got = Q_(value, src).to(dst).magnitude
    assert math.isclose(got, exact, rel_tol=2e-15, abs_tol=1e-12), got
```

Expected: `written ... | 96 lines`; ruff clean; `29 failed, 14 passed` (the references and identities that the old factors break)

- [ ] **Step 2: Apply the `units` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
unset VIRTUAL_ENV
uv run --no-sync --project physure-python python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_catalog.py units
```

Expected: `edited` lines for `physure.conf`, `tests.rs`, `unit_literal_regression.rs`, `test_conversions_realworld.py`, `test_unit_algebra_parity.py` and `CHANGELOG.md`.

- [ ] **Step 3: Rebuild and regenerate** (see Conventions)

- [ ] **Step 4: Check**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/measurement_tests/test_catalog_exact.py tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider 2>&1 | tail -1
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
cargo test -p physure 2>&1 | grep -E "^test result"
cargo test -p physure-script 2>&1 | grep -E "^test result" | head -8
```

Expected: ruff clean; `49 passed, 5 xfailed` (the 43 exactness tests, the 6 parity tests that pass and the 5 that stay strict xfail); `984 passed, 6 xfailed`; the core crate prints three `test result: ok` lines (`184, 10 and 3 passed`) and the language crate only `ok` lines, none failed.

- [ ] **Step 5: Regenerate once more, then commit**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 scripts/compile_units.py 2>&1 | tail -1
uv run --no-sync python -X utf8 scripts/generate_units_readme.py 2>&1 | tail -1
uv run --no-sync python -X utf8 -m physure.cli sync-types 2>&1 | tail -1
cd ..
git add physure-core/src/units/physure.conf physure-core/src/units/tests.rs physure-script/tests/unit_literal_regression.rs physure-python/tests/measurement_tests/test_catalog_exact.py physure-python/tests/measurement_tests/test_conversions_realworld.py physure-python/tests/core/test_unit_algebra_parity.py CHANGELOG.md
git add -u -- docs/UNITS.md physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi physure-python/physure/units/_index.py physure-python/physure/units/core.py
git diff --cached --name-status
git commit -m "fix(units): factors typed with a few digits are the exact value of their definition" -m "degree, arcminute, arcsecond, light_year, parsec, slug, acre, gallon_us, fluid_ounce, pound_force, btu, horsepower, psi, torr, mmHg, franklin and debye carried 5 to 9 digits (up to 7e-6 off): 1 deg was 60.000043 arcmin, 1 atm was 760.0021 Torr, 1 gal was 230.99989 in^3, 1 pc was 3.26160 ly. Each factor is now the nearest f64 of the exact value; btu is the International Table BTU, torr is atm/760 and mmHg the conventional millimetre of mercury. New tests compare every changed factor with its definition (exact fractions, pi where the definition has it) and check 21 conversions that are exact by definition, in Python and in the Rust registry. The last-bit list of the parity test is updated." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: 

```
M	CHANGELOG.md
M	docs/UNITS.md
M	physure-core/src/units/physure.conf
M	physure-core/src/units/tests.rs
M	physure-python/tests/core/test_unit_algebra_parity.py
A	physure-python/tests/measurement_tests/test_catalog_exact.py
M	physure-python/tests/measurement_tests/test_conversions_realworld.py
M	physure-script/tests/unit_literal_regression.rs
```

Nothing else.

---

### Task 4: One name per definition

**Files:**
- Create: `physure-python/tests/measurement_tests/test_catalog_variants.py`
- Create: `physure-python/tests/core/test_catalog_namespace.py`
- Modify: `physure-core/src/units/physure.conf`
- Modify: `physure-python/tests/core/test_unit_algebra_parity.py` (`KNOWN_LAST_BIT`)
- Modify: `CHANGELOG.md`
- Regenerated: the generated files

- [ ] **Step 1: Write the tests and watch them fail**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/measurement_tests/test_catalog_variants.py` with exactly this content:' python physure-python/tests/measurement_tests/test_catalog_variants.py
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/core/test_catalog_namespace.py` with exactly this content:' python physure-python/tests/core/test_catalog_namespace.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/measurement_tests/test_catalog_variants.py tests/core/test_catalog_namespace.py -q -p no:cacheprovider 2>&1 | tail -1
```

The two blocks written by the first two commands are:

Create `physure-python/tests/measurement_tests/test_catalog_variants.py` with exactly this content:

```python
"""An ambiguous unit has one name per definition; the short symbol is the default.

`cal`, `BTU`, `acre`, `gal`, `fl_oz`, `hp` and `yr` each have several
definitions in use. The short symbol names the default one, and every
definition, the default included, can be written with a qualified name that
nothing else can take.
"""

import math
from fractions import Fraction

import pytest

from physure import Q_

G0 = Fraction("9.80665")
FT = Fraction("0.3048")
US_FT = Fraction(1200, 3937)

# (qualified name, the SI unit it converts to, exact value)
VARIANTS = [
    ("cal_th", "J", Fraction("4.184")),
    ("cal_it", "J", Fraction("4.1868")),
    ("btu_it", "J", Fraction("1055.05585262")),
    ("btu_th", "J", Fraction("453.59237") * 5 * Fraction("4.184") / 9),
    ("btu_iso", "J", Fraction("1055.056")),
    ("acre_international", "m^2", 43560 * FT**2),
    ("acre_us_survey", "m^2", 43560 * US_FT**2),
    ("gal_us", "m^3", Fraction("0.003785411784")),
    ("gallon_imperial", "m^3", Fraction("0.00454609")),
    ("fl_oz_us", "m^3", Fraction("0.003785411784") / 128),
    ("fluid_ounce_imperial", "m^3", Fraction("0.00454609") / 160),
    ("hp_mechanical", "W", 550 * FT * Fraction("0.45359237") * G0),
    ("hp_metric", "W", 75 * G0),
    ("hp_electric", "W", 746),
    ("year_julian", "s", Fraction("365.25") * 86400),
    ("year_gregorian", "s", Fraction("365.2425") * 86400),
]


@pytest.mark.parametrize(
    ("name", "si", "exact"), VARIANTS, ids=[v[0] for v in VARIANTS]
)
def test_each_definition_has_its_exact_value_under_its_own_name(
    name, si, exact
):
    got = Q_(1, name).to(si).magnitude
    assert math.isclose(got, float(exact), rel_tol=1e-15), (name, got)


# (short symbol, the qualified name it stands for, an SI unit to compare in)
DEFAULTS = [
    ("cal", "cal_th", "J"),
    ("calorie", "cal_th", "J"),
    ("BTU", "btu_it", "J"),
    ("btu", "btu_it", "J"),
    ("acre", "acre_international", "m^2"),
    ("gal", "gal_us", "m^3"),
    ("gallon", "gal_us", "m^3"),
    ("fl_oz", "fl_oz_us", "m^3"),
    ("hp", "hp_mechanical", "W"),
    ("yr", "year_julian", "s"),
    ("year", "year_julian", "s"),
]


@pytest.mark.parametrize(
    ("short", "qualified", "si"), DEFAULTS, ids=[d[0] for d in DEFAULTS]
)
def test_a_short_symbol_is_the_documented_default(short, qualified, si):
    assert Q_(1, short).to(si).magnitude == Q_(1, qualified).to(si).magnitude


@pytest.mark.parametrize(
    ("one", "other", "si"),
    [
        ("cal_th", "cal_it", "J"),
        ("btu_it", "btu_th", "J"),
        ("btu_it", "btu_iso", "J"),
        ("acre_international", "acre_us_survey", "m^2"),
        ("gal_us", "gallon_imperial", "m^3"),
        ("hp_mechanical", "hp_metric", "W"),
        ("year_julian", "year_gregorian", "s"),
    ],
)
def test_the_definitions_really_differ(one, other, si):
    a = Q_(1, one).to(si).magnitude
    b = Q_(1, other).to(si).magnitude
    assert not math.isclose(a, b, rel_tol=1e-9)
```

Create `physure-python/tests/core/test_catalog_namespace.py` with exactly this content:

```python
"""Every name in physure.conf means one unit; the shadowed readings are listed.

The registry only logs a warning when two units claim a name and the later one
silently wins (the `gal` gallon/galileo incident). This test reads the catalog
and fails instead.
"""

import re

from physure import Q_
from physure._core import default_conf

# A short symbol that is also a prefix followed by another unit's symbol
# (centi + d, femto + t, pico + H, yocto + d). The exact symbol wins; the other
# reading needs a long prefixed name, which the unit parser does not accept yet.
SHADOWED = {"cd", "ft", "kg", "pH", "yd"}


def _catalog():
    text = default_conf()
    prefixes = []
    for line in text.split("[Prefixes]")[1].split("[Units]")[0].splitlines():
        m = re.match(r"^(\w+)\s*=\s*(\w+),", line)
        if m:
            prefixes.append(m.group(2))
    units = []
    for line in text.split("[Units]")[1].split("[Constants]")[0].splitlines():
        m = re.match(r"^(\w+)\s*=\s*(.*)$", line)
        alias_list = m and re.search(r"\[([^\]]*)\]\s*$", m.group(2))
        if alias_list:
            aliases = {a.strip() for a in alias_list.group(1).split(",")}
            units.append(
                (m.group(1), aliases | {m.group(1)}, "noprefix" in m.group(2))
            )
    return prefixes, units


def test_no_name_is_claimed_by_two_units():
    _, units = _catalog()
    owners = {}
    clashes = []
    for name, names, _ in units:
        for symbol in names:
            if symbol in owners and owners[symbol] != name:
                clashes.append((symbol, owners[symbol], name))
            owners[symbol] = name
    assert not clashes, f"names claimed by two units: {clashes}"


def test_the_shadowed_readings_are_exactly_the_known_ones():
    prefixes, units = _catalog()
    exact = {symbol for _, names, _ in units for symbol in names}
    readings = {}
    for name, names, noprefix in units:
        if noprefix:
            continue
        for symbol in names:
            for prefix in prefixes:
                readings.setdefault(prefix + symbol, set()).add(name)
    shadowed = {s for s in exact if s in readings}
    assert shadowed == SHADOWED
    ambiguous = {
        s: u for s, u in readings.items() if s not in exact and len(u) > 1
    }
    assert not ambiguous, f"prefix readings that collide: {ambiguous}"


def test_every_unit_can_be_written_with_its_own_name():
    _, units = _catalog()
    unresolved = []
    for name, _, _ in units:
        try:
            Q_(1, name)
        except Exception:
            unresolved.append(name)
    assert not unresolved, f"names that do not resolve: {unresolved}"
```

Expected: two `written` lines (`89` and `76` lines); ruff clean; `34 failed, 3 passed` (the 34 variant tests fail; the 3 namespace tests already pass, because they are a guard on the catalog, not a feature)

- [ ] **Step 2: Apply the `variants` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
unset VIRTUAL_ENV
uv run --no-sync --project physure-python python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_catalog.py variants
```

Expected: `edited` lines for `physure.conf`, `test_unit_algebra_parity.py` and `CHANGELOG.md`.

- [ ] **Step 3: Rebuild and regenerate** (see Conventions), then confirm that nothing is redefined at bootstrap and that the new names resolve:

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 -c "
import logging; logging.basicConfig(level=logging.WARNING)
from physure import Q_
for u in ['cal_it', 'btu_th', 'acre_us_survey', 'gal_uk', 'hp_metric', 'year_gregorian', 'fl_oz_uk']:
    print(u, Q_(1, u))
"
```

Expected: seven lines `<name> 1.0 <name>` and no `is being redefined` warning. A warning means a name collides: stop and report it.

- [ ] **Step 4: Check**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/measurement_tests/test_catalog_variants.py tests/core/test_catalog_namespace.py tests/measurement_tests/test_catalog_exact.py tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider 2>&1 | tail -1
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
cargo test -p physure 2>&1 | grep -E "^test result"
```

Expected: ruff clean; `86 passed, 5 xfailed`; `1021 passed, 6 xfailed`; three `test result: ok` lines for the core crate.

- [ ] **Step 5: Regenerate once more, then commit**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 scripts/compile_units.py 2>&1 | tail -1
uv run --no-sync python -X utf8 scripts/generate_units_readme.py 2>&1 | tail -1
uv run --no-sync python -X utf8 -m physure.cli sync-types 2>&1 | tail -1
cd ..
git add physure-core/src/units/physure.conf physure-python/tests/core/test_catalog_namespace.py physure-python/tests/measurement_tests/test_catalog_variants.py physure-python/tests/core/test_unit_algebra_parity.py CHANGELOG.md
git add -u -- docs/UNITS.md physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi physure-python/physure/units/_index.py physure-python/physure/units/core.py
git diff --cached --name-status
git commit -m "feat(units): one name per definition, and a test that no name means two units" -m "cal, BTU, acre, gal, fl_oz, hp and yr each have several definitions in use. Each definition now has its own entry under a qualified name (cal_it, btu_th, btu_iso, acre_us_survey, gallon_imperial, fluid_ounce_imperial, hp_metric, hp_electric, year_gregorian) and the default also has one (cal_th, btu_it, acre_international, gal_us, fl_oz_us, hp_mechanical, year_julian); the short symbol keeps naming the default, which the catalog states in a comment above each group. A new test reads the catalog and fails if two units claim a name, if a short symbol is shadowed by a prefix reading other than the five known ones, or if an entry name does not resolve." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: 

```
M	CHANGELOG.md
M	docs/UNITS.md
M	physure-core/src/units/physure.conf
M	physure-python/physure/_generated_types.py
M	physure-python/physure/core/registry.pyi
M	physure-python/physure/units/__init__.pyi
M	physure-python/physure/units/_index.py
M	physure-python/physure/units/core.py
A	physure-python/tests/core/test_catalog_namespace.py
M	physure-python/tests/core/test_unit_algebra_parity.py
A	physure-python/tests/measurement_tests/test_catalog_variants.py
```

Nothing else.

---

### Task 5: Verify the whole project

**Files:** none

- [ ] **Step 1: Lint, format, and the strict docs build**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync mkdocs build --strict 2>&1 | tail -1
```

Expected: `All checks passed!` and every file formatted; `INFO    -  Documentation built in ... seconds`.

- [ ] **Step 2: The full suites, as CI runs them**

```bash
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
cargo test -p physure 2>&1 | grep -E "^test result"
cargo test -p physure-script 2>&1 | grep -E "^test result"
```

Expected: **1021 passed, 6 xfailed**, in about 3 minutes; the core crate three `ok` lines; the language crate only `ok` lines. If any other test fails, run it alone and report; do not change anything else.

- [ ] **Step 3: The generated files are up to date and the first use is still fast**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 scripts/compile_units.py 2>&1 | tail -1
uv run --no-sync python -X utf8 scripts/generate_units_readme.py 2>&1 | tail -1
uv run --no-sync python -X utf8 -m physure.cli sync-types 2>&1 | tail -1
cd ..
git status --short | grep -v Cargo.lock
uv run --no-sync --project physure-python python -m physure "500 N / 2 m^2 => kPa" > /dev/null
time uv run --no-sync --project physure-python python -m physure "500 N / 2 m^2 => kPa"
```

Expected: the `git status` prints nothing (regenerating changes no file); `0.25 kPa` in about 0.1 to 0.2 s on the second run (0.3 s at most when the machine is busy; the first run compiles the regenerated modules).

- [ ] **Step 4: The audit, re-run on the fixed catalog**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_units_and_constants.py 2>&1 | sed -n 4,8p
uv run --no-sync python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_units_and_constants.py 2>&1 | grep -E "^[0-9]+ constants"
uv run --no-sync python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_identities.py 2>&1 | grep -E "identities off|eps0|sigma|alpha"
cd ..
git status --short | grep -v Cargo.lock
```

Expected: `checked 69 of 105 entries; exact within f64: 69; off: 0`, then `411 constants. Full precision: 411. Cut to 7 significant digits: 0. Other differences: 0. Not in scipy: 0`, then `0 of 27 identities off by more than 4e-16`, and `eps0 * mu0 * c^2 - 1` of about 1e-12; the last `git status` prints nothing (the two audit scripts are tracked in `docs/superpowers/specs/catalog-audit/` since the audit was merged, and running them changes nothing).

- [ ] **Step 5: Confirm the branch**

```bash
git log --oneline -5
```

Expected: three new commits on `fix/catalog-exactness` on top of `main`.

- [ ] **Step 6: Hand-off**

Report the branch name, the three commit SHAs and the measured numbers (they should match the table above). Nothing is pushed. Integrating the branch is the maintainer's decision. The Stage 0 parity test lists symbols whose Python and Rust results differ in the last bit; that list shrinks to nothing when A1 stage 1 gives the core exact rational scales.

---

## Reference: the script this plan embeds

Create `apply_catalog.py` with exactly this content:

```python
"""Catalog fix: CODATA 2022 at full precision, exact unit factors, one name per definition.

Usage (from the repository root of the worktree):
    python -X utf8 apply_catalog.py constants_tests | constants | units | variants

Every replacement asserts that its anchor occurs exactly as many times as
expected, and every file keeps its own line endings.
"""

import importlib.util
import pathlib
import re
import sys

CONF = pathlib.Path("physure-core/src/units/physure.conf")
GENERATOR = pathlib.Path("physure-python/scripts/generate_constants_config.py")
TESTS = pathlib.Path("physure-python/tests")
CHANGELOG = pathlib.Path("CHANGELOG.md")


def load(path):
    raw = path.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    return raw.replace(eol, "\n"), eol


def save(path, text, eol):
    path.write_bytes(text.replace("\n", eol).encode("utf-8"))
    print("edited", path)


def edit(path, replacements):
    text, eol = load(path)
    for old, new, count in replacements:
        assert text.count(old) == count, (
            f"{path}: {old[:70]!r} found {text.count(old)}, expected {count}"
        )
        text = text.replace(old, new)
    save(path, text, eol)


def set_factors(factors):
    """Replaces the leading factor of [Units] entries: name -> literal."""
    text, eol = load(CONF)
    head, units_and_rest = text.split("\n[Units]\n", 1)
    units, rest = units_and_rest.split("\n[Constants]\n", 1)
    for name, literal in factors.items():
        pattern = re.compile(
            r"^(" + re.escape(name) + r"\s*=\s*)[-+0-9.eE]+(,)", re.M
        )
        units, n = pattern.subn(lambda m: m.group(1) + literal + m.group(2), units)
        assert n == 1, (name, n)
    save(CONF, head + "\n[Units]\n" + units + "\n[Constants]\n" + rest, eol)


def set_last_bit(symbols):
    """Rewrites KNOWN_LAST_BIT of the unit parity test (sorted, as the test compares)."""
    path = TESTS / "core/test_unit_algebra_parity.py"
    text, eol = load(path)
    body = "".join(f'    "{s}",\n' for s in symbols)
    pattern = re.compile(r'KNOWN_LAST_BIT = \(\n(?:    "[^"]+",\n)+\)\n')
    text, n = pattern.subn(lambda m: "KNOWN_LAST_BIT = (\n" + body + ")\n", text)
    assert n == 1
    save(path, text, eol)


def changelog(section, bullet):
    """Adds a bullet at the end of a section of [Unreleased]."""
    if section == "Changed":
        old = "\n\n### Fixed\n- **`SymbolicQuantity`,"
        new = "\n" + bullet + "\n\n### Fixed\n- **`SymbolicQuantity`,"
    elif section == "Fixed":
        old = "\n\n## [0.2.4] - 2026-08-28\n"
        new = "\n" + bullet + "\n\n## [0.2.4] - 2026-08-28\n"
    else:
        raise SystemExit(section)
    edit(CHANGELOG, [(old, new, 1)])


part = sys.argv[1]

if part == "constants_tests":
    # the tests move first, so that they fail for the right reason
    edit(TESTS / "measurement_tests/test_constants_codata.py", [
        ("(CODATA 2018 recommended values -- the vintage the [Units] \"ATOMIC UNITS\"\n"
         "section of physure.conf already matches exactly, e.g. bohr = 5.29177210903e-11 m,\n"
         "electron_mass = 9.1093837015e-31 kg). The 2019 SI redefinition fixes h, e,\n",
         "(CODATA 2022 recommended values, the edition of the whole catalog: the\n"
         "[Units] \"ATOMIC UNITS\" section and [Constants] agree to the last digit,\n"
         "e.g. bohr = 5.29177210544e-11 m). The 2019 SI redefinition fixes h, e,\n", 1),
        ("# name -> (CODATA-2018 published value, relative tolerance)\n"
         "MEASURED_CONSTANTS = [\n"
         '    ("vacuum_electric_permittivity", 8.8541878128e-12, 1e-6),\n'
         '    ("vacuum_mag_permeability", 1.25663706212e-6, 1e-6),\n'
         '    ("fine_structure_constant", 7.2973525693e-3, 1e-6),\n'
         '    ("bohr_radius", 5.29177210903e-11, 1e-6),\n'
         '    ("rydberg_constant", 10973731.568160, 1e-6),\n'
         '    ("bohr_magneton", 9.2740100783e-24, 1e-6),\n'
         '    ("electron_mass", 9.1093837015e-31, 1e-6),\n'
         '    ("proton_mass", 1.67262192369e-27, 1e-6),\n'
         '    ("neutron_mass", 1.67492749804e-27, 1e-6),\n'
         '    ("atomic_mass_constant", 1.66053906660e-27, 1e-6),\n'
         "    (\n"
         '        "newtonian_constant_of_gravitation",\n'
         "        6.67430e-11,\n"
         "        1e-4,\n"
         "    ),  # G: only ~5 sig figs known\n"
         '    ("stefan_boltzmann_constant", 5.670374419e-8, 1e-6),\n'
         "]\n",
         "# name -> (CODATA-2022 published value, relative tolerance). The table keeps\n"
         "# every digit that scipy.constants has, so the tolerance only absorbs\n"
         "# float rounding.\n"
         "MEASURED_CONSTANTS = [\n"
         '    ("vacuum_electric_permittivity", 8.8541878188e-12, 1e-12),\n'
         '    ("vacuum_mag_permeability", 1.25663706127e-6, 1e-12),\n'
         '    ("fine_structure_constant", 7.2973525643e-3, 1e-12),\n'
         '    ("bohr_radius", 5.29177210544e-11, 1e-12),\n'
         '    ("rydberg_constant", 10973731.568157, 1e-12),\n'
         '    ("bohr_magneton", 9.2740100657e-24, 1e-12),\n'
         '    ("electron_mass", 9.1093837139e-31, 1e-12),\n'
         '    ("proton_mass", 1.67262192595e-27, 1e-12),\n'
         '    ("neutron_mass", 1.67492750056e-27, 1e-12),\n'
         '    ("atomic_mass_constant", 1.66053906892e-27, 1e-12),\n'
         '    ("newtonian_constant_of_gravitation", 6.6743e-11, 1e-12),\n'
         '    ("stefan_boltzmann_constant", 5.6703744191844314e-8, 1e-12),\n'
         "]\n", 1),
        ("    assert math.isclose(f.magnitude, n_a.magnitude * e.magnitude, rel_tol=1e-6)\n",
         "    assert math.isclose(\n        f.magnitude, n_a.magnitude * e.magnitude, rel_tol=1e-12\n    )\n", 1),
        ("    assert math.isclose(sigma.magnitude, derived, rel_tol=1e-6)\n",
         "    assert math.isclose(sigma.magnitude, derived, rel_tol=1e-12)\n", 1),
        ("    assert math.isclose(mu_b.magnitude, derived, rel_tol=1e-6)\n",
         "    assert math.isclose(mu_b.magnitude, derived, rel_tol=1e-10)\n", 1),
        ("    assert math.isclose(c.magnitude, derived, rel_tol=1e-6)\n",
         "    assert math.isclose(c.magnitude, derived, rel_tol=1e-10)\n", 1),
        ("    assert math.isclose(r_inf.magnitude, derived, rel_tol=1e-6)\n",
         "    assert math.isclose(r_inf.magnitude, derived, rel_tol=1e-10)\n", 1),
    ])
    edit(pathlib.Path("physure-script/tests/unit_literal_regression.rs"), [
        ('("1 a0", 5.29177210903e-11, "a0")', '("1 a0", 5.29177210544e-11, "a0")', 1),
    ])

elif part == "constants":
    # the generator wrote `f"{val:e}"`, which keeps 7 significant digits
    edit(GENERATOR, [
        ('        val_str = f"{val:e}" if abs(val) < 1e-4 or abs(val) > 1e4 else f"{val}"\n',
         "        val_str = repr(float(val))\n", 1),
        ("def generate_constants_section():\n    lines = []\n",
         "# scipy.constants carries the CODATA edition of its release. This table is\n"
         "# CODATA 2022 (scipy 1.15 or later): refuse to write any other edition.\n"
         "CODATA_2022_ELECTRON_MASS = 9.1093837139e-31\n\n\n"
         "def generate_constants_section():\n"
         '    electron_mass = scipy.constants.physical_constants["electron mass"][0]\n'
         "    if electron_mass != CODATA_2022_ELECTRON_MASS:\n"
         '        raise SystemExit("scipy.constants is not CODATA 2022")\n'
         "    lines = []\n", 1),
        ('    lines.append(\n        "# Fundamental Physical Constants (CODATA 2022 generated from scipy)"\n    )\n',
         "    lines.append(\n"
         '        "# Fundamental Physical Constants (CODATA 2022, scipy "\n'
         '        f"{scipy.__version__}), generated by "\n'
         '        "physure-python/scripts/generate_constants_config.py: do not edit by hand"\n'
         "    )\n", 1),
    ])
    spec = importlib.util.spec_from_file_location("generator", GENERATOR)
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)
    section = generator.generate_constants_section()
    text, eol = load(CONF)
    head, rest = text.split("\n[Constants]\n", 1)
    old_section, tail = rest.split("\n[Categories]\n", 1)
    assert section.startswith("[Constants]\n")
    new_section = section[len("[Constants]\n"):]
    save(CONF, head + "\n[Constants]\n" + new_section + "\n\n[Categories]\n" + tail, eol)
    # the atomic and Planck units of [Units] are CODATA 2018 today
    set_factors({
        "bohr": "5.29177210544e-11",
        "hartree": "4.359744722206e-18",
        "electron_mass": "9.1093837139e-31",
        "atomic_time": "2.4188843265864e-17",
        "planck_charge": "1.8755460384151476e-18",
    })
    set_last_bit((
        "C", "Gb", "a0", "acre", "acres", "atomic_time", "bohr", "bohr_radius",
        "celsius", "centigrade", "coulomb", "coulombs", "degC", "degF", "dyn",
        "dyne", "electron_mass", "fahrenheit", "gal", "gallon", "gallon_us",
        "gallons", "gilbert", "ha", "hectare", "hectares", "inch", "lb", "lbf",
        "lbs", "me", "mp", "ounce", "ounces", "oz", "planck_mass", "pound",
        "pound_force", "pounds", "ppm", "psi", "rad", "radian", "radians", "tau0",
    ))
    changelog(
        "Fixed",
        "- **The constants table keeps every digit.** `[Constants]` was written with `f\"{value:e}\"`, which keeps 7 significant digits, so 189 of its 411 values (`eps0`, `mu0`, `sigma`, `m_p`, `m_n`, `u`, `a0`, `mu_B`, `Ryd`, ...) were wrong in the seventh digit and `eps0 * mu0 * c^2` was off by 3e-8. The table is now regenerated with `repr` from CODATA 2022 (scipy 1.15 or later), and the atomic and Planck units of `[Units]`, which were CODATA 2018, follow: `electron_mass` is `9.1093837139e-31` kg and `bohr` `5.29177210544e-11` m.",
    )

elif part == "units":
    # factors that were typed with a few digits: exact value of the definition
    set_factors({
        "light_year": "9460730472580800.0",
        "parsec": "3.085677581491367e+16",
        "slug": "14.593902937206364",
        "acre": "4046.8564224",
        "gallon_us": "0.003785411784",
        "fluid_ounce": "2.95735295625e-05",
        "pound_force": "4.4482216152605",
        "btu": "1055.05585262",
        "horsepower": "745.6998715822702",
        "psi": "6894.757293168362",
        "torr": "133.32236842105263",
        "mmHg": "133.322387415",
        "degree": "0.017453292519943295",
        "arcminute": "0.0002908882086657216",
        "arcsecond": "4.84813681109536e-06",
        "franklin": "3.3356409519815207e-10",
        "debye": "3.335640951981521e-30",
    })
    edit(pathlib.Path("physure-core/src/units/tests.rs"), [
        ('    assert!((deg.scale - 0.0174532925).abs() < 1e-12, "deg carries the degrees → radians factor");\n',
         '    assert_eq!(deg.scale, std::f64::consts::PI / 180.0, "deg carries the degrees → radians factor");\n', 1),
        ("/// The temperature scales are the registry's only affine units: their conversion needs an\n",
         "/// A relation that holds by definition has to come out exact, not as 60.000043 or 760.0021:\n"
         "/// the factors in physure.conf are the exact values of their definitions.\n"
         "#[test]\n"
         "fn scale_ratios_that_are_exact_by_definition_come_out_exact() {\n"
         "    let (reg, _) = conf::build_registry_from_conf();\n"
         '    let scale = |symbol: &str| reg.get_unit(symbol).unwrap().scale;\n'
         "    let close = |got: f64, expected: f64| (got - expected).abs() <= expected.abs() * 1e-14;\n"
         '    assert!(close(scale("deg") / scale("arcmin"), 60.0));\n'
         '    assert!(close(scale("arcmin") / scale("arcsec"), 60.0));\n'
         '    assert!(close(scale("atm") / scale("Torr"), 760.0));\n'
         '    assert!(close(scale("gal") / scale("inch").powi(3), 231.0));\n'
         '    assert!(close(scale("acre") / scale("ft").powi(2), 43560.0));\n'
         '    assert!(close(scale("hp") / (scale("ft") * scale("lbf")), 550.0));\n'
         '    assert!(close(scale("psi") / (scale("lbf") / scale("inch").powi(2)), 1.0));\n'
         "}\n\n"
         "/// The temperature scales are the registry's only affine units: their conversion needs an\n", 1),
    ])
    edit(pathlib.Path("physure-script/tests/unit_literal_regression.rs"), [
        ("        // physure.conf:185 declares degree to 10 significant digits (π/180 is\n"
         "        // 0.017453292519943295); this asserts the parser preserves the registry's value.\n"
         '        ("1 deg", 0.0174532925, "deg"),\n',
         "        // The registry's degree is π/180 to the last bit; the parser has to keep it.\n"
         '        ("1 deg", std::f64::consts::PI / 180.0, "deg"),\n', 1),
        ('        ("1 °", 0.0174532925, "°"),\n',
         '        ("1 °", std::f64::consts::PI / 180.0, "°"),\n', 1),
        ("    let deg: f64 = 0.0174532925; // the factor physure.conf gives `deg`\n",
         "    let deg: f64 = std::f64::consts::PI / 180.0; // the factor physure.conf gives `deg`\n", 1),
        ('    assert_close(radians.value.mean(), 90.0 * 0.0174532925, "90 deg in radians");\n',
         '    assert_close(radians.value.mean(), 90.0 * std::f64::consts::PI / 180.0, "90 deg in radians");\n', 1),
    ])
    edit(TESTS / "measurement_tests/test_conversions_realworld.py", [
        ("        Q_(1.0, \"slug\").to(\"kg\").magnitude, 14.5939, rel_tol=1e-6\n",
         "        Q_(1.0, \"slug\").to(\"kg\").magnitude, 14.593902937206364, rel_tol=1e-12\n", 1),
        ("        Q_(1.0, \"acre\").to(\"m^2\").magnitude, 4046.856, rel_tol=1e-6\n",
         "        Q_(1.0, \"acre\").to(\"m^2\").magnitude, 4046.8564224, rel_tol=1e-12\n", 1),
        ("        Q_(1.0, \"ly\").to(\"m\").magnitude, 9.4607e15, rel_tol=1e-5\n",
         "        Q_(1.0, \"ly\").to(\"m\").magnitude, 9460730472580800.0, rel_tol=1e-12\n", 1),
        ("    # International Table BTU = 1055.05585262 J; .conf stores 1055.06 (6 sig figs).\n"
         "    assert math.isclose(\n"
         "        Q_(1.0, \"BTU\").to(\"J\").magnitude, 1055.06, rel_tol=1e-6\n"
         "    )\n",
         "    # International Table BTU = 1055.05585262 J, exactly.\n"
         "    assert math.isclose(\n"
         "        Q_(1.0, \"BTU\").to(\"J\").magnitude, 1055.05585262, rel_tol=1e-15\n"
         "    )\n", 1),
        ("        Q_(1.0, \"mmHg\").to(\"Pa\").magnitude, 133.322, rel_tol=1e-6\n",
         "        Q_(1.0, \"mmHg\").to(\"Pa\").magnitude, 133.322387415, rel_tol=1e-15\n", 1),
    ])
    set_last_bit((
        "Gb", "a0", "acre", "acres", "atomic_time", "bohr", "bohr_radius",
        "celsius", "centigrade", "degC", "degF", "dyn", "dyne", "electron_mass",
        "fahrenheit", "gilbert", "ha", "hectare", "hectares", "inch", "lb", "lbf",
        "lbs", "light_year", "ly", "me", "mp", "ounce", "ounces", "oz",
        "planck_mass", "pound", "pound_force", "pounds", "ppm", "psi", "tau0",
    ))
    changelog(
        "Fixed",
        "- **Unit factors that were typed with a few digits are now the exact value of their definition.** `1 deg` was `60.000043 arcmin`, `1 atm` was `760.0021 Torr`, `1 gal` was `230.99989 in^3` and `1 pc` was `3.26160 ly`, because `degree`, `arcminute`, `arcsecond`, `light_year`, `parsec`, `slug`, `acre`, `gallon_us`, `fluid_ounce`, `pound_force`, `btu`, `horsepower`, `psi`, `torr`, `mmHg`, `franklin` and `debye` carried 5 to 9 digits (up to 7e-6 off). `torr` is now exactly `101325/760` Pa and `mmHg` the conventional `133.322387415` Pa, `btu` is the International Table BTU.",
    )

elif part == "variants":
    # one name per definition; the short symbol is the documented default
    edit(CONF, [
        ("[Units]\n# --- SPECIAL ---\n",
         "[Units]\n"
         "# Names. Every entry can be written with its own name below, and no name belongs\n"
         "# to two entries (tests/core/test_catalog_namespace.py). Where a quantity has\n"
         "# several definitions in use, each definition has its own entry, named\n"
         "# <unit>_<qualifier> (_th, _it, _iso, _us, _us_survey, _imperial, _metric,\n"
         "# _electric, _julian, _gregorian...), and the short symbol is the default named\n"
         "# in the comment above the group.\n"
         "# --- SPECIAL ---\n", 1),
        ("year   = 31557600.0, T, [yr, year, years]\n",
         "# Default: the Julian year (365.25 days), as astronomy defines it.\n"
         "year   = 31557600.0, T, [yr, year, years, year_julian, yr_julian]\n"
         "year_gregorian = 31556952.0, T, [yr_gregorian]\n", 1),
        ("acre    = 4046.8564224, L^2, [acre, acres]\n",
         "# Default: the international acre (43560 square international feet).\n"
         "acre    = 4046.8564224, L^2, [acre, acres, acre_international]\n"
         "acre_us_survey = 4046.872609874252, L^2, [acre_us_survey]\n", 1),
        ("gallon_us = 0.003785411784, L^3, [gal, gallon, gallons]\n"
         "fluid_ounce = 2.95735295625e-05, L^3, [fl_oz, fluid_ounce]\n",
         "# Default: the US liquid gallon (231 cubic inches) and its fluid ounce (1/128).\n"
         "gallon_us = 0.003785411784, L^3, [gal, gallon, gallons, gal_us]\n"
         "gallon_imperial = 0.00454609, L^3, [gallon_imperial, gal_imp, gal_uk]\n"
         "fluid_ounce = 2.95735295625e-05, L^3, [fl_oz, fluid_ounce, fl_oz_us]\n"
         "fluid_ounce_imperial = 2.84130625e-05, L^3, [fluid_ounce_imperial, fl_oz_imp, fl_oz_uk]\n", 1),
        ("calorie = 4.184, M*L^2*T^-2, J, [cal, calorie, calories]\n",
         "# Default: the thermochemical calorie (4.184 J).\n"
         "calorie = 4.184, M*L^2*T^-2, J, [cal, calorie, calories, cal_th, calorie_th]\n"
         "calorie_it = 4.1868, M*L^2*T^-2, J, [cal_it, calorie_it]\n", 1),
        ("btu           = 1055.05585262, M*L^2*T^-2, J, [BTU, btu]\n",
         "# Default: the International Table BTU.\n"
         "btu           = 1055.05585262, M*L^2*T^-2, J, [BTU, btu, btu_it]\n"
         "btu_th        = 1054.3502644888888, M*L^2*T^-2, J, [btu_th]\n"
         "btu_iso       = 1055.056, M*L^2*T^-2, J, [btu_iso]\n", 1),
        ("horsepower = 745.6998715822702, M*L^2*T^-3, W, [hp, horsepower]\n",
         "# Default: the mechanical horsepower (550 ft lbf/s).\n"
         "horsepower = 745.6998715822702, M*L^2*T^-3, W, [hp, horsepower, hp_mechanical, horsepower_mechanical]\n"
         "horsepower_metric = 735.49875, M*L^2*T^-3, W, [hp_metric, horsepower_metric]\n"
         "horsepower_electric = 746.0, M*L^2*T^-3, W, [hp_electric, horsepower_electric]\n", 1),
        ("torr   = 133.32236842105263, M*L^-1*T^-2, Pa, [Torr, torr]\n",
         "# torr is exactly 1/760 atm; mmHg is the conventional millimetre of mercury\n"
         "# (13595.1 kg/m^3 at standard gravity). They differ by 1.4e-7.\n"
         "torr   = 133.32236842105263, M*L^-1*T^-2, Pa, [Torr, torr]\n", 1),
    ])
    set_last_bit((
        "Gb", "a0", "acre", "acre_international", "acre_us_survey", "acres",
        "atomic_time", "bohr", "bohr_radius", "btu_iso", "celsius", "centigrade",
        "degC", "degF", "dyn", "dyne", "electron_mass", "fahrenheit", "fl_oz_imp",
        "fl_oz_uk", "fluid_ounce_imperial", "gal_imp", "gal_uk", "gallon_imperial",
        "gilbert", "ha", "hectare", "hectares", "horsepower_metric", "hp_metric",
        "inch", "lb", "lbf", "lbs", "light_year", "ly", "me", "mp", "ounce",
        "ounces", "oz", "planck_mass", "pound", "pound_force", "pounds", "ppm",
        "psi", "tau0",
    ))
    changelog(
        "Changed",
        "- **Units with several definitions have one name per definition.** `cal` is the thermochemical calorie (`cal_th`; `cal_it` is the International Table one), `BTU` the International Table BTU (`btu_it`; `btu_th`, `btu_iso`), `acre` the international acre (`acre_international`; `acre_us_survey`), `gal` and `fl_oz` the US ones (`gal_us`, `fl_oz_us`; `gallon_imperial`, `fluid_ounce_imperial`), `hp` the mechanical horsepower (`hp_mechanical`; `hp_metric`, `hp_electric`) and `yr` the Julian year (`year_julian`; `year_gregorian`). The short symbols keep their meaning, except `btu`/`BTU`, whose old value `1055.06` was a rounding that matched no definition. A test reads the catalog and fails if two units claim the same name.",
    )

else:
    raise SystemExit(f"unknown part: {part}")
```
