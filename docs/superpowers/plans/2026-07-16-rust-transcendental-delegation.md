# Rust Transcendental Delegation (Opt-In Scalar Path) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delegate the six transcendental functions (`sin`/`cos`/`tan`/`exp`/`log`/`tanh`) to Rust for scalar `Quantity` objects already wrapped in `physure._core.Quantity` under `gaussian`/`monte_carlo`/`unscented` propagation modes, fixing a dormant `tan`/`tanh` formula bug in Rust along the way, and hardening `TensorBackend::mean()`/`std_dev()` to fail loudly instead of returning `NaN`/`0.0`.

**Architecture:** Two Rust-side fixes (a correctness fix to the enum-level `propagate_function`, and a consistency fix to the three backends' own trait impls) land first, each with red/green Rust unit tests. Then six new `PyQuantity` pymethods expose them to Python. Then `_apply_transcendental` in Python gets a Rust-delegation early-exit mirroring the existing arithmetic delegation pattern. `TensorBackend` hardening is independent and can land anywhere after Task 1, but is sequenced last among the Rust work since it touches unrelated code paths.

**Tech Stack:** Rust (PyO3, ndarray), Python (pytest), Cargo workspace at the repo root (`cargo test -p physure-core`), `maturin develop` (run from `physure-python/`) to rebuild the Python extension after any Rust change.

**Two corrections to the approved spec** (`physure-python/docs/superpowers/specs/2026-07-16-rust-transcendental-delegation-design.md`), discovered while tracing the live code path and folded into the tasks below:

1. **The spec names the wrong primary fix site for the tan/tanh bug.** `Quantity.value` is typed `UncertaintyValue` (`physure-core/src/quantity.rs:12`), and every `PyQuantity` pymethod that calls `propagate_function` calls it on `self.0.value` — i.e. on the `UncertaintyValue` enum's own **inherent** method (`physure-core/src/uncertainty/trait_def.rs:199-240`), which has its own independently-duplicated match arms per variant. The three backend files the spec names (`gaussian.rs`/`monte_carlo.rs`/`unscented.rs`) implement a *separate* trait method on the concrete structs, reachable only through `UncertaintyValue::Custom` — which is only ever constructed with `TensorBackend` (two call sites, both in `physure-python/src/lib.rs`). So today, patching only the three named files would **not** fix the live bug. Task 1 below fixes the actual (enum-level) call site; Task 2 patches the three named files too, as a secondary consistency measure so they don't stay silently divergent from the enum they mirror.
2. **The spec's illustrative pymethod snippet contradicts its own prose.** It shows `self.0.unit.clone()` (copied from `__abs__`, correct for `abs` since that doesn't change units) but the spec text says the result must always be dimensionless. Task 3 uses `RationalUnit::dimensionless()` (confirmed to exist, `physure-core/src/units/rational.rs:59`) instead.

---

### Task 1: Fix the primary tan/tanh bug in `UncertaintyValue::propagate_function`

**Files:**
- Modify: `physure-core/src/uncertainty/mod.rs` (add test module declaration)
- Create: `physure-core/src/uncertainty/tests.rs`
- Modify: `physure-core/src/uncertainty/trait_def.rs:213-234` (the `Gaussian`/`MonteCarlo`/`Unscented` match arms inside `impl UncertaintyValue { pub fn propagate_function }`)

- [ ] **Step 1: Declare the test module**

`physure-core/src/uncertainty/mod.rs` currently has no `#[cfg(test)]` declaration. Add one:

```rust
pub mod trait_def;
pub mod gaussian;
pub mod monte_carlo;
pub mod unscented;

pub use trait_def::{UncertaintyBackend, UncertaintyValue};
pub use gaussian::GaussianBackend;
pub use monte_carlo::MonteCarloBackend;
pub use unscented::UnscentedBackend;

#[cfg(test)]
mod tests;
```

- [ ] **Step 2: Write the failing tests**

Create `physure-core/src/uncertainty/tests.rs`:

```rust
use super::*;
use ndarray::Array1;

#[test]
fn test_gaussian_tan_enum() {
    let g = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.5, std_dev: 0.1 });
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_gaussian_tanh_enum() {
    let g = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.5, std_dev: 0.1 });
    let result = g.propagate_function("tanh").unwrap();
    let expected_mean = 0.5_f64.tanh();
    let expected_std = ((1.0 - expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_montecarlo_tan_enum() {
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend {
        samples: Array1::from_vec(vec![0.0, 0.5, -0.5]),
    });
    let result = mc.propagate_function("tan").unwrap();
    match result {
        UncertaintyValue::MonteCarlo(m) => {
            let expected = [0.0_f64.tan(), 0.5_f64.tan(), (-0.5_f64).tan()];
            for (actual, expected) in m.samples.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected MonteCarlo variant"),
    }
}

#[test]
fn test_montecarlo_tanh_enum() {
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend {
        samples: Array1::from_vec(vec![0.0, 0.5, -0.5]),
    });
    let result = mc.propagate_function("tanh").unwrap();
    match result {
        UncertaintyValue::MonteCarlo(m) => {
            let expected = [0.0_f64.tanh(), 0.5_f64.tanh(), (-0.5_f64).tanh()];
            for (actual, expected) in m.samples.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected MonteCarlo variant"),
    }
}

#[test]
fn test_unscented_tan_enum() {
    let u = UncertaintyValue::Unscented(UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    });
    let result = u.propagate_function("tan").unwrap();
    match result {
        UncertaintyValue::Unscented(uu) => {
            let expected = [0.0_f64.tan(), 0.5_f64.tan(), (-0.5_f64).tan()];
            for (actual, expected) in uu.sigma_points.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected Unscented variant"),
    }
}

#[test]
fn test_unscented_tanh_enum() {
    let u = UncertaintyValue::Unscented(UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    });
    let result = u.propagate_function("tanh").unwrap();
    match result {
        UncertaintyValue::Unscented(uu) => {
            let expected = [0.0_f64.tanh(), 0.5_f64.tanh(), (-0.5_f64).tanh()];
            for (actual, expected) in uu.sigma_points.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected Unscented variant"),
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run (from repo root): `cargo test -p physure-core tan_enum`

Expected: all four `*_tan_enum`/`*_tanh_enum` tests **FAIL** — today's `propagate_function` falls through to its `_` wildcard arm (unchanged passthrough) for `"tan"`/`"tanh"`, so e.g. `test_gaussian_tan_enum` sees `result.mean() == 0.5` instead of `0.5_f64.tan() ≈ 0.4794`.

- [ ] **Step 4: Fix `trait_def.rs`**

Read the current match arms first — they live inside `impl UncertaintyValue { pub fn propagate_function(&self, func: &str) -> PhysureResult<UncertaintyValue> }` (`physure-core/src/uncertainty/trait_def.rs:199-240`). The `Gaussian`, `MonteCarlo`, and `Unscented` arms each match on `func` with a `match func { "sin" => ..., "cos" => ..., "exp" => ..., "log" => ..., "abs" => ..., _ => (unchanged) }` shape. Add `"tan"` and `"tanh"` arms to each of the three, consistent with the existing sin/cos/exp/log formulas and with the derivatives already used in `_arithmetic_mixin.py`'s `_apply_transcendental` (`der = 1 + tan(val)^2` for tan, `der = 1 - tanh(val)^2` for tanh):

For the `Gaussian` arm (the tuple-match producing `(new_mean, new_std)`):
```rust
"tan" => (m.tan(), ((1.0 + m.tan().powi(2)) * s).abs()),
"tanh" => (m.tanh(), ((1.0 - m.tanh().powi(2)) * s).abs()),
```
Add these alongside the existing `"sin"`/`"cos"`/`"exp"`/`"log"`/`"abs"` arms, before the `_ => (m, s)` fallback.

For the `MonteCarlo` arm (the `match func { ... }` producing `new_samples`):
```rust
"tan" => m.samples.mapv(|x| x.tan()),
"tanh" => m.samples.mapv(|x| x.tanh()),
```
Add alongside the existing arms, before `_ => m.samples.clone()`.

For the `Unscented` arm (the `match func { ... }` producing `new_points`):
```rust
"tan" => u.sigma_points.mapv(|x| x.tan()),
"tanh" => u.sigma_points.mapv(|x| x.tanh()),
```
Add alongside the existing arms, before `_ => u.sigma_points.clone()`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p physure-core tan_enum`

Expected: all 6 tests (`test_gaussian_tan_enum`, `test_gaussian_tanh_enum`, `test_montecarlo_tan_enum`, `test_montecarlo_tanh_enum`, `test_unscented_tan_enum`, `test_unscented_tanh_enum`) **PASS**.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/uncertainty/mod.rs physure-core/src/uncertainty/tests.rs physure-core/src/uncertainty/trait_def.rs
git commit -m "fix(uncertainty): add tan/tanh formulas to UncertaintyValue::propagate_function"
```

---

### Task 2: Consistency fix — same tan/tanh arms in the three backend structs' own trait impls

**Files:**
- Modify: `physure-core/src/uncertainty/gaussian.rs:55-66` (the `propagate_function` match arm inside `impl UncertaintyBackend for GaussianBackend`)
- Modify: `physure-core/src/uncertainty/monte_carlo.rs` (the `propagate_function` match arm inside `impl UncertaintyBackend for MonteCarloBackend`)
- Modify: `physure-core/src/uncertainty/unscented.rs:72-82` (the `propagate_function` match arm inside `impl UncertaintyBackend for UnscentedBackend`)
- Modify: `physure-core/src/uncertainty/tests.rs` (append)

These three trait impls are reachable only via `UncertaintyValue::Custom(Box<dyn UncertaintyBackend>)`, which nothing in the codebase constructs with these types today (only `TensorBackend` is ever boxed into `Custom`). They are dead code for the live bug, but they duplicate the same sin/cos/exp/log/abs formulas Task 1 just fixed at the enum level — leaving them silently still wrong would be a trap for any future code that does box a `Gaussian`/`MonteCarlo`/`Unscented` backend into `Custom`.

- [ ] **Step 1: Write the failing tests**

Append to `physure-core/src/uncertainty/tests.rs`:

```rust
#[test]
fn test_gaussian_backend_tan_trait_impl() {
    let g = GaussianBackend { mean: 0.5, std_dev: 0.1 };
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
    assert_eq!(result.get_model_name(), "gaussian");
}

#[test]
fn test_gaussian_backend_tanh_trait_impl() {
    let g = GaussianBackend { mean: 0.5, std_dev: 0.1 };
    let result = g.propagate_function("tanh").unwrap();
    let expected_mean = 0.5_f64.tanh();
    let expected_std = ((1.0 - expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_montecarlo_backend_tan_trait_impl() {
    let mc = MonteCarloBackend { samples: Array1::from_vec(vec![0.0, 1.0, -1.0]) };
    let result = mc.propagate_function("tan").unwrap();
    let expected_mean = (0.0_f64.tan() + 1.0_f64.tan() + (-1.0_f64).tan()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
    assert_eq!(result.get_model_name(), "monte_carlo");
}

#[test]
fn test_montecarlo_backend_tanh_trait_impl() {
    let mc = MonteCarloBackend { samples: Array1::from_vec(vec![0.0, 1.0, -1.0]) };
    let result = mc.propagate_function("tanh").unwrap();
    let expected_mean = (0.0_f64.tanh() + 1.0_f64.tanh() + (-1.0_f64).tanh()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
}

#[test]
fn test_unscented_backend_tan_trait_impl() {
    let u = UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    };
    let result = u.propagate_function("tan").unwrap();
    let expected_mean = (0.0_f64.tan() + 0.5_f64.tan() + (-0.5_f64).tan()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
    assert_eq!(result.get_model_name(), "unscented");
}

#[test]
fn test_unscented_backend_tanh_trait_impl() {
    let u = UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    };
    let result = u.propagate_function("tanh").unwrap();
    let expected_mean = (0.0_f64.tanh() + 0.5_f64.tanh() + (-0.5_f64).tanh()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p physure-core trait_impl`

Expected: all 6 `*_trait_impl` tests **FAIL** (same passthrough issue as Task 1, but in the separate per-struct trait impls).

- [ ] **Step 3: Fix the three backend files**

In `physure-core/src/uncertainty/gaussian.rs`, inside `impl UncertaintyBackend for GaussianBackend`'s `propagate_function` (the tuple-match at lines 55-66 with `"sin"`/`"cos"`/`"exp"`/`"log"`/`"abs"` arms), add:
```rust
"tan" => (m.tan(), ((1.0 + m.tan().powi(2)) * s).abs()),
"tanh" => (m.tanh(), ((1.0 - m.tanh().powi(2)) * s).abs()),
```
before the `_ => (m, s)` fallback.

In `physure-core/src/uncertainty/monte_carlo.rs`, inside `impl UncertaintyBackend for MonteCarloBackend`'s `propagate_function`, add:
```rust
"tan" => self.samples.mapv(|x| x.tan()),
"tanh" => self.samples.mapv(|x| x.tanh()),
```
before its `_ => self.samples.clone()` fallback.

In `physure-core/src/uncertainty/unscented.rs`, inside `impl UncertaintyBackend for UnscentedBackend`'s `propagate_function` (lines 72-82), add:
```rust
"tan" => self.sigma_points.mapv(|x| x.tan()),
"tanh" => self.sigma_points.mapv(|x| x.tanh()),
```
before the `_ => self.sigma_points.clone()` fallback.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p physure-core trait_impl`

Expected: all 6 tests **PASS**.

- [ ] **Step 5: Run the full Rust test suite**

Run: `cargo test -p physure-core`

Expected: all tests pass (Task 1's 6 tests + Task 2's 6 tests + any pre-existing tests).

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/uncertainty/gaussian.rs physure-core/src/uncertainty/monte_carlo.rs physure-core/src/uncertainty/unscented.rs physure-core/src/uncertainty/tests.rs
git commit -m "fix(uncertainty): add tan/tanh to per-backend UncertaintyBackend::propagate_function impls"
```

---

### Task 3: Add six transcendental pymethods to `PyQuantity`

**Files:**
- Modify: `physure-python/src/lib.rs:461-462` (insert after `__abs__`, before `__repr__`)

`RationalUnit` is already imported at the top of `lib.rs` (`use ::physure_core::{RationalUnit, UnitRegistry, Quantity, PruningConfig, CovarianceStore, ...}`), so no new import is needed. `RationalUnit::dimensionless()` is used for the result unit instead of `self.0.unit.clone()` — the spec's own prose says the result is always dimensionless, and the input unit is meaningless for a transcendental result (e.g. `sin(3 rad)` should not carry a `rad` label on the raw Rust object).

There is no PyO3-level unit test harness in this codebase (Rust tests run against native types only — `PyQuantity` requires a live Python interpreter via `pyo3::Python::with_gil`, which isn't set up in `cargo test`). Verification for this task is a manual `maturin develop` + Python smoke test, which Task 5's automated pytest suite then covers permanently.

- [ ] **Step 1: Add the six pymethods**

In `physure-python/src/lib.rs`, insert immediately after `__abs__` (ending at line 461) and before `__repr__` (starting at line 463):

```rust
    fn sin(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("sin")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn cos(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("cos")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn tan(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("tan")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn exp(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("exp")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn log(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("log")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }

    fn tanh(&self) -> PyResult<PyQuantity> {
        let new_val = self.0.value.propagate_function("tanh")
            .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
        Ok(PyQuantity(Quantity::from_value(new_val, RationalUnit::dimensionless())))
    }
```

- [ ] **Step 2: Rebuild the extension**

Run (from `physure-python/`): `cd physure-python && maturin develop`

Expected: builds cleanly, no errors.

- [ ] **Step 3: Manual smoke test**

Run:
```bash
uv run python -c "
from physure._core import Quantity, RationalUnit
from physure_core import GaussianBackend
q = Quantity(0.5, RationalUnit.dimensionless(), 0.1)
print('sin:', q.sin().magnitude, q.sin().uncertainty)
print('tan:', q.tan().magnitude, q.tan().uncertainty)
print('tanh:', q.tanh().magnitude, q.tanh().uncertainty)
"
```
(Adjust the `Quantity`/`RationalUnit` constructor call to whatever the actual `PyQuantity::new` pyclass signature is — check `physure-python/src/lib.rs`'s `#[new]` constructor for `PyQuantity` if this exact call errors; the point of this step is simply confirming `q.tan().magnitude` prints `math.tan(0.5) ≈ 0.4794` rather than `0.5` unchanged, and that no `Custom`/passthrough bug survived.)

Expected: `tan`/`tanh` print correctly-transformed values, not passthrough values.

- [ ] **Step 4: Commit**

```bash
git add physure-python/src/lib.rs
git commit -m "feat(core): add sin/cos/tan/exp/log/tanh pymethods to PyQuantity"
```

---

### Task 4: Harden `TensorBackend::mean()` / `std_dev()`

**Files:**
- Modify: `physure-python/src/lib.rs:70-90` (`impl UncertaintyBackend for TensorBackend`'s `mean`/`std_dev`)

- [ ] **Step 1: Replace the silent-failure defaults**

Current code (`physure-python/src/lib.rs:69-90`):
```rust
impl UncertaintyBackend for TensorBackend {
    fn mean(&self) -> f64 {
        // For tensor backends, mean() is not a meaningful scalar.
        // Use mean_object() via Python for full tensor access.
        Python::with_gil(|py| {
            self.value
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .unwrap_or(f64::NAN)
        })
    }

    fn std_dev(&self) -> f64 {
        Python::with_gil(|py| {
            self.uncertainty
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .unwrap_or(0.0)
        })
    }
```

Replace with:
```rust
impl UncertaintyBackend for TensorBackend {
    fn mean(&self) -> f64 {
        Python::with_gil(|py| {
            self.value
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .expect(
                    "TensorBackend: cannot extract scalar mean from a multi-element magnitude — \
                     array-valued Rust uncertainty backends are not yet supported",
                )
        })
    }

    fn std_dev(&self) -> f64 {
        Python::with_gil(|py| {
            self.uncertainty
                .bind(py)
                .call_method0("item")
                .and_then(|v| v.extract::<f64>())
                .expect(
                    "TensorBackend: cannot extract scalar std_dev from a multi-element magnitude — \
                     array-valued Rust uncertainty backends are not yet supported",
                )
        })
    }
```

- [ ] **Step 2: Rebuild the extension**

Run: `cd physure-python && maturin develop`

Expected: builds cleanly.

- [ ] **Step 3: Manual smoke test — confirm the panic surfaces as a catchable Python exception, not a process abort**

This confirms the spec's stated open risk (PyO3's panic-to-`PanicException` conversion at the `#[pymethods]` FFI boundary) empirically, per the spec's "Risks / open questions" section.

Run:
```bash
uv run python -c "
import physure as mk
import numpy as np
with mk.uncertainty_mode('gaussian'):
    q = mk.Q_(1.0, 'm', uncertainty=0.1)
    try:
        result = q + mk.Q_(np.array([1.0, 2.0, 3.0]), 'm')
        print('NO EXCEPTION RAISED — bug still present:', result)
    except Exception as e:
        print('Correctly raised:', type(e).__name__, e)
"
```

Expected: prints `Correctly raised: PanicException ...` (or similar), not a process crash and not a silent `NaN` result.

- [ ] **Step 4: Commit**

```bash
git add physure-python/src/lib.rs
git commit -m "fix(core): panic instead of silently returning NaN/0.0 in TensorBackend::mean/std_dev"
```

---

### Task 5: Wire Rust delegation into `_apply_transcendental`

**Files:**
- Modify: `physure-python/physure/domain/measurement/_arithmetic_mixin.py` (add `_check_and_handle_rust_transcendental` near `_check_and_handle_rust_propagation` at lines 176-189; wire into `_apply_transcendental` after line 863)
- Test: `physure-python/tests/test_rust_transcendental_delegation.py` (new)

- [ ] **Step 1: Write the failing unit test for the new method**

`_check_and_handle_rust_transcendental` needs its own direct test because black-box numeric parity alone won't fail before wiring — both the existing Python path and the not-yet-added Rust path compute the same correct values, so a numeric-only test would pass even with `_check_and_handle_rust_transcendental` never called. This test asserts the method exists and returns the right thing in both branches.

Create `physure-python/tests/test_rust_transcendental_delegation.py`:

```python
import math

import numpy as np
import pytest

import physure as mk
from physure import Q_
from physure.domain.measurement.quantity import Quantity


def get_val(q):
    mag = q.magnitude
    return float(mag.mean()) if hasattr(mag, "mean") else float(mag)


def test_check_and_handle_rust_transcendental_not_rust_wrapped():
    with mk.propagation_mode("correlated"):
        q = Q_(0.5, "rad", uncertainty=0.1)
        assert q._check_and_handle_rust_transcendental("sin") is None


def test_check_and_handle_rust_transcendental_rust_wrapped():
    with mk.uncertainty_mode("gaussian"):
        q = Q_(0.5, "rad", uncertainty=0.1)
        result = q._check_and_handle_rust_transcendental("sin")
        assert result is not None
        assert isinstance(result, Quantity)
        assert math.isclose(get_val(result), math.sin(0.5), rel_tol=1e-9)
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest physure-python/tests/test_rust_transcendental_delegation.py -xvs`

Expected: FAIL with `AttributeError: 'Quantity' object has no attribute '_check_and_handle_rust_transcendental'` (or similar) on both tests.

- [ ] **Step 3: Add `_check_and_handle_rust_transcendental`**

In `physure-python/physure/domain/measurement/_arithmetic_mixin.py`, add immediately after `_check_and_handle_rust_propagation` (which ends at line 189), mirroring its shape:

```python
    def _check_and_handle_rust_transcendental(self, func_name: str) -> Quantity | None:
        if not self._has_rust_operand(None):
            return None
        new_core = getattr(self._core_magnitude, func_name)()
        return type(self).from_input(new_core, CompoundUnit({}), self.system)
```

- [ ] **Step 4: Wire it into `_apply_transcendental`**

`_apply_transcendental` (`physure-python/physure/domain/measurement/_arithmetic_mixin.py:844-905`) currently does dimension/angle verification first (assigning the verified quantity to `q`, ending at line 863), then computes in Python. Insert the Rust-delegation check on `q` right after verification, before the `# 2. Get backend function` comment (line 865):

Before:
```python
        else:
            raise DimensionError(
                f"Argument to {func_name}() must be dimensionless or "
                f"an angle, got unit '{self.unit}'."
            )

        # 2. Get backend function
        op = getattr(self._backend, func_name)
```

After:
```python
        else:
            raise DimensionError(
                f"Argument to {func_name}() must be dimensionless or "
                f"an angle, got unit '{self.unit}'."
            )

        # 1b. Delegate to Rust if this quantity is Rust-wrapped (opt-in scalar path)
        if (rust_result := q._check_and_handle_rust_transcendental(func_name)) is not None:
            return rust_result

        # 2. Get backend function
        op = getattr(self._backend, func_name)
```

(Note: the check runs on `q`, the dimension/angle-verified quantity — e.g. converted to `rad` for angles — not on `self`, matching the existing verification step's already-established variable.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `uv run pytest physure-python/tests/test_rust_transcendental_delegation.py -xvs`

Expected: both tests **PASS**.

- [ ] **Step 6: Commit**

```bash
git add physure-python/physure/domain/measurement/_arithmetic_mixin.py physure-python/tests/test_rust_transcendental_delegation.py
git commit -m "feat(measurement): delegate transcendental functions to Rust when Rust-wrapped"
```

---

### Task 6: Parity and hardening pytest suite

**Files:**
- Modify: `physure-python/tests/test_rust_transcendental_delegation.py` (append)

- [ ] **Step 1: Write parity tests for all six transcendentals**

Append to `physure-python/tests/test_rust_transcendental_delegation.py`:

```python
@pytest.mark.parametrize(
    ("func_name", "input_value", "input_unit"),
    [
        ("sin", 0.5, "rad"),
        ("cos", 0.5, "rad"),
        ("tan", 0.5, "rad"),
        ("exp", 0.5, ""),
        ("log", 2.0, ""),
        ("tanh", 0.5, ""),
    ],
)
def test_rust_python_parity(func_name, input_value, input_unit):
    with mk.uncertainty_mode("gaussian"):
        rust_q = Q_(input_value, input_unit, uncertainty=0.1)
        rust_result = getattr(rust_q, func_name)()

    with mk.propagation_mode("correlated"):
        python_q = Q_(input_value, input_unit, uncertainty=0.1)
        python_result = getattr(python_q, func_name)()

    assert math.isclose(get_val(rust_result), get_val(python_result), rel_tol=1e-9)
    assert math.isclose(rust_result.uncertainty, python_result.uncertainty, rel_tol=1e-6)
```

- [ ] **Step 2: Run the parity tests**

Run: `uv run pytest physure-python/tests/test_rust_transcendental_delegation.py -xvs -k parity`

Expected: all 6 parametrized cases **PASS**. If any fail, it means the Rust formula (Task 1/2) and the Python formula (`_apply_transcendental`'s explicit derivatives) disagree — fix the Rust formula to match Python's, since Python's is the existing, already-trusted behavior.

- [ ] **Step 3: Write the `TensorBackend` hardening test**

Append:

```python
def test_tensor_backend_mean_raises_on_multi_element_array():
    with mk.uncertainty_mode("gaussian"):
        rust_q = Q_(1.0, "m", uncertainty=0.1)
        array_q = Q_(np.array([1.0, 2.0, 3.0]), "m")
        with pytest.raises(Exception):
            _ = rust_q + array_q
```

- [ ] **Step 4: Run it**

Run: `uv run pytest physure-python/tests/test_rust_transcendental_delegation.py -xvs -k tensor_backend`

Expected: **PASS** (Task 4 already made this raise; this just locks it into the permanent suite).

- [ ] **Step 5: Commit**

```bash
git add physure-python/tests/test_rust_transcendental_delegation.py
git commit -m "test: add Rust/Python parity and TensorBackend hardening tests"
```

---

### Task 7: Full verification and final commit

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test -p physure-core`

Expected: all tests pass.

- [ ] **Step 2: Rebuild the extension one final time**

Run: `cd physure-python && maturin develop`

Expected: clean build.

- [ ] **Step 3: Run the full Python test suite with coverage**

Run: `uv run pytest --cov=physure --cov-report=term-missing`

Expected: all tests pass, coverage stays ≥ 80%.

- [ ] **Step 4: Lint and format**

Run:
```bash
uv run ruff check .
uv run ruff format --check .
```

Expected: both clean. If `ruff format --check` fails, run `uv run ruff format .` and re-check.

- [ ] **Step 5: Final review commit (if lint/format required changes)**

```bash
git add -A
git commit -m "style: ruff format fixes for transcendental delegation feature"
```

(Skip this step if Step 4 required no changes — nothing to commit.)

---

## Self-Review

**Spec coverage:**
- Scope item 1 (tan/tanh fix in the three named Rust files) → Task 2, plus Task 1 for the actually-live enum-level fix the spec missed. Both corrections stated up front.
- Scope item 2 (six new `PyQuantity` pymethods) → Task 3, using `RationalUnit::dimensionless()` per the corrected understanding.
- Scope item 3 (`_check_and_handle_rust_transcendental` + wiring) → Task 5.
- Scope item 4 (`TensorBackend` hardening) → Task 4.
- Testing section (Rust unit tests, Rust/Python parity tests, `TensorBackend` hardening test) → Tasks 1, 2, 6.
- Non-goals (default modes, array magnitudes, `UncertaintyBackend` signature widening, `TensorBackend` arithmetic) → untouched by any task, as intended.

**Placeholder scan:** No "TBD"/"TODO"/"implement later" strings. Task 3 Step 3's smoke test includes a parenthetical telling the executor to check the real `#[new]` constructor if the illustrative call doesn't match — this is a legitimate hedge for a manual verification step against a constructor signature not pinned in this plan (the pymethods themselves, which are the actual deliverable, have complete, exact code), not a deferred implementation detail.

**Type/signature consistency:** `_check_and_handle_rust_transcendental(self, func_name: str) -> Quantity | None` (Task 5) matches its two call sites (Task 5 Step 4's wiring, Task 5 Step 1's direct test). The six Rust pymethod names (`sin`/`cos`/`tan`/`exp`/`log`/`tanh`) match exactly across Task 3 (definition), Task 5 (`getattr(self._core_magnitude, func_name)()`), and Task 6 (`getattr(rust_q, func_name)()` in the parametrized test). `RationalUnit::dimensionless()` used consistently in all six Task 3 pymethods.

---

**Plan complete and saved to `physure-python/docs/superpowers/plans/2026-07-16-rust-transcendental-delegation.md`.** Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
