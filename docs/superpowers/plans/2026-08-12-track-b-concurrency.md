# Track B — Concurrency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add transparent `rayon`-based parallelism to `Expr::ForExpr` above a configurable
element-count threshold, plus an explicit `parallel_map(fn, vector)` builtin, per
[docs/superpowers/specs/2026-08-12-track-b-concurrency-design.md](../specs/2026-08-12-track-b-concurrency-design.md).

**Architecture:** `physure-core` gains a `settings` module holding a process-wide
`parallel_threshold` atomic, set from `physure.conf`'s `[Settings]` section (same pattern as
the existing `propagation_mode`). `physure-script` adds `rayon` as a dependency and reads that
threshold in `interpreter.rs`'s `Expr::ForExpr` arm to pick sequential vs. parallel evaluation.
A new `parallel_map` case in `builtins.rs`'s `eval_core_builtin` always runs on the `rayon`
thread pool, fail-fast, and needs `eval_core_builtin` to gain an `env` parameter so it can call
`interpreter.call_function_node`. `gradient`/`trapz`/`while` are explicitly out of scope (see
spec §1).

**Tech Stack:** Rust, `rayon` (new dependency, `physure-script` only).

---

### Task 1: `physure-core` settings module (`parallel_threshold` getter/setter)

**Files:**
- Create: `physure-core/src/settings.rs`
- Modify: `physure-core/src/lib.rs`

- [ ] **Step 1: Write failing test**

In `physure-core/src/settings.rs` (new file, test module at the bottom):
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parallel_threshold_returns_previous_and_updates() {
        let original = set_parallel_threshold(42);
        assert_eq!(parallel_threshold(), 42);
        let previous = set_parallel_threshold(original);
        assert_eq!(previous, 42);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-core --lib settings::`
Expected: "running 0 tests" — `physure-core/src/settings.rs` exists on disk but isn't declared
as a module in `lib.rs` yet, so `cargo` doesn't see it as part of the crate at all.

- [ ] **Step 3: Wire the module into the crate**

In `physure-core/src/lib.rs`, add `pub mod settings;` next to the other `pub mod` lines and
re-export the two functions at crate root, matching how `uncertainty::{propagation_mode,
set_propagation_mode}` are already re-exported:
```rust
pub mod settings;
```
and in the existing `pub use uncertainty::{...}` block area, add:
```rust
pub use settings::{parallel_threshold, set_parallel_threshold};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p physure-core --lib settings::`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/settings.rs physure-core/src/lib.rs
git commit -m "feat(core): add process-wide parallel_threshold setting"
```

---

### Task 2: Parse `parallel_threshold` from `physure.conf`

**Files:**
- Modify: `physure-core/src/units/conf.rs`
- Modify: `physure-core/src/units/physure.conf`

- [ ] **Step 1: Write failing test**

Append to `physure-core/src/units/conf.rs` (new `#[cfg(test)] mod tests` block at the end of the
file — none exists there yet):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::registry::UnitRegistry;
    use std::collections::HashMap;

    #[test]
    fn parses_parallel_threshold_from_settings_section() {
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf(
            "[Settings]\nparallel_threshold = 500\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::parallel_threshold(), 500);
        // Restore the default so other tests in this binary aren't affected.
        crate::settings::set_parallel_threshold(10_000);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-core --lib units::conf::tests::parses_parallel_threshold_from_settings_section`
Expected: FAIL — `parallel_threshold()` still reports the default (`10_000`), not `500`,
because `parse_physure_conf` doesn't recognize the key yet.

- [ ] **Step 3: Add the key to `parse_physure_conf`'s `"Settings"` arm**

In `physure-core/src/units/conf.rs`, the `"Settings"` match arm currently only checks for
`"propagation_mode"`. Change it to a `match key` so a second key can be added cleanly:
```rust
"Settings" => {
    if let Some((key, raw)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
        match key {
            "propagation_mode" => match raw.parse::<PropagationMode>() {
                Ok(mode) => {
                    set_propagation_mode(mode);
                }
                // A conf is read before there is anywhere to report to, and the mode
                // it names is not worth refusing to start over. Saying so on stderr
                // beats falling back to correlated with nothing on screen: that is
                // the wrong answer that looks right.
                Err(why) => eprintln!("physure.conf: {why}; keeping correlated"),
            },
            "parallel_threshold" => match raw.parse::<usize>() {
                Ok(n) => {
                    crate::settings::set_parallel_threshold(n);
                }
                Err(_) => eprintln!(
                    "physure.conf: invalid parallel_threshold '{raw}'; keeping default"
                ),
            },
            _ => {}
        }
    }
}
```
This replaces the existing `if let Some(("propagation_mode", raw)) = ...` block — same
behavior for that key, just nested one level deeper under a `match key` so a second key fits.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p physure-core --lib units::conf::tests::parses_parallel_threshold_from_settings_section`
Expected: PASS.

- [ ] **Step 5: Document the key in `physure.conf` itself**

In `physure-core/src/units/physure.conf`, add the new key to `[Settings]` right after
`propagation_mode`:
```ini
propagation_mode = correlated
# Minimum element count at which a PHS `for`-expression switches from sequential to
# rayon-parallel evaluation. A `physure.conf` in the working directory overrides this one.
parallel_threshold = 10000
mkml_recursion_limit = 100
```

- [ ] **Step 6: Run the full `physure-core` test suite to check for regressions**

Run: `cargo test -p physure-core`
Expected: PASS (all tests, including the new one).

- [ ] **Step 7: Commit**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/physure.conf
git commit -m "feat(core): read parallel_threshold from physure.conf [Settings]"
```

---

### Task 3: `rayon` dependency + transparent `for`-expression parallelism

**Files:**
- Modify: `physure-script/Cargo.toml`
- Modify: `physure-script/src/interpreter.rs:665-715` (the `Expr::ForExpr` arm)

- [ ] **Step 1: Add the dependency**

In `physure-script/Cargo.toml`, under `[dependencies]` (alongside `pest`/`pest_derive`, which
are also direct, non-workspace deps in this file):
```toml
rayon = "1"
```

- [ ] **Step 2: Write the equivalence test**

`PhsValue` already derives `PartialEq` ([value.rs:5](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/value.rs#L5)), so the two
paths' output can be compared directly. In `physure-script/src/interpreter.rs`'s existing
`mod tests` block (near `test_interpreter_for_expr_large_scale`):
```rust
#[test]
fn for_expr_parallel_and_sequential_paths_agree() {
    let script = "res = for i in 1..20000 { i * 3 + 1 }";

    let original = physure_core::settings::set_parallel_threshold(usize::MAX);
    let mut interp_seq = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs(script).unwrap();
    interp_seq.run_statements(&stmts).unwrap();
    let seq_val = interp_seq.get_var("res").unwrap().clone();

    physure_core::settings::set_parallel_threshold(0);
    let mut interp_par = PhsInterpreter::default();
    interp_par.run_statements(&stmts).unwrap();
    let par_val = interp_par.get_var("res").unwrap().clone();

    physure_core::settings::set_parallel_threshold(original);

    assert_eq!(seq_val, par_val);
}
```
Note this is an equivalence/regression test, not a strict TDD red/green test: before Step 4,
only the sequential path exists, so `set_parallel_threshold` has no effect yet and the test
passes trivially (both runs execute identical code). It becomes a meaningful check only once
Step 4 introduces a second, genuinely different code path that must agree with the first.

- [ ] **Step 3: Run test to confirm it compiles and passes (trivially, pre-change)**

Run: `cargo test -p physure-script --lib interpreter::tests::for_expr_parallel_and_sequential_paths_agree`
Expected: PASS (only the sequential path exists at this point, so both branches run identical
code — this step just confirms the test itself is wired up correctly before Step 4 gives it a
second path to actually verify).

- [ ] **Step 4: Add the parallel branch to `Expr::ForExpr`**

In `physure-script/src/interpreter.rs`, the `Expr::ForExpr` arm currently ends with:
```rust
                let mut local_env = env.clone();
                let old_val = local_env.get(var).cloned();
                let mut results = Vec::with_capacity(items.len());
                for item in items {
                    local_env.insert(var.clone(), item);
                    results.push(self.eval_expr(body, &local_env)?);
                }
                if let Some(old) = old_val {
                    local_env.insert(var.clone(), old);
                } else {
                    local_env.remove(var);
                }
                Ok(PhsValue::Vector(results))
```
Replace it with a threshold branch — parallel path clones `env` per item (can't share one
mutable `HashMap` across threads), sequential path keeps the existing single-clone-and-mutate
loop unchanged:
```rust
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
                    let mut local_env = env.clone();
                    let old_val = local_env.get(var).cloned();
                    let mut results = Vec::with_capacity(items.len());
                    for item in items {
                        local_env.insert(var.clone(), item);
                        results.push(self.eval_expr(body, &local_env)?);
                    }
                    if let Some(old) = old_val {
                        local_env.insert(var.clone(), old);
                    } else {
                        local_env.remove(var);
                    }
                    Ok(PhsValue::Vector(results))
                }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p physure-script --lib interpreter::tests::for_expr_parallel_and_sequential_paths_agree`
Expected: PASS. Also re-run the pre-existing large-scale test, since it now exercises the
parallel path at 99,999 elements (default threshold is 10,000):
Run: `cargo test -p physure-script --lib interpreter::tests::test_interpreter_for_expr_large_scale`
Expected: PASS.

- [ ] **Step 6: Run the full `physure-script` test suite to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add physure-script/Cargo.toml physure-script/src/interpreter.rs
git commit -m "feat(phs): parallelize for-expression above parallel_threshold via rayon"
```

---

### Task 4: `parallel_map(fn, vector)` builtin

**Files:**
- Modify: `physure-script/src/builtins.rs`
- Modify: `physure-script/src/interpreter.rs:635` (the `eval_core_builtin` call site)

- [ ] **Step 1: Write failing tests**

In `physure-script/src/interpreter.rs`'s `mod tests` block:
```rust
#[test]
fn parallel_map_applies_function_to_every_element() {
    let mut interp = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs(
        "fn double(x) = x * 2.0\nres = parallel_map(double, vector(1.0, 2.0, 3.0))",
    )
    .unwrap();
    interp.run_statements(&stmts).unwrap();
    let val = interp.get_var("res").unwrap();
    let PhsValue::Vector(v) = val else { panic!("expected vector, got {val:?}") };
    let means: Vec<f64> = v
        .iter()
        .map(|x| match x {
            PhsValue::Number(n) => *n,
            PhsValue::Quantity(q) => q.value.mean(),
            other => panic!("expected numeric element, got {other:?}"),
        })
        .collect();
    assert_eq!(means, vec![2.0, 4.0, 6.0]);
}

#[test]
fn parallel_map_reports_failing_index() {
    let mut interp = PhsInterpreter::default();
    let stmts = crate::parser::parse_phs(
        "@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x * 2.0\n\
         parallel_map(f, vector(1.0, 2.0, -1.0, 4.0))",
    )
    .unwrap();
    let err = interp.run_statements(&stmts).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("index 2"), "expected the failing index in the error, got: {msg}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p physure-script --lib interpreter::tests::parallel_map`
Expected: FAIL with `Undefined function 'parallel_map'`.

- [ ] **Step 3: Give `eval_core_builtin` access to `env`**

In `physure-script/src/builtins.rs`, change the signature:
```rust
pub fn eval_core_builtin(
    name: &str,
    args: &[PhsValue],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
```
(the leading `_interpreter` becomes `interpreter` too, since `parallel_map` uses it — check
whether any other match arm already relies on it being unused before renaming; if none do,
renaming is safe and required either way since it's now used).

In `physure-script/src/interpreter.rs:635`, update the call site to pass `env` (which is
already in scope one line below, at the `eval_domain_builtin_with_kwargs` call):
```rust
                if let Some(val) = crate::builtins::eval_core_builtin(name, &arg_vals, self, env)? {
```

- [ ] **Step 4: Add the `parallel_map` case**

In `physure-script/src/builtins.rs`, add a new match arm inside `eval_core_builtin` (near
`"vector"`):
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

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p physure-script --lib interpreter::tests::parallel_map`
Expected: PASS (both tests).

- [ ] **Step 6: Run the full `physure-script` test suite, and the rest of the Rust workspace, to check for regressions**

Run: `cargo test -p physure-script`
Expected: PASS.

Run: `cargo build --workspace`
Expected: builds cleanly — `eval_core_builtin`'s new `env` parameter has exactly one call site
(`interpreter.rs:635`), but confirm no other crate (`physure-cli`, `physure-lsp`,
`physure-wasm`) calls it directly:
Run: `grep -rn "eval_core_builtin" --include=*.rs .`
Expected: only the definition in `builtins.rs` and the one call site in `interpreter.rs`.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/builtins.rs physure-script/src/interpreter.rs
git commit -m "feat(phs): add parallel_map(fn, vector) builtin"
```

---

### Task 5: Update the roadmap checklist

**Files:**
- Modify: `docs/language_readiness_roadmap.md`

- [ ] **Step 1: Check off Track B's milestone items**

In `docs/language_readiness_roadmap.md`'s §9 Milestone Checklists, change:
```markdown
- [ ] **Track B: Concurrency**
  - [ ] Add `rayon` dependency to `physure-script`.
  - [ ] Transparent `par_iter` parallelism for array builtins and `for`-expression above size threshold.
  - [ ] `parallel_map(fn, vector)` builtin with fail-fast error semantics.
  - [ ] Determinism test (parallel output == sequential output) and mid-batch-failure test.
```
to:
```markdown
- [x] **Track B: Concurrency** — see
      [`docs/superpowers/specs/2026-08-12-track-b-concurrency-design.md`](superpowers/specs/2026-08-12-track-b-concurrency-design.md)
      and [`docs/superpowers/plans/2026-08-12-track-b-concurrency.md`](superpowers/plans/2026-08-12-track-b-concurrency.md).
      One scope change from this section's original description, intentional and tracked
      below: `gradient`/`trapz` are **not** parallelized — `trapz` is a running accumulation
      that can't be parallelized without a parallel-reduce rewrite, and `gradient`'s
      per-element cost is too small for thread-dispatch to pay off. Threshold is configurable
      via `physure.conf`'s `[Settings] parallel_threshold` (default 10,000), not a bare
      constant or env var.
  - [x] Add `rayon` dependency to `physure-script`.
  - [x] Transparent `par_iter` parallelism for `for`-expression above `parallel_threshold`.
  - [x] `parallel_map(fn, vector)` builtin with fail-fast error semantics.
  - [x] Determinism test (parallel output == sequential output) and mid-batch-failure test.
```
Also update the doc's top status line (near the top of the file) and §1 Executive Summary bullet
for Track B to mark it `✅ *implemented*`, following the exact phrasing style already used for
Track A and Track F in those two spots.

- [ ] **Step 2: Commit**

```bash
git add docs/language_readiness_roadmap.md
git commit -m "docs: mark Track B (Concurrency) complete in language readiness roadmap"
```
