# Track B — Concurrency Design Spec (`rayon` parallelism + `parallel_map`)

**Date**: 2026-08-12
**Status**: Approved
**Subsystem**: `physure-script` (Interpreter, Builtins), `physure-core` (Settings/conf)

---

## 1. Overview & Goals

Track B adds real compute parallelism to PhysureScript (PHS), scoped to the two places the
roadmap calls for and nothing else:

1. **Transparent parallelism for `Expr::ForExpr`** — above a configurable element-count
   threshold, the interpreter evaluates for-loop bodies on `rayon`'s thread pool instead of
   sequentially. No PHS syntax changes.
2. **`parallel_map(fn, vector)` builtin** — an explicit, always-parallel primitive for running a
   user-defined PHS function across a vector's elements (parameter sweeps, Monte Carlo trials).

**Explicitly out of scope** (deviation from the roadmap's original sketch, decided during
brainstorming): `gradient` and `trapz` are **not** parallelized. `trapz` is a running
accumulation (each step depends on the previous total) and cannot be parallelized without
restructuring as a parallel-reduce; `gradient`'s per-element work is a couple of subtractions,
cheap enough that thread-dispatch overhead would likely make it net-negative. Neither is worth
the complexity for LAB-READY. `Statement::While` is also out of scope — convergence loops are
inherently sequential (each iteration depends on the last).

---

## 2. Configuration (`physure-core`)

Threshold is configurable via `physure.conf`, not a hardcoded constant, so it can be tuned
per-deployment without a recompile — and not a per-binding API/env-var, so every language
binding (CLI, LSP, Python, WASM) gets it for free with zero additional plumbing.

### 2.1 `physure.conf`

New key in the existing `[Settings]` section (`physure-core/src/units/physure.conf`), alongside
`propagation_mode`:

```ini
[Settings]
...
propagation_mode = correlated
parallel_threshold = 10000
```

### 2.2 Parsing (`physure-core/src/units/conf.rs`)

`parse_physure_conf`'s `"Settings"` match arm gains one more key, following the exact pattern
already used for `propagation_mode`:

```rust
"Settings" => {
    if let Some((key, raw)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
        match key {
            "propagation_mode" => { /* existing */ }
            "parallel_threshold" => match raw.parse::<usize>() {
                Ok(n) => { crate::settings::set_parallel_threshold(n); }
                Err(_) => eprintln!("physure.conf: invalid parallel_threshold '{raw}'; keeping default"),
            },
            _ => {}
        }
    }
}
```

### 2.3 New `physure-core/src/settings.rs`

Mirrors `uncertainty::mode`'s `AtomicU8`/`CONFIGURED` pattern (process-wide, no thread-local
override needed here — there's no per-thread reason to vary this the way propagation mode does):

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

const DEFAULT_PARALLEL_THRESHOLD: usize = 10_000;
static PARALLEL_THRESHOLD: AtomicUsize = AtomicUsize::new(DEFAULT_PARALLEL_THRESHOLD);

/// Minimum element count at which a `for`-expression switches from sequential
/// evaluation to `rayon`-parallel evaluation. Set from `physure.conf`'s
/// `[Settings] parallel_threshold`; `parallel_map` ignores this and is always parallel.
pub fn parallel_threshold() -> usize {
    PARALLEL_THRESHOLD.load(Ordering::Relaxed)
}

/// Sets the process-wide threshold, returning the one it replaced. This is what reading
/// a `physure.conf` calls; tests use it directly to force sequential (`usize::MAX`) or
/// parallel (`0`) execution deterministically.
pub fn set_parallel_threshold(n: usize) -> usize {
    PARALLEL_THRESHOLD.swap(n, Ordering::Relaxed)
}
```

Registered as `pub mod settings;` in `physure-core/src/lib.rs`. `physure-script` reads it via
`physure_core::settings::parallel_threshold()`; it never re-parses `physure.conf` itself —
`physure-core` stays the single source of truth for every key in `[Settings]`.

---

## 3. Dependency

`rayon` added to `physure-script/Cargo.toml` only. Not `physure-core` (no parallel code lives
there — just the threshold setting) and not `tokio` (that's the LSP/web server's async I/O,
unrelated to compute parallelism, per the roadmap).

---

## 4. Transparent `for`-expression parallelism (`interpreter.rs`)

Current sequential path (`Expr::ForExpr` arm) mutates one shared `local_env: HashMap` in place
per iteration. That can't be shared mutably across threads, so the parallel path clones `env`
once per item instead — strictly more allocation than the sequential path, which is precisely
why this only pays off above a size threshold:

```rust
Expr::ForExpr { var, iterable, body } => {
    let items: Vec<PhsValue> = /* ...unchanged resolution of iterable... */;

    if items.len() >= physure_core::settings::parallel_threshold() {
        use rayon::prelude::*;
        let results: Vec<PhsValue> = items
            .into_par_iter()
            .map(|item| {
                let mut local_env = env.clone();
                local_env.insert(var.clone(), item);
                self.eval_expr(body, &local_env)
            })
            .collect::<PhysureResult<Vec<PhsValue>>>()?;
        Ok(PhsValue::Vector(results))
    } else {
        /* existing sequential loop, unchanged */
    }
}
```

- `PhsInterpreter`'s mutable state (`plugin_state`, `unlocked_builtins`, `dynamic_externals`) is
  already `Arc<Mutex<_>>`, and `env`/`resolver`/`externals` are read-only during evaluation, so
  `&PhsInterpreter` is safely shared across `rayon` worker threads without further changes.
- Order is preserved automatically — `collect()` on an indexed parallel iterator reassembles
  results in original order — so there's no floating-point reduction-order nondeterminism to
  worry about; each element is computed independently, not folded together.
- A panic/error from any element propagates out of `collect()` as the first `Err` `rayon`
  encounters; this is "which one" not necessarily "the very first index chronologically" since
  work is chunked across threads, but it is always *an* error from the batch, never silently
  dropped.

---

## 5. `parallel_map(fn, vector)` builtin (`builtins.rs`)

Lives in `eval_core_builtin` — always available, like `sin`/`linspace`, not domain-gated behind
`use ... from array`. Always parallel; no threshold (explicit user opt-in already signals "this
is worth parallelizing").

### 5.1 Plumbing change

`eval_core_builtin` currently doesn't receive `env`. It needs it to call
`interpreter.call_function_node(&func, vec![item], env)` for each element, so its signature
gains an `env` parameter, and its one call site (`interpreter.rs:635`) passes the `env` already
in scope there — mirroring what `eval_domain_builtin_with_kwargs` already does one line below.

### 5.2 Implementation

```rust
"parallel_map" => {
    let (func, vec) = match (args.first(), args.get(1)) {
        (Some(PhsValue::Function(f)), Some(PhsValue::Vector(v))) => (f, v.clone()),
        _ => return Err(PhysureError::Generic("parallel_map expects (fn, vector)".into())),
    };
    use rayon::prelude::*;
    let results: Vec<PhsValue> = vec
        .into_par_iter()
        .enumerate()
        .map(|(i, item)| {
            interpreter
                .call_function_node(func, vec![item], env)
                .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
        })
        .collect::<PhysureResult<Vec<PhsValue>>>()?;
    Ok(Some(PhsValue::Vector(results)))
}
```

No new `PhysureError` variant — the roadmap's "surfaces which index failed and why" is met by
formatting the index into the existing `Generic` variant, consistent with how the rest of
`builtins.rs` already reports errors.

**Fail-fast caveat (documented, not silently under-delivered):** `rayon`'s `collect::<Result<Vec<_>, _>>()`
is a best-effort short-circuit — it stops *scheduling new* work once an error surfaces, but
work already dispatched to a thread when the error occurs may still run to completion. This
matches what the underlying library actually provides; it is not perfect mid-flight
cancellation, and the design does not claim to be.

---

## 6. Testing & Verification Strategy

1. **`physure-core` unit test**: `parallel_threshold = 500` in a conf string, parsed via
   `parse_physure_conf`, is readable back via `settings::parallel_threshold()`.
2. **Determinism test** (`physure-script`): the same `for`-expression script evaluated twice —
   once with `set_parallel_threshold(usize::MAX)` (forces sequential) and once with
   `set_parallel_threshold(0)` (forces parallel) — asserted to produce identical `Vector`
   output. This validates the parallel path directly against the sequential path without a
   second independently-written implementation to compare against.
3. **`parallel_map` happy-path test**: a simple PHS function mapped over a vector, asserted
   equal to the sequential/interpreted equivalent.
4. **`parallel_map` mid-batch-failure test**: one element's function call raises (e.g. a
   `@requires` contract violation or unit mismatch), asserted that the error surfaces and names
   that element's index.
