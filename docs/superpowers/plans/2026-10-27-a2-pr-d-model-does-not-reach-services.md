# A2 PR D: the model stops reaching the services — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the last three `a2a` import exceptions, the ones where the model reaches a service, so that the only exceptions left in the layering contract are the ones A1 removes (plus the documented `__version__` one). The contract test from PR A proves it: its exception list shrinks by exactly these imports.

**Architecture:** Four commits, one per mechanism. (1) The parser that splits `"10 m/s"` into a value and a unit moves from `QuantityFactory` to the model (`domain/measurement/quantity_text.py`); the factory and the Pydantic schema of `Quantity` both use it. (2) `trace_formulas`, which builds a `FormulaTracer`, moves from the kernel's tracer-state module to the tracer's own module; the kernel keeps only the `ContextVar` and its accessors. (3) `domain.symbolic.*` is classified as services in the layer map: it is a client of `Quantity` and of the function service, not part of the model. (4) The `a2a` label is retired from the documentation, because no record uses it any more. Design: `docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md`, row "D".

**Tech Stack:** Python 3.11+, pytest, ruff. No Rust changes.

## What was measured while planning (use it to check your results)

The change was implemented and verified on a prototype branch before this plan was written.

| Measure | Before | After |
|---|---|---|
| Imports from the model or the kernel up into services | 3 `a2a` | 0 |
| Exception records | 10: 6 `a1`, 3 `a2a`, 1 `release` | 7: 6 `a1`, 1 `release` |
| Records after each commit | 10 | 9 after the first, 8 after the second, 7 after the third, 7 after the fourth |
| New tests | — | 7 (`tests/core/test_quantity_text.py`) |
| Full suite | 933 passed, 6 xfailed | **940 passed, 6 xfailed** |
| First use (`python -m physure "500 N / 2 m^2 => kPa"`) | 0.11 s | 0.11 to 0.13 s, prints `0.25 kPa` |

## Decisions taken while planning

- **The spec's wording about `Quantity.from_input` is not what the code does.** `from_input` never parses text. The only text parser is in `QuantityFactory` (`_NUMBER_PREFIX_REGEX` and `_parse_string_value`), and the model reached the factory from exactly one place: the string validator inside `Quantity.__get_pydantic_core_schema__`. So the parser itself moves down, as a pure function `parse_quantity_text(text, system)`; the factory calls it, and so does the Pydantic validator. Behaviour is identical (the new test file also characterizes `Q_("10 m/s")`, which no earlier test did).
- **`trace_formulas` changes module, without a shim.** It moves to `physure/domain/symbolic/tracer.py`, next to `FormulaTracer`. A forwarding name left in the kernel module would have to import upward and would need its own exception record, which is what this PR removes. `trace_formulas` was never exported from `physure`, no document mentions it and its only user in the repository is one test; the CHANGELOG says so. The kernel module gains `reset_active_tracer(token)`, the counterpart of `set_active_tracer`, so the tracer module does not touch the private `ContextVar`.
- **`domain.symbolic.*` is a service.** The contract test passes with that single line in the layer map and the third record removed: no new violation appears, which is the evidence that the classification is right.
- **The `a2a` label is retired.** After the third commit no record carries it. The legend line in the TOML header and the clause in `structure.md` go, and the TOML block that the test prints for a new violation no longer suggests `a2a`; it prints `cause = "<who removes it>"`, so whoever records an exception has to say who owns removing it.

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `refactor/a2-model-below-services` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/`, and always use `uv run --no-sync ...`. Never run `maturin develop` or `uv sync`.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files.
- The source edits are made by one script, `apply_pr_d.py`, in parts, so that each commit leaves the contract consistent with the code. Do not edit the sources by hand. The test file is written by extracting the block shown in this plan with `plan_block.py` (Task 1), never retyped.
- Commands run from the worktree root `D:\Projects\physure\.claude\worktrees\single-conf` unless a step says `physure-python`.

---

### Task 1: Create the branch and the two helper scripts

**Files:** none tracked

- [ ] **Step 1: Start from a clean tree at the tip of `main`**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
git status -sb | head -3
git log --oneline -1
git log --oneline -1 main
```

Expected: no changed files; the two `log` lines show the same commit. If the worktree is on another branch with no uncommitted changes, `git switch --detach main` first. Any uncommitted change you did not make: stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c refactor/a2-model-below-services main
git status -sb | head -1
```

Expected: `## refactor/a2-model-below-services`.

- [ ] **Step 3: Write `plan_block.py`, which extracts a fenced block from this plan**

```bash
cat > /c/Users/Irvin/AppData/Local/Temp/plan_block.py <<'EOF'
"""Writes the fenced block that follows an anchor line of the plan to a file.

Usage: python -X utf8 plan_block.py "<anchor line>" <language> <destination>
"""
import pathlib
import re
import sys

PLAN = pathlib.Path(
    r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a2-pr-d-model-does-not-reach-services.md"
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

- [ ] **Step 4: Extract `apply_pr_d.py` (defined at the end of this plan)**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `apply_pr_d.py` with exactly this content:' python /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py
```

Expected: `written ... | 254 lines`. An `AssertionError: block not found` means the plan file is not at the path inside `plan_block.py`: stop and report.

---

### Task 2: The text of a quantity is parsed by the model

**Files:**
- Create: `physure-python/tests/core/test_quantity_text.py`
- Create: `physure-python/physure/domain/measurement/quantity_text.py` (by the script)
- Modify: `physure-python/physure/application/factories.py`
- Modify: `physure-python/physure/domain/measurement/quantity.py`
- Modify: `physure-python/tests/core/import_layers.toml` (one record removed)

- [ ] **Step 1: Write the test and watch it fail**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/core/test_quantity_text.py` with exactly this content:' python physure-python/tests/core/test_quantity_text.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_quantity_text.py && uv run --no-sync ruff format --check tests/core/test_quantity_text.py
uv run --no-sync pytest tests/core/test_quantity_text.py -q -p no:cacheprovider 2>&1 | grep -E "Error|passed|failed"
```

The block written by the first command is:

Create `physure-python/tests/core/test_quantity_text.py` with exactly this content:

```python
"""The text of a quantity ("10 m/s") is parsed by the model, not by a factory."""

import pytest

from physure.domain.measurement.quantity_text import parse_quantity_text
from physure.domain.measurement.units import get_default_system


@pytest.fixture
def system():
    return get_default_system()


@pytest.mark.parametrize(
    ("text", "value", "unit"),
    [
        ("10 m", 10, "m"),
        ("2.5 kg", 2.5, "kg"),
        ("-3 m/s", -3, "m/s"),
        ("1e3 s", 1000.0, "s"),
        ("7", 7, "dimensionless"),
    ],
)
def test_the_number_and_the_unit_are_separated(system, text, value, unit):
    parsed_value, parsed_unit = parse_quantity_text(text, system)
    assert parsed_value == value
    assert type(parsed_value) is type(value)
    assert parsed_unit == system.get_unit(unit)


def test_text_without_a_number_is_left_alone(system):
    parsed = parse_quantity_text("m/s", system)
    assert parsed == ("m/s", system.get_unit("dimensionless"))


def test_the_factory_reads_the_same_text():
    from physure import Q_

    assert Q_("10 m/s") == Q_(10, "m/s")
    assert Q_("2.5 kg").magnitude == 2.5
    assert str(Q_("7").unit) == "1"
```

Expected: ruff clean; `ModuleNotFoundError: No module named 'physure.domain.measurement.quantity_text'` (the whole file fails at collection). If anything else: stop and report.

- [ ] **Step 2: Apply the `text` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py text
```

Expected: `created ...quantity_text.py`; two `edited` lines for `factories.py`; one for `quantity.py`; one for `import_layers.toml`; `(dropped 1 records)`. Any `AssertionError` means an anchor changed: stop and report.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_quantity_text.py tests/core/test_import_layers.py tests/core/test_pydantic.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean (`213 files already formatted`); `14 passed`; `9` records.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/domain/measurement/quantity_text.py physure-python/physure/application/factories.py physure-python/physure/domain/measurement/quantity.py physure-python/tests/core/test_quantity_text.py physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "refactor: the text of a quantity is parsed by the model" -m "Quantity.__get_pydantic_core_schema__ imported QuantityFactory only to turn a string such as '10 m' into a value and a unit. That parser (the leading-number regex and the split) moves to domain/measurement/quantity_text.py as parse_quantity_text; QuantityFactory and the Pydantic validator both call it, so the model no longer reaches a service. Behaviour is unchanged." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `A` for `quantity_text.py` and the test; `M` for the other three.

---

### Task 3: `trace_formulas` moves to the tracer

**Files:**
- Modify: `physure-python/tests/symbolic/test_tracing.py`
- Modify: `physure-python/physure/application/tracing/context.py`
- Modify: `physure-python/physure/domain/symbolic/tracer.py`
- Modify: `physure-python/tests/core/import_layers.toml` (one record removed)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Move the test's import first and watch it fail**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py tracer_test
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync pytest tests/symbolic/test_tracing.py -q -p no:cacheprovider 2>&1 | grep -E "Error|passed|failed"
```

Expected: `edited ...test_tracing.py`, then `ImportError: cannot import name 'trace_formulas' from 'physure.domain.symbolic.tracer'`.

- [ ] **Step 2: Apply the `tracer` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py tracer
```

Expected: two `edited` lines for `context.py`, two for `tracer.py`, `edited ...import_layers.toml`, `(dropped 1 records)`, `edited CHANGELOG.md`.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/symbolic tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean; `175 passed`; `8` records.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/application/tracing/context.py physure-python/physure/domain/symbolic/tracer.py physure-python/tests/symbolic/test_tracing.py physure-python/tests/core/import_layers.toml CHANGELOG.md
git diff --cached --name-status
git commit -m "refactor: trace_formulas lives with the tracer, not in the kernel" -m "trace_formulas builds a FormulaTracer, so the tracer-state module of the kernel imported the symbolic package to define it. It moves next to FormulaTracer; the kernel keeps the ContextVar, get_active_tracer, set_active_tracer and the new reset_active_tracer. It was never exported from physure." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: five `M`.

---

### Task 4: `domain.symbolic` is a service

**Files:**
- Modify: `physure-python/tests/core/import_layers.toml`

- [ ] **Step 1: Apply the `symbolic` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py symbolic
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
git diff --stat
```

Expected: two `edited` lines for `import_layers.toml` and `(dropped 1 records)`; `4 passed` (the contract holds with the classification and without the record: no new violation appears); `7` records; `git diff --stat` shows only `import_layers.toml`, 1 insertion and 6 deletions.

- [ ] **Step 2: Commit**

```bash
cd ..
git add physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "test(layers): domain.symbolic is a service, not part of the model" -m "The symbolic package builds on Quantity and on the function service, so it sits above the model. Classifying physure.domain.symbolic.* as services removes the last import of the model into a service, and no other import breaks the rule." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: one `M` entry.

---

### Task 5: Retire the `a2a` label

**Files:**
- Modify: `physure-python/tests/core/import_layers.toml`
- Modify: `physure-python/tests/core/test_import_layers.py`
- Modify: `structure.md`

- [ ] **Step 1: Apply the `legend` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_d.py legend
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -rn "a2a" tests/core/import_layers.toml tests/core/test_import_layers.py ../structure.md ../CONTRIBUTING.md; echo "grep exit $?"
```

Expected: `edited` lines for the TOML, the test and `structure.md`; ruff clean; `4 passed`; the `grep` prints nothing and exits `1`.

- [ ] **Step 2: Commit**

```bash
cd ..
git add physure-python/tests/core/import_layers.toml physure-python/tests/core/test_import_layers.py structure.md
git diff --cached --name-status
git commit -m "docs: retire the a2a label of the layering exceptions" -m "No record carries it any more. The legend line and the clause in structure.md go, and the TOML block that the contract test prints for a new violation asks for the owner of the removal instead of suggesting a cause." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: three `M`.

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

Expected: `940 passed, 6 xfailed` (the 933 of `main`'s tip plus the 7 new tests), in about 70 s. If any other test fails, run it alone and report; do not change application code.

- [ ] **Step 3: First use stays fast and the contract has no `a2a` left**

```bash
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
python -X utf8 -c "
import tomllib, collections
ex = tomllib.load(open('tests/core/import_layers.toml', 'rb'))['exception']
print(dict(collections.Counter(e['cause'] for e in ex)))"
grep -n "application.factories" physure/domain/measurement/quantity.py
echo "grep exit $?"
```

Expected: `0.25 kPa` in about 0.1 to 0.2 s; `7`; `{'a1': 6, 'release': 1}`; the last `grep` prints nothing and exits `1` (`quantity.py` no longer mentions the factory).

- [ ] **Step 4: Confirm the branch**

```bash
cd ..
git status --short
git log --oneline -6
```

Expected: `git status --short` empty (or only ` M Cargo.lock`); four new commits on `refactor/a2-model-below-services` on top of `main`.

- [ ] **Step 5: Hand-off**

Report the branch name, the four commit SHAs and the measured numbers (they should match the table above). Nothing is pushed. Integrating the branch is the maintainer's decision. After this PR the only exceptions left are the six `a1` ones, which A1 removes by deleting the Python unit algebra, and the documented `release` one.

---

## Reference: the script this plan embeds

Create `apply_pr_d.py` with exactly this content:

```python
"""A2 PR D: the model stops reaching the services.

Usage (from the repository root of the worktree):
    python -X utf8 apply_pr_d.py text | tracer_test | tracer | symbolic | legend

Every replacement asserts that its anchor occurs exactly as many times as
expected, and every file keeps its own line endings.
"""

import pathlib
import re
import sys

SRC = pathlib.Path("physure-python/physure")
TESTS = pathlib.Path("physure-python/tests")
TOML = TESTS / "core/import_layers.toml"
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
            f"{path}: {old[:60]!r} found {text.count(old)}, expected {count}"
        )
        text = text.replace(old, new)
    save(path, text, eol)


def cut_tail(path, marker, new_tail):
    """Replaces everything from `marker` (which must occur once) to the end."""
    text, eol = load(path)
    assert text.count(marker) == 1, (path, marker)
    save(path, text[: text.index(marker)] + new_tail, eol)


def drop_exceptions(records):
    """Removes the recorded exceptions (importer, imported, kind, cause)."""
    text, eol = load(TOML)
    for importer, imported, kind, cause in records:
        pattern = re.compile(
            r"\[\[exception\]\]\nimporter = \"" + re.escape(importer)
            + r"\"\nimported = \"" + re.escape(imported)
            + r"\"\nkind = \"" + kind + r"\"\n(?:count = \d+\n)?cause = \""
            + cause + r"\"\n\n?"
        )
        text, n = pattern.subn("", text)
        assert n == 1, (importer, imported, kind, n)
    save(TOML, text, eol)
    print(f"  (dropped {len(records)} records)")


def changelog_changed(bullet):
    """Adds a bullet at the end of the Changed section of [Unreleased]."""
    edit(CHANGELOG, [(
        "\n\n### Fixed\n- **`SymbolicQuantity`,",
        "\n" + bullet + "\n\n### Fixed\n- **`SymbolicQuantity`,",
        1,
    )])


part = sys.argv[1]

if part == "text":
    # the text of a quantity is parsed by the model; the factory and the
    # pydantic schema both use it
    (SRC / "domain/measurement/quantity_text.py").write_bytes('''"""Splits the text of a quantity such as "10 m/s" into a value and a unit."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit

# Matches only the leading numeric literal (optional sign, int/float, optional
# exponent); the unit is whatever text follows and is sliced off in Python.
# No trailing `.*$` wildcard and an unambiguous digit pattern
# (`\\d+\\.?\\d*|\\.\\d+`, not `\\d*\\.?\\d+`), so there is nothing to force the engine
# to re-partition a long digit run — immune to ReDoS backtracking.
_NUMBER_PREFIX_REGEX = re.compile(
    r"\\s*([-+]?(?:\\d+\\.?\\d*|\\.\\d+)(?:[eE][-+]?\\d+)?)"
)


def parse_quantity_text(
    text: str, system: UnitSystem
) -> tuple[float | str, CompoundUnit]:
    """Parses a string like '10 m/s' into a value and a unit."""
    match = _NUMBER_PREFIX_REGEX.match(text)
    if not match:
        return text, system.get_unit("dimensionless")

    num_str = match.group(1)
    unit_str = text[match.end() :].strip()
    try:
        parsed_value = float(num_str)
        # If the number was an integer (e.g., "10"), convert back to int
        # for cleanness
        if (
            parsed_value.is_integer()
            and "." not in num_str
            and "e" not in num_str.lower()
        ):
            parsed_value = int(parsed_value)
    except ValueError:
        return text, system.get_unit("dimensionless")

    unit = (
        system.get_unit(unit_str)
        if unit_str
        else system.get_unit("dimensionless")
    )
    return parsed_value, unit
'''.encode("utf-8"))
    print("created", SRC / "domain/measurement/quantity_text.py")
    factories = SRC / "application/factories.py"
    text, eol = load(factories)
    text, n = re.subn(
        r"# Matches only the leading numeric literal.*?\n\)\n\n\n(?=_V = TypeVar)",
        "",
        text,
        flags=re.S,
    )
    assert n == 1
    assert text.count("\n    def _parse_string_value(") == 1
    text = text[: text.index("\n    def _parse_string_value(")].rstrip("\n") + "\n"
    save(factories, text, eol)
    edit(factories, [
        ("import re\nfrom typing import", "from typing import", 1),
        ("from physure.domain.measurement.quantity import (\n    Quantity,\n)\n",
         "from physure.domain.measurement.quantity import (\n    Quantity,\n)\n"
         "from physure.domain.measurement.quantity_text import parse_quantity_text\n", 1),
        ("            value, unit = self._parse_string_value(value, system)\n",
         "            value, unit = parse_quantity_text(value, system)\n", 1),
    ])
    edit(SRC / "domain/measurement/quantity.py", [
        ("                from physure.application.factories import QuantityFactory\n\n"
         "                # We use the factory because it handles strings with units\n"
         "                q = QuantityFactory()(value)\n"
         "                return q\n",
         "                from physure.application.context import get_active_system\n"
         "                from physure.domain.measurement.quantity_text import (\n"
         "                    parse_quantity_text,\n"
         "                )\n\n"
         "                system = get_active_system()\n"
         "                number, unit = parse_quantity_text(value, system)\n"
         "                return Quantity.from_input(number, unit, system)\n", 1),
    ])
    drop_exceptions([
        ("physure.domain.measurement.quantity", "physure.application.factories", "lazy", "a2a"),
    ])

elif part == "tracer_test":
    # the test moves first, so that it fails for the right reason
    edit(TESTS / "symbolic/test_tracing.py", [
        ("from physure.application.tracing.context import trace_formulas\n",
         "from physure.domain.symbolic.tracer import trace_formulas\n", 1),
    ])

elif part == "tracer":
    # trace_formulas builds a FormulaTracer: it lives with the tracer, and the
    # kernel keeps only the ContextVar
    context = SRC / "application/tracing/context.py"
    edit(context, [
        ("import sys\nfrom collections.abc import Generator\nfrom contextlib import contextmanager\nfrom contextvars import ContextVar, Token\n",
         "import sys\nfrom contextvars import ContextVar, Token\n", 1),
        ("# The tracer is imported only inside functions or using TYPE_CHECKING\n# to avoid circular imports.\n",
         "# The tracer is imported only under TYPE_CHECKING: its module imports this one.\n", 1),
    ])
    cut_tail(
        context,
        "\n\n@contextmanager\ndef trace_formulas()",
        "\n\ndef reset_active_tracer(token: Token) -> None:\n"
        '    """Restores the tracer that was active before `set_active_tracer`."""\n'
        "    _current_tracer.reset(token)\n",
    )
    tracer = SRC / "domain/symbolic/tracer.py"
    edit(tracer, [
        ("import weakref\nfrom typing import Any\n\nfrom physure.domain.symbolic.graph import LeafNode, OpNode, SymbolicNode\n",
         "import weakref\nfrom collections.abc import Generator\nfrom contextlib import contextmanager\n"
         "from typing import Any\n\n"
         "from physure.application.tracing.context import (\n    reset_active_tracer,\n    set_active_tracer,\n)\n"
         "from physure.domain.symbolic.graph import LeafNode, OpNode, SymbolicNode\n", 1),
    ])
    text, eol = load(tracer)
    save(tracer, text.rstrip("\n") + '''


@contextmanager
def trace_formulas() -> Generator[FormulaTracer, None, None]:
    """Context manager to enable symbolic tracing.

    Example:
        with trace_formulas() as tracer:
            m = Q_(10, "kg", symbol="m")
            a = Q_(5, "m/s^2", symbol="a")
            f = m * a
            print(tracer.get_equation(f))
    """
    tracer = FormulaTracer()
    token = set_active_tracer(tracer)
    try:
        yield tracer
    finally:
        reset_active_tracer(token)
''', eol)
    drop_exceptions([
        ("physure.application.tracing.context", "physure.domain.symbolic.tracer", "lazy", "a2a"),
    ])
    changelog_changed(
        "- **`trace_formulas` moved from `physure.application.tracing.context` to `physure.domain.symbolic.tracer`**, next to `FormulaTracer`, so the tracer state that the arithmetic reads no longer imports the symbolic package. It was never exported from `physure`; code that imported it from the old path must follow.",
    )

elif part == "symbolic":
    # domain.symbolic is a client of the model (it builds on Quantity and on the
    # function service): it is a service, not part of the model
    edit(TOML, [
        ('    "physure.symbolic",\n]\n',
         '    "physure.symbolic",\n    "physure.domain.symbolic.*",\n]\n', 1),
    ])
    drop_exceptions([
        ("physure.domain.symbolic.expression", "physure.application.functions.functions", "top", "a2a"),
    ])

elif part == "legend":
    # no record is owed to the A2 clean-up any more
    edit(TOML, [
        ("#   a2a - fixed by the A2 pull requests that clean up the imports\n", "", 1),
    ])
    edit(TESTS / "core/test_import_layers.py", [
        ('def _toml_block(key, count, cause="a2a") -> str:',
         'def _toml_block(key, count, cause="<who removes it>") -> str:', 1),
    ])
    edit(pathlib.Path("structure.md"), [
        ("(`a1`: removed when A1 deletes the module; `a2a`: removed by the import clean-up)",
         "(`a1`: removed when A1 deletes the module)", 1),
    ])

else:
    raise SystemExit(f"unknown part: {part}")
```
