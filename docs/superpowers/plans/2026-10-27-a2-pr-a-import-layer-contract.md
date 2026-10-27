# A2 PR A: the import-layer contract — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write down which part of the `physure` Python package may import which, and make a test enforce it, with today's 26 violations recorded as exceptions that can only shrink. No behaviour change: this is the baseline the other three A2 pull requests (B, C, D) start from.

**Architecture:** A layer map and the list of exceptions live in one TOML file. One pytest module reads the package's source with `ast` (nothing is imported, no dependency is added), classifies every module, collects every import as `top` (runs at import time), `lazy` (inside a function) or `typing` (under `TYPE_CHECKING`), and checks the rule. It fails on a new violation, on an exception that no longer matches the code, and on a module that belongs to no layer. Design: `docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md`.

**Tech Stack:** Python 3.11+ (`ast`, `tomllib` from the standard library), pytest, ruff.

## What was measured while planning (use it to check your results)

| Measure | Value |
|---|---|
| Modules in `physure-python/physure/` | 92 |
| Imports found of `physure` modules | 317: 122 `top`, 113 `lazy`, 82 `typing` |
| Layers | 6: `kernel`, `backends`, `model`, `services`, `extensions`, `facade` |
| Violations of the rule | 26 import statements, in 23 exception records |
| Tagged `a1` (A1 deletes the module involved) | 8 imports |
| Tagged `a2a` (fixed by A2 PRs B, C, D) | 18 imports: 10 import the facade, 5 are `backends`/`_jit` reaching `Quantity`, 3 are the model reaching services |
| Tests added | 4, run time about 0.5 s |

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `test/a2-import-layer-contract` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/`, and always use `uv run --no-sync ...`. Never run `maturin develop` or `uv sync` in this plan.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files with non-ASCII text.
- This PR adds a test, a data file and documentation. Do not edit any file under `physure-python/physure/` except in the mutation checks of Task 3, each of which is reverted in the same step.

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

Expected: `git status --short` prints nothing; the two `log` lines show the same commit. If `physure-python/tests/core/test_import_layers.py` or `physure-python/tests/core/import_layers.toml` show as untracked, they are leftovers of the planning session: confirm they are the only entries, then delete exactly those two paths (Task 2 recreates them). Any other untracked or modified file (apart from `Cargo.lock`): stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c test/a2-import-layer-contract main
git status -sb | head -1
```

Expected: `## test/a2-import-layer-contract`.

- [ ] **Step 3: Check that the environment works**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync python -c "import physure._core as c; print(c.__file__)"
uv run --no-sync python -c "import tomllib, ast; print('stdlib ok')"
```

Expected: the path is inside `.claude\worktrees\single-conf\physure-python`, and `stdlib ok`.

---

### Task 2: Add the layer map and the test

**Files:**
- Create: `physure-python/tests/core/import_layers.toml`
- Create: `physure-python/tests/core/test_import_layers.py`

- [ ] **Step 1: Create the layer map**

Create `physure-python/tests/core/import_layers.toml` with exactly this content:

```toml
# The import-layer contract of the physure Python package.
#
# Layers run from the lowest to the highest. A module may import from its own
# layer or from any lower one. Two relaxations:
#   * an import inside a function (a deferred import) may also reach the layers
#     in `lazy_may_reach`: optional features with heavy dependencies, such as
#     Quantity.plot() or Quantity.to_hdf();
#   * imports under `if TYPE_CHECKING:` are not checked.
# The facade (the public API) may import from anywhere, but nothing may import it
# from inside the package: use the module that really defines the name.
#
# Modules are named with their full dotted path. A pattern ending in ".*" means
# the module itself and everything below it; any other pattern is one exact
# module. The most specific pattern wins, so a package can be split by listing a
# few of its modules in a lower layer. Every module must match a pattern.
#
# `[[exception]]` records an import that breaks the rule today. The test fails on
# a violation that is not recorded, and also on a record that no longer matches
# the code, so this list can only shrink. `cause` says who will remove it:
#   a1  - the module involved is deleted by the A1 work (Rust owns the unit algebra)
#   a2a - fixed by the A2 pull requests that clean up the imports
# Spec: docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md

lazy_may_reach = ["extensions"]

[[layer]]
name = "kernel"
modules = [
    "physure.core.protocols",
    "physure.core.dispatcher",
    "physure.core.registry",
    "physure.core.formatting",
    "physure.core.autograd",
    "physure.domain.exceptions",
    "physure.application.context",
    "physure.application.tracing.context",
    "physure._generated_types",
    "physure.infrastructure.*",
]

[[layer]]
name = "backends"
modules = [
    "physure.backends.*",
    "physure._jit.*",
]

[[layer]]
name = "model"
modules = [
    "physure.domain.*",
    "physure.units.*",
]

[[layer]]
name = "services"
modules = [
    "physure.application.*",
    "physure.core.*",
    "physure.functional",
    "physure.module",
    "physure.symbolic",
]

[[layer]]
name = "extensions"
modules = [
    "physure.ext.*",
    "physure.nn.*",
    "physure.plotting",
    "physure.pandas_ext",
]

[[layer]]
name = "facade"
modules = [
    "physure",
    "physure.cli",
    "physure.repl",
    "physure.__main__",
    "physure.constants",
    "physure.scripts.*",
]

# --- recorded exceptions ---------------------------------------------------

[[exception]]
importer = "physure.application.functions.functions"
imported = "physure"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure.backends.torch_backend"
imported = "physure.domain.measurement.quantity"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure.domain.measurement.system"
imported = "physure.application.factories"
kind = "top"
cause = "a1"

[[exception]]
importer = "physure.domain.measurement.system"
imported = "physure.application.parsing"
kind = "top"
cause = "a1"

[[exception]]
importer = "physure.domain.symbolic.expression"
imported = "physure"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure.domain.symbolic.expression"
imported = "physure.application.functions.functions"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure.domain.symbolic.quantity"
imported = "physure"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure.ext.io"
imported = "physure"
kind = "top"
cause = "a2a"

[[exception]]
importer = "physure._jit.tracer"
imported = "physure.domain.measurement.quantity"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure._jit.tracer"
imported = "physure.domain.measurement.units"
kind = "lazy"
cause = "a1"

[[exception]]
importer = "physure.application.context"
imported = "physure.application.startup"
kind = "lazy"
count = 2
cause = "a1"

[[exception]]
importer = "physure.application.tracing.context"
imported = "physure.domain.symbolic.tracer"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.backends.jax_backend"
imported = "physure.domain.measurement.quantity"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.backends.jax_backend"
imported = "physure.domain.measurement.uncertainty"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.backends.torch.autograd_store"
imported = "physure.domain.measurement.vectorized_uncertainty"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.domain.measurement.dimensions"
imported = "physure.application.parsing"
kind = "lazy"
cause = "a1"

[[exception]]
importer = "physure.domain.measurement.quantity"
imported = "physure.application.factories"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.domain.measurement.quantity"
imported = "physure.application.startup"
kind = "lazy"
cause = "a1"

[[exception]]
importer = "physure.ext.chemistry.equivalency"
imported = "physure"
kind = "lazy"
count = 2
cause = "a2a"

[[exception]]
importer = "physure.ext.chemistry.species"
imported = "physure"
kind = "lazy"
cause = "a2a"

[[exception]]
importer = "physure.ext.chemistry.thermo_kinetics"
imported = "physure"
kind = "lazy"
count = 2
cause = "a2a"

[[exception]]
importer = "physure.infrastructure.config.systems"
imported = "physure.application.startup"
kind = "lazy"
cause = "a1"

[[exception]]
importer = "physure.module"
imported = "physure"
kind = "lazy"
cause = "a2a"
```

- [ ] **Step 2: Create the test**

Create `physure-python/tests/core/test_import_layers.py` with exactly this content:

```python
"""The import-layer contract: which part of physure may import which.

The rule and the layer map live in ``import_layers.toml``. This test reads the
package's source with ``ast`` (nothing is imported, so it needs no optional
dependency) and checks three things:

* every module belongs to a layer, so a new file has to be classified;
* no import breaks the rule unless it is recorded as an exception;
* every recorded exception still happens, so the list can only shrink.
"""

from __future__ import annotations

import ast
import collections
import tomllib
from pathlib import Path
from typing import NamedTuple

import pytest

PACKAGE = "physure"
SOURCE = Path(__file__).resolve().parents[2] / PACKAGE
CONTRACT = Path(__file__).with_name("import_layers.toml")

# The compiled extension is not a Python module of this tree.
EXTERNAL = (f"{PACKAGE}._core",)


class Edge(NamedTuple):
    importer: str
    imported: str
    kind: str  # "top" (runs at import), "lazy" (inside a function), "typing"
    path: str
    line: int


def _module_name(path: Path) -> str:
    parts = list(path.relative_to(SOURCE.parent).with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


class _Collector(ast.NodeVisitor):
    def __init__(self, module: str, path: Path, known: set[str]):
        self.module = module
        self.path = path
        self.known = known
        self.is_package = path.name == "__init__.py"
        self.in_function = 0
        self.in_typing = 0
        self.edges: list[Edge] = []

    def _resolve(self, name: str) -> str | None:
        if name == EXTERNAL[0] or name.startswith(EXTERNAL[0] + "."):
            return None
        parts = name.split(".")
        for end in range(len(parts), 0, -1):
            candidate = ".".join(parts[:end])
            if candidate in self.known:
                return candidate
        return None

    def _add(self, name: str, line: int) -> None:
        target = self._resolve(name)
        if target is None or target == self.module:
            return
        kind = (
            "typing"
            if self.in_typing
            else "lazy"
            if self.in_function
            else "top"
        )
        self.edges.append(
            Edge(self.module, target, kind, str(self.path), line)
        )

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self.in_function += 1
        self.generic_visit(node)
        self.in_function -= 1

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self.in_function += 1
        self.generic_visit(node)
        self.in_function -= 1

    def visit_If(self, node: ast.If) -> None:
        if "TYPE_CHECKING" in ast.unparse(node.test):
            self.in_typing += 1
            for child in node.body:
                self.visit(child)
            self.in_typing -= 1
            for child in node.orelse:
                self.visit(child)
        else:
            self.generic_visit(node)

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            if alias.name == PACKAGE or alias.name.startswith(PACKAGE + "."):
                self._add(alias.name, node.lineno)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        base = node.module or ""
        if node.level:
            package = self.module.split(".")
            if not self.is_package:
                package = package[:-1]
            package = package[: len(package) - (node.level - 1)]
            base = ".".join(
                [*package, *([node.module] if node.module else [])]
            )
        if not (base == PACKAGE or base.startswith(PACKAGE + ".")):
            return
        if base == EXTERNAL[0] or base.startswith(EXTERNAL[0] + "."):
            return
        found_submodule = False
        for (
            alias
        ) in node.names:  # `from physure.a import b` may import module b
            if f"{base}.{alias.name}" in self.known:
                self._add(f"{base}.{alias.name}", node.lineno)
                found_submodule = True
        if not found_submodule:
            self._add(base, node.lineno)


def _read_edges() -> tuple[set[str], list[Edge]]:
    paths = sorted(SOURCE.rglob("*.py"))
    known = {_module_name(path) for path in paths}
    edges: list[Edge] = []
    for path in paths:
        collector = _Collector(_module_name(path), path, known)
        collector.visit(ast.parse(path.read_text(encoding="utf-8")))
        edges.extend(collector.edges)
    return known, edges


class Contract(NamedTuple):
    rank: dict
    patterns: list
    lazy_may_reach: set
    exceptions: dict


def _load_contract() -> Contract:
    data = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
    rank = {layer["name"]: index for index, layer in enumerate(data["layer"])}
    patterns = [
        (pattern, layer["name"])
        for layer in data["layer"]
        for pattern in layer["modules"]
    ]
    exceptions = {
        (item["importer"], item["imported"], item["kind"]): item.get(
            "count", 1
        )
        for item in data.get("exception", [])
    }
    return Contract(rank, patterns, set(data["lazy_may_reach"]), exceptions)


def _layer_of(module: str, contract: Contract) -> str | None:
    best: tuple[int, str] | None = None
    for pattern, layer in contract.patterns:
        if pattern.endswith(".*"):
            base = pattern[:-2]
            hit = module == base or module.startswith(base + ".")
            weight = len(base)
        else:
            hit = module == pattern
            weight = len(pattern) + 1  # an exact module beats its package
        if hit and (best is None or weight > best[0]):
            best = (weight, layer)
    return best[1] if best else None


def _violations(edges, contract) -> dict:
    found = collections.defaultdict(list)
    for edge in edges:
        if edge.kind == "typing":
            continue
        source = _layer_of(edge.importer, contract)
        target = _layer_of(edge.imported, contract)
        if source is None or target is None or source == "facade":
            continue
        if contract.rank[target] <= contract.rank[source]:
            continue
        if edge.kind == "lazy" and target in contract.lazy_may_reach:
            continue
        found[(edge.importer, edge.imported, edge.kind)].append(edge)
    return found


def _toml_block(key, count, cause="a2a") -> str:
    importer, imported, kind = key
    lines = [
        "[[exception]]",
        f'importer = "{importer}"',
        f'imported = "{imported}"',
        f'kind = "{kind}"',
    ]
    if count != 1:
        lines.append(f"count = {count}")
    lines.append(f'cause = "{cause}"')
    return "\n".join(lines)


@pytest.fixture(scope="module")
def analysis():
    known, edges = _read_edges()
    contract = _load_contract()
    return known, edges, contract, _violations(edges, contract)


def test_the_contract_reads_the_whole_package(analysis):
    """Guards against a contract that silently checks nothing."""
    known, edges, _, _ = analysis
    assert len(known) >= 80
    assert len(edges) >= 250


def test_every_module_belongs_to_a_layer(analysis):
    known, _, contract, _ = analysis
    unclassified = sorted(m for m in known if _layer_of(m, contract) is None)
    assert not unclassified, (
        "These modules match no pattern in tests/core/import_layers.toml. "
        "Add each one to the layer it belongs to:\n  "
        + "\n  ".join(unclassified)
    )


def test_no_import_breaks_the_layering_unrecorded(analysis):
    _, _, contract, violations = analysis
    problems = []
    for key, edges in sorted(violations.items()):
        recorded = contract.exceptions.get(key, 0)
        if len(edges) <= recorded:
            continue
        importer, imported, kind = key
        where = ", ".join(f"line {e.line}" for e in edges)
        problems.append(
            f"{importer} ({_layer_of(importer, contract)}) imports "
            f"{imported} ({_layer_of(imported, contract)}), "
            f"{kind}, at {Path(edges[0].path).name} {where}\n"
            f"{_toml_block(key, len(edges))}"
        )
    assert not problems, (
        "Imports that go up the layers (see the rule in import_layers.toml). "
        "Import from a lower layer, or move the code. Only if the import is "
        "temporary and someone owns removing it, record it:\n\n"
        + "\n\n".join(problems)
    )


def test_every_recorded_exception_still_happens(analysis):
    _, _, contract, violations = analysis
    stale = []
    for key, recorded in sorted(contract.exceptions.items()):
        actual = len(violations.get(key, ()))
        if actual != recorded:
            importer, imported, kind = key
            stale.append(
                f"{importer} -> {imported} ({kind}): recorded {recorded}, "
                f"found {actual}"
            )
    assert not stale, (
        "These exceptions in import_layers.toml no longer match the code. "
        "Delete them, or lower their count, now that the imports are gone:\n  "
        + "\n  ".join(stale)
    )
```

- [ ] **Step 3: Extract both files from this plan instead of retyping them (recommended)**

Both blocks above were validated as they are. If you are an agent, write them by extracting them from this plan file, which guarantees they are byte-identical:

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 - <<'EOF'
import pathlib, re
FENCE = chr(96) * 3
plan = pathlib.Path(r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a2-pr-a-import-layer-contract.md").read_text(encoding="utf-8")
def block(after, lang):
    m = re.search(re.escape(after) + r"\n\n" + FENCE + lang + r"\n(.*?)\n" + FENCE + r"\n", plan, re.S)
    assert m, f"block not found after: {after}"
    return m.group(1) + "\n"
root = pathlib.Path(r"D:\Projects\physure\.claude\worktrees\single-conf\physure-python\tests\core")
(root / "import_layers.toml").write_text(block("Create `physure-python/tests/core/import_layers.toml` with exactly this content:", "toml"), encoding="utf-8", newline="\n")
(root / "test_import_layers.py").write_text(block("Create `physure-python/tests/core/test_import_layers.py` with exactly this content:", "python"), encoding="utf-8", newline="\n")
print("written both files")
EOF
```

Expected: `written both files`.

- [ ] **Step 4: Lint and format the test**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_import_layers.py
uv run --no-sync ruff format --check tests/core/test_import_layers.py
```

Expected: `All checks passed!` and `1 file already formatted`.

- [ ] **Step 5: Run it**

```bash
uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -3
```

Expected: `4 passed`. If a test fails, stop and report the full output: a number measured while planning changed (for instance a new module or a new import appeared on `main`).

---

### Task 3: Prove that the test can fail

A test that only ever passes proves nothing. Each mutation below changes one thing, checks that the test fails with the expected message, and reverts it. Run the whole block as one command.

**Files:** none are kept modified.

- [ ] **Step 1: Run the four mutations**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
cp tests/core/import_layers.toml /tmp/import_layers.toml.bak
cp physure/core/protocols.py /tmp/protocols.py.bak
run() { uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | grep -E "^E  |passed|failed" | grep -vE "^E\s*$" | head -12 | cut -c1-175; }

echo "=== baseline ==="; run
echo "=== MUTATION 1: a new import that goes up (kernel -> services), under 'if False' so nothing executes ==="
printf '\nif False:  # mutation check\n    from physure.application.factories import QuantityFactory\n' >> physure/core/protocols.py
run
cp /tmp/protocols.py.bak physure/core/protocols.py
echo "=== MUTATION 2: a stale exception (an import that does not exist) ==="
printf '\n[[exception]]\nimporter = "physure.core.protocols"\nimported = "physure.application.factories"\nkind = "top"\ncause = "a2a"\n' >> tests/core/import_layers.toml
run
cp /tmp/import_layers.toml.bak tests/core/import_layers.toml
echo "=== MUTATION 3: a count that is out of date (2 -> 3) ==="
python -X utf8 -c "
import pathlib; p = pathlib.Path('tests/core/import_layers.toml'); s = p.read_text(encoding='utf-8')
assert s.count('count = 2\ncause = \"a1\"') == 1
p.write_text(s.replace('count = 2\ncause = \"a1\"', 'count = 3\ncause = \"a1\"'), encoding='utf-8', newline='\n')"
run
cp /tmp/import_layers.toml.bak tests/core/import_layers.toml
echo "=== MUTATION 4: a new module nobody classified ==="
printf 'x = 1\n' > physure/zz_unclassified.py
run
rm -f physure/zz_unclassified.py
echo "=== after reverting everything ==="; run
git status --short
```

Expected, in order:
1. baseline: `4 passed`;
2. mutation 1: `1 failed, 3 passed`, with a line `physure.core.protocols (kernel) imports physure.application.factories (services), top, at protocols.py line N` followed by a ready-to-paste `[[exception]]` block;
3. mutation 2: `1 failed, 3 passed`, with `physure.core.protocols -> physure.application.factories (top): recorded 1, found 0`;
4. mutation 3: `1 failed, 3 passed`, with `physure.application.context -> physure.application.startup (lazy): recorded 3, found 2`;
5. mutation 4: `1 failed, 3 passed`, with `physure.zz_unclassified` listed as matching no pattern;
6. after reverting: `4 passed`, and `git status --short` lists only `?? physure-python/tests/core/import_layers.toml` and `?? physure-python/tests/core/test_import_layers.py` (plus possibly ` M Cargo.lock`).

If any mutation does NOT fail, or `git status` shows `physure/core/protocols.py` modified, stop and report: do not commit.

---

### Task 4: Document the rule

**Files:**
- Modify: `structure.md` (new section before the closing note)
- Modify: `CONTRIBUTING.md` (one paragraph after the package layout block)

Both files may have CRLF line endings in the working tree; the script below detects and preserves them.

- [ ] **Step 1: Apply both edits**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 - <<'EOF'
import pathlib

FENCE = chr(96) * 3

def edit(path, old, new):
    p = pathlib.Path(path)
    raw = p.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    old, new = old.replace("\n", eol), new.replace("\n", eol)
    assert raw.count(old) == 1, f"{path}: anchor found {raw.count(old)} times"
    p.write_bytes(raw.replace(old, new).encode("utf-8"))
    print("edited", path, "(CRLF)" if eol == "\r\n" else "(LF)")

SECTION = """## Python import layers

Inside `physure-python/physure/`, imports follow a layer contract that a test enforces: `physure-python/tests/core/test_import_layers.py` reads the layer map in `physure-python/tests/core/import_layers.toml`. Layers are assigned per module, not per package: `application/context.py`, for example, is `kernel` (state every layer needs) although it lives in `application/`.

| # | Layer | What lives here |
|---|---|---|
| 1 | `kernel` | backend interfaces, the dispatcher, the registry, exceptions, the active-system context |
| 2 | `backends` | NumPy/Torch/JAX adapters and the tracer |
| 3 | `model` | `Quantity`, units, dimensions, uncertainty |
| 4 | `services` | factories, fit and solve services, I/O, symbolic helpers |
| 5 | `extensions` | `ext/`, `nn/`, `plotting`, pandas |
| 6 | `facade` | the public API (`physure/__init__.py`), the CLI and the REPL |

A module may import from its own layer or a lower one. A deferred import (inside a function) may also reach `extensions`, which is how `Quantity.plot()` uses matplotlib only when it is called. Imports under `if TYPE_CHECKING:` are not checked. Nothing inside the package imports the facade: import the module that defines the name. A few imports break the rule today and are recorded as exceptions in the TOML file (`a1`: removed when A1 deletes the module; `a2a`: removed by the import clean-up). The test fails if a record no longer matches the code, so the list only shrinks. The design is in `docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md`.

"""
edit("structure.md", "**Note:** if this file and `CLAUDE.md`'s architecture section ever disagree", SECTION + "**Note:** if this file and `CLAUDE.md`'s architecture section ever disagree")

PARAGRAPH = "ext/, nn/       optional integrations, unit-aware torch layers\n" + FENCE + """

Which package may import which is a rule, and a test enforces it: `tests/core/test_import_layers.py` reads the layer map in `tests/core/import_layers.toml` and fails on an import that goes up the layers (see "Python import layers" in `structure.md`). When it fails it prints the importing module, the imported module, the layer of each and the line. Import from a lower layer instead of the package facade (`from physure import X` inside the package is always wrong), put a new module in a layer in the TOML file, and record an exception only if the import is temporary and someone owns removing it.
"""
edit("CONTRIBUTING.md", "ext/, nn/       optional integrations, unit-aware torch layers\n" + FENCE + "\n", PARAGRAPH)
EOF
git diff --stat structure.md CONTRIBUTING.md | tail -3
```

Expected: both files are edited; `structure.md` gains 15 lines and `CONTRIBUTING.md` 2 (one long paragraph and a blank line), and no whole-file line-ending churn (the stat must not show hundreds of changed lines).

- [ ] **Step 2: Check the documentation renders and has no relative links**

```bash
grep -nE "\]\((\.\./|\./)?[A-Za-z_./-]+\.(rs|py|toml|md|yml|conf)\)" structure.md CONTRIBUTING.md; echo "link-scan-exit=$?"
grep -c "Python import layers" structure.md CONTRIBUTING.md
```

Expected: no matches and `link-scan-exit=1`; one match in each file.

---

### Task 5: Verify the whole project

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
uv run --no-sync pytest -q --ignore=tests/core/test_serialization.py 2>&1 | tail -3
```

Expected: `914 passed, 6 xfailed` (the previous 910 passed and 6 xfailed, plus the 4 new tests). If any other test fails, run it alone and report; do not change application code.

- [ ] **Step 3: The docs build is unaffected**

```bash
uv run --no-sync mkdocs build --strict 2>&1 | grep -E "ERROR|WARNING|Documentation built|Aborted" | grep -v "Div at"; echo "build-exit=${PIPESTATUS[0]}"
cd ..
git status --short
```

Expected: `Documentation built in ... seconds`, `build-exit=0`, and `git status --short` listing the two new test files and the two modified documents (plus possibly ` M Cargo.lock`).

---

### Task 6: Commit

- [ ] **Step 1: Commit the contract and its test**

```bash
git add physure-python/tests/core/import_layers.toml physure-python/tests/core/test_import_layers.py
git diff --cached --name-status
git commit -m "test(layers): enforce the package's import layering with a recorded baseline" -m "Six layers assigned per module (kernel, backends, model, services, extensions, facade); a module may import from its own or a lower layer, a deferred import may also reach the optional extensions, and nothing inside the package imports the facade. A stdlib-only ast test fails on a new violation, on an exception that no longer matches the code and on a module that has no layer. Today's 26 violating imports are recorded in 23 exceptions: 8 disappear when A1 deletes the modules involved, 18 are fixed by the next A2 changes." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: two `A` entries, nothing else.

- [ ] **Step 2: Commit the documentation**

```bash
git add structure.md CONTRIBUTING.md
git diff --cached --name-status
git commit -m "docs: document the import layers and how the test reports a violation" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git status --short
git log --oneline -3
```

Expected: two `M` entries; `git status --short` empty (or only ` M Cargo.lock`); two new commits on `test/a2-import-layer-contract`.

- [ ] **Step 3: Hand-off**

Report the branch name, the two commit SHAs and the measured numbers (they should match the table above). Nothing is pushed. Integrating the branch is the maintainer's decision; the next plans (A2 PRs B, C and D) start from this baseline, and each one deletes the exception records it fixes.
