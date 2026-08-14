# Track C Follow-Up Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the gaps found by the final holistic code review of Track C (Breakpoints), run
after all five task groups were merged. The flagship gap: `step`/`next`/`finish` are no-ops —
the interpreter discards every `DebugAction` a hook returns, so once any breakpoint exists,
those commands behave exactly like `continue`. Alongside it: a stale doc comment, dormant
`Clone`-sharing risk, undocumented `call_site_line == 0`, and several debug-path-only
performance/reuse/cleanup items.

**Architecture:** Two task groups, dispatched in parallel — they touch disjoint files.

- **Group A** — `physure-script/src/interpreter.rs` only. The step/next/finish/pause
  bookkeeping (the real fix), plus every other fixable finding that lives in this same file
  (stale comment, `call_site_line` doc, `Clone`-sharing doc, breakpoint list `Arc`-swap).
- **Group B** — `physure-core/src/units/registry.rs`, `physure-cli/src/debug.rs`. The
  `split_prefix` double-lookup, the CLI's `locals`/`globals` reuse of `RichRenderer`, building
  the `UnitRegistry` once per debug session instead of per `inspect` command, removing the dead
  `BreakpointSpec::FunctionEntry` variant, and rejecting `--break 0` (which would otherwise
  collide with the "unknown line" sentinel synthesized functions get).

**Three findings from the review are deliberately NOT fixed here — verified during planning, not
skipped by oversight:**

1. **"`inspect.rs`'s dimension-flattening duplicates `RationalUnit::dimensions_map()`"** — checked
   `dimensions_map()`'s actual signature: it returns `HashMap<String, (i64, i64)>`, not the
   ordered `Vec` `inspect.rs` needs. `RationalUnit.dimensions` is a `SmallVec` "maintained sorted
   by unit name" (its own doc comment); routing through `dimensions_map()` would drop that
   ordering guarantee since `HashMap` iteration order is unspecified and randomized per-process
   in Rust — `Inspection.dimension`'s element order would become nondeterministic between runs,
   a real regression traded for removing a few duplicated lines. Not worth it; `inspect.rs`
   keeps its direct, order-preserving iteration over `q.unit.dimensions`.
2. **"`collect_declared` doesn't handle `where`-expression bindings"** — checked how `where` is
   actually evaluated: it desugars to `Expr::FunctionCall { name: "let", args: [var, value,
   body] }` (`parser.rs`'s `check_expr_shadowing`, confirmed against `phs.pest`'s `where_expr`
   rule), and `interpreter.rs:658-666` evaluates it by cloning `env`, inserting the binding into
   the *clone*, evaluating `body` against that clone, and returning — the binding never escapes
   back into the caller's persistent `env`. `debug_checkpoint` only fires between whole
   statements (via `eval_statement_with_env_at`), never mid-expression-evaluation, so there is no
   live pause point where a `where`-bound name is ever present in `ctx.env` for `locals` to show
   regardless of what `collect_declared` does. The original finding's premise doesn't hold up;
   `collect_declared` is already correct as shipped.
3. **"`StackFrame::declared` should be memoized per `FunctionDefNode` instead of recomputed every
   call"** — the obvious fix (cache by the `FunctionDefNode`'s address) turns out to be unsound
   given how this codebase actually represents function values: `PhsValue::Function` holds an
   owned `FunctionDefNode`, not an `Rc`/`Arc<FunctionDefNode>`, and `call_function_node_at`
   unconditionally does `let mut local_env = env.clone();` at the start of *every* call —
   `HashMap::clone()` deep-clones every stored `PhsValue`, so any `PhsValue::Function` reachable
   from `env` (including the very function being called, for a recursive call) gets a **fresh
   address** on every single call. For the exact "hot recursive function" scenario the finding
   cited (e.g. `fib(25)`), each recursion depth would get its own address, so a pointer-keyed
   cache would essentially never hit — and worse, being a `static`, globally-scoped, never-evicted
   cache, it would grow one entry per address per call forever, an unbounded memory leak traded
   for zero actual speedup. A correct fix would need `PhsValue::Function` to hold an
   `Rc<FunctionDefNode>` instead of an owned one, which ripples through every place `PhsValue`
   is constructed/matched across the whole crate — real, valuable work, but a much larger change
   than a "follow-up fixes" plan should take on. Left as a known, documented perf characteristic
   instead of a broken "fix."

**Tech Stack:** Rust only. No new dependencies.

---

## Task Group A — Step/next/finish bookkeeping + related interpreter.rs fixes

**Files:**
- Modify: `physure-script/src/interpreter.rs` (`PhsInterpreter`, `debug_checkpoint`,
  `call_function_node`, doc comments) — the only file this group touches.

### Task A.1: Step/next/finish/pause bookkeeping (the flagship fix)

- [ ] **Step 1: Write failing tests**

In `physure-script/src/interpreter.rs`'s `mod tests` block, add three tests:
```rust
#[test]
fn step_over_skips_statements_inside_a_deeper_nested_call() {
    use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct ScriptedHook {
        actions: Mutex<Vec<DebugAction>>,
        seen: Arc<Mutex<Vec<usize>>>,
    }
    impl DebugHook for ScriptedHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.seen.lock().unwrap().push(ctx.line);
            let mut actions = self.actions.lock().unwrap();
            if actions.is_empty() { DebugAction::Continue } else { actions.remove(0) }
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(ScriptedHook {
        actions: Mutex::new(vec![DebugAction::StepOver, DebugAction::Continue]),
        seen: seen.clone(),
    });
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    interp.add_breakpoint(Breakpoint::Line(4));
    // Lines: 1 "fn helper(x) =", 2 "  y = x * 2", 3 "  return y", 4 "z = helper(1)", 5 "w = 2".
    let program = crate::parser::parse_phs(
        "fn helper(x) =\n  y = x * 2\n  return y\nz = helper(1)\nw = 2\n",
    )
    .unwrap();
    interp.run_statements_with_lines(&program).unwrap();

    // Paused at line 4 (the breakpoint, depth 0). StepOver should skip both statements inside
    // helper's body (lines 2-3, depth 1 -- deeper than where StepOver was issued) and land on
    // line 5 (depth 0 again, back at or above the issuing depth) -- never on 2 or 3.
    assert_eq!(*seen.lock().unwrap(), vec![4, 5]);
}

#[test]
fn step_into_fires_on_the_very_next_checkpoint_regardless_of_depth() {
    use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct ScriptedHook {
        actions: Mutex<Vec<DebugAction>>,
        seen: Arc<Mutex<Vec<usize>>>,
    }
    impl DebugHook for ScriptedHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.seen.lock().unwrap().push(ctx.line);
            let mut actions = self.actions.lock().unwrap();
            if actions.is_empty() { DebugAction::Continue } else { actions.remove(0) }
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(ScriptedHook {
        actions: Mutex::new(vec![DebugAction::StepInto, DebugAction::Continue]),
        seen: seen.clone(),
    });
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    interp.add_breakpoint(Breakpoint::Line(4));
    let program = crate::parser::parse_phs(
        "fn helper(x) =\n  y = x * 2\n  return y\nz = helper(1)\nw = 2\n",
    )
    .unwrap();
    interp.run_statements_with_lines(&program).unwrap();

    // Unlike StepOver, StepInto must fire on the *very* next checkpoint even though it's
    // deeper (inside helper's body) -- line 2, not line 5.
    assert_eq!(*seen.lock().unwrap(), vec![4, 2]);
}

#[test]
fn continue_after_a_breakpoint_does_not_refire_until_the_next_breakpoint_match() {
    use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
    use std::sync::{Arc, Mutex};

    struct RecordingHook(Arc<Mutex<Vec<usize>>>);
    impl DebugHook for RecordingHook {
        fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
            self.0.lock().unwrap().push(ctx.line);
            DebugAction::Continue
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::new(RecordingHook(seen.clone()));
    let mut interp = PhsInterpreter::with_debug_hook(
        std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
        hook,
    );
    interp.add_breakpoint(Breakpoint::Line(2));
    let program = crate::parser::parse_phs("x = 1\ny = 2\nz = 3\n").unwrap();
    interp.run_statements_with_lines(&program).unwrap();

    // This is the regression case for the original bug: only line 2 (the breakpoint) should
    // ever have paused. Before the fix, the discarded DebugAction made no difference either
    // way here since nothing was implemented to *use* it -- this test's real job is to prove
    // the *new* step-bookkeeping doesn't accidentally make Continue behave like a step.
    assert_eq!(*seen.lock().unwrap(), vec![2]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p physure-script --lib interpreter::tests::step_over_skips`
Run: `cargo test -p physure-script --lib interpreter::tests::step_into_fires`
Expected: both FAIL. Today's unfixed `debug_checkpoint` only fires the hook when a checkpoint's
line literally matches a registered `Breakpoint` (`DebugAction` is discarded entirely, so
nothing beyond a direct match ever fires) — with only `Breakpoint::Line(4)` registered in both
tests, that means `seen` ends up as just `[4]` in each case today, regardless of which
`DebugAction` the scripted hook returns. So `step_over_skips` fails with actual `[4]` vs.
expected `[4, 5]`, and `step_into_fires` fails with actual `[4]` vs. expected `[4, 2]` — both
correctly red, for the same underlying reason (no step bookkeeping exists yet to make anything
beyond an exact breakpoint match ever fire).

- [ ] **Step 3: Add `StepMode` and the `step_mode` field**

`DebugAction` is currently only imported inside individual test functions in this file (`use
crate::debug::{DebugAction, DebugContext, DebugHook};`, repeated per-test) — the module-level
import at the top of the file (`interpreter.rs` line 13) is missing it:
```rust
use crate::debug::{DebugContext, DebugHook, StackFrame};
```
Since Step 4 below adds production code (inside `debug_checkpoint`, not a test) that matches on
`DebugAction` variants directly, add it to this top-level import first:
```rust
use crate::debug::{DebugAction, DebugContext, DebugHook, StackFrame};
```
(Leave every per-test `use crate::debug::{DebugAction, ...}` line exactly as it is — a local
`use` shadowing an already-in-scope module-level one is harmless, not a conflict, and removing
them isn't part of this task.)

In `physure-script/src/interpreter.rs`, add this type near `CallStackGuard` (after its `impl
Drop` block, before `strip_unit_comment`):
```rust
/// Tracks what a `Step*`/`Pause` `DebugAction` committed the interpreter to doing next, so a
/// later `debug_checkpoint` call can decide whether to fire the hook even when no `Breakpoint`
/// matches -- this is what actually makes `step`/`next`/`finish` do something once at least one
/// breakpoint is registered (previously the `DebugAction` a hook returned was thrown away
/// entirely, so those commands were indistinguishable from `Continue`). `None` means "no step
/// pending" -- either nothing has been returned yet, or the last action was `Continue`.
#[derive(Clone, Copy)]
enum StepMode {
    /// Fire on the very next checkpoint, whatever its call-stack depth.
    Into,
    /// Fire once `call_stack` depth is back down to (or shallower than) the depth recorded when
    /// this was issued -- skips over anything deeper (a nested call).
    Over(usize),
    /// Fire once `call_stack` depth is strictly shallower than the depth recorded when this was
    /// issued -- i.e. only after the current frame has actually returned.
    Out(usize),
}
```

Add the field to `PhsInterpreter` (right after `breakpoints`):
```rust
    breakpoints: Arc<Mutex<Vec<crate::debug::Breakpoint>>>,
    step_mode: Arc<Mutex<Option<StepMode>>>,
```

Initialize it in `PhsInterpreter::new` (right after `breakpoints: Arc::new(Mutex::new(Vec::new())),`):
```rust
            breakpoints: Arc::new(Mutex::new(Vec::new())),
            step_mode: Arc::new(Mutex::new(None)),
```

- [ ] **Step 4: Rewrite `debug_checkpoint` to consult and update `step_mode`**

The current `debug_checkpoint` (`interpreter.rs`, right after `add_breakpoint`) reads:
```rust
    fn debug_checkpoint(&self, line: usize, env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        let Some(hook) = &self.debug_hook else { return Ok(()) };

        // Snapshot the breakpoint list and the innermost frame's name, then drop both locks
        // *before* evaluating any `Conditional` breakpoint's condition below: that condition
        // may call a PHS-defined function, which re-enters `debug_checkpoint` on this same
        // thread via `eval_expr` -> `call_function_node` -> `call_function_node_at` ->
        // `eval_statement_with_env_at`. `std::sync::Mutex` is not reentrant, so holding
        // `call_stack`/`breakpoints` locked (as `MutexGuard`s) across that call would
        // self-deadlock the thread forever -- NLL only relaxes borrow-checking, it doesn't
        // change when a `MutexGuard`'s `Drop` actually runs, so the naive "just lock at the
        // top of the function" version hangs the instant a condition calls back in.
        // `Breakpoint` and `StackFrame` are both `Clone`, so cloning out of the lock is cheap
        // and correct.
        let breakpoints = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let innermost_fn_name = self
            .call_stack
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .map(|f| f.fn_name.clone());

        let mut hits = false;
        for bp in &breakpoints {
            hits = match bp {
                crate::debug::Breakpoint::Line(l) => *l == line,
                crate::debug::Breakpoint::Conditional(l, cond) => {
                    // Let a condition-eval error (typo'd variable, type error, ...) propagate
                    // as a real error instead of silently treating it as "didn't match" --
                    // every call site of `debug_checkpoint` already propagates its
                    // `PhysureResult` with `?`, so a user with a broken breakpoint condition
                    // gets a real error message instead of a breakpoint that quietly never
                    // fires.
                    *l == line && is_truthy(&self.eval_expr(cond, env)?)
                }
                // Fires on every statement inside the named function's innermost frame, not
                // only its first statement -- see the doc comment on `Breakpoint::FunctionEntry`
                // in debug.rs for why.
                crate::debug::Breakpoint::FunctionEntry(name) => {
                    innermost_fn_name.as_deref() == Some(name.as_str())
                }
            };
            if hits {
                break;
            }
        }

        // Two different "no match" cases, deliberately handled differently: no breakpoints
        // registered at all means every checkpoint still reaches the hook, exactly as before
        // C3 (preserves C1's "hook sees everything" behavior so plain step/next/continue work
        // without requiring a breakpoint to be set first); breakpoints registered but none of
        // them matched *this* checkpoint means stay silent.
        if !hits && !breakpoints.is_empty() {
            return Ok(());
        }

        // Re-acquire `call_stack` only now, right before the hook call, to build the
        // `DebugContext` -- safe to hold during `hook.on_statement` because `DebugHook` only
        // ever receives `&DebugContext`, never a `PhsInterpreter` reference, so there is no way
        // for the hook to call back into `self` and re-enter this lock.
        let call_stack = self.call_stack.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = DebugContext { line, call_stack: &call_stack, env };
        let _ = hook.on_statement(&ctx);
        Ok(())
    }
```
Replace it with:
```rust
    fn debug_checkpoint(&self, line: usize, env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        let Some(hook) = &self.debug_hook else { return Ok(()) };

        // Snapshot the breakpoint list, the innermost frame's name/depth, and the pending step
        // mode, then drop every lock *before* evaluating any `Conditional` breakpoint's
        // condition below: that condition may call a PHS-defined function, which re-enters
        // `debug_checkpoint` on this same thread via `eval_expr` -> `call_function_node` ->
        // `call_function_node_at` -> `eval_statement_with_env_at`. `std::sync::Mutex` is not
        // reentrant, so holding any of these locked (as `MutexGuard`s) across that call would
        // self-deadlock the thread forever -- NLL only relaxes borrow-checking, it doesn't
        // change when a `MutexGuard`'s `Drop` actually runs, so the naive "just lock at the top
        // of the function" version hangs the instant a condition calls back in. `Breakpoint`,
        // `StackFrame`, and `StepMode` are all `Clone`/`Copy`, so cloning out of the locks is
        // cheap and correct.
        let breakpoints = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (innermost_fn_name, current_depth) = {
            let call_stack = self.call_stack.lock().unwrap_or_else(|e| e.into_inner());
            (call_stack.last().map(|f| f.fn_name.clone()), call_stack.len())
        };
        let pending_step = *self.step_mode.lock().unwrap_or_else(|e| e.into_inner());

        let mut hits = false;
        for bp in &breakpoints {
            hits = match bp {
                crate::debug::Breakpoint::Line(l) => *l == line,
                crate::debug::Breakpoint::Conditional(l, cond) => {
                    // Let a condition-eval error (typo'd variable, type error, ...) propagate
                    // as a real error instead of silently treating it as "didn't match" --
                    // every call site of `debug_checkpoint` already propagates its
                    // `PhysureResult` with `?`, so a user with a broken breakpoint condition
                    // gets a real error message instead of a breakpoint that quietly never
                    // fires.
                    *l == line && is_truthy(&self.eval_expr(cond, env)?)
                }
                // Fires on every statement inside the named function's innermost frame, not
                // only its first statement -- see the doc comment on `Breakpoint::FunctionEntry`
                // in debug.rs for why.
                crate::debug::Breakpoint::FunctionEntry(name) => {
                    innermost_fn_name.as_deref() == Some(name.as_str())
                }
            };
            if hits {
                break;
            }
        }

        // A pending `Step*`/`Pause` can also justify firing even when no breakpoint matched
        // *this* checkpoint -- this is what makes `step`/`next`/`finish` actually do something
        // once at least one breakpoint exists, instead of being indistinguishable from
        // `continue`. `Into` (StepInto and Pause both map here) fires unconditionally; `Over`
        // and `Out` are gated on `call_stack` depth relative to where the step was issued.
        let step_due = match pending_step {
            Some(StepMode::Into) => true,
            Some(StepMode::Over(saved_depth)) => current_depth <= saved_depth,
            Some(StepMode::Out(saved_depth)) => current_depth < saved_depth,
            None => false,
        };

        // Three cases, deliberately handled differently: no breakpoints registered at all means
        // every checkpoint still reaches the hook, exactly as before C3 (preserves C1's "hook
        // sees everything" behavior so plain step/next/continue work without requiring a
        // breakpoint to be set first); breakpoints registered and a step is due means fire even
        // without a match; breakpoints registered, none matched, and no step is due means stay
        // silent.
        if !hits && !step_due && !breakpoints.is_empty() {
            return Ok(());
        }

        // Re-acquire `call_stack` only now, right before the hook call, to build the
        // `DebugContext` -- safe to hold during `hook.on_statement` because `DebugHook` only
        // ever receives `&DebugContext`, never a `PhsInterpreter` reference, so there is no way
        // for the hook to call back into `self` and re-enter this lock.
        let call_stack = self.call_stack.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = DebugContext { line, call_stack: &call_stack, env };
        let action = hook.on_statement(&ctx);
        drop(call_stack);

        *self.step_mode.lock().unwrap_or_else(|e| e.into_inner()) = match action {
            DebugAction::Continue => None,
            DebugAction::StepInto | DebugAction::Pause => Some(StepMode::Into),
            DebugAction::StepOver => Some(StepMode::Over(current_depth)),
            DebugAction::StepOut => Some(StepMode::Out(current_depth)),
        };

        Ok(())
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p physure-script --lib interpreter::tests::step_over_skips`
Run: `cargo test -p physure-script --lib interpreter::tests::step_into_fires`
Run: `cargo test -p physure-script --lib interpreter::tests::continue_after_a_breakpoint`
Expected: all three PASS.

- [ ] **Step 6: Run the full `physure-script` test suite**

Run: `cargo test -p physure-script --lib`
Expected: PASS — including every pre-existing `debug`/breakpoint test from C1/C3 (their
`RecordingHook`s always return `Continue`, and since none of them register any breakpoints,
they hit the unchanged `breakpoints.is_empty()` branch and keep firing on every statement
exactly as before; this step is what proves that).

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "feat(phs): implement step/next/finish bookkeeping so DebugAction is no longer discarded"
```

### Task A.2: Fix the now-stale `call_stack` doc comment

- [ ] **Step 1: Update the comment**

The `call_stack` field's doc comment (`interpreter.rs`, just above the field) currently reads:
```rust
    /// `Arc<Mutex<..>>`, not `RefCell`: Track B's `for`-expression and `parallel_map` rayon
    /// paths require `&PhsInterpreter: Send + Sync` at compile time regardless of whether a
    /// hook is set at runtime -- `RefCell` would break both of those already-shipped parallel
    /// paths. The mutex is safe today because a debugging session only exercises sequential
    /// execution paths in practice; `parallel_map`'s rayon path does not yet check
    /// `debug_hook` and would corrupt this stack if used concurrently with an active hook --
    /// closing that gap is planned as a later Integration task, not yet implemented.
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
```
This is now factually wrong: the Integration task (already merged) added exactly that guard to
both `parallel_map` (`builtins.rs`) and the `for`-expression parallel path (this same file).
Replace it with:
```rust
    /// `Arc<Mutex<..>>`, not `RefCell`: Track B's `for`-expression and `parallel_map` rayon
    /// paths require `&PhsInterpreter: Send + Sync` at compile time regardless of whether a
    /// hook is set at runtime -- `RefCell` would break both of those already-shipped parallel
    /// paths. Both of those rayon entry points check `debug_hook_is_set()` before choosing the
    /// parallel branch and fall back to plain sequential execution whenever a hook is attached
    /// (see `builtins.rs`'s `parallel_map` arm and this file's `Expr::ForExpr` arm), so this
    /// mutex is never contended by more than one thread in practice -- a debugging session only
    /// ever exercises sequential execution paths.
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
```

- [ ] **Step 2: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "docs(phs): correct the call_stack safety comment now that the Integration task landed"
```

### Task A.3: Document `call_site_line`'s current `0` limitation inline

- [ ] **Step 1: Add a doc comment where the `0` is chosen**

`call_function_node` (`interpreter.rs`) currently reads:
```rust
    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        self.call_function_node_at(func, arg_vals, env, 0)
    }
```
Change to:
```rust
    /// `call_site_line` is always `0` here -- `Expr` carries no line numbers (only `Statement`
    /// does), and this is the only call path `Expr::FunctionCall` reaches, so there is no real
    /// line to pass. This is a known, accepted v1 limitation, not a bug: it means
    /// `StackFrame::call_site_line` (and therefore the CLI debugger's "called from line N" and
    /// `backtrace` output) always reads `0` for every call, for every debug session, today.
    /// Expression-level call-site precision is out of scope for LAB-READY.
    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        self.call_function_node_at(func, arg_vals, env, 0)
    }
```

- [ ] **Step 2: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "docs(phs): document that call_site_line is always 0 (known v1 limitation, not a bug)"
```

### Task A.4: Document the dormant `Clone`-sharing risk

- [ ] **Step 1: Add a doc comment on `with_debug_hook`**

`with_debug_hook` (`interpreter.rs`) currently reads:
```rust
    pub fn with_debug_hook(resolver: Arc<dyn ModuleResolver>, hook: Arc<dyn DebugHook>) -> Self {
        let mut interp = Self::new(resolver);
        interp.debug_hook = Some(hook);
        interp
    }
```
Change to:
```rust
    /// `PhsInterpreter` derives `Clone`, and `call_stack`/`breakpoints`/`step_mode` are
    /// `Arc<Mutex<..>>` -- every clone shares the *same* underlying call stack, breakpoint list,
    /// and step state, not an independent copy. `physure-script/src/function.rs`'s
    /// `PhyFunction::deriv`/`integral`/`solve`/`compose` already clone `self.interpreter` freely.
    /// No current binding (Python/WASM/Java) attaches a debug hook, so this is dormant today --
    /// but an embedder that builds a hook-attached interpreter, derives a `PhyFunction` from it,
    /// and calls the original and the derivative concurrently on separate threads would have
    /// both share one call stack, corrupting what a hook sees. Don't attach a debug hook to an
    /// interpreter that will be cloned and used concurrently across threads.
    pub fn with_debug_hook(resolver: Arc<dyn ModuleResolver>, hook: Arc<dyn DebugHook>) -> Self {
        let mut interp = Self::new(resolver);
        interp.debug_hook = Some(hook);
        interp
    }
```

- [ ] **Step 2: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "docs(phs): document that Clone shares call_stack/breakpoints/step_mode across debug-hook-attached interpreters"
```

### Task A.5: `breakpoints` `Arc`-swap to avoid a deep clone per checkpoint

- [ ] **Step 1: Write a failing test for the new shape**

This is an internal representation change with no new observable behavior, so there's no new
test to write beyond confirming the existing suite still passes (Step 3) — skip straight to the
change.

- [ ] **Step 2: Change `breakpoints`'s type and its two accessors**

Change the field:
```rust
    breakpoints: Arc<Mutex<Vec<crate::debug::Breakpoint>>>,
```
to:
```rust
    /// `Mutex<Arc<Vec<..>>>`, not `Mutex<Vec<..>>`: `debug_checkpoint` needs to read this list
    /// on every single statement checkpoint while debugging, and cloning a `Vec<Breakpoint>`
    /// means deep-cloning every embedded `Expr` AST in every `Conditional` breakpoint each time.
    /// Cloning an `Arc` is a refcount bump; the `Vec` itself is only ever cloned once, inside
    /// `add_breakpoint`, when a new breakpoint is actually added (copy-on-write).
    breakpoints: Arc<Mutex<std::sync::Arc<Vec<crate::debug::Breakpoint>>>>,
```
(Note: `std::sync::Arc` is spelled out here because `Arc` is already imported and used for the
outer `Mutex` wrapper in this same field -- both refer to the same type, this is just being
explicit about which `Arc` wraps what.)

Update initialization in `new`:
```rust
            breakpoints: Arc::new(Mutex::new(Vec::new())),
```
to:
```rust
            breakpoints: Arc::new(Mutex::new(std::sync::Arc::new(Vec::new()))),
```

Update `add_breakpoint`:
```rust
    pub fn add_breakpoint(&self, bp: crate::debug::Breakpoint) {
        self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).push(bp);
    }
```
to:
```rust
    pub fn add_breakpoint(&self, bp: crate::debug::Breakpoint) {
        let mut guard = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner());
        let mut updated = (**guard).clone();
        updated.push(bp);
        *guard = std::sync::Arc::new(updated);
    }
```

Update `debug_checkpoint`'s snapshot line (added in Task A.1's Step 4):
```rust
        let breakpoints = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).clone();
```
stays exactly the same *text* — `.clone()` on the `MutexGuard<Arc<Vec<..>>>` target now clones
the `Arc` (a pointer + refcount bump), not the `Vec`'s contents, because the field's type
changed. No other line in `debug_checkpoint` needs to change: `breakpoints` is still used as
`&breakpoints` (iterating `&Vec<Breakpoint>` via `Arc`'s `Deref`) and `breakpoints.is_empty()`
(also via `Deref`), both of which already work identically against `Arc<Vec<T>>` as they did
against a bare `Vec<T>`.

- [ ] **Step 3: Run the full `physure-script` test suite**

Run: `cargo test -p physure-script --lib`
Expected: PASS — this is a pure representation change, no behavior difference.

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/interpreter.rs
git commit -m "perf(phs): store breakpoints behind an Arc so debug_checkpoint clones a pointer, not the AST"
```

---

## Task Group B — physure-core/registry.rs + physure-cli/debug.rs cleanup

**Files:**
- Modify: `physure-core/src/units/registry.rs` (`split_prefix` single-lookup)
- Modify: `physure-cli/src/debug.rs` (`RichRenderer` reuse, registry-once-per-session,
  `BreakpointSpec::FunctionEntry` removal, reject `--break 0`)

### Task B.1: `split_prefix` returns the resolved unit, avoiding a second lookup in `get_unit`

- [ ] **Step 1: Write a failing test**

In `physure-core/src/units/registry.rs`'s `mod tests` block, alongside the existing
`split_prefix_*` tests:
```rust
#[test]
fn split_prefix_result_carries_the_resolved_unit_so_get_unit_does_not_look_it_up_twice() {
    let (reg, _) = crate::units::conf::build_registry_from_conf();
    let (symbol, factor, unit) = reg.split_prefix("km").expect("km should split as k + m");
    assert_eq!(symbol, "k");
    assert_eq!(factor, 1000.0);
    // The returned unit is the *unprefixed* remainder ("m"), scale 1.0 -- get_unit is what
    // applies `factor` on top of it, not split_prefix's job.
    assert_eq!(unit.scale, 1.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure --lib units::registry::tests::split_prefix_result_carries`
Expected: FAIL to compile — `split_prefix` currently returns `Option<(String, f64)>`, a 2-tuple,
not a 3-tuple with the resolved `RationalUnit`.

- [ ] **Step 3: Change `split_prefix`'s return type and `get_unit`'s caller**

`split_prefix` currently reads:
```rust
    pub fn split_prefix(&self, name: &str) -> Option<(String, f64)> {
        for (p_sym, p_factor) in &self.prefixes {
            if name.starts_with(p_sym.as_str()) && name.len() > p_sym.len() {
                let rest = &name[p_sym.len()..];
                let rest_resolved = self.resolve_symbol(rest);
                let known = self.base_units.contains_key(&rest_resolved)
                    || self.derived_units.contains_key(&rest_resolved)
                    || self.base_units.contains_key(rest)
                    || self.derived_units.contains_key(rest);
                if known {
                    return Some((p_sym.clone(), *p_factor));
                }
            }
        }
        None
    }
```
Change to:
```rust
    /// Returns `(prefix symbol, prefix factor, the unprefixed remainder's own `RationalUnit`)`.
    /// Carrying the resolved unit forward means `get_unit`'s caller doesn't have to redo the
    /// same `resolve_symbol` + four-way lookup a second time just to fetch what this method
    /// already found while checking whether `name` was a known prefix+unit combination.
    pub fn split_prefix(&self, name: &str) -> Option<(String, f64, RationalUnit)> {
        for (p_sym, p_factor) in &self.prefixes {
            if name.starts_with(p_sym.as_str()) && name.len() > p_sym.len() {
                let rest = &name[p_sym.len()..];
                let rest_resolved = self.resolve_symbol(rest);
                let base_opt = self
                    .base_units
                    .get(&rest_resolved)
                    .or_else(|| self.derived_units.get(&rest_resolved))
                    .or_else(|| self.base_units.get(rest))
                    .or_else(|| self.derived_units.get(rest));
                if let Some(unit) = base_opt {
                    return Some((p_sym.clone(), *p_factor, unit.clone()));
                }
            }
        }
        None
    }
```

`get_unit`'s caller currently reads:
```rust
        } else {
            self.split_prefix(name).and_then(|(p_sym, p_factor)| {
                let rest = &name[p_sym.len()..];
                let rest_resolved = self.resolve_symbol(rest);
                let base_opt = self
                    .base_units
                    .get(&rest_resolved)
                    .or_else(|| self.derived_units.get(&rest_resolved))
                    .or_else(|| self.base_units.get(rest))
                    .or_else(|| self.derived_units.get(rest));
                base_opt.map(|base_u| {
                    let new_scale = base_u.scale * p_factor;
                    let mut prefixed = base_u.clone().with_scale(new_scale);
                    prefixed.display_name = Some(name.to_string());
                    prefixed
                })
            })
        };
```
Change to:
```rust
        } else {
            self.split_prefix(name).map(|(_p_sym, p_factor, base_u)| {
                let new_scale = base_u.scale * p_factor;
                let mut prefixed = base_u.with_scale(new_scale);
                prefixed.display_name = Some(name.to_string());
                prefixed
            })
        };
```

- [ ] **Step 4: Update `inspect.rs`'s caller of `split_prefix`**

`physure-script/src/inspect.rs` currently reads:
```rust
            let prefix = q.unit.display_name.as_ref().and_then(|dn| registry.split_prefix(dn));
```
Change to:
```rust
            let prefix = q.unit.display_name.as_ref().and_then(|dn| registry.split_prefix(dn)).map(|(sym, factor, _unit)| (sym, factor));
```
(`Inspection.prefix`'s type is unchanged — `Option<(String, f64)>` — so the third tuple element
is simply dropped here; `inspect.rs` never needed the resolved unit itself, only the symbol and
factor.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p physure --lib units::registry::tests::split_prefix_result_carries`
Expected: PASS.

Run: `cargo test -p physure --lib`
Expected: PASS (every existing prefixed-unit test — `"km"`, `"kN"`, etc. — resolves identically;
this is a pure refactor of where the lookup happens).

Run: `cargo test -p physure-script --lib inspect::tests`
Expected: PASS (the existing `inspects_a_km_quantity_with_prefix_present` test still asserts
`insp.prefix == Some(("k".to_string(), 1000.0))`, unaffected by dropping the third tuple element
at the call site).

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/units/registry.rs physure-script/src/inspect.rs
git commit -m "perf(core): have split_prefix return the resolved unit so get_unit doesn't look it up twice"
```

### Task B.2: `phs debug`'s `locals`/`globals` reuse `RichRenderer::render_variable_card`

- [ ] **Step 1: Update the two `println!` sites**

In `physure-cli/src/debug.rs`, add the import at the top of the file, alongside the existing
`use` block:
```rust
use crate::rich::RichRenderer;
```

Change the `Locals` arm:
```rust
                DebuggerCommand::Locals => {
                    let Some(frame) = ctx.call_stack.last() else {
                        println!("(no locals at global scope)");
                        continue;
                    };
                    for name in &frame.declared {
                        if let Some(val) = ctx.env.get(name) {
                            println!("  {name} = {val}");
                        }
                    }
                }
```
to:
```rust
                DebuggerCommand::Locals => {
                    let Some(frame) = ctx.call_stack.last() else {
                        println!("(no locals at global scope)");
                        continue;
                    };
                    for name in &frame.declared {
                        if let Some(val) = ctx.env.get(name) {
                            RichRenderer::render_variable_card(name, val);
                        }
                    }
                }
```

Change the `Globals` arm:
```rust
                DebuggerCommand::Globals => {
                    let local_names: std::collections::HashSet<&str> = ctx
                        .call_stack
                        .last()
                        .map(|f| f.declared.iter().map(String::as_str).collect())
                        .unwrap_or_default();
                    for (name, val) in ctx.env {
                        if !local_names.contains(name.as_str()) {
                            println!("  {name} = {val}");
                        }
                    }
                }
```
to:
```rust
                DebuggerCommand::Globals => {
                    let local_names: std::collections::HashSet<&str> = ctx
                        .call_stack
                        .last()
                        .map(|f| f.declared.iter().map(String::as_str).collect())
                        .unwrap_or_default();
                    for (name, val) in ctx.env {
                        if !local_names.contains(name.as_str()) {
                            RichRenderer::render_variable_card(name, val);
                        }
                    }
                }
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p physure-cli`
Expected: builds cleanly (no test to add here — `RichRenderer::render_variable_card` is
`println!`-based, same as the code it replaces, and is already exercised by the plain REPL's
existing usage; the end-to-end `debug_session.rs` test from the Integration task already covers
`inspect`, and the `locals`/`globals` commands aren't part of that scripted session — adding
them there is optional polish, not required by this task).

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/debug.rs
git commit -m "refactor(cli): reuse RichRenderer::render_variable_card in phs debug's locals/globals"
```

### Task B.3: Build the `UnitRegistry` once per debug session, not once per `inspect` command

- [ ] **Step 1: Thread a registry into `CliDebugHook`**

`CliDebugHook` currently reads:
```rust
struct CliDebugHook;

impl DebugHook for CliDebugHook {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
```
Change to:
```rust
struct CliDebugHook {
    registry: physure_core::UnitRegistry,
}

impl DebugHook for CliDebugHook {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
```

The `Inspect` arm currently reads:
```rust
                DebuggerCommand::Inspect(name) => {
                    let Some(val) = ctx.env.get(&name) else {
                        println!("error: no variable named '{name}'");
                        continue;
                    };
                    let scope = ctx
                        .call_stack
                        .last()
                        .filter(|f| f.declared.contains(&name))
                        .map(|f| ScopeKind::Local { owner_fn: f.fn_name.clone(), frame_depth: ctx.call_stack.len() })
                        .unwrap_or(ScopeKind::Global);
                    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
                    let insp = inspect(&name, val, scope, &registry);
```
Change to:
```rust
                DebuggerCommand::Inspect(name) => {
                    let Some(val) = ctx.env.get(&name) else {
                        println!("error: no variable named '{name}'");
                        continue;
                    };
                    let scope = ctx
                        .call_stack
                        .last()
                        .filter(|f| f.declared.contains(&name))
                        .map(|f| ScopeKind::Local { owner_fn: f.fn_name.clone(), frame_depth: ctx.call_stack.len() })
                        .unwrap_or(ScopeKind::Global);
                    let insp = inspect(&name, val, scope, &self.registry);
```
(`inspect`'s signature is `fn inspect(name: &str, value: &PhsValue, scope: ScopeKind, registry:
&UnitRegistry) -> Inspection` — `registry` moved from a locally-built value to `&self.registry`,
same position.)

`run_debug`'s construction of `CliDebugHook` currently reads:
```rust
    let mut interp = PhsInterpreter::with_debug_hook(
        Arc::new(physure_script::resolver::FsModuleResolver::default()),
        Arc::new(CliDebugHook),
    );
```
Change to:
```rust
    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
    let mut interp = PhsInterpreter::with_debug_hook(
        Arc::new(physure_script::resolver::FsModuleResolver::default()),
        Arc::new(CliDebugHook { registry }),
    );
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p physure-cli`
Expected: builds cleanly.

Run: `cargo test -p physure-cli --test debug_session`
Expected: PASS (the end-to-end test's `inspect speed` step exercises exactly this path).

- [ ] **Step 3: Commit**

```bash
git add physure-cli/src/debug.rs
git commit -m "perf(cli): build phs debug's UnitRegistry once per session instead of once per inspect command"
```

### Task B.4: Remove the dead `BreakpointSpec::FunctionEntry` variant and its `unreachable!()` arm

- [ ] **Step 1: Confirm it's genuinely unreachable**

```bash
grep -n "BreakpointSpec::FunctionEntry" physure-cli/src/debug.rs
```
Expected: two hits — the variant's own declaration, and the `unreachable!()` match arm in
`run_debug`. `parse_break_flag` (the only producer of `BreakpointSpec`) never constructs
`FunctionEntry` — function-entry breakpoints are added directly via the separate `--break-fn`
flag branch, which calls `Breakpoint::FunctionEntry` (a different type, in `physure_script`)
directly, never through `BreakpointSpec`.

- [ ] **Step 2: Remove the variant and the dead arm**

`BreakpointSpec` currently reads:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointSpec {
    Line(usize),
    Conditional(usize, String),
    FunctionEntry(String),
}
```
Change to:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointSpec {
    Line(usize),
    Conditional(usize, String),
}
```

In `run_debug`, the match on `spec` currently reads:
```rust
                match spec {
                    BreakpointSpec::Line(l) => interp.add_breakpoint(Breakpoint::Line(l)),
                    BreakpointSpec::Conditional(l, cond_src) => {
                        match parse_phs(&cond_src) {
                            Ok(prog) if !prog.statements.is_empty() => {
                                if let physure_script::Statement::Expr(e) = prog.statements.into_iter().next().unwrap() {
                                    interp.add_breakpoint(Breakpoint::Conditional(l, e));
                                }
                            }
                            _ => eprintln!("warning: could not parse breakpoint condition '{cond_src}'"),
                        }
                    }
                    BreakpointSpec::FunctionEntry(_) => unreachable!("parse_break_flag never returns FunctionEntry"),
                }
```
Change to:
```rust
                match spec {
                    BreakpointSpec::Line(l) => interp.add_breakpoint(Breakpoint::Line(l)),
                    BreakpointSpec::Conditional(l, cond_src) => {
                        match parse_phs(&cond_src) {
                            Ok(prog) if !prog.statements.is_empty() => {
                                if let physure_script::Statement::Expr(e) = prog.statements.into_iter().next().unwrap() {
                                    interp.add_breakpoint(Breakpoint::Conditional(l, e));
                                }
                            }
                            _ => eprintln!("warning: could not parse breakpoint condition '{cond_src}'"),
                        }
                    }
                }
```
(Removing the third enum variant makes this match exhaustive with just the two arms — no
`unreachable!()` needed, the impossible case is now unrepresentable, matching the review's
suggested fix directly.)

- [ ] **Step 3: Build to verify**

Run: `cargo build -p physure-cli`
Expected: builds cleanly, and the previously-present "variant is never constructed" dead-code
warning is gone.

Run: `cargo test -p physure-cli debug::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add physure-cli/src/debug.rs
git commit -m "refactor(cli): remove the unreachable BreakpointSpec::FunctionEntry variant"
```

### Task B.5: Reject `--break 0` (collides with the "unknown line" sentinel)

- [ ] **Step 1: Write a failing test**

In `physure-cli/src/debug.rs`'s `mod tests` block, alongside the existing `parse_break_flag`
tests:
```rust
#[test]
fn rejects_a_zero_line_breakpoint() {
    // Line 0 is never a real source line (parser.rs's line_col() is 1-based), and it's the
    // sentinel synthesized/composed functions get for "unknown line" (see
    // physure-script/src/interpreter.rs's function-composition sites) -- accepting it as a
    // literal breakpoint would spuriously pause on every call to such a function.
    assert_eq!(parse_break_flag("0"), None);
}

#[test]
fn rejects_a_zero_line_conditional_breakpoint() {
    assert_eq!(parse_break_flag("0:x > 1"), None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p physure-cli debug::tests::rejects_a_zero_line`
Expected: both FAIL — `parse_break_flag` currently accepts any valid `usize`, including `0`.

- [ ] **Step 3: Reject `0` in `parse_break_flag`**

`parse_break_flag` currently reads:
```rust
pub fn parse_break_flag(value: &str) -> Option<BreakpointSpec> {
    if let Some((line_str, cond)) = value.split_once(':') {
        line_str.trim().parse::<usize>().ok().map(|l| BreakpointSpec::Conditional(l, cond.trim().to_string()))
    } else {
        value.trim().parse::<usize>().ok().map(BreakpointSpec::Line)
    }
}
```
Change to:
```rust
pub fn parse_break_flag(value: &str) -> Option<BreakpointSpec> {
    if let Some((line_str, cond)) = value.split_once(':') {
        line_str.trim().parse::<usize>().ok()
            .filter(|&l| l > 0)
            .map(|l| BreakpointSpec::Conditional(l, cond.trim().to_string()))
    } else {
        value.trim().parse::<usize>().ok().filter(|&l| l > 0).map(BreakpointSpec::Line)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p physure-cli debug::tests::rejects_a_zero_line`
Expected: both PASS.

Run: `cargo test -p physure-cli debug::tests`
Expected: PASS (all existing parsing tests unaffected — none of them use `"0"`).

- [ ] **Step 5: Commit**

```bash
git add physure-cli/src/debug.rs
git commit -m "fix(cli): reject --break 0, which collides with synthesized functions' unknown-line sentinel"
```

---

## Integration

- [ ] **Step 1: After both groups are merged, run full verification**

Run: `cargo test --workspace`
Expected: PASS, zero failures.

Run: `cargo build --workspace`
Expected: builds cleanly, zero errors. The previously-present `BreakpointSpec::FunctionEntry`
dead-code warning should be gone (Task B.4); no new warnings expected.

- [ ] **Step 2: Manual smoke check of the flagship fix**

Run `phs debug` against a small script with a breakpoint and confirm `next`/`step`/`finish`
now behave differently from `continue` (e.g. `next` over a function call lands on the statement
after the call, not inside it). This is the concrete, human-observable proof the flagship gap is
closed.

- [ ] **Step 3: Commit** (only if Step 2 surfaces something to fix; otherwise no commit needed —
      the two task groups' own commits are the deliverable)

---

## Plan Self-Review

**Spec coverage:** all 10 review findings are accounted for: 5 fixed in Group A (step/next/finish
bookkeeping, stale comment, `call_site_line` doc, `Clone`-sharing doc, breakpoint `Arc`-swap), 5
fixed in Group B (`split_prefix` double-lookup, `RichRenderer` reuse, registry-once-per-session,
dead `BreakpointSpec::FunctionEntry` variant, `--break 0` rejection). Three findings
(dimension-flatten dedup, `collect_declared`'s `where`-binding gap, `StackFrame::declared`
memoization) are deliberately not fixed, each with the investigation that led to that decision
documented at the top of this plan rather than silently dropped — the memoization one in
particular was caught only by tracing through how `env.clone()` interacts with recursive calls
during this plan's own self-review, after the task was already drafted with working-looking code
and tests; it was removed, not shipped.

**Placeholder scan:** no TBD/TODO; every step shows real code or an exact `cargo`/`git` command.

**Type consistency check:** `StepMode` (Task A.1) is used consistently in `debug_checkpoint` only
— it's a private `interpreter.rs` type, not exposed via `debug.rs`'s public `DebugAction`/
`Breakpoint` API, so no cross-file signature to keep in sync. `split_prefix`'s new 3-tuple return
(Task B.1) is used consistently by both its two callers (`get_unit` in the same file,
`inspect.rs` in a different crate) — both were updated in the same task. `StackFrame.declared`
stays a plain `HashSet<String>`, unchanged from before this plan (Task A.6, which would have
wrapped it in `Arc`, was removed during self-review — see the "deliberately not fixed" list
above).
