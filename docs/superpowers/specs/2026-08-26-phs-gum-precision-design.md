# GUM-style correlated precision for uncertain quantities

## Context

`Quantity`'s `Display` impl (`physure-core/src/quantity.rs:105`) formats a magnitude and its
uncertainty independently: `format!("{} ± {}", format_float(val), format_float(std_dev))`. Each
side is rendered with `format_float` — "shortest string that round-trips the f64" — with no
correlation between them. The `phs_primer.phs` tutorial exposes this directly:

```
kinetic_energy          = 625.0 ± 40.0195264839553 J
```

Fifteen significant figures on an uncertainty is itself a false-precision claim: no measurement
process determines its own error bar to fifteen digits. The GUM (*Guide to the Expression of
Uncertainty in Measurement*) convention — and the common lab-physics rule of thumb it's usually
taught as (Taylor, *An Introduction to Error Analysis*) — rounds the uncertainty to one or two
significant figures and rounds the magnitude to match, so the reported precision of the value
never overstates the precision of the measurement that produced it.

This spec covers two pieces: the default rounding behavior (Section 1) and an opt-in per-variable
override (Section 2), because the second only makes sense once the first exists to override.

## Section 1: Default GUM rounding

### The rule

1. Round the uncertainty to **1 significant figure**, unless its leading digit is `1`, in which
   case use **2** — dropping `0.15` to one figure would be `0.2`, an ~33% jump; keeping two
   figures (`0.15`) avoids rounding away a meaningful fraction of the value on exactly the cases
   where it matters most. This is the standard "leading-digit" convention.
2. Round the magnitude to the **same decimal place** the rounded uncertainty landed on. Never
   show the magnitude to more precision than the (rounded) uncertainty justifies.

Rounding to 1 significant figure can itself carry a digit over (`0.96` → `1.0`), which would
silently produce a leading digit of `1` that step 1's own rule exists to catch. Resolved by
computing the 1-sig-fig candidate first and checking *its* leading digit, not the pre-rounding
value's: if the 1-sig-fig candidate's leading digit is `1` (whether that came from the original
value or from carrying over), use the 2-sig-fig rounding instead.

Worked examples:

| Input | Uncertainty leading digit | Sig figs used | Output |
|---|---|---|---|
| `625.0 ± 40.0195264839553` | 4 | 1 | `630 ± 40` |
| `2.0 ± 0.1` | 1 | 2 | `2.00 ± 0.10` |
| `9.81 ± 0.05` | 5 | 1 | `9.81 ± 0.05` (already matches) |
| `19.62 ± 0.981` | 9 (rounds to `1.0`, so the carry-over rule above applies) | 2 | `19.62 ± 0.98` |

A value with no uncertainty (`std_dev == 0.0`) is untouched — there's nothing to round against,
and this path already exists (`Display`'s `if std_dev > 0.0 { .. } else { format_float(val) }`).

### Where it lives

A new free function in `physure-core/src/quantity.rs`, next to `format_float`:

```rust
pub fn gum_round(mean: f64, std_dev: f64, sig_figs_override: Option<u32>) -> (String, String)
```

Returns `(mean_str, std_dev_str)`, both already formatted to the matching decimal place.
`sig_figs_override` is `None` for the default leading-digit rule; Section 2 is the only caller
that ever passes `Some(n)`.

### Call sites (fixes a real duplication as a side effect)

Three places currently format a magnitude/uncertainty pair, and only one of them is `Quantity`'s
own `Display`:

- `physure-core/src/quantity.rs` — `Display for Quantity` (the `val_str` branch when
  `std_dev > 0.0`). Switches from independent `format_float` calls to `gum_round(mean, std_dev,
  None)`.
- `physure-cli/src/html.rs` — `format_val_latex`. Independently calls `format_float` on
  `q.value.mean()` and `q.value.std_dev()` today; switches to `gum_round`.
- `physure-cli/src/web.rs` — `format_val_latex`. Byte-for-byte the same duplicated logic as
  `html.rs`'s version; switches to `gum_round` too.

All three keep their existing scientific-notation handling (the `e`/`E` → `\times 10^{...}`
LaTeX conversion in the CLI's two copies) — `gum_round` returns plain decimal strings for the
normal range and defers to `format_float`'s existing `{:e}` path outside it (very large/small
magnitudes, `NaN`/`inf`), so that conversion still has something to act on.

### What this does not touch

- **The underlying value never changes.** `gum_round` takes `f64`s and returns `String`s; it
  never writes back into `Quantity.value`. Every subsequent calculation, `:.15f` format spec,
  `: base` / `: frac` (which already build their own strings via `apply_format_spec`'s `render`
  closure, never through `Display`), JSON export, and transpiled code keeps full `f64` precision
  — this is presentation-only, by construction, not a policy that has to be remembered to hold.
- **`physure-python`'s `Quantity.__str__`/`__repr__`/`__rich__`** (`physure-python/physure/
  domain/measurement/quantity.py`) build their own `f"{magnitude} ± {uncertainty}"` string in
  Python, independent of Rust's `Display` — and the Python side supports array-valued
  uncertainty across multiple backends (torch/numpy/jax) that the scalar Rust `Quantity` doesn't
  model at all. Extending GUM rounding there is a separate, larger piece of work (what "leading
  digit" even means for a tensor of uncertainties is its own design question) and is explicitly
  out of scope for this spec.
- **Codegen** (`physure-script/src/codegen/*`) generates typed source code from the AST, never
  calls `Display` to do it — no interaction with this change.

## Section 2: `@precision(N)` per-variable override

### Syntax

```phs
@precision(2)
kinetic_energy = 0.5 * mass * velocity^2
```

One positional integer argument, on a plain assignment's decorator line (`decorated_stmt`
already accepts `assignment`, alongside `function_def`/`assignment_fn` — no grammar change
needed). Scoped to assignments only, matching "per variable" — not valid on a `fn` definition,
enforced the same way `@requires`/`@ensures`/`@implicit_units` already restrict themselves to
one AST shape in `decorators.rs`'s `check_decorator_list`.

### Semantics — one argument, two meanings by context

Decided over `@precision(sig_figs, decimals)` (both always independent) specifically to avoid
reopening the false-precision problem Section 1 exists to close: an independent `decimals`
that disagreed with what the (possibly overridden) uncertainty's sig-figs actually justify would
let someone show `123.456 ± 20`, precision the measurement doesn't have.

`N`'s meaning is read off the variable's own computed value at render time, not from anything in
the decorator syntax itself:

- **The value carries uncertainty (`std_dev > 0.0`):** `N` overrides how many significant
  figures the uncertainty rounds to (replacing the leading-digit rule's automatic 1-or-2). The
  magnitude still derives its decimal place from that — same correlation as the default, just a
  chosen sig-fig count instead of the automatic one. Calls `gum_round(mean, std_dev, Some(n))`.
- **The value is exact (`std_dev == 0.0`):** there's no uncertainty to round against, so `N` is
  read as a plain decimal-places count instead: `format!("{:.*}", n, mean)`.

This keeps `@precision`'s one argument meaningful in both cases without ever letting the
magnitude's shown precision drift independently of what the uncertainty (chosen or automatic)
justifies.

### Validation (`physure-script/src/decorators.rs`)

Add `"precision"` to `KNOWN_DECORATORS`. In `check_decorator_list`'s match:

```rust
"precision" => {
    if func.is_some() {
        return Err(PhysureError::Generic(
            "@precision is only valid on a variable assignment, not a function definition".to_string(),
        ));
    }
    if dec.args.len() != 1 {
        return Err(PhysureError::Generic(format!(
            "@precision expects exactly 1 argument (a positive integer), got {}",
            dec.args.len()
        )));
    }
    let is_valid_count = matches!(
        &dec.args[0],
        Expr::Quantity(QuantityNode { magnitude, .. }) if *magnitude > 0.0 && magnitude.fract() == 0.0
    );
    if !is_valid_count {
        return Err(PhysureError::Generic(
            "@precision's argument must be a positive whole number".to_string(),
        ));
    }
}
```

(Mirrors `@requires`'s `func.is_none()` check, inverted — `@requires` requires a function,
`@precision` requires *not* one.)

### Threading the value through to the renderer

`ExecutionStep` (`physure-cli/src/step.rs`) gains a field:

```rust
pub precision_override: Option<u32>,
```

In `main.rs`'s statement loop, alongside the existing `display_label` computation: if `stmt` is
`Statement::Assignment(node)` and `node.decorators` contains `precision`, extract its integer
argument into `precision_override`. Every consumer of `ExecutionStep` that renders a `Quantity`
picks it up from there — no separate plumbing per renderer:

- `RichRenderer::render_variable_card` (`physure-cli/src/rich.rs`) gains a `precision_override:
  Option<u32>` parameter; when `Some(n)`, builds the value string via `gum_round(mean, std_dev,
  Some(n))` (uncertain case) or `format!("{:.*}", n, mean)` (exact case) instead of `q.to_string()`.
- `html.rs`/`web.rs`'s `format_val_latex` gain the same parameter, same branch.

### What this does not touch

- **Functions.** `@precision` on a `fn` definition is a validation error (see above) — a
  function's return value has no single call site to attach a fixed precision to; this is
  explicitly a per-variable, per-assignment feature for this pass.
- **physure-python**, for the same reason as Section 1.

## Testing strategy

- `physure-core`: unit tests for `gum_round` covering the table in Section 1 (including the
  leading-digit-1 two-sig-fig case), the `std_dev == 0.0` passthrough, and a `sig_figs_override`
  case.
- `physure-script`: `decorators.rs` unit tests mirroring the existing `@stable`/`@requires`
  pattern — accepts a valid positive integer, rejects a non-assignment target, rejects wrong
  arity, rejects a non-integer/negative/zero argument.
- `physure-cli`: integration tests (mirroring `test_bare_string_expressions_print_as_plain_text_
  not_a_label_equals_value_card`'s shape) proving: (a) the default GUM rounding on a plain script
  with no `@precision` (the `phs_primer.phs` kinetic-energy line is a real, already-available
  fixture), (b) `@precision(N)` on an uncertain value overrides the sig-fig count, (c)
  `@precision(N)` on an exact value sets decimal places, (d) an explicit format spec (`:.15f`)
  still bypasses all of this and shows full precision on request.

## Explicitly out of scope

- `physure-python`'s independent `Quantity` formatting (Sections 1 and 2 both).
- Any change to codegen output.
- `@precision` on function definitions.
- Changing what gets *stored* — this spec is display-only, end to end.
