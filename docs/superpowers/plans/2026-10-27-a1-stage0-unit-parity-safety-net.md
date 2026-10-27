# A1 Stage 0: unit parity safety net — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the tests that compare Python's unit algebra with the Rust core over the whole catalog, freeze Python's unit semantics as a golden file, and publish the divergence report. No behaviour changes: this is the safety net the later A1 stages are built on.

**Architecture:** Rust is the oracle for conversions. The PHS `=>` operator runs inside `physure-core`'s registry and returns a native `Quantity` with a `float` magnitude, reachable from Python as `physure._core.evaluate_phs_native`; Python's `Q_(v, a).to(b)` is compared with it using exact equality. Every difference is classified. An unclassified difference fails the suite; each known gap is a strict `xfail`, so it can only shrink. Python facts that Rust cannot answer yet (`kind`, `is_linear`, `simplify`, alias formatting) are frozen in a golden JSON file. Design: `docs/superpowers/specs/2026-10-27-a1-rust-owned-unit-algebra-design.md`, Stage 0.

**Tech Stack:** Python 3.11+, pytest, PyO3 extension `physure._core`, `uv`, ruff.

## What was measured while planning (use it to check your results)

The comparator below was run against `main` at `0ac08a3`. These numbers are what you should see; a different number means something changed and must be explained, not ignored.

| Measure | Value |
|---|---|
| Names in the generated catalog index | 6158 |
| Names Python cannot build a `Q_` for (`UnknownUnitError`) | 3720 |
| Dimension groups (by Python's dimension) | 34 |
| References per group (shortest symbols Rust knows) | 2 |
| Differences found (5 classes, 0 unclassified) | 8877 |
| `rust-rejects-long-prefix-name` (`kilometer`, `attoampere`: Python yes, Rust no); 2183 symbols | 8732 |
| `last-bit-difference` (`acre`, `psi`, `degC`, `eV`, `coulomb`, ...); 46 symbols | 87 |
| `dimensionless-kinds-merged-by-python` (`arcmin` to `sr`, `percent` to `pH`); 19 symbols | 38 |
| `rust-rejects-delta-temperature` (`delta_celsius` to `K`); 3 symbols | 12 |
| `rust-does-not-know-unit` (the logarithmic units `dB`, `decibel`); 2 symbols | 8 |
| Compound conversions compared / differing | 17 / 1 (`g/cm^3` to `kg/m^3`: Python `1000.0`, Rust `999.9999999999999`) |
| Frozen semantic entries (golden file) | 32, none an error |
| Test run time | about 20 s |

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`. It already has a built environment and sits at the tip of local `main`. Create the branch `test/a1-stage0-unit-parity` there (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/`, and always use `uv run --no-sync ...` (a plain `uv run` re-syncs the editable install and can replace the release-built extension; on Windows it also fails while pytest holds `_core.pyd`).
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Run commands from `physure-python/` unless a step says otherwise.
- Do not edit any code under `physure/` or `src/`: this stage adds tests and docs only.

---

### Task 1: Create the branch and check the environment

**Files:** none

- [ ] **Step 1: Start from a clean tree at the tip of `main`**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
git status --short
git log --oneline -1
git log --oneline -1 main
```

Expected: `git status --short` prints nothing; the two `log` lines show the same commit. If `git status` lists `physure-python/tests/core/test_unit_algebra_parity.py` or `physure-python/tests/core/golden/` as untracked, they are leftovers of the planning session: run `git status --short` again to confirm they are the only entries, then delete exactly those paths with `rm -rf physure-python/tests/core/golden physure-python/tests/core/test_unit_algebra_parity.py` (Task 2 recreates them). Any other untracked or modified file: stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c test/a1-stage0-unit-parity main
git status -sb | head -1
```

Expected: `## test/a1-stage0-unit-parity`.

- [ ] **Step 3: Check that the extension in this worktree is the one under test**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync python -c "import physure._core as c; print(c.__file__); print(len(c.default_conf()))"
```

Expected: the path is inside `.claude\worktrees\single-conf\physure-python` and the length is `30764` (it proves `default_conf()` exists). If the import fails or the path points elsewhere, stop and report: the environment must be rebuilt first with `uv run maturin develop --release` from this directory.

---

### Task 2: Add the parity and golden tests

**Files:**
- Create: `physure-python/tests/core/test_unit_algebra_parity.py`
- Create (generated): `physure-python/tests/core/golden/unit_semantics.json`

- [ ] **Step 1: Create the test file with exactly this content**

```python
"""A1, Stage 0: Python's unit algebra compared with the Rust core.

Rust is the oracle for conversions: the PHS ``=>`` operator runs inside
physure-core's registry and returns a native Quantity whose magnitude is a float
computed by Rust. Python is compared against it with exact equality, over the
whole catalog and over a set of representative compound units.

Every difference is classified. An unclassified difference fails the suite, and
a known gap that disappears turns its strict ``xfail`` into a failure, so the
list of gaps can only shrink. Stages 1 and 2 of A1 close the gaps; this file is
their acceptance test.

Set ``PHYSURE_UPDATE_GOLDEN=1`` to rewrite the golden file after reviewing a
deliberate change in the frozen behaviour.
"""

from __future__ import annotations

import collections
import configparser
import itertools
import json
import math
import os
from pathlib import Path
from typing import NamedTuple

import pytest

import physure._core as core
from physure import Q_
from physure.application.context import get_current_system
from physure.domain.exceptions import UnknownUnitError

# Stage 2 replaces this import with the registry-backed namespace.
from physure.units._index import UNIT_INDEX

GOLDEN_FILE = Path(__file__).parent / "golden" / "unit_semantics.json"
UPDATE_GOLDEN = os.environ.get("PHYSURE_UPDATE_GOLDEN") == "1"

# 1.0 shows scale factors; 25.0 also shows the offset of temperature scales.
VALUES = (1.0, 25.0)

# Each unit is converted to this many reference units of its dimension. One
# is not enough: a dimension can hold several kinds (angles and solid angle,
# ratios and logarithmic units), and a reference of one kind hides the rest.
REFERENCES_PER_GROUP = 2

# Symbols whose conversion differs from Rust in the last bit. Python converts
# with exact Fraction scales and one rounding; Rust stores scales as f64.
# Stage 1 gives the core exact scales, and this tuple becomes empty.
KNOWN_LAST_BIT = (
    "BTU",
    "C",
    "Gb",
    "acre",
    "acres",
    "btu",
    "celsius",
    "centigrade",
    "coulomb",
    "coulombs",
    "degC",
    "degF",
    "dyn",
    "dyne",
    "eV",
    "electron_mass",
    "electron_volt",
    "fahrenheit",
    "gal",
    "gallon",
    "gallon_us",
    "gallons",
    "gilbert",
    "ha",
    "hectare",
    "hectares",
    "inch",
    "lb",
    "lbf",
    "lbs",
    "me",
    "mp",
    "ounce",
    "ounces",
    "oz",
    "planck_charge",
    "planck_mass",
    "pound",
    "pound_force",
    "pounds",
    "ppm",
    "psi",
    "qp",
    "rad",
    "radian",
    "radians",
)

# Compound conversions (value, from, to) every implementation must agree on.
COMPOUND_CASES = (
    (1.0, "km/h", "m/s"),
    (1.0, "kg*m/s^2", "N"),
    (1.0, "N*m", "J"),
    (1.0, "J/s", "W"),
    (1.0, "kWh", "J"),
    (1.0, "psi", "Pa"),
    (1.0, "mmHg", "Pa"),
    (1.0, "g/cm^3", "kg/m^3"),
    (1.0, "L/min", "m^3/s"),
    (1.0, "eV", "J"),
    (1.0, "cal", "J"),
    (1.0, "BTU/h", "W"),
    (1.0, "hp", "W"),
    (25.0, "degC", "K"),
    (77.0, "degF", "degC"),
    (1.0, "mol/L", "mol/m^3"),
    (1.0, "ft*lbf", "J"),
)
KNOWN_COMPOUND_LAST_BIT = (("g/cm^3", "kg/m^3"),)

# Units whose semantics are frozen so Stage 1 has a target to match.
SEMANTIC_EXPRESSIONS = (
    "m", "km", "kg", "s", "K", "Pa", "Hz",
    "kg*m/s^2", "N*m", "J/s", "km/h", "m/s^2", "g/cm^3", "L/min", "mol/L",
    "degC", "degF", "delta_celsius", "delta_fahrenheit",
    "percent", "ppm", "dB", "pH",
    "psi", "mmHg", "gallon", "eV", "cal", "hp", "kWh", "BTU/h", "ft*lbf",
)  # fmt: skip


CLASSIFIED_KINDS = {
    "last-bit-difference",
    "rust-rejects-long-prefix-name",
    "rust-does-not-know-unit",
    "rust-rejects-delta-temperature",
    "dimensionless-kinds-merged-by-python",
}


class Difference(NamedTuple):
    symbol: str
    dimension: str
    reference: str
    value: float
    python: object
    rust: object
    kind: str


class Comparison(NamedTuple):
    groups: dict
    reference: dict
    unresolvable: list
    differences: list


def _long_prefix_names() -> set[str]:
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    parser.read_string(core.default_conf())
    return set(parser["Prefixes"])


def _attempt(function, *args):
    try:
        return function(*args), None
    except Exception as exc:  # both sides are probed on purpose
        return None, exc


def _python_convert(value, source, target):
    return Q_(value, source).to(target).magnitude


def _rust_convert(value, source, target):
    return core.evaluate_phs_native(f"{value!r} {source} => {target}")[
        0
    ].magnitude


def _rust_knows(symbol) -> bool:
    return _attempt(_rust_convert, 1.0, symbol, symbol)[1] is None


def _long_prefixed(symbol, long_prefixes, names) -> bool:
    return any(
        symbol.startswith(prefix) and symbol[len(prefix) :] in names
        for prefix in long_prefixes
    )


def _classify(symbol, dimension, py, py_exc, rs, rs_exc, long_prefixes, names):
    """Returns None when both sides agree, otherwise the kind of difference."""
    if py_exc is None and rs_exc is None:
        if py == rs:
            return None
        if abs(py - rs) <= 4 * math.ulp(abs(py)):
            return "last-bit-difference"
        return "value-differs"
    if py_exc is None:
        if "Unknown unit" in str(rs_exc) and _long_prefixed(
            symbol, long_prefixes, names
        ):
            return "rust-rejects-long-prefix-name"
        if "Unknown unit" in str(rs_exc):
            return "rust-does-not-know-unit"
        if dimension == "O" and "Unit mismatch" in str(rs_exc):
            return "rust-rejects-delta-temperature"
        if dimension in {"A", "Dimensionless"}:
            return "dimensionless-kinds-merged-by-python"
        return "rust-error-other"
    if rs_exc is None:
        return "python-error-other"
    return None  # both raise: they agree the conversion is invalid


def _describe(value, exc):
    return value if exc is None else f"{type(exc).__name__}: {str(exc)[:60]}"


@pytest.fixture(scope="module")
def comparison() -> Comparison:
    names = set(UNIT_INDEX)
    long_prefixes = _long_prefix_names()
    groups = collections.defaultdict(list)
    unresolvable = []
    for name in sorted(names):
        try:
            groups[str(Q_(1.0, name).dimension)].append(name)
        except UnknownUnitError:
            unresolvable.append(name)
    reference = {
        dim: [
            s
            for s in sorted(members, key=lambda s: (len(s), s))
            if _rust_knows(s)
        ][:REFERENCES_PER_GROUP]
        for dim, members in groups.items()
    }
    differences = []
    for dim, members in groups.items():
        for target in reference[dim]:
            for symbol in members:
                if symbol == target:
                    continue
                for value in VALUES:
                    py, py_exc = _attempt(
                        _python_convert, value, symbol, target
                    )
                    rs, rs_exc = _attempt(_rust_convert, value, symbol, target)
                    kind = _classify(
                        symbol,
                        dim,
                        py,
                        py_exc,
                        rs,
                        rs_exc,
                        long_prefixes,
                        names,
                    )
                    if kind:
                        differences.append(
                            Difference(
                                symbol,
                                dim,
                                target,
                                value,
                                _describe(py, py_exc),
                                _describe(rs, rs_exc),
                                kind,
                            )
                        )
    return Comparison(dict(groups), reference, unresolvable, differences)


def _of_kind(comparison, kind):
    return [d for d in comparison.differences if d.kind == kind]


def _show(differences, limit=12):
    head = "\n".join(f"  {d}" for d in differences[:limit])
    more = (
        f"\n  ... and {len(differences) - limit} more"
        if len(differences) > limit
        else ""
    )
    return f"{len(differences)} difference(s):\n{head}{more}"


def test_the_comparison_covers_the_catalog(comparison):
    """Guards against a comparison that silently compares nothing."""
    assert len(comparison.groups) >= 30
    assert sum(len(m) for m in comparison.groups.values()) >= 2000


def test_every_difference_is_classified(comparison):
    unclassified = [
        d for d in comparison.differences if d.kind not in CLASSIFIED_KINDS
    ]
    assert not unclassified, _show(unclassified)


def test_last_bit_differences_are_the_known_ones(comparison):
    found = tuple(
        sorted({d.symbol for d in _of_kind(comparison, "last-bit-difference")})
    )
    assert found == KNOWN_LAST_BIT, (
        "Python and Rust disagree in the last bit for a different set of units "
        "than the one recorded.\n"
        f"  recorded: {KNOWN_LAST_BIT}\n  found:    {found}\n"
        "If the set shrank, update KNOWN_LAST_BIT; if it grew, a new unit or a "
        "change introduced an exactness divergence that needs a look."
    )


@pytest.mark.xfail(
    strict=True,
    reason=(
        "Gap for A1 Stage 1: Python accepts long prefix names ('kilometer', "
        "'attoampere'); the Rust registry only accepts symbol prefixes ('km')."
    ),
)
def test_rust_accepts_the_long_prefix_names_python_accepts(comparison):
    rejected = _of_kind(comparison, "rust-rejects-long-prefix-name")
    assert not rejected, _show(rejected)


@pytest.mark.xfail(
    strict=True,
    reason=(
        "Gap for A1 Stage 1: the PHS registry does not know the logarithmic "
        "units ('dB', 'decibel', 'pH') that the catalog and Python define."
    ),
)
def test_rust_knows_every_unit_python_knows(comparison):
    unknown = _of_kind(comparison, "rust-does-not-know-unit")
    assert not unknown, _show(unknown)


@pytest.mark.xfail(
    strict=True,
    reason=(
        "Gap for A1 Stage 1: Python converts 'delta_celsius' to 'K' (a "
        "temperature difference is a difference in kelvin); Rust rejects it "
        "as a unit mismatch."
    ),
)
def test_rust_converts_delta_temperatures_like_python(comparison):
    rejected = _of_kind(comparison, "rust-rejects-delta-temperature")
    assert not rejected, _show(rejected)


@pytest.mark.xfail(
    strict=True,
    reason=(
        "Open question for A1 Stage 1: Python files angles (rad, deg, arcmin), "
        "solid angle (sr), plain ratios (percent, ppm) and logarithmic units "
        "(dB, pH) under one dimension, so it converts arcmin to sr and percent "
        "to dB; Rust refuses. Decide which behaviour is right."
    ),
)
def test_python_does_not_mix_unrelated_dimensionless_kinds(comparison):
    merged = _of_kind(comparison, "dimensionless-kinds-merged-by-python")
    assert not merged, _show(merged)


@pytest.mark.xfail(
    strict=True,
    reason=(
        "Gap for A1 Stage 2: the generated index lists prefix-name + symbol "
        "combinations (for example 'attoA') that Python itself cannot resolve."
    ),
)
def test_every_catalog_name_can_be_resolved_by_python(comparison):
    assert not comparison.unresolvable, comparison.unresolvable[:12]


def test_incompatible_dimensions_are_rejected_by_both(comparison):
    references = sorted(
        refs[0] for refs in comparison.reference.values() if refs
    )
    accepted_by_rust = []
    accepted_by_python = []
    for source, target in itertools.pairwise(references):
        _, py_exc = _attempt(_python_convert, 1.0, source, target)
        _, rs_exc = _attempt(_rust_convert, 1.0, source, target)
        if py_exc is None:
            accepted_by_python.append((source, target))
        if rs_exc is None:
            accepted_by_rust.append((source, target))
    assert not accepted_by_python, accepted_by_python
    assert not accepted_by_rust, accepted_by_rust


def test_compound_conversions_agree_exactly():
    differing = {}
    for value, source, target in COMPOUND_CASES:
        py, py_exc = _attempt(_python_convert, value, source, target)
        rs, rs_exc = _attempt(_rust_convert, value, source, target)
        assert py_exc is None, (source, target, py_exc)
        assert rs_exc is None, (source, target, rs_exc)
        if py != rs:
            differing[(source, target)] = (py, rs)
    assert tuple(sorted(differing)) == tuple(
        sorted(KNOWN_COMPOUND_LAST_BIT)
    ), (
        f"compound conversions that differ: {differing}\n"
        f"recorded: {KNOWN_COMPOUND_LAST_BIT}"
    )


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _semantic_facts(expression):
    system = get_current_system()
    try:
        unit = Q_(1.0, expression).unit
        return {
            "dimension": {
                key: _plain(exp)
                for key, exp in sorted(
                    unit.dimension(system).exponents.items()
                )
            },
            "kind": unit.kind(system),
            "is_linear": unit.is_linear(system),
            "alias": unit.to_string(system, use_alias=True),
            "simplified": str(unit.simplify(system)),
        }
    except Exception as exc:  # an error is a fact worth freezing
        return {"error": type(exc).__name__}


def test_unit_semantics_match_the_frozen_python_behaviour():
    """Freezes what Python says about units, as the target for Stage 1.

    Rust has no counterpart for ``kind``, ``is_linear``, ``simplify`` or the
    alias formatting yet. Until Stage 1 adds them, this golden file is the
    specification the Rust implementation must reproduce.
    """
    facts = {expr: _semantic_facts(expr) for expr in SEMANTIC_EXPRESSIONS}
    if UPDATE_GOLDEN:
        GOLDEN_FILE.parent.mkdir(parents=True, exist_ok=True)
        GOLDEN_FILE.write_text(
            json.dumps(facts, indent=2, ensure_ascii=False, sort_keys=True)
            + "\n",
            encoding="utf-8",
        )
    frozen = json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))
    assert facts == frozen
```

- [ ] **Step 2: Lint and format it**

```bash
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_unit_algebra_parity.py
uv run --no-sync ruff format --check tests/core/test_unit_algebra_parity.py
```

Expected: `All checks passed!` and `1 file already formatted`. If `ruff format` wants a change, run `uv run --no-sync ruff format tests/core/test_unit_algebra_parity.py`, and report the diff it applied (only whitespace is acceptable).

- [ ] **Step 3: Generate the golden file, once**

```bash
PHYSURE_UPDATE_GOLDEN=1 uv run --no-sync pytest tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider -rx 2>&1 | tail -10
```

Expected: `6 passed, 5 xfailed`, and the five `XFAIL` lines name these tests: `test_rust_accepts_the_long_prefix_names_python_accepts`, `test_rust_knows_every_unit_python_knows`, `test_rust_converts_delta_temperatures_like_python`, `test_python_does_not_mix_unrelated_dimensionless_kinds`, `test_every_catalog_name_can_be_resolved_by_python`. Any `FAILED`, or an `XPASS(strict)`, means a number in the table above changed: stop and report with the full output.

- [ ] **Step 4: Check the golden file and that the next run is deterministic without the flag**

```bash
python -X utf8 -c "
import json
g = json.load(open('tests/core/golden/unit_semantics.json', encoding='utf-8'))
print('entries:', len(g), '| errors:', [k for k, v in g.items() if 'error' in v])
print(g['kg*m/s^2'])
print(g['degC'])
"
uv run --no-sync pytest tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider 2>&1 | tail -2
```

Expected: `entries: 32 | errors: []`; the `kg*m/s^2` entry is `{'alias': 'N', 'dimension': {'L': 1, 'M': 1, 'T': -2}, 'is_linear': True, 'kind': 'delta', 'simplified': 'kg·m/s²'}` and the `degC` entry has `'is_linear': False` and `'kind': 'absolute'`; the second command ends with `6 passed, 5 xfailed`.

---

### Task 3: Write the divergence report

**Files:**
- Create: `docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md`

- [ ] **Step 1: Reproduce the numbers in the report from the comparator itself**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 - <<'EOF'
import collections, importlib
mod = importlib.import_module("tests.core.test_unit_algebra_parity")
fixture = mod.comparison
build = getattr(fixture, "__wrapped__", None) or fixture._get_wrapped_function()
cmp_ = build()
print("groups:", len(cmp_.groups), "| unresolvable:", len(cmp_.unresolvable), "| differences:", len(cmp_.differences))
by_kind = collections.Counter(d.kind for d in cmp_.differences)
for kind, n in sorted(by_kind.items()):
    symbols = sorted({d.symbol for d in cmp_.differences if d.kind == kind})
    print(f"{kind:40s} comparisons={n:5d} symbols={len(symbols):5d} e.g. {symbols[:6]}")
EOF
```

Expected output matches the measured table at the top of this plan: `groups: 34 | unresolvable: 3720 | differences: 8877`, and by kind `rust-rejects-long-prefix-name` 8732 (2183 symbols), `last-bit-difference` 87 (46 symbols), `rust-rejects-delta-temperature` 12 (3 symbols), `dimensionless-kinds-merged-by-python` 38 (19 symbols), `rust-does-not-know-unit` 8 (2 symbols). If any number differs, use the numbers you measured in the report and say so in your final report.

- [ ] **Step 2: Create the report**

Write `docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md` with this content, replacing a number only if Step 1 measured a different one:

````markdown
# A1 Stage 0 — divergence report

**Date:** 2026-10-27
**Measured on:** local `main` after the repo-hygiene work (Python and Rust share one catalog)
**Tests:** `physure-python/tests/core/test_unit_algebra_parity.py`, golden file `physure-python/tests/core/golden/unit_semantics.json`
**Design:** `2026-10-27-a1-rust-owned-unit-algebra-design.md`

## Method

Rust is the oracle. For every name in the generated catalog that Python can resolve, the test converts 1.0 and 25.0 of that unit to two reference units of the same Python dimension, once with Python (`Q_(v, a).to(b)`) and once with the PHS `=>` operator running inside `physure-core` (`physure._core.evaluate_phs_native`). Magnitudes are compared with `==`. The references of a dimension are its two shortest symbols **that Rust knows**: one reference is not enough, because a dimension can hold several kinds of quantity (angles and solid angle; ratios and logarithmic units), and a reference of one kind hides the others. With a single reference (`dB`) the whole dimensionless group, `percent` and `ppm` included, was silently not compared.

## Scale

| | |
|---|---|
| Names in the generated catalog index | 6158 |
| Names Python itself cannot resolve | 3720 |
| Dimension groups | 34 |
| Differences | 8877 |

## The five classes of difference

| Class | Comparisons | What happens | Resolved by |
|---|---|---|---|
| `rust-rejects-long-prefix-name` | 8732 (2183 symbols) | Python accepts a long prefix name with a long unit name (`kilometer`, `attoampere`). Rust only accepts a symbol prefix with a symbol (`km`, `kA`) and answers "Unknown unit". | Stage 1: the core must accept the prefix names the catalog already defines. |
| `last-bit-difference` | 87 (46 symbols, among them `acre`, `gal`/`gallon`, `psi`, `lb`/`pound`, `oz`, `inch`, `eV`, `BTU`, `coulomb`, `degC`, `degF`, `celsius`, `fahrenheit`, `ppm`, `rad`) | The results differ by a few units in the last place. Python converts with exact `Fraction` scales and a single rounding; Rust stores scales as `f64`. One compound conversion shows it too: `g/cm^3` to `kg/m^3` is `1000.0` in Python and `999.9999999999999` in Rust. A direct example: `1 ppm => percent` is `0.0001` in Python and `9.999999999999999e-05` in Rust. | Stage 1: exact rational scales in the core. |
| `dimensionless-kinds-merged-by-python` | 38 (19 symbols) | Python files angles (`rad`, `deg`, `arcmin`, `arcsec`), solid angle (`sr`), plain ratios (`percent`, `ppm`, `unity`) and `pH` under one dimension, so it converts `arcmin` to `sr`. Rust answers "Unit mismatch". | Open question for Stage 1: Rust's stricter behaviour looks right; Python's looks like a bug. |
| `rust-rejects-delta-temperature` | 12 (3 symbols: `delta_celsius`, `degC_delta`, `delta_degC`) | Python converts a temperature difference in Celsius to kelvin one to one. Rust says "Unit mismatch". | Stage 1: decide and implement the rule in the core. |
| `rust-does-not-know-unit` | 8 (2 symbols: `dB`, `decibel`) | The PHS registry does not know the logarithmic units that the catalog and Python define. | Stage 1: register the logarithmic units in the core. |

A sixth finding is not a difference between the two sides but a defect of the generated catalog: 3720 of its 6158 names (for example `attoA`, `attoBTU`) are a long prefix name glued to a unit symbol, and Python itself raises `UnknownUnitError` for them. Stage 2 removes the generated catalog.

## What agrees

- Every unit that both sides resolve converts identically, bit for bit, except the 46 symbols above.
- 16 of 17 representative compound conversions agree exactly (`km/h`, `kg*m/s^2`, `J/s`, `kWh`, `psi`, `mmHg`, `L/min`, `eV`, `cal`, `BTU/h`, `hp`, `degC`, `degF`, `mol/L`, `ft*lbf`, `N*m`).
- Both sides reject conversions between incompatible dimensions (33 cross-dimension pairs checked).

## What Rust cannot answer yet

`kind` (absolute or delta), `is_linear`, `simplify` and alias formatting have no Rust counterpart. Their current Python results for 32 units and compounds are frozen in the golden file, which becomes the specification for Stage 1.

## How to use this

Each class has a strict `xfail` test (or a recorded set, for the last-bit differences). When Stage 1 or 2 closes a gap, the matching test turns into an unexpected pass, which fails the suite until its `xfail` marker is removed. `KNOWN_LAST_BIT` and `KNOWN_COMPOUND_LAST_BIT` become empty when the core has exact scales.
````

- [ ] **Step 3: Check the report contains no relative links and renders**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
grep -nE "\]\((\.\./|\./)?[A-Za-z_./-]+\.(rs|py|toml|md|yml|conf)\)" docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md; echo "link-scan-exit=$?"
```

Expected: no matches and `link-scan-exit=1`.

---

### Task 4: Verify the whole suite

**Files:** none

- [ ] **Step 1: Lint and format the project**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
```

Expected: `All checks passed!` and every file formatted.

- [ ] **Step 2: Run the full suite, as CI does**

```bash
uv run --no-sync pytest -q --ignore=tests/core/test_serialization.py 2>&1 | tail -4
```

Expected: `910 passed, 6 xfailed` (the previous 904 passed and 1 xfailed, plus 6 passed and 5 xfailed from this stage). The run takes about 20 s longer than before. If a test other than the new ones fails, run it alone to see whether the new file's module fixture leaked state; report it, do not change application code.

- [ ] **Step 3: The docs build is unaffected (the new report lives under an excluded folder)**

```bash
uv run --no-sync mkdocs build --strict 2>&1 | grep -E "ERROR|WARNING|Documentation built|Aborted" | grep -v "Div at"; echo "build-exit=${PIPESTATUS[0]}"
test ! -e site/superpowers && echo "superpowers excluded from the site"
cd ..
git status --short
```

Expected: `Documentation built in ... seconds`, `build-exit=0`, `superpowers excluded from the site`, and `git status --short` listing only `?? physure-python/tests/core/test_unit_algebra_parity.py`, `?? physure-python/tests/core/golden/` and `?? docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md`. If the docs build touched a tracked file, report it and do not revert anything by hand.

---

### Task 5: Commit

**Files:** the three paths above.

- [ ] **Step 1: Commit the tests and the golden file**

```bash
git add physure-python/tests/core/test_unit_algebra_parity.py physure-python/tests/core/golden/unit_semantics.json
git diff --cached --name-status
git commit -m "test(units): compare Python's unit algebra with the Rust core over the whole catalog" -m "A1 Stage 0 safety net. Rust is the oracle (PHS => runs in physure-core's registry); Python is compared with exact equality over every resolvable catalog name and 17 compound conversions. Differences fall into five named classes, each a strict xfail or a recorded set, so the gaps can only shrink. Python's kind/is_linear/simplify/alias behaviour, which Rust cannot answer yet, is frozen in a golden file as the target for Stage 1." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `A` for the test file and `A` for `physure-python/tests/core/golden/unit_semantics.json`, nothing else.

- [ ] **Step 2: Commit the divergence report**

```bash
git add docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md
git diff --cached --name-status
git commit -m "docs(spec): A1 Stage 0 divergence report" -m "Five classes of difference between Python and the Rust core, measured over the whole catalog with two Rust-known references per dimension, plus the 3720 generated names Python itself cannot resolve." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git status --short
git log --oneline -3
```

Expected: a single `A` for the report; `git status --short` empty; two new commits on `test/a1-stage0-unit-parity`.

- [ ] **Step 3: Hand-off**

Report to the maintainer: the branch name, the two commit SHAs, the measured numbers (they should match the table above), and that nothing was pushed. Integrating the branch (local merge or pull request) is the maintainer's decision.
