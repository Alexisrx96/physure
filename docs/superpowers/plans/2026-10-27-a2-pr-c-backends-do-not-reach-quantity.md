# A2 PR C: the backends and `_jit` stop reaching the model — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `physure.backends` and `physure._jit` independent of `Quantity` and the uncertainty models, closing the five `a2a` import exceptions that point from the backends layer up into the model, and remove the two stray `print("DEBUG: ...")` calls. The layering contract from PR A proves it: its exception list shrinks by exactly these imports.

**Architecture:** Four commits, one per mechanism. (1) The two debug prints go. (2) The backends no longer import the model to register its pytrees: a backend *announces* itself on the dispatcher when it loads (`BackendManager.announce("torch")`), and the model *subscribes* with `BackendManager.on_load(...)` and registers its own torch and jax pytrees. (3) `propagate_affine`, which only needs a backend, moves into `physure/backends/affine.py`; its old import path keeps working. (4) `jit`, the only part of `_jit/tracer.py` that builds `Quantity`, moves to `physure/application/jit.py` (services layer); the tracing primitives stay. Design: `docs/superpowers/specs/2026-10-27-a2-layering-contract-design.md`, row "C".

**Tech Stack:** Python 3.11+, pytest, ruff. torch and jax are installed in the development environment and their tests run. No Rust changes.

## What was measured while planning (use it to check your results)

The change was implemented and verified on a prototype branch before this plan was written.

| Measure | Before | After |
|---|---|---|
| `print("DEBUG: ...")` in library code | 2 (`torch_backend.py:544`, `torch/autograd_store.py:90`) | 0 |
| Imports from `backends` / `_jit` up into the model | 5 `a2a` + 1 `a1` (`_jit.tracer` → `units`) | 0 |
| Exception records | 16 | 10: 3 `a2a` (PR D), 6 `a1`, 1 `release` |
| Records after each commit | 16 | 16 after the first, 13 after the second, 12 after the third, 10 after the fourth |
| New tests | — | 10 (1 + 7 + 1 + 1), all passing |
| Full suite | 923 passed, 6 xfailed | **933 passed, 6 xfailed** |
| First use (`python -m physure "500 N / 2 m^2 => kPa"`) | 0.11 s | 0.11 s, prints `0.25 kPa` |
| `import physure` loads torch, jax or `physure.application.jit` | no | no |

## Decisions taken while planning

- **Backends announce, the model subscribes.** The spec said the dispatcher calls a hook when a backend loads. The backends already call into the dispatcher (`enforce_tensor_contract`), so the cheapest faithful form is: the backend module calls `BackendManager.announce("torch")` once its library is in play, and the model registers hooks with `BackendManager.on_load("torch", fn)`. A hook registered after the announcement runs at once, so import order never matters, and each hook is idempotent. The registry lives in `core/dispatcher.py`, which is already in the `kernel` layer, so the layer map does not change. `clear_backends()` does not clear the registry: the registrations are import-time facts, not caches.
- **`register_jax_behavior()` stays** in `jax_backend.py` as the one-line announcement. The dispatcher and two existing tests call it by that name.
- **`propagate_affine` moves whole.** The spec left open whether it drags the global covariance stores with it. It does not: it is a pure function of a matrix, slices and a backend. `vectorized_uncertainty.py` re-exports it (`functional.py`, `application/io.py`, `plotting.py` and several tests import that path).
- **`jit` goes to `application/jit.py`**, and `physure/_jit/__init__.py` stops exporting it (importing it there would be a `backends` → `services` import). `_jit` is private; the facade (`physure.jit`, `from physure import jit`) is unchanged. This also removes the `a1` record `_jit.tracer` → `units`, because `jit` is what imported it.

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `refactor/a2-backends-below-the-model` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/`, and always use `uv run --no-sync ...`. Never run `maturin develop` or `uv sync`.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` may be rewritten by an IDE tool: never stage it.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files.
- The source edits are made by one script, `apply_pr_c.py`, in parts, so that each commit leaves the contract consistent with the code. Do not edit the sources by hand. The test files are written by extracting the blocks shown in this plan with `plan_block.py` (Task 1), never retyped.
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
git switch -c refactor/a2-backends-below-the-model main
git status -sb | head -1
```

Expected: `## refactor/a2-backends-below-the-model`.

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
    r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a2-pr-c-backends-do-not-reach-quantity.md"
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

- [ ] **Step 4: Extract `apply_pr_c.py` (defined at the end of this plan)**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `apply_pr_c.py` with exactly this content:' python /c/Users/Irvin/AppData/Local/Temp/apply_pr_c.py
```

Expected: `written ... | 377 lines`. An `AssertionError: block not found` means the plan file is not at the path inside `plan_block.py`: stop and report.

---

### Task 2: Remove the two debug prints

**Files:**
- Create: `physure-python/tests/core/test_no_debug_output.py`
- Modify: `physure-python/physure/backends/torch_backend.py`
- Modify: `physure-python/physure/backends/torch/autograd_store.py`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Write the test and watch it fail**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/core/test_no_debug_output.py` with exactly this content:' python physure-python/tests/core/test_no_debug_output.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_no_debug_output.py && uv run --no-sync ruff format --check tests/core/test_no_debug_output.py
uv run --no-sync pytest tests/core/test_no_debug_output.py -q -p no:cacheprovider 2>&1 | grep -E "debug prints|passed|failed"
```

The block written by the first command is:

Create `physure-python/tests/core/test_no_debug_output.py` with exactly this content:

```python
"""Library code must not print debugging output."""

import ast
import pathlib

import physure


def test_the_library_prints_no_debug_output():
    root = pathlib.Path(physure.__file__).parent
    found = []
    for path in sorted(root.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Name)
                and node.func.id == "print"
                and node.args
                and "DEBUG" in ast.unparse(node.args[0])
            ):
                found.append(f"{path.relative_to(root)}:{node.lineno}")
    assert not found, f"debug prints left in the library: {found}"
```

Expected: ruff passes; `1 failed`, and the assertion message names exactly two places: `backends\torch\autograd_store.py:90` and `backends\torch_backend.py:544` (with `/` instead of `\` on non-Windows). If it names a different place or none, stop and report.

- [ ] **Step 2: Apply the `debug` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_c.py debug
```

Expected: `edited` lines for the two backend files and `CHANGELOG.md`. Any `AssertionError` means an anchor changed: stop and report.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_no_debug_output.py tests/core/test_import_layers.py tests/ext/test_autograd.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean; `6 passed, 1 warning` (the warning is an unrelated torch sparse-tensor `UserWarning`); `16` records.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/backends/torch_backend.py physure-python/physure/backends/torch/autograd_store.py physure-python/tests/core/test_no_debug_output.py CHANGELOG.md
git diff --cached --name-status
git commit -m "fix(torch): stop printing DEBUG lines from the torch backend" -m "TorchBackend.sparse_matmul and AutogradCovarianceStore.propagate wrote a debugging line to standard output on every call. A test reads the package with ast and fails if a print of a DEBUG message comes back." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `M` for the two backends and `CHANGELOG.md`, `A` for the test; nothing else.

---

### Task 3: The model registers its own pytrees

**Files:**
- Create: `physure-python/tests/core/test_backend_hooks.py`
- Modify: `physure-python/physure/core/dispatcher.py`
- Modify: `physure-python/physure/backends/jax_backend.py`
- Modify: `physure-python/physure/backends/torch_backend.py`
- Modify: `physure-python/physure/domain/measurement/quantity.py`
- Modify: `physure-python/physure/domain/measurement/uncertainty.py`
- Modify: `physure-python/tests/core/import_layers.toml` (three records removed)

- [ ] **Step 1: Write the test and watch it fail**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/core/test_backend_hooks.py` with exactly this content:' python physure-python/tests/core/test_backend_hooks.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_backend_hooks.py && uv run --no-sync ruff format --check tests/core/test_backend_hooks.py
uv run --no-sync pytest tests/core/test_backend_hooks.py -q -p no:cacheprovider 2>&1 | tail -1
```

The block written by the first command is:

Create `physure-python/tests/core/test_backend_hooks.py` with exactly this content:

```python
"""A backend announces itself when it loads; the model subscribes to be told.

The model registers its own torch and jax pytrees. A backend module used to
import `Quantity` to do it, which put the model underneath the backends.
"""

import pytest

from physure.core.dispatcher import BackendManager


@pytest.fixture
def clean_hooks(monkeypatch):
    monkeypatch.setattr(BackendManager, "_load_hooks", {})
    monkeypatch.setattr(BackendManager, "_announced", set())


def test_a_hook_runs_when_its_backend_announces(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("demo"))
    assert calls == []
    BackendManager.announce("demo")
    assert calls == ["demo"]


def test_a_hook_registered_after_the_announcement_runs_at_once(clean_hooks):
    BackendManager.announce("demo")
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("late"))
    assert calls == ["late"]


def test_a_hook_is_registered_once(clean_hooks):
    calls = []

    def hook():
        calls.append(1)

    BackendManager.on_load("demo", hook)
    BackendManager.on_load("demo", hook)
    BackendManager.announce("demo")
    assert calls == [1]


def test_a_hook_only_hears_its_own_backend(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("demo"))
    BackendManager.announce("other")
    assert calls == []


def test_clearing_the_backends_keeps_the_registrations(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append(1))
    BackendManager.clear_backends()
    BackendManager.announce("demo")
    assert calls == [1]


def test_torch_flattens_a_quantity():
    torch = pytest.importorskip("torch")
    from torch.utils import _pytree

    from physure import Q_

    q = Q_(torch.tensor([1.0, 2.0], dtype=torch.float64), "m")
    leaves, spec = _pytree.tree_flatten(q)
    rebuilt = _pytree.tree_unflatten(leaves, spec)
    assert type(rebuilt) is type(q)
    assert rebuilt.unit == q.unit
    assert torch.equal(rebuilt.magnitude, q.magnitude)


def test_jax_flattens_a_quantity_and_the_uncertainty_models():
    jax = pytest.importorskip("jax")
    jnp = pytest.importorskip("jax.numpy")
    from physure import Q_
    from physure.backends.jax_backend import register_jax_behavior
    from physure.domain.measurement.uncertainty import (
        CovarianceModel,
        VarianceModel,
    )

    register_jax_behavior()
    q = Q_(jnp.array([1.0, 2.0]), "m")
    leaves, spec = jax.tree_util.tree_flatten(q)
    rebuilt = jax.tree_util.tree_unflatten(spec, leaves)
    assert type(rebuilt) is type(q)
    assert rebuilt.unit == q.unit

    variance = VarianceModel(variance=jnp.array([0.04, 0.09]))
    v_leaves, v_spec = jax.tree_util.tree_flatten(variance)
    v_rebuilt = jax.tree_util.tree_unflatten(v_spec, v_leaves)
    assert isinstance(v_rebuilt, VarianceModel)

    covariance = CovarianceModel(
        std_dev_internal=jnp.array([0.2, 0.3]), lineage={}
    )
    c_leaves, c_spec = jax.tree_util.tree_flatten(covariance)
    c_rebuilt = jax.tree_util.tree_unflatten(c_spec, c_leaves)
    assert isinstance(c_rebuilt, CovarianceModel)
```

Expected: ruff clean; `2 passed, 5 errors`. The five errors are the hook tests failing in their fixture (`BackendManager` has no `_load_hooks` yet); the two passing tests are the torch and jax pytree tests, which pass on the old code and must still pass after the change. If any other result: stop and report.

- [ ] **Step 2: Apply the `hooks` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_c.py hooks
```

Expected: `edited` lines for `dispatcher.py`, `jax_backend.py` (twice), `torch_backend.py`, `quantity.py`, `uncertainty.py` and `import_layers.toml`, then `(dropped 3 records)`.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_backend_hooks.py tests/core/test_import_layers.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
uv run --no-sync pytest tests/core tests/ext tests/test_jit_core.py tests/test_jit_uncertainty.py tests/test_uncertainty_gum.py tests/functions tests/symbolic -q -p no:cacheprovider 2>&1 | tail -1
```

Expected: ruff clean; `11 passed`; `13` records; `469 passed, 5 xfailed, 1 warning`. The last command exercises every torch and jax path that depends on the pytrees (`tests/core/test_jit.py`, `tests/core/test_functional_jax.py`, `tests/ext/test_autograd.py`).

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/core/dispatcher.py physure-python/physure/backends/jax_backend.py physure-python/physure/backends/torch_backend.py physure-python/physure/domain/measurement/quantity.py physure-python/physure/domain/measurement/uncertainty.py physure-python/tests/core/test_backend_hooks.py physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "refactor: the model registers its own torch and jax pytrees" -m "The torch and jax backends imported Quantity and the uncertainty models to register them as pytrees, which put the model underneath the backends. A backend now announces itself on the dispatcher when it loads (BackendManager.announce), and Quantity and the uncertainty models subscribe (BackendManager.on_load) and register themselves. A hook registered after the announcement runs at once, so the import order does not matter, and each hook is idempotent. register_jax_behavior stays as the one-line announcement." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: six `M` and one `A` (the test).

---

### Task 4: `propagate_affine` moves into the backends

**Files:**
- Create: `physure-python/physure/backends/affine.py` (by the script)
- Create: `physure-python/tests/core/test_relocated_modules.py`
- Modify: `physure-python/physure/domain/measurement/vectorized_uncertainty.py`
- Modify: `physure-python/physure/backends/torch/autograd_store.py`
- Modify: `physure-python/tests/core/import_layers.toml` (one record removed)

- [ ] **Step 1: Write the test and watch it fail**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-python/tests/core/test_relocated_modules.py` with exactly this content:' python physure-python/tests/core/test_relocated_modules.py
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_relocated_modules.py && uv run --no-sync ruff format --check tests/core/test_relocated_modules.py
uv run --no-sync pytest tests/core/test_relocated_modules.py -q -p no:cacheprovider 2>&1 | grep -E "Error|passed|failed"
```

The block written by the first command is:

Create `physure-python/tests/core/test_relocated_modules.py` with exactly this content:

```python
"""Code that lives in the layer it belongs to, and still answers to its old name."""


def test_propagate_affine_lives_with_the_backends():
    from physure.backends.affine import propagate_affine
    from physure.domain.measurement.vectorized_uncertainty import (
        propagate_affine as old_name,
    )

    assert old_name is propagate_affine
    assert propagate_affine.__module__ == "physure.backends.affine"
```

Expected: ruff clean; `ModuleNotFoundError: No module named 'physure.backends.affine'` and `1 failed`.

- [ ] **Step 2: Apply the `affine` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_c.py affine
```

Expected: `created ...backends\affine.py`; three `edited` lines for `vectorized_uncertainty.py`; one for `autograd_store.py`; one for `import_layers.toml`; `(dropped 1 records)`.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_relocated_modules.py tests/core/test_import_layers.py tests/core/test_vectorized_unc.py tests/core/test_functional.py tests/ext/test_autograd.py tests/core/test_covariance_store.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
```

Expected: ruff clean; `18 passed, 1 warning`; `12` records.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/backends/affine.py physure-python/physure/domain/measurement/vectorized_uncertainty.py physure-python/physure/backends/torch/autograd_store.py physure-python/tests/core/test_relocated_modules.py physure-python/tests/core/import_layers.toml
git diff --cached --name-status
git commit -m "refactor: propagate_affine lives with the backends" -m "propagate_affine is a pure function of a matrix, some slices and a backend; it needs nothing from the model. It moves to physure/backends/affine.py, so the torch autograd store no longer reaches up into vectorized_uncertainty, which re-exports it for the callers of the old path." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: three `M` and two `A`.

---

### Task 5: `jit` moves to the services layer

**Files:**
- Create: `physure-python/physure/application/jit.py` (by the script)
- Modify: `physure-python/physure/_jit/tracer.py`
- Modify: `physure-python/physure/_jit/__init__.py`
- Modify: `physure-python/physure/__init__.py`
- Modify: `physure-python/tests/core/test_relocated_modules.py`
- Modify: `physure-python/tests/core/import_layers.toml` (two records removed)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add the test and watch it fail**

```bash
cat >> physure-python/tests/core/test_relocated_modules.py <<'EOF'


def test_jit_lives_in_the_application_layer():
    import physure
    from physure.application.jit import jit

    assert physure.jit is jit
EOF
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check tests/core/test_relocated_modules.py && uv run --no-sync ruff format --check tests/core/test_relocated_modules.py
uv run --no-sync pytest tests/core/test_relocated_modules.py -q -p no:cacheprovider 2>&1 | grep -E "Error|passed|failed"
```

Expected: ruff clean; `ModuleNotFoundError: No module named 'physure.application.jit'`, `1 failed, 1 passed`.

- [ ] **Step 2: Apply the `jit` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_pr_c.py jit
```

Expected: `edited ...\_jit\tracer.py`, `created ...\application\jit.py`, `edited ...\_jit\__init__.py`, `edited ...\physure\__init__.py`, `edited ...import_layers.toml`, `(dropped 2 records)`, `edited CHANGELOG.md`.

- [ ] **Step 3: Check**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_relocated_modules.py tests/core/test_import_layers.py tests/test_jit_core.py tests/test_jit_uncertainty.py tests/ext/test_compiler.py tests/core/test_jit.py -q -p no:cacheprovider 2>&1 | tail -1
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
uv run --no-sync python -c "
import sys, physure
print('jit loaded at import:', 'physure.application.jit' in sys.modules)
physure.jit
print('after attribute:', 'physure.application.jit' in sys.modules)"
```

Expected: ruff clean; `20 passed`; `10` records; the two lines `jit loaded at import: False` and `after attribute: True`.

- [ ] **Step 4: Commit**

```bash
cd ..
git add physure-python/physure/application/jit.py physure-python/physure/_jit/tracer.py physure-python/physure/_jit/__init__.py physure-python/physure/__init__.py physure-python/tests/core/test_relocated_modules.py physure-python/tests/core/import_layers.toml CHANGELOG.md
git diff --cached --name-status
git commit -m "refactor: move jit out of the tracing package into application" -m "jit wraps the result in a Quantity, so it belongs above the model; the tracing primitives (TracerQuantity, bake_kernel, the unit signature helpers) stay in physure._jit.tracer and no longer import the model. physure.jit and from physure import jit are unchanged." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: `A` for `application/jit.py`, `M` for the other six.

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

Expected: `933 passed, 6 xfailed` (the 923 of `main`'s tip plus the 10 new tests), in about 70 s. If any other test fails, run it alone and report; do not change application code.

- [ ] **Step 3: First use stays fast and what is left of the contract is what PR D owns**

```bash
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
grep -c '^\[\[exception\]\]' tests/core/import_layers.toml
python -X utf8 -c "
import tomllib, collections
ex = tomllib.load(open('tests/core/import_layers.toml', 'rb'))['exception']
print(dict(collections.Counter(e['cause'] for e in ex)))
print([e['importer'] for e in ex if e['cause'] == 'a2a'])"
grep -rnE --include="*.py" "^\s*(from|import) physure\.(domain|application)" physure/backends physure/_jit
```

Expected: `0.25 kPa` in about 0.1 to 0.2 s; `10`; `{'a1': 6, 'a2a': 3, 'release': 1}`; the three `a2a` importers are `physure.domain.symbolic.expression`, `physure.application.tracing.context` and `physure.domain.measurement.quantity` (PR D's three); the final `grep` prints nothing and exits 1 (no module of `backends` or `_jit` imports from `domain` or `application` any more). Any other line: stop and report.

- [ ] **Step 4: Confirm the branch**

```bash
cd ..
git status --short
git log --oneline -5
```

Expected: `git status --short` empty (or only ` M Cargo.lock`); four new commits on `refactor/a2-backends-below-the-model` on top of `main`.

- [ ] **Step 5: Hand-off**

Report the branch name, the four commit SHAs and the measured numbers (they should match the table above). Nothing is pushed. Integrating the branch is the maintainer's decision. The remaining `a2a` exceptions (3) are the ones A2 PR D closes.

---

## Reference: the script this plan embeds

Create `apply_pr_c.py` with exactly this content:

```python
"""A2 PR C: the backends and _jit stop reaching the model.

Usage (from the repository root of the worktree):
    python -X utf8 apply_pr_c.py debug | hooks | affine | jit

Every replacement asserts that its anchor occurs exactly as many times as
expected, and every file keeps its own line endings.
"""

import pathlib
import re
import sys

SRC = pathlib.Path("physure-python/physure")
TOML = pathlib.Path("physure-python/tests/core/import_layers.toml")
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


def append(path, addition):
    text, eol = load(path)
    save(path, text.rstrip("\n") + "\n" + addition, eol)


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


def changelog(section, bullet):
    """Adds a bullet at the end of a section of [Unreleased]."""
    if section == "Changed":
        old = "\n\n### Fixed\n- **`SymbolicQuantity`,"
        new = "\n" + bullet + "\n\n### Fixed\n- **`SymbolicQuantity`,"
    else:
        old = "\n\n## [0.2.4] - 2026-08-28\n"
        new = "\n" + bullet + "\n\n## [0.2.4] - 2026-08-28\n"
    edit(CHANGELOG, [(old, new, 1)])


part = sys.argv[1]

if part == "debug":
    # two print("DEBUG: ...") left in library code
    edit(SRC / "backends/torch_backend.py", [
        ('        print(f"DEBUG: sparse_matmul a={type(a)}, b={type(b)}")\n',
         "", 1),
    ])
    edit(SRC / "backends/torch/autograd_store.py", [
        ("        print(\n"
         '            f"DEBUG: AutogradCovarianceStore.propagate called for out_id={out_id}"\n'
         "        )\n", "", 1),
    ])
    changelog(
        "Fixed",
        "- **The torch backend no longer prints `DEBUG:` lines.** `TorchBackend.sparse_matmul` and `AutogradCovarianceStore.propagate` wrote a line to standard output on every call.",
    )

elif part == "hooks":
    # backends announce themselves; the model registers its own pytrees
    edit(SRC / "core/dispatcher.py", [
        ("if TYPE_CHECKING:\n    from physure.core.protocols import BackendOps\n",
         "if TYPE_CHECKING:\n    from collections.abc import Callable\n\n"
         "    from physure.core.protocols import BackendOps\n", 1),
        ("    _python_backend: ClassVar[BackendOps | None] = None\n\n"
         "    @classmethod\n    def get_backend(cls, data_obj: Any) -> BackendOps:\n",
         "    _python_backend: ClassVar[BackendOps | None] = None\n"
         "    # Import-time registrations made by the layers above the backends\n"
         "    # (the model registers its pytrees). `clear_backends` keeps them.\n"
         "    _load_hooks: ClassVar[dict[str, list[Callable[[], None]]]] = {}\n"
         "    _announced: ClassVar[set[str]] = set()\n\n"
         "    @classmethod\n"
         "    def on_load(cls, name: str, hook: Callable[[], None]) -> None:\n"
         '        """Calls `hook` when backend `name` announces itself, or now if it has."""\n'
         "        hooks = cls._load_hooks.setdefault(name, [])\n"
         "        if hook in hooks:\n"
         "            return\n"
         "        hooks.append(hook)\n"
         "        if name in cls._announced:\n"
         "            hook()\n\n"
         "    @classmethod\n"
         "    def announce(cls, name: str) -> None:\n"
         '        """Called by a backend module once its library is in play."""\n'
         "        cls._announced.add(name)\n"
         "        for hook in list(cls._load_hooks.get(name, ())):\n"
         "            hook()\n\n"
         "    @classmethod\n    def get_backend(cls, data_obj: Any) -> BackendOps:\n", 1),
    ])
    edit(SRC / "backends/jax_backend.py", [
        ("from physure.core.dispatcher import enforce_tensor_contract\n",
         "from physure.core.dispatcher import (\n    BackendManager,\n    enforce_tensor_contract,\n)\n", 1),
    ])
    cut_tail(
        SRC / "backends/jax_backend.py",
        "_jax_registered = False\n",
        "def register_jax_behavior():\n"
        '    """Tells the model that JAX is in play so it registers its pytrees."""\n'
        '    BackendManager.announce("jax")\n',
    )
    edit(SRC / "backends/torch_backend.py", [
        ("from physure.core.dispatcher import enforce_tensor_contract\n",
         "from physure.core.dispatcher import (\n    BackendManager,\n    enforce_tensor_contract,\n)\n", 1),
        ("if torch is not None:\n"
         "    # Quantity defers its torch pytree registration until torch is in play;\n"
         "    # loading this backend is the signal that it is.\n"
         "    from physure.domain.measurement.quantity import _register_torch_pytree\n\n"
         "    _register_torch_pytree()\n",
         "if torch is not None:\n"
         "    # The model defers registering its torch pytrees until torch is in\n"
         "    # play; loading this backend is the signal that it is.\n"
         '    BackendManager.announce("torch")\n', 1),
    ])
    append(SRC / "domain/measurement/quantity.py", '''

# --- JAX Integration ---
_jax_pytree_registered = False


def _register_jax_pytree() -> None:
    """Register Quantity as a JAX pytree node. Idempotent; no-op without JAX."""
    global _jax_pytree_registered
    if _jax_pytree_registered:
        return
    try:
        import jax
    except ImportError:
        return
    jax.tree_util.register_pytree_node(
        Quantity,
        Quantity.tree_flatten,
        Quantity.tree_unflatten,
    )
    _jax_pytree_registered = True


# The backends announce themselves when they load; the model asks to be told.
BackendManager.on_load("torch", _register_torch_pytree)
BackendManager.on_load("jax", _register_jax_pytree)
''')
    append(SRC / "domain/measurement/uncertainty.py", '''

# --- JAX Integration ---
_jax_pytrees_registered = False


def _register_jax_pytrees() -> None:
    """Register the uncertainty models as JAX pytree nodes.

    Idempotent; no-op without JAX.
    """
    global _jax_pytrees_registered
    if _jax_pytrees_registered:
        return
    try:
        import jax
    except ImportError:
        return

    def cov_flatten(m):
        keys = tuple(sorted(m.lineage.keys()))
        values = tuple(m.lineage[k] for k in keys)
        return (m.std_dev_internal, values), (m.vector_slice, keys)

    def cov_unflatten(aux, children):
        vector_slice, keys = aux
        std_dev, values = children
        lineage = dict(zip(keys, values, strict=False))
        return CovarianceModel(
            std_dev_internal=std_dev,
            lineage=lineage,
            vector_slice=vector_slice,
        )

    jax.tree_util.register_pytree_node(
        CovarianceModel,
        cov_flatten,
        cov_unflatten,
    )

    def var_flatten(m):
        return (m.variance,), ()

    def var_unflatten(_aux, children):
        return VarianceModel(variance=children[0])

    jax.tree_util.register_pytree_node(
        VarianceModel,
        var_flatten,
        var_unflatten,
    )
    _jax_pytrees_registered = True


BackendManager.on_load("jax", _register_jax_pytrees)
''')
    drop_exceptions([
        ("physure.backends.torch_backend", "physure.domain.measurement.quantity", "top", "a2a"),
        ("physure.backends.jax_backend", "physure.domain.measurement.quantity", "lazy", "a2a"),
        ("physure.backends.jax_backend", "physure.domain.measurement.uncertainty", "lazy", "a2a"),
    ])

elif part == "affine":
    # propagate_affine only needs a backend: it moves into the backends
    vu = SRC / "domain/measurement/vectorized_uncertainty.py"
    text, _ = load(vu)
    marker = "def propagate_affine("
    assert text.count(marker) == 1
    function = text[text.index(marker):].rstrip("\n") + "\n"
    new_module = (
        '"""Affine propagation of a covariance matrix, written against a backend."""\n\n'
        "from __future__ import annotations\n\n"
        "from typing import TYPE_CHECKING, Any\n\n"
        "if TYPE_CHECKING:\n"
        "    from physure.core.protocols import BackendOps, Numeric\n\n\n"
        + function
    )
    target = SRC / "backends/affine.py"
    target.write_bytes(new_module.encode("utf-8"))
    print("created", target)
    cut_tail(vu, marker, "")
    text, eol = load(vu)
    save(vu, text.rstrip("\n") + "\n", eol)
    edit(vu, [
        ("from physure._core import PruningConfig\n",
         "from physure._core import PruningConfig\n"
         "from physure.backends.affine import propagate_affine  # noqa: F401\n", 1),
    ])
    edit(SRC / "backends/torch/autograd_store.py", [
        ("import torch\n\nif TYPE_CHECKING:\n",
         "import torch\n\nfrom physure.backends.affine import propagate_affine\n\n"
         "if TYPE_CHECKING:\n", 1),
        ("        from physure.domain.measurement.vectorized_uncertainty import (\n"
         "            propagate_affine,\n        )\n\n        self._matrix = propagate_affine(",
         "        self._matrix = propagate_affine(", 1),
    ])
    drop_exceptions([
        ("physure.backends.torch.autograd_store", "physure.domain.measurement.vectorized_uncertainty", "lazy", "a2a"),
    ])

elif part == "jit":
    # jit builds Quantity: it moves to the services layer, the primitives stay
    tracer = SRC / "_jit/tracer.py"
    text, eol = load(tracer)
    body, n = re.subn(
        r"def jit\(func: Callable\) -> Callable:\n.*?\n\n\n(?=def _ensure_rational)",
        "",
        text,
        flags=re.S,
    )
    assert n == 1
    assert body.count("import functools\n") == 1
    body = body.replace("import functools\n", "")
    save(tracer, body, eol)
    (SRC / "application/jit.py").write_bytes('''"""`physure.jit`: unit-aware JIT for plain Python functions.

Traces the function to validate units, then bakes a unit-less numerical kernel.
The tracing primitives live in `physure._jit.tracer`.
"""

from __future__ import annotations

import functools
from typing import TYPE_CHECKING, Any

from physure._jit.tracer import (
    _JIT_CACHE,
    _build_unit_sig,
    _rational_unit_to_exponents,
    _trace_and_bake,
)
from physure.application.context import get_uncertainty_mode
from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import CompoundUnit, get_default_system

if TYPE_CHECKING:
    from collections.abc import Callable


def jit(func: Callable) -> Callable:
    """JIT compiler for unit-aware functions.

    Traces the function to validate units and bakes a unit-less numerical kernel.
    """

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        unit_sig = _build_unit_sig(args)
        sig = (func, tuple(unit_sig))

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        if sig not in _JIT_CACHE:
            result = _trace_and_bake(func, args, unit_sig, kwargs)
            if result is None:
                return func(*args, **kwargs)
            _JIT_CACHE[sig] = result

        kernel, out_r_unit = _JIT_CACHE[sig]
        magnitudes = [getattr(a, "magnitude", a) for a in args]
        raw_res = kernel(*magnitudes)

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        exponents = _rational_unit_to_exponents(out_r_unit)
        out_unit = CompoundUnit(exponents)
        return Quantity.from_input(raw_res, out_unit, get_default_system())

    return wrapper
'''.encode("utf-8"))
    print("created", SRC / "application/jit.py")
    (SRC / "_jit/__init__.py").write_bytes(
        '"""Tracing primitives for compile-time unit checking.\n\n'
        "`physure.jit`, which uses them, lives in `physure.application.jit`.\n"
        '"""\n'.encode("utf-8")
    )
    print("edited", SRC / "_jit/__init__.py")
    edit(SRC / "__init__.py", [
        ("    from physure._jit import jit\n    from physure.application.context import (\n        get_current_system,\n        get_propagation_mode,\n",
         "    from physure.application.context import (\n        get_current_system,\n        get_propagation_mode,\n", 1),
        ("    from physure.application.io import load_state, save_state\n    from physure.application.startup import (\n        create_default_system,\n",
         "    from physure.application.io import load_state, save_state\n    from physure.application.jit import jit\n    from physure.application.startup import (\n        create_default_system,\n", 1),
        ("def _load_jit(name: str) -> Any:\n    from physure._jit import jit\n",
         "def _load_jit(name: str) -> Any:\n    from physure.application.jit import jit\n", 1),
    ])
    drop_exceptions([
        ("physure._jit.tracer", "physure.domain.measurement.quantity", "lazy", "a2a"),
        ("physure._jit.tracer", "physure.domain.measurement.units", "lazy", "a1"),
    ])
    changelog(
        "Changed",
        "- **`jit` moved out of the private `physure._jit` package** to `physure.application.jit`, so the tracing primitives no longer import `Quantity`. `physure.jit` and `from physure import jit` are unchanged.",
    )

else:
    raise SystemExit(f"unknown part: {part}")
```
