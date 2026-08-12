# PHS `assert` / `exact_assert` builtins

## Context

There is no way today for a `.phs` script to declare "this computed value must equal
that value" and have the check survive transpilation. `@requires`/`@ensures` are the
closest existing mechanism, but they are interpreter-only — none of the four codegen
files (`rust.rs`, `python.rs`, `java.rs`, `js.rs`) touch decorators at all, so they are
silently dropped from every transpiled target. This spec adds two new builtins,
`assert` and `exact_assert`, that *do* flow through every codegen target, so a
generated program fails at runtime in whichever environment gets the physics wrong.

This is prerequisite infrastructure for a separate, external verification harness
(`D:\Projects\test_physure`, not part of this repo) that will run each `.phs` example
through the interpreter and through every transpile target and rely on `assert`/
`exact_assert` failures to catch cross-environment discrepancies. That harness is out
of scope for this spec.

v1 scope is deliberately narrow — Quantity operands only, two fixed builtins, no
configurable tolerance, no boolean-condition form. All of that can expand later.

## Decision: semantics

- **`assert(actual, expected)`** — passes when `actual` and `expected` have compatible
  dimensions and their magnitudes agree after unit conversion, within a fixed
  tolerance. This is exactly `Quantity::approx_eq(rel_tol, abs_tol)`, which already
  exists in `physure-core` — no new comparison math. Defaults: `rel_tol = 1e-9`,
  `abs_tol = 1e-12`. Fails (raises) on dimension mismatch or magnitude outside
  tolerance.
- **`exact_assert(actual, expected)`** — passes when `actual.unit == expected.unit`
  (bit-exact `scale`/`offset`/`id`) *and* the magnitudes are bit-exact. No conversion.
  `RationalUnit`'s existing `PartialEq` already ignores `display_name`, so `m` and
  `meters` — same `id`/`scale`/`offset`, different alias spelling — already compare
  equal; "unit aliases are OK" falls out for free, nothing new to build there.
- Both are 2-argument calls used as standalone statements (`assert(a, b)` on its own
  line), not composed inside larger expressions, for v1.
- On failure: a new `PhysureError::AssertionFailed { kind: &'static str, message:
  String }` variant (`kind` is `"assert"` or `"exact_assert"`), following the existing
  `ContractViolation` variant's shape. `message` names both operands, e.g. `"3.94 kOhm
  != 3.9 kOhm (diff 0.04 kOhm exceeds tolerance)"`. This propagates through the
  interpreter and every generated-code error path exactly like any other
  `PhysureError` today — no new CLI plumbing.

## Architecture

Bottom-up, mirroring how every other cross-language capability in this repo is laid
out per `structure.md`: implement once in `physure-core`, thin-wrap in each binding.

**1. `physure-core/src/quantity.rs`** — two new methods:

```rust
pub fn phs_assert(&self, other: &Quantity) -> PhysureResult<()>
pub fn phs_exact_assert(&self, other: &Quantity) -> PhysureResult<()>
```

`phs_assert` wraps `approx_eq` with the fixed default tolerances above, returning
`AssertionFailed` with a formatted message on `false`/dimension-mismatch.
`phs_exact_assert` checks `self.unit == other.unit` and bit-exact magnitude
(`.to_bits()` comparison, so `NaN` compares consistently), same error shape.

**2. `physure-script`** — register `assert`/`exact_assert` as known 2-arg builtin
names (arity-checked, both args must evaluate to `Quantity` — clear error otherwise,
per the "raise, don't silently coerce" project philosophy). `interpreter.rs` evaluates
both arguments and calls the matching new core method, propagating its error.

**3. Codegen — one new emission arm per target**, added wherever
`Statement::Expr(FunctionCall { .. })` is currently handled:

| Target | Emitted call | Binding-side plumbing needed |
|---|---|---|
| Rust (`rust.rs`) | `a.phs_assert(&b)?;` | none — `physure-core` method used directly |
| Python (`python.rs`) | `a.phs_assert(b)` | new PyO3 method on `physure-python`'s `Quantity` (`physure-python/src/lib.rs`), using its existing `PhysureError`→`PyErr` conversion pattern |
| Java (`java.rs`) | `a.physAssert(b);` | new JNI method on `com.physure.Quantity` (`physure-java/src/lib.rs`), using the existing `throw_new(env, "com/physure/PhysureException", msg)` helper |
| JS/TS (`js.rs`, shared) | `a.physAssert(b);` | new wasm-bindgen method on `physure-wasm`'s `Quantity`, using the existing `to_js_error` helper so it throws a JS `Error` |

`exact_assert` mirrors the same row per target (`phs_exact_assert` / `physExactAssert`).
JS/TS method naming follows Java's camelCase, per the existing js.rs convention
(`getValue()`-style) documented in the JS/TS codegen spec.

`@requires`/`@ensures`/`@range` and their decorator machinery (`decorators.rs`) are
untouched — a separate, pre-existing mechanism this spec does not change.

## Testing

Each layer gets its *existing* test pattern extended, no new test infrastructure:

- `physure-core`: unit tests for `phs_assert`/`phs_exact_assert` — same-unit pass,
  cross-unit-but-equal-dimension pass/fail for `assert`, alias-unit pass for
  `exact_assert`, dimension-mismatch error, bit-exact failure.
- `physure-script`: interpreter test (pass + failure-message shape) plus one codegen
  test per target confirming the emitted line, following `java.rs`'s existing
  per-target unit test style, and extended into the two shared loop-based tests in
  `mod.rs` (same pattern the JS/TS codegen spec used for adding new targets).
- Each binding crate (`physure-python`, `physure-java`, `physure-wasm`): one test in
  whatever test suite already exists there, confirming the new method raises/throws on
  a mismatched pair.

## Out of scope

- Non-Quantity operands (bool/string/etc.).
- Inline/composable use inside larger expressions.
- A configurable-tolerance argument (`assert(a, b, tol=...)`).
- A boolean-condition `assert(cond)` form.
- The `test_physure` verification harness itself (separate spec, depends on this one).
