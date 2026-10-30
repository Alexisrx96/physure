# N5a: the sandboxed interpreter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `PhsInterpreter::sandboxed(Limits)` runs untrusted PHS text without reading files, loading plugins, writing files, overflowing the native stack, running past a deadline or drawing unseeded random numbers.

**Architecture:** a `Limits` value carried by the interpreter; every user-function call goes through `call_function_node_at`, which counts depth in a thread-local and checks the deadline; statements and loop items check the deadline too; the mesh-export builtin and the `for`/`parallel_map` rayon paths read the flags. `physure-core` gets a scoped seed for its two `thread_rng()` sites.

**Tech Stack:** Rust (`physure-script`, `physure-core`), `rand` 0.8 (`StdRng`, `SeedableRng`).

Design: `docs/superpowers/specs/2026-10-30-agent-validation-surface-design.md` (section "N5a"). N5b (JSON, CLI) and N5c (MCP) get their own plans when they start.

Run tests per task with `cargo test -p physure-script <filter>`; the whole crate once at the end (`cargo test -p physure -p physure-script`).

---

### Task 1: `Limits` and the interpreter field

**Files:**
- Create: `physure-script/src/limits.rs`
- Modify: `physure-script/src/lib.rs` (module and re-export), `physure-script/src/interpreter/mod.rs` (field, constructors)
- Test: `physure-script/tests/sandbox.rs` (new)

- [ ] **Step 1: Write the failing test** in `physure-script/tests/sandbox.rs`:

```rust
use physure_script::{parse_phs, Limits, PhsInterpreter, PhsValue};
use std::time::Duration;

/// Runs `source` statement by statement in `interp`, returning the last non-empty value.
fn run(interp: &PhsInterpreter, source: &str) -> Result<Vec<PhsValue>, String> {
    let program = parse_phs(source).map_err(|e| e.to_string())?;
    let mut env = std::collections::HashMap::new();
    let mut out = Vec::new();
    for stmt in &program.statements {
        let value = interp.eval_statement_with_env(stmt, &mut env).map_err(|e| e.to_string())?;
        if value != PhsValue::None {
            out.push(value);
        }
    }
    Ok(out)
}

#[test]
fn the_default_interpreter_is_unrestricted_and_the_sandbox_is_not() {
    let open = Limits::default();
    assert!(open.allow_files && !open.sequential && open.deadline.is_none() && open.max_output_bytes.is_none());
    let closed = Limits::sandboxed(Duration::from_secs(5));
    assert!(!closed.allow_files && closed.sequential && closed.deadline.is_some() && closed.max_output_bytes.is_some());
    assert!(closed.max_call_depth <= open.max_call_depth);
    let interp = PhsInterpreter::sandboxed(closed);
    assert_eq!(run(&interp, "1 m + 2 m").unwrap().len(), 1);
}
```

- [ ] **Step 2: Run it:** `cargo test -p physure-script --test sandbox` — fails to compile (`Limits` missing).

- [ ] **Step 3: Implement.** `limits.rs`:

```rust
//! What an interpreter is allowed to do. The default is unrestricted (apart from a call-depth ceiling that
//! turns a native stack overflow into an error); [`Limits::sandboxed`] is for text from outside.

use std::time::{Duration, Instant};

/// Nested user-function calls any interpreter allows. See the `call_depth` tests for how it was chosen.
pub const DEFAULT_MAX_CALL_DEPTH: usize = 1_000;
/// Nested calls a sandboxed interpreter allows: it runs on a thread with a stack sized for this.
pub const SANDBOX_MAX_CALL_DEPTH: usize = 200;
/// Bytes of rendered output a sandboxed run may produce.
pub const SANDBOX_MAX_OUTPUT_BYTES: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct Limits {
    /// Deepest nesting of user-function calls; one more is an error with the prefix `call depth limit`.
    pub max_call_depth: usize,
    /// Wall-clock point after which evaluation stops with an error prefixed `time limit`.
    pub deadline: Option<Instant>,
    /// Ceiling for rendered output; applied by whoever renders results (the report), carried here.
    pub max_output_bytes: Option<usize>,
    /// Whether a builtin may write a file (the mesh export).
    pub allow_files: bool,
    /// Whether `for` expressions and `parallel_map` must stay on one thread (reproducible random draws).
    pub sequential: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_call_depth: DEFAULT_MAX_CALL_DEPTH, deadline: None, max_output_bytes: None, allow_files: true, sequential: false }
    }
}

impl Limits {
    pub fn sandboxed(timeout: Duration) -> Self {
        Self {
            max_call_depth: SANDBOX_MAX_CALL_DEPTH,
            deadline: Some(Instant::now() + timeout),
            max_output_bytes: Some(SANDBOX_MAX_OUTPUT_BYTES),
            allow_files: false,
            sequential: true,
        }
    }
}
```

`lib.rs`: `pub mod limits;` and `pub use limits::Limits;`. `interpreter/mod.rs`: add `pub limits: Limits` (after `externals`), `limits: Limits::default()` in `new`, and

```rust
    /// An interpreter for text from outside: an empty module resolver (no file is read), no base dir (no native
    /// plugin is ever loaded), and the given limits. Nothing here can be re-enabled by the program it runs.
    pub fn sandboxed(limits: Limits) -> Self {
        let mut interp = Self::new(Arc::new(crate::resolver::MemoryModuleResolver::new()));
        interp.limits = limits;
        interp
    }
```

- [ ] **Step 4: Run:** `cargo test -p physure-script --test sandbox` — passes.
- [ ] **Step 5: Commit** `feat(script): Limits and PhsInterpreter::sandboxed`.

### Task 2: the call-depth limit

**Files:** Modify `physure-script/src/interpreter/expressions.rs` (`call_function_node_at`), `physure-script/src/limits.rs` (depth guard). Test: `physure-script/tests/sandbox.rs`.

- [ ] **Step 1: Tests:**

```rust
const RUNAWAY: &str = "fn f(x) = f(x)\nf(1)";
const DEEP: &str = "fn down(n) =\n  if n <= 0 then return 0\n  return down(n - 1)\ndown(DEPTH)";

#[test]
fn runaway_recursion_is_an_error_not_a_crash_in_the_sandbox() {
    let interp = PhsInterpreter::sandboxed(Limits::sandboxed(Duration::from_secs(30)));
    let error = on_a_big_stack(move || run(&interp, RUNAWAY)).unwrap_err();
    assert!(error.contains("call depth limit"), "{error}");
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash_in_the_default_interpreter() {
    let interp = PhsInterpreter::default();
    let error = on_a_big_stack(move || run(&interp, RUNAWAY)).unwrap_err();
    assert!(error.contains("call depth limit"), "{error}");
}

#[test]
fn recursion_within_the_limit_still_works() {
    let interp = PhsInterpreter::sandboxed(Limits::sandboxed(Duration::from_secs(30)));
    let source = DEEP.replace("DEPTH", "150");
    assert_eq!(on_a_big_stack(move || run(&interp, &source)).unwrap().len(), 1);
}

#[test]
fn the_depth_counter_is_released_after_an_error() {
    let interp = PhsInterpreter::sandboxed(Limits::sandboxed(Duration::from_secs(30)));
    let source = DEEP.replace("DEPTH", "150");
    on_a_big_stack(move || {
        assert!(run(&interp, RUNAWAY).is_err());
        assert!(run(&interp, &source).is_ok(), "depth leaked from the failed run");
    });
}
```

`on_a_big_stack<R>(f) -> R` spawns a thread with `stack_size(SANDBOX_STACK_BYTES)` (a `pub const` in `limits.rs`, calibrated in Step 3) and joins it.

- [ ] **Step 2: Run** — the runaway tests abort the test binary (stack overflow) or fail to compile; that is the failing state.
- [ ] **Step 3: Implement** in `limits.rs`:

```rust
use std::cell::Cell;
thread_local! { static CALL_DEPTH: Cell<usize> = const { Cell::new(0) }; }

/// Held for the duration of one user-function call.
pub(crate) struct CallDepth;

impl CallDepth {
    /// Enters one more call, or says the limit is reached.
    pub(crate) fn enter(max: usize) -> Result<Self, String> {
        CALL_DEPTH.with(|depth| {
            if depth.get() >= max {
                return Err(format!("call depth limit of {max} reached (a function that calls itself without end?)"));
            }
            depth.set(depth.get() + 1);
            Ok(CallDepth)
        })
    }
}
impl Drop for CallDepth {
    fn drop(&mut self) { CALL_DEPTH.with(|depth| depth.set(depth.get() - 1)); }
}
```

In `call_function_node_at`, first line after the arity check: `let _depth = crate::limits::CallDepth::enter(self.limits.max_call_depth).map_err(PhysureError::Generic)?;`.

Calibration (do before fixing the constants): write a throwaway test that runs `DEEP` with growing `DEPTH` on a thread of 1 MiB, in debug and in `--release`, with `max_call_depth` set to `usize::MAX`, and record the last depth that survives. Set `DEFAULT_MAX_CALL_DEPTH` to at most half the release figure on 1 MiB (Windows main thread), `SANDBOX_STACK_BYTES` so that `SANDBOX_MAX_CALL_DEPTH` is at most half the **debug** figure for that size. Write the figures into the doc comments.

- [ ] **Step 4: Run** `cargo test -p physure-script --test sandbox` in debug — passes.
- [ ] **Step 5: Commit** `feat(script): a call-depth limit turns runaway recursion into an error`.

### Task 3: the deadline

**Files:** Modify `physure-script/src/interpreter/mod.rs` (`check_budget`), `statements.rs` (`eval_statement_with_env_at`, `While`), `expressions.rs` (call, `for` items). Test: `sandbox.rs`.

- [ ] **Step 1: Tests:** a `while` that never ends (`x = 0\nwhile x >= 0 do ...`, exact syntax taken from `interpreter/tests.rs`) under `Limits::sandboxed(Duration::from_millis(50))` returns an error containing `time limit` within one second; a program under a one-hour deadline is unaffected; an interpreter whose deadline is already past refuses its first statement.
- [ ] **Step 2: Run** — fails (no check yet).
- [ ] **Step 3: Implement** `pub(crate) fn check_budget(&self) -> PhysureResult<()>`: when `self.limits.deadline` is `Some(d)` and `Instant::now() >= d`, `Err(PhysureError::Generic("time limit reached".into()))`. Call it at the top of `eval_statement_with_env_at`, in `call_function_node_at` (before the depth guard), once per `While` iteration and once per sequential `for` item.
- [ ] **Step 4: Run** — passes.
- [ ] **Step 5: Commit** `feat(script): a deadline for sandboxed evaluation`.

### Task 4: files, plugins, modules and threads

**Files:** Modify `physure-script/src/builtins/plot.rs` (the `std::fs::write`), `interpreter/expressions.rs` (`for` branch), `builtins/core.rs` (`parallel_map` branch). Test: `sandbox.rs`.

- [ ] **Step 1: Tests:** (a) `export3d` with a temp path in a sandboxed interpreter returns an error containing `forbidden` and the file does not exist; in a default interpreter the file is written (existing test keeps covering it). (b) A temp directory holding `helper.phs` with `x = 1 m`: `import helper` fails in the sandbox and works through `PhsInterpreter::with_base_dir(dir)`. (c) `use f from nothing_here` fails in the sandbox with no file access (it has no base dir: nothing to load). (d) With `sequential`, a `for` over more elements than `parallel_threshold` runs and returns the same vector as the default interpreter.
- [ ] **Step 2: Run** — (a) fails (file is written); (b) may already hold, which is fine and is kept as the regression guard.
- [ ] **Step 3: Implement.** Before the `std::fs::write` in `plot.rs`: `if !interpreter.limits.allow_files { return Err(PhysureError::Generic(format!("forbidden: writing '{filename}' is not allowed here"))); }`. In the `ForExpr` arm and the `parallel_map` arm, add `&& !self.limits.sequential` (resp. `&& !interpreter.limits.sequential`) to the condition that picks rayon.
- [ ] **Step 4: Run** — passes.
- [ ] **Step 5: Commit** `feat(script): the sandbox refuses file writes and stays on one thread`.

### Task 5: seeded Monte Carlo (`physure-core`)

**Files:** Modify `physure-core/src/uncertainty/monte_carlo.rs`, `physure-core/src/uncertainty/mod.rs`, `physure-core/src/lib.rs`. Test: in `monte_carlo.rs` (`#[cfg(test)]`).

- [ ] **Step 1: Tests:**

```rust
#[test]
fn the_same_seed_gives_the_same_samples_and_another_seed_differs() {
    let draw = |seed| with_seed(seed, || MonteCarloBackend::from_stats(10.0, 1.0, 64).samples.to_vec());
    assert_eq!(draw(7), draw(7));
    assert_ne!(draw(7), draw(8));
}

#[test]
fn successive_draws_in_one_scope_differ_but_replay_exactly() {
    let two = |seed| with_seed(seed, || {
        (MonteCarloBackend::from_stats(0.0, 1.0, 8).samples.to_vec(), MonteCarloBackend::from_stats(0.0, 1.0, 8).samples.to_vec())
    });
    let (a, b) = two(3);
    assert_ne!(a, b);
    assert_eq!(two(3), (a, b));
}

#[test]
fn outside_a_scope_nothing_is_seeded() {
    assert_ne!(MonteCarloBackend::from_stats(0.0, 1.0, 64).samples.to_vec(), MonteCarloBackend::from_stats(0.0, 1.0, 64).samples.to_vec());
}
```

- [ ] **Step 2: Run** `cargo test -p physure monte_carlo` — fails (`with_seed` missing).
- [ ] **Step 3: Implement** in `monte_carlo.rs`:

```rust
use std::cell::Cell;

thread_local! {
    /// `Some((seed, draws so far))` while a [`with_seed`] scope is open on this thread.
    static SEEDED: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

/// Runs `f` with every Monte Carlo draw on this thread taken from a generator derived from `seed`, so the same
/// sequence of operations gives the same samples. Reproducible only for single-threaded evaluation.
pub fn with_seed<R>(seed: u64, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<(u64, u64)>);
    impl Drop for Restore { fn drop(&mut self) { SEEDED.with(|s| s.set(self.0)); } }
    let _restore = Restore(SEEDED.with(|s| s.replace(Some((seed, 0)))));
    f()
}

/// The generator for the next draw: seeded inside a [`with_seed`] scope, the thread's own outside.
fn next_rng() -> StdRng {
    SEEDED.with(|s| match s.get() {
        Some((seed, n)) => {
            s.set(Some((seed, n + 1)));
            StdRng::seed_from_u64(seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        }
        None => StdRng::from_rng(thread_rng()).expect("thread_rng does not fail"),
    })
}
```

and replace both `let mut rng = thread_rng();` with `let mut rng = next_rng();`. Export `with_seed` from `uncertainty/mod.rs` and `physure-core/src/lib.rs`.
- [ ] **Step 4: Run** — passes.
- [ ] **Step 5: Commit** `feat(core): with_seed makes Monte Carlo draws reproducible`.

### Task 6: the end-to-end determinism test and the docs

- [ ] **Step 1:** in `sandbox.rs`, a test that sets the Monte Carlo propagation mode through PHS text (the form used by existing Monte Carlo tests in the crate) and checks two `with_seed` runs print the same value and a different seed prints another.
- [ ] **Step 2:** `CHANGELOG.md` under Unreleased: Added (`Limits`, `PhsInterpreter::sandboxed`, `physure_core::uncertainty::with_seed`), Fixed (runaway recursion no longer aborts the process: it stops with a `call depth limit` error).
- [ ] **Step 3:** spec status line: N5a done. Run `cargo test -p physure -p physure-script` once; commit `docs: N5a in the changelog`.
