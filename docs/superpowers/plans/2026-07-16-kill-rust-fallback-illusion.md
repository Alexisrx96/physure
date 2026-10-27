# Kill the fake Rust-fallback illusion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the permanently-dead try/except in `quantity.py` that pretends a real Rust
`Quantity` import might be used but never is, rewrite `CoreQuantity`'s docstring to state why it
actually exists, and delete two other vestigial `IS_CORE_AVAILABLE` flags — all with zero
behavior change.

**Architecture:** Pure deletion/rewording in three files. No new abstractions, no new tests (spec
explicitly calls this a comment/dead-code-only cleanup) — verification is "existing suite stays
green."

**Tech Stack:** Python (physure-python), pytest, ruff, ty.

---

## Design spec

`physure-python/docs/superpowers/specs/2026-07-16-kill-rust-fallback-illusion-design.md`

## File Structure

- Modify: `physure-python/physure/domain/measurement/quantity.py:78-135` — delete the dead
  try/except, replace `CoreQuantity`'s docstring.
- Modify: `physure-python/physure/domain/measurement/units.py:147-148` — delete the vestigial
  `IS_CORE_AVAILABLE = True` line.
- Modify: `physure-python/physure/domain/symbolic/native.py:805-806` — delete the vestigial
  `IS_CORE_AVAILABLE = True` line.

No files created. No test files touched (no behavior change to cover).

---

### Task 1: Remove the dead try/except in quantity.py

**Files:**
- Modify: `physure-python/physure/domain/measurement/quantity.py:78-135`
- Test (regression only, not new): `physure-python/tests/measurement_tests/test_quantity.py`

- [ ] **Step 1: Confirm `IS_CORE_AVAILABLE` has no consumers before deleting it**

Run:
```bash
grep -rn "IS_CORE_AVAILABLE" /mnt/d/Projects/physure/physure-python/physure /mnt/d/Projects/physure/physure-python/tests
```
Expected: only the three definition sites show up (`quantity.py:87`, `quantity.py:91`,
`units.py:148`, `native.py:806`) — no other file reads the name. If a new consumer appears,
stop and re-scope before continuing.

- [ ] **Step 2: Replace lines 78-135 of `quantity.py`**

Current content at `quantity.py:78-135`:

```python
try:
    # High-level Python Quantity uses Python CoreQuantity for PyTorch Dynamo
    # & multiple inheritance compatibility; high-speed operations delegate
    # directly to physure._core functions.
    raise ImportError("Use Python CoreQuantity container")
    from physure._core import (
        Quantity as CoreQuantity,
    )

    IS_CORE_AVAILABLE = True


except ImportError:
    IS_CORE_AVAILABLE = False

    # Minimal fallback for build/env issues
    class CoreQuantity:
        """Pure-Python stand-in when physure._core is unavailable."""

        # ponytail: this class exists only as a drop-in stand-in for the
        # Rust extension type (physure._core, an exempt adapter boundary)
        # when the compiled core is unavailable, so it mirrors that type's
        # dynamic constructor exactly.
        def __new__(
            cls,
            magnitude: Numeric,
            unit: CompoundUnit,
            uncertainty: Any,  # pyright: ignore[reportAny, reportExplicitAny]
            *args: Any,  # pyright: ignore[reportAny, reportExplicitAny]
            **kwargs: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        ) -> Self:
            """Stores magnitude/unit/uncertainty on the instance."""
            obj = super().__new__(cls)
            object.__setattr__(obj, "_core_magnitude", magnitude)
            object.__setattr__(obj, "_core_unit", unit)
            object.__setattr__(obj, "_core_uncertainty", uncertainty)
            return obj

        @property
        def magnitude(self) -> Numeric:
            """Returns the stored magnitude."""
            val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
            if _CORE_QUANTITY_TYPE in str(type(val)):
                return val.magnitude
            return val

        @property
        def unit(self) -> CompoundUnit:
            """Returns the stored unit."""
            return self._core_unit

        @property
        def std_dev(self) -> Any:  # pyright: ignore[reportExplicitAny]
            """Returns the stored uncertainty."""
            val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
            if _CORE_QUANTITY_TYPE in str(type(val)):
                return val.std_dev
            return self._core_uncertainty
```

Replace it with (no try/except, no `IS_CORE_AVAILABLE`, same class body, new docstring):

```python
class CoreQuantity:
    """Container base for `Quantity`: holds magnitude/unit/uncertainty as plain attributes.

    Not a fallback for a missing Rust extension — physure._core.Quantity is a real,
    always-available PyO3 type (see physure/__init__.py's unconditional import of it).
    Quantity doesn't inherit that Rust type directly because Quantity needs multiple
    inheritance (with ArithmeticMixin/BackendMixin) and must stay traceable by PyTorch
    Dynamo, and a compiled PyO3 base class doesn't reliably support either. High-speed
    operations still delegate directly to physure._core functions/methods; this class only
    stores the magnitude/unit/uncertainty triple and exposes it as properties.
    """

    def __new__(
        cls,
        magnitude: Numeric,
        unit: CompoundUnit,
        uncertainty: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        *args: Any,  # pyright: ignore[reportAny, reportExplicitAny]
        **kwargs: Any,  # pyright: ignore[reportAny, reportExplicitAny]
    ) -> Self:
        """Stores magnitude/unit/uncertainty on the instance."""
        obj = super().__new__(cls)
        object.__setattr__(obj, "_core_magnitude", magnitude)
        object.__setattr__(obj, "_core_unit", unit)
        object.__setattr__(obj, "_core_uncertainty", uncertainty)
        return obj

    @property
    def magnitude(self) -> Numeric:
        """Returns the stored magnitude."""
        val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
        if _CORE_QUANTITY_TYPE in str(type(val)):
            return val.magnitude
        return val

    @property
    def unit(self) -> CompoundUnit:
        """Returns the stored unit."""
        return self._core_unit

    @property
    def std_dev(self) -> Any:  # pyright: ignore[reportExplicitAny]
        """Returns the stored uncertainty."""
        val: Any = self._core_magnitude  # pyright: ignore[reportAny, reportExplicitAny]
        if _CORE_QUANTITY_TYPE in str(type(val)):
            return val.std_dev
        return self._core_uncertainty
```

Leave everything else in the file untouched, including `_CORE_QUANTITY_TYPE = "physure._core.Quantity"`
at line 51 (it's a live check used inside the properties above and elsewhere in the file) and the
`from physure._core import RationalUnit as _CoreRationalUnit` line that follows this block.

- [ ] **Step 3: Run the quantity test module**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run pytest tests/measurement_tests/test_quantity.py tests/core/test_backends_and_quantity.py -x -q
```
Expected: all tests PASS (same pass count as before the edit — this is a no-behavior-change
refactor).

- [ ] **Step 4: Commit**

```bash
cd /mnt/d/Projects/physure/physure-python && git add physure/domain/measurement/quantity.py && git commit -m "refactor(quantity): remove dead Rust-fallback illusion, document CoreQuantity's real purpose"
```

---

### Task 2: Remove the vestigial flag in units.py

**Files:**
- Modify: `physure-python/physure/domain/measurement/units.py:147-148`
- Test (regression only): `physure-python/tests/measurement_tests/test_units.py`

- [ ] **Step 1: Delete the flag line**

Current content at `units.py:147-148`:

```python
from physure._core import RationalUnit
IS_CORE_AVAILABLE = True
```

Replace with:

```python
from physure._core import RationalUnit
```

- [ ] **Step 2: Run the units test module**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run pytest tests/measurement_tests/test_units.py -x -q
```
Expected: all tests PASS.

- [ ] **Step 3: Commit**

```bash
cd /mnt/d/Projects/physure/physure-python && git add physure/domain/measurement/units.py && git commit -m "refactor(units): remove unused IS_CORE_AVAILABLE flag"
```

---

### Task 3: Remove the vestigial flag in native.py

**Files:**
- Modify: `physure-python/physure/domain/symbolic/native.py:805-806`
- Test (regression only): whatever module covers `physure/domain/symbolic/native.py` (run the
  full symbolic test directory since there's no single `test_native.py`)

- [ ] **Step 1: Delete the flag line**

Current content at `native.py:805-806`:

```python
from physure._core import Expr
IS_CORE_AVAILABLE = True
```

Replace with:

```python
from physure._core import Expr
```

- [ ] **Step 2: Run the symbolic tests**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run pytest tests/ -k symbolic -x -q
```
Expected: all tests PASS. If this collects zero tests, instead run the full suite in Task 4
before committing (don't skip verification).

- [ ] **Step 3: Commit**

```bash
cd /mnt/d/Projects/physure/physure-python && git add physure/domain/symbolic/native.py && git commit -m "refactor(symbolic): remove unused IS_CORE_AVAILABLE flag"
```

---

### Task 4: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Confirm `IS_CORE_AVAILABLE` is fully gone**

Run:
```bash
grep -rn "IS_CORE_AVAILABLE" /mnt/d/Projects/physure/physure-python/physure /mnt/d/Projects/physure/physure-python/tests
```
Expected: no output.

- [ ] **Step 2: Run the full test suite**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run pytest
```
Expected: all tests PASS, same total count as on `master` before Task 1 (no test was added or
removed).

- [ ] **Step 3: Lint and format check**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run ruff check . && uv run ruff format --check .
```
Expected: both PASS with no new violations.

- [ ] **Step 4: Type check (advisory)**

Run:
```bash
cd /mnt/d/Projects/physure/physure-python && uv run ty check
```
Expected: error count on the three touched files (`quantity.py`, `units.py`, `native.py`) does
not increase versus `master` (ty is advisory project-wide per CLAUDE.md, but don't add new
errors to files touched in this plan).
