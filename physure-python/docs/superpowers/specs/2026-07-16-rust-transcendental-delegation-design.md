# Complete Rust delegation for transcendental functions (opt-in scalar path)

## Context

`physure` has two parallel uncertainty-propagation systems. The default `correlated`/
`uncorrelated` modes run entirely in Python/Autograd (`Uncertainty.propagate` →
`CovarianceModel`/`VarianceModel`, driven by `AutogradPropagator`) for both scalar and array
magnitudes — that system is explicitly out of scope for this work. Separately, an opt-in scalar
path wraps `Quantity._core_magnitude` in `physure._core.Quantity` (Rust `PyQuantity`) when
propagation mode is `monte_carlo`, `unscented`, or `gaussian` and the magnitude is a scalar float
(`Quantity._maybe_wrap_in_rust_core`, quantity.py:471). Arithmetic (`+ - * / ** neg abs`) already
delegates to Rust when this wrapping is active, via `_check_and_handle_rust_propagation` in
`_arithmetic_mixin.py`.

Transcendental functions (`sin`/`cos`/`tan`/`exp`/`log`/`tanh`) do not participate in this
delegation today — `_apply_transcendental` always computes in Python regardless of whether
`_core_magnitude` is Rust-wrapped. Investigation found two concrete gaps blocking straightforward
delegation:

1. **Rust has no transcendental methods on `PyQuantity` at all.** Only `__abs__` calls
   `self.0.value.propagate_function("abs")` (physure-python/src/lib.rs:457-461); there is no
   `sin`/`cos`/`tan`/`exp`/`log`/`tanh` pymethod to call into.
2. **`tan`/`tanh` are unimplemented in all three Rust uncertainty backends.** `propagate_function`
   in `GaussianBackend`, `MonteCarloBackend`, and `UnscentedBackend`
   (physure-core/src/uncertainty/{gaussian,monte_carlo,unscented}.rs) matches `"sin"`/`"cos"`/
   `"exp"`/`"log"`/`"abs"` and falls through to an unchanged passthrough for anything else —
   meaning `tan`/`tanh` would silently return the *input* mean/std_dev/samples unchanged if called
   today. This is a dormant bug, never triggered because Python never calls it for these two
   functions.

A related, separately-reachable bug was found in the same investigation: `TensorBackend::mean()`/
`std_dev()` (physure-python/src/lib.rs) extract a scalar via `.item()` and swallow failure with
`.unwrap_or(f64::NAN)` / `.unwrap_or(0.0)` respectively. This is not theoretical — it is reachable
today via ordinary arithmetic: `rust_scalar_quantity + numpy_array` (array magnitude, size > 1)
goes `__add__` → `extract_value_and_unit` → `to_backend` (wraps the array as `Custom(TensorBackend)`
since it isn't a scalar float) → `propagate_add`'s cross-backend arm calls `other.mean()` on it →
`.item()` raises for a multi-element array → silently becomes `NaN` instead of an error. This
violates the project's core invariant: "a wrong answer with confident units is worse than an
exception."

## Scope

### 1. Fix `tan`/`tanh` in the three Rust uncertainty backends

Add matching arms to `propagate_function` in each of `GaussianBackend`, `MonteCarloBackend`,
`UnscentedBackend`, consistent with existing `sin`/`cos`/`exp`/`log`/`abs` arms and with the
derivative formulas already used by the Python path (`_arithmetic_mixin.py`'s
`_apply_transcendental`):

- **Gaussian**: `"tan" => (m.tan(), ((1.0 + m.tan().powi(2)) * s).abs())`,
  `"tanh" => (m.tanh(), ((1.0 - m.tanh().powi(2)) * s).abs())`
- **MonteCarlo**: `"tan" => self.samples.mapv(|x| x.tan())`,
  `"tanh" => self.samples.mapv(|x| x.tanh())`
- **Unscented**: `"tan" => self.sigma_points.mapv(|x| x.tan())`,
  `"tanh" => self.sigma_points.mapv(|x| x.tanh())`

This must ship in the same change as item 2 below — until fixed, calling `tan`/`tanh` through
`propagate_function` produces silently wrong results, so wiring Python delegation before this fix
lands would introduce a live correctness bug, not just fail to improve anything.

### 2. Add transcendental methods to `PyQuantity` (physure-python/src/lib.rs)

Add `sin`, `cos`, `tan`, `exp`, `log`, `tanh` as new `#[pymethods]` on `PyQuantity`, each mirroring
`__abs__`'s existing shape exactly:

```rust
fn sin(&self) -> PyResult<PyQuantity> {
    let new_val = self.0.value.propagate_function("sin")
        .map_err(|e| pyo3::exceptions::PyArithmeticError::new_err(e.to_string()))?;
    Ok(PyQuantity(Quantity::from_value(new_val, self.0.unit.clone())))
}
```

(and same for the other five, substituting the function name). Result unit is always
`RationalUnit::dimensionless()`-equivalent — matches Python's contract, since the dimension/angle
handling (see item 3) happens before Rust is ever called.

### 3. Wire Rust delegation into `_apply_transcendental` (`_arithmetic_mixin.py`)

Add `_check_and_handle_rust_transcendental(self, func_name: str) -> Quantity | None`, mirroring
`_check_and_handle_rust_propagation`'s existing pattern:

- If `_has_rust_operand(None)` is `False` (not Rust-wrapped, or `physure._core` unavailable),
  return `None` immediately — caller falls through to the existing Python computation, unchanged.
- Otherwise call the corresponding new `PyQuantity` method (e.g. `self._core_magnitude.sin()`) and
  wrap the result via `type(self).from_input(new_core, CompoundUnit({}), self.system)`.

`_apply_transcendental` calls this check immediately after its existing dimension/angle
verification step (unchanged — still runs first, still converts to `rad` or dimensionless before
either path executes), and only proceeds to the current Python numeric branch if the check returns
`None`.

The existing Python path (explicit derivatives for `sin`/`cos`/`tan`/`exp`/`log`/`tanh`, finite-
difference fallback otherwise) is not modified. It remains the only path for: the default
`correlated`/`uncorrelated` modes, array magnitudes (which never get Rust-wrapped —
`_maybe_wrap_in_rust_core` calls `float(value)` unconditionally once mode matches, which raises for
multi-element arrays before Rust is ever involved), and environments without the compiled Rust
extension.

### 4. Harden `TensorBackend::mean()` / `std_dev()` (physure-python/src/lib.rs)

Replace the silent-failure defaults:

```rust
.and_then(|v| v.extract::<f64>()).unwrap_or(f64::NAN)   // mean()
.and_then(|v| v.extract::<f64>()).unwrap_or(0.0)         // std_dev()
```

with `.expect("TensorBackend: cannot extract scalar mean/std_dev from a multi-element magnitude —
array-valued Rust uncertainty backends are not yet supported")` (or equivalent loud failure).

`mean()`/`std_dev()` return plain `f64` per the `UncertaintyBackend` trait (not
`PhysureResult<f64>`), and are called from ~10 sites across the three concrete backends' mixed-type
arithmetic arms. Panicking rather than widening the trait signature is the deliberate choice here:
PyO3 already catches unwinds at every `#[pymethods]` boundary these calls are reachable from
(`__add__`, `__sub__`, `__mul__`, etc.) and converts them to a Python `PanicException` — so the
failure surfaces as a normal Python exception with zero signature changes and zero blast radius
beyond these two methods. Widening `UncertaintyBackend::mean`/`std_dev` to return a `Result` would
ripple through every implementor (`Gaussian`, `MonteCarlo`, `Unscented`, `Custom`) for one backend's
one failure mode.

## Non-goals

- Any change to the default `correlated`/`uncorrelated` propagation path, or to array-magnitude
  handling in general — explicitly out of scope per prior decision this session. That's Phase 2
  (N-d array/tensor support in Rust), a separate future spec.
- Broadening Rust delegation to any operation beyond the six transcendentals listed (e.g. no new
  triggers, no change to which modes cause Rust-wrapping).
- Widening `UncertaintyBackend::mean`/`std_dev` to a `Result`-returning signature — rejected above
  in favor of `.expect()`.
- Any change to `TensorBackend`'s arithmetic (`propagate_add`/`sub`/`mul`/`div`/`pow`) beyond the
  two extraction methods — those already correctly delegate to Python dunder methods generically.

## Testing

- **Rust unit tests** (`#[cfg(test)]` in each of `gaussian.rs`/`monte_carlo.rs`/`unscented.rs`):
  assert `propagate_function("tan")`/`("tanh")` against hand-computed values for a known
  `(mean, std_dev)`, e.g. `mean=0.5` → `new_mean == 0.5_f64.tan()`,
  `new_std == ((1.0 + 0.5_f64.tan().powi(2)) * std_dev).abs()`.
- **Rust/Python parity tests** (`tests/`): for each of `sin`/`cos`/`tan`/`exp`/`log`/`tanh`,
  construct the same scalar `Quantity` under `propagation_mode("gaussian")` (Rust-eligible) and
  under `"correlated"` (Python-only), call the method on both, assert `magnitude` and `std_dev`
  match within tolerance. This is what actually proves delegation is behavior-preserving and is
  the test most likely to catch a tan/tanh formula error before it ships.
- **TensorBackend hardening test**: construct a Rust-wrapped scalar `Quantity` (`monte_carlo` or
  `gaussian` mode), add or multiply it with a raw multi-element numpy array, assert it now raises
  instead of returning a `Quantity` with `NaN` magnitude. One test covers `mean()`; a second for
  `std_dev()` isn't needed since the fix is structurally identical.
- No new tests needed for `abs` (already delegates, unaffected), the Python fallback path itself
  (untouched, covered by existing suite), or default-mode behavior (out of scope).

## Risks / open questions

None blocking. The `.expect()`-based panic approach for `TensorBackend` relies on PyO3's documented
panic-to-`PanicException` conversion at the `#[pymethods]` FFI boundary; this should be confirmed
empirically during implementation (one quick manual check: trigger the panic path and confirm
Python sees a catchable exception, not a process abort) before considering the hardening item
done.
