pub(crate) mod binary_ops;
pub(crate) use binary_ops::coerce_equation_string;
pub(crate) mod expressions;
pub(crate) mod statements;
pub(crate) use statements::StepMode;
pub(crate) mod helpers;

use std::collections::HashMap;
use physure_core::error::PhysureResult;
use std::sync::{Arc, Mutex};

/// A host-registered function callable from PHS source by name. Lets embedders
/// (e.g. the PyO3 binding) expose functions without physure-script depending on them.
pub type ExternalFn = Arc<dyn Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync>;

use crate::debug::{DebugHook, StackFrame};
use crate::resolver::{ModuleResolver, FsModuleResolver};
use crate::PhsValue;


#[derive(Clone)]
pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
    pub externals: HashMap<String, ExternalFn>,
    pub(crate) plugin_state: Arc<Mutex<crate::plugin::PluginState>>,
    pub(crate) plugin_base_dir: Option<std::path::PathBuf>,
    /// call-name -> (domain, canonical builtin name), populated by `use x from calc` etc.
    pub(crate) unlocked_builtins: Arc<Mutex<HashMap<String, (&'static str, String)>>>,
    /// Lazily-loaded plugin/ext functions, keyed by their `use`d (possibly aliased) name.
    pub(crate) dynamic_externals: Arc<Mutex<HashMap<String, ExternalFn>>>,
    // TODO: a `context: PhsContext` belongs here, next to `unlocked_builtins` -- the one
    // thing this interpreter already scopes to a program. A script cannot say how its
    // uncertainties should propagate; it depends on a `physure.conf` it never mentions,
    // and the transpilers drop that dependency entirely. See
    // docs/superpowers/specs/2026-08-02-phs-execution-context.md.
    pub(crate) debug_hook: Option<Arc<dyn DebugHook>>,
    /// `Arc<Mutex<..>>`, not `RefCell`: Track B's `for`-expression and `parallel_map` rayon
    /// paths require `&PhsInterpreter: Send + Sync` at compile time regardless of whether a
    /// hook is set at runtime -- `RefCell` would break both of those already-shipped parallel
    /// paths. Both of those rayon entry points check `debug_hook_is_set()` before choosing the
    /// parallel branch and fall back to plain sequential execution whenever a hook is attached
    /// (see `builtins.rs`'s `parallel_map` arm and this file's `Expr::ForExpr` arm), so this
    /// mutex is never contended by more than one thread in practice -- a debugging session only
    /// ever exercises sequential execution paths.
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
    /// `Mutex<Arc<Vec<..>>>`, not `Mutex<Vec<..>>`: `debug_checkpoint` needs to read this list
    /// on every single statement checkpoint while debugging, and cloning a `Vec<Breakpoint>`
    /// means deep-cloning every embedded `Expr` AST in every `Conditional` breakpoint each time.
    /// Cloning an `Arc` is a refcount bump; the `Vec` itself is only ever cloned once, inside
    /// `add_breakpoint`, when a new breakpoint is actually added (copy-on-write).
    pub(crate) breakpoints: Arc<Mutex<Arc<Vec<crate::debug::Breakpoint>>>>,
    pub(crate) step_mode: Arc<Mutex<Option<StepMode>>>,
}

impl Default for PhsInterpreter {
    fn default() -> Self {
        Self::new(Arc::new(FsModuleResolver::default()))
    }
}

impl PhsInterpreter {
    pub fn new(resolver: Arc<dyn ModuleResolver>) -> Self {
        Self {
            env: HashMap::new(),
            resolver,
            externals: HashMap::new(),
            plugin_state: Arc::new(Mutex::new(crate::plugin::PluginState::default())),
            plugin_base_dir: None,
            unlocked_builtins: Arc::new(Mutex::new(HashMap::new())),
            dynamic_externals: Arc::new(Mutex::new(HashMap::new())),
            debug_hook: None,
            call_stack: Arc::new(Mutex::new(Vec::new())),
            breakpoints: Arc::new(Mutex::new(Arc::new(Vec::new()))),
            step_mode: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_default() -> Self {
        Self::default()
    }

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

    pub(crate) fn debug_hook_is_set(&self) -> bool {
        self.debug_hook.is_some()
    }

    /// Like `default()`, but resolves `import` paths relative to `base_dir`
    /// (typically the directory containing the script being run) instead of `.`.
    /// Native plugins under `<base_dir>/ext/` are not loaded eagerly — they're
    /// dlopen'd on demand the first time a script `use`s a symbol from them.
    pub fn with_base_dir(base_dir: impl Into<std::path::PathBuf>) -> Self {
        let base_dir = base_dir.into();
        let mut interp = Self::new(Arc::new(FsModuleResolver::new(base_dir.clone())));
        interp.plugin_base_dir = Some(base_dir);
        interp
    }

    /// Re-checks only the native plugin stems some `use` statement has already
    /// caused to be loaded, and installs any updated functions. No-op if the
    /// interpreter wasn't constructed with a base dir or nothing has been
    /// `use`d yet. Returns the names of functions (re)installed.
    pub fn reload_native_ext(&mut self) -> Vec<String> {
        if self.plugin_base_dir.is_none() {
            return Vec::new();
        }
        let plugin_state = self.plugin_state.clone();
        let mut state = plugin_state.lock().unwrap_or_else(|e| e.into_inner());
        let mut dynamic_externals = self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner());
        state.reload_loaded_into(&mut dynamic_externals)
    }

    /// Registers a host function under `name`, callable from PHS source like any builtin.
    /// Takes precedence over user-defined PHS functions but not over builtins.
    pub fn register_fn<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync + 'static,
    {
        self.externals.insert(name.into(), Arc::new(f));
    }

}

pub fn eval_phs(input: &str) -> PhysureResult<Vec<PhsValue>> {
    let program = crate::parser::parse_phs(input)?;
    let mut interp = PhsInterpreter::default();
    
    let mut results = Vec::new();
    for stmt in &program.statements {
        let val = interp.eval_statement(stmt)?;
        if val != PhsValue::None {
            results.push(val);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::error::PhysureError;
    use physure_core::units::parser::Parser as UnitParser;
    use crate::ast::*;
    use crate::resolver::{MemoryModuleResolver, ModuleExport};

    #[test]
    fn debug_hook_fires_once_per_statement_including_function_return() {
        use crate::debug::{DebugAction, DebugContext, DebugHook};
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
        // PHS function bodies are indentation-delimited (see the note on the C0.1 test) --
        // "fn double(x) =" on line 1, its two-statement body on lines 2-3, then the top-level call
        // on line 4 (no closing brace to account for).
        let program = crate::parser::parse_phs(
            "fn double(x) =\n  y = x * 2\n  return y\nres = double(3)\n",
        )
        .unwrap();
        interp.run_statements_with_lines(&program).unwrap();

        let lines = seen.lock().unwrap();
        // line 4 (top-level call), then the two statements inside double's body (lines 2 and 3).
        // Line 3 is an explicit `return`, which `call_function_node_at` special-cases with its own
        // `break` instead of routing through `eval_statement_with_env_at` like every other
        // statement -- this is the actual regression case for the choke-point gap (a bare
        // expression used as an implicit return, e.g. just `y` with no `return` keyword, would
        // already have been checkpointed by the ordinary `_` arm and wouldn't exercise this path).
        assert!(lines.contains(&4), "top-level call not recorded: {lines:?}");
        assert!(lines.contains(&2), "first body statement not recorded: {lines:?}");
        assert!(lines.contains(&3), "function's explicit return statement not recorded: {lines:?}");
    }

    #[test]
    fn call_stack_pops_frame_when_body_errors_mid_execution() {
        use crate::debug::{DebugAction, DebugContext, DebugHook};
        use std::sync::Arc;

        struct NoopHook;
        impl DebugHook for NoopHook {
            fn on_statement(&self, _ctx: &DebugContext) -> DebugAction {
                DebugAction::Continue
            }
        }

        let mut interp = PhsInterpreter::with_debug_hook(
            std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
            Arc::new(NoopHook),
        );
        // `boom`'s body calls an undefined function partway through, which errors out of
        // `eval_statement_with_env_at` via `?` while `call_function_node_at` is mid-loop over
        // `boom`'s body -- exactly the path that used to leave `boom`'s `StackFrame` on
        // `call_stack` forever, since the old unguarded `pop()` after the loop was never
        // reached once the `?` in the `_` arm returned early.
        let program = crate::parser::parse_phs(
            "fn boom(x) =\n  y = undefined_fn(x)\n  return y\nboom(3)\n",
        )
        .unwrap();
        let err = interp.run_statements_with_lines(&program);
        assert!(err.is_err(), "expected the undefined-function call inside boom's body to error");
        assert_eq!(
            interp.call_stack_depth(),
            0,
            "boom's StackFrame leaked on call_stack after the error propagated out"
        );

        // Run an unrelated, successful statement afterward: if a frame had leaked, subsequent
        // calls would push on top of the stale one, so any `DebugContext::call_stack` a hook
        // observes from here on would show a phantom `boom` frame beneath the real ones.
        interp.eval_str("fn ok(x) = x + 1\nok(1)").unwrap();
        assert_eq!(interp.call_stack_depth(), 0, "call_stack not clean after a later, unrelated successful call");
    }

    #[test]
    fn function_entry_breakpoint_pauses_on_every_call() {
        use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
        use std::sync::{Arc, Mutex};

        struct CountingHook(Arc<Mutex<usize>>);
        impl DebugHook for CountingHook {
            fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
                if ctx.call_stack.last().map(|f| f.fn_name.as_str()) == Some("double") {
                    *self.0.lock().unwrap() += 1;
                }
                DebugAction::Continue
            }
        }

        let hits = Arc::new(Mutex::new(0));
        let hook = Arc::new(CountingHook(hits.clone()));
        let mut interp = PhsInterpreter::with_debug_hook(
            std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
            hook,
        );
        interp.add_breakpoint(Breakpoint::FunctionEntry("double".to_string()));
        // Single-expression function body (no indentation block needed): "fn f(x) = expr".
        // NB: because `double`'s body is a single statement, this test can't by itself
        // distinguish "fires once per call" from the actual "fires once per statement in the
        // frame" semantics (see `Breakpoint::FunctionEntry`'s doc comment in debug.rs) -- two
        // calls to a one-statement function produce the same hit count either way. See
        // `function_entry_breakpoint_fires_on_every_statement_in_frame_not_just_entry` below
        // for the test that actually tells the two apart.
        let program = crate::parser::parse_phs(
            "fn double(x) = x * 2\na = double(1)\nb = double(2)\n",
        )
        .unwrap();
        interp.run_statements_with_lines(&program).unwrap();

        assert_eq!(*hits.lock().unwrap(), 2, "expected a pause on each of the two calls");
    }

    #[test]
    fn function_entry_breakpoint_fires_on_every_statement_in_frame_not_just_entry() {
        use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
        use std::sync::{Arc, Mutex};

        struct CountingHook(Arc<Mutex<usize>>);
        impl DebugHook for CountingHook {
            fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
                if ctx.call_stack.last().map(|f| f.fn_name.as_str()) == Some("double") {
                    *self.0.lock().unwrap() += 1;
                }
                DebugAction::Continue
            }
        }

        let hits = Arc::new(Mutex::new(0));
        let hook = Arc::new(CountingHook(hits.clone()));
        let mut interp = PhsInterpreter::with_debug_hook(
            std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
            hook,
        );
        interp.add_breakpoint(Breakpoint::FunctionEntry("double".to_string()));
        // Two-statement body, ONE call: `FunctionEntry` matches on "innermost frame is this
        // function", checked at every checkpoint inside that frame -- so a single call to a
        // two-statement function must hit twice, not once, which is what actually distinguishes
        // this from a true once-per-call breakpoint.
        let program = crate::parser::parse_phs(
            "fn double(x) =\n  y = x * 2\n  return y\na = double(1)\n",
        )
        .unwrap();
        interp.run_statements_with_lines(&program).unwrap();

        assert_eq!(
            *hits.lock().unwrap(),
            2,
            "FunctionEntry should fire on both statements of the single call, not just entry"
        );
    }

    #[test]
    fn conditional_breakpoint_condition_calling_a_phs_function_does_not_deadlock() {
        use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
        use std::sync::mpsc;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        struct CountingHook(Arc<Mutex<usize>>);
        impl DebugHook for CountingHook {
            fn on_statement(&self, _ctx: &DebugContext) -> DebugAction {
                *self.0.lock().unwrap() += 1;
                DebugAction::Continue
            }
        }

        let hits = Arc::new(Mutex::new(0));
        let hook = Arc::new(CountingHook(hits.clone()));
        let mut interp = PhsInterpreter::with_debug_hook(
            std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
            hook,
        );
        let program = crate::parser::parse_phs(
            "fn helper(v) = v > 0\nx = 1\nx = 2\ny = x\n",
        )
        .unwrap();
        let cond_line = program.lines[3]; // the "y = x" statement
        let cond_expr = crate::parser::parse_phs("helper(x)").unwrap().statements.remove(0);
        let crate::ast::Statement::Expr(cond) = cond_expr else { panic!("expected expr") };
        // Regression test for the self-deadlock this used to cause: the condition calls a
        // PHS-defined function, so evaluating it re-enters `debug_checkpoint` on this same
        // thread (via `eval_expr` -> `call_function_node` -> `call_function_node_at` ->
        // `eval_statement_with_env_at`) while this very breakpoint check is still in progress.
        // The old implementation held `call_stack`/`breakpoints` locked (as `MutexGuard`s)
        // across that `eval_expr` call, so the re-entrant lock attempt hung forever. Run on a
        // background thread with a bounded `recv_timeout` so a reintroduced deadlock fails this
        // test outright instead of hanging the whole `cargo test` invocation (and CI) forever.
        interp.add_breakpoint(Breakpoint::Conditional(cond_line, cond));

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            interp.run_statements_with_lines(&program).unwrap();
            let _ = tx.send(*hits.lock().unwrap());
        });

        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(count) => assert_eq!(count, 1, "should pause exactly once, once x has settled to 2"),
            Err(_) => panic!(
                "debug_checkpoint deadlocked evaluating a Conditional breakpoint whose \
                 condition calls a PHS-defined function"
            ),
        }
    }

    #[test]
    fn conditional_breakpoint_pauses_only_when_condition_holds() {
        use crate::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
        use std::sync::{Arc, Mutex};

        struct CountingHook(Arc<Mutex<usize>>);
        impl DebugHook for CountingHook {
            fn on_statement(&self, _ctx: &DebugContext) -> DebugAction {
                *self.0.lock().unwrap() += 1;
                DebugAction::Continue
            }
        }

        let hits = Arc::new(Mutex::new(0));
        let hook = Arc::new(CountingHook(hits.clone()));
        let mut interp = PhsInterpreter::with_debug_hook(
            std::sync::Arc::new(crate::resolver::FsModuleResolver::default()),
            hook,
        );
        // A checkpoint fires *before* its own statement's effect (see debug_checkpoint's call at
        // the top of eval_statement_with_env_at, ahead of the match on the statement itself) --
        // so the condition targets the line *after* the assignment it depends on, where `x` has
        // already settled to its final value from the fully-executed previous statement.
        let program = crate::parser::parse_phs(
            "x = 1\nx = 2\nx = 3\ny = x\n",
        )
        .unwrap();
        let cond_line = program.lines[3]; // the "y = x" statement
        let cond_expr = crate::parser::parse_phs("x > 2").unwrap().statements.remove(0);
        let crate::ast::Statement::Expr(cond) = cond_expr else { panic!("expected expr") };
        interp.add_breakpoint(Breakpoint::Conditional(cond_line, cond));

        interp.run_statements_with_lines(&program).unwrap();

        assert_eq!(*hits.lock().unwrap(), 1, "should only pause once x has actually reached 3");
    }

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

    #[test]
    fn parallel_map_falls_back_to_sequential_when_debug_hook_is_set() {
        use crate::debug::{DebugAction, DebugContext, DebugHook};
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
        let stmts = crate::parser::parse_phs(
            "fn double(x) = x * 2.0\nres = parallel_map(double, vector(1.0, 2.0, 3.0))",
        )
        .unwrap();
        interp.run_statements(&stmts).unwrap();

        // Sequential execution means the hook is called in a well-defined, deterministic order
        // (three checkpoints, one per element, each dispatched from the same thread) rather than
        // racing across rayon workers -- this is what "sequential fallback" is actually testing:
        // not just that the result is right (parallel_map's own Track B tests already prove that),
        // but that debugging one didn't need `DebugHook: Sync`-across-threads reasoning at all.
        //
        // `run_statements` (unlike `run_statements_with_lines`) always checkpoints each
        // top-level statement at line 0 (see `eval_statement_with_env`'s `eval_statement_with_env_at(stmt, env, 0)`),
        // so the two top-level statements here (the `fn double` definition and the
        // `res = ...` assignment) each contribute one line-0 checkpoint that has nothing to do
        // with parallel_map's fallback. The checkpoints that actually matter -- one per element,
        // fired from inside `double`'s body via `call_function_node_at` -- land on line 1
        // (`double` is a one-line `fn double(x) = x * 2.0`, so its `body_lines[0]` is that same
        // line, never 0). Filter those out explicitly rather than asserting a raw total of 5,
        // so the "one checkpoint per element" property stays the thing under test.
        let seen = seen.lock().unwrap();
        let per_element_checkpoints = seen.iter().filter(|&&l| l != 0).count();
        assert_eq!(per_element_checkpoints, 3, "expected one checkpoint per double() call, got {seen:?}");
        // Also assert output correctness survives -- the real regression this guards is a future
        // change to parallel_map's rayon path forgetting this check, not today's behavior.
        let PhsValue::Vector(v) = interp.get_var("res").unwrap() else { panic!("expected vector") };
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn for_expr_falls_back_to_sequential_when_debug_hook_is_set() {
        use crate::debug::{DebugAction, DebugContext, DebugHook};
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
        // Force the parallel branch (threshold 0) so this test would exercise the rayon path if
        // the fallback below didn't override it.
        let _guard = physure_core::settings::scoped(0);
        let stmts = crate::parser::parse_phs(
            "fn double(x) = x * 2.0\nres = for i in vector(1.0, 2.0, 3.0) { double(i) }",
        )
        .unwrap();
        interp.run_statements(&stmts).unwrap();

        let PhsValue::Vector(v) = interp.get_var("res").unwrap() else { panic!("expected vector") };
        assert_eq!(v.len(), 3);
        // Same reasoning as parallel_map's fallback test: a deterministic, same-thread checkpoint
        // count is what "fell back to sequential" actually proves, not just correct output.
        assert!(!seen.lock().unwrap().is_empty(), "hook should have been called for double()'s body");
    }

    #[test]
    fn requires_violation_returns_contract_violation_error() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str(
                "@requires(m > 0.0, \"mass must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(-1.0)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"));
    }

    #[test]
    fn requires_satisfied_returns_normally() {
        let mut interp = PhsInterpreter::default();
        let results = interp
            .eval_str(
                "@requires(m > 0.0, \"mass must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(1.0)",
            )
            .unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 2.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 2.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn ensures_violation_returns_contract_violation_error() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str(
                "@ensures(result > 100.0, \"result must exceed 100\")\nfn small(m) = m\nsmall(1.0)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "ensures"));
    }

    #[test]
    fn ensures_satisfied_returns_normally() {
        let mut interp = PhsInterpreter::default();
        let results = interp
            .eval_str(
                "@ensures(result > 0.0, \"result must be positive\")\nfn small(m) = m\nsmall(1.0)",
            )
            .unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 1.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 1.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn requires_and_ensures_together_compose_independently() {
        let mut interp = PhsInterpreter::default();

        // Both satisfied: m=5.0 passes @requires (m > 0.0), result=10.0 passes @ensures (result > 0.0)
        let results = interp
            .eval_str(
                "@requires(m > 0.0, \"m must be positive\")\n@ensures(result > 0.0, \"result must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(5.0)",
            )
            .unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 10.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 10.0),
            other => panic!("expected numeric value, got {other:?}"),
        }

        // @requires fails independently: m=-1.0 violates @requires before @ensures is ever checked
        let mut interp2 = PhsInterpreter::default();
        let err = interp2
            .eval_str(
                "@requires(m > 0.0, \"m must be positive\")\n@ensures(result > 0.0, \"result must be positive\")\nfn double_mass(m) = m * 2.0\ndouble_mass(-1.0)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"));

        // @ensures fails independently: m=0.5 passes @requires but body output (0.5) fails @ensures's `result > 1.0`
        let mut interp3 = PhsInterpreter::default();
        let err = interp3
            .eval_str(
                "@requires(m > 0.0, \"m must be positive\")\n@ensures(result > 1.0, \"result must exceed 1\")\nfn identity(m) = m\nidentity(0.5)",
            )
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "ensures"));
    }

    #[test]
    fn range_lowered_to_requires_is_enforced_at_call_time() {
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str("@range(v, 0.0, 10.0)\nfn identity(v) = v\nidentity(20.0)")
            .unwrap_err();
        assert!(matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"));
    }

    #[test]
    fn assert_passes_when_dimensions_and_magnitude_agree() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("assert(1.0 km, 1000.0 m)").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn assert_fails_with_assertion_failed_error_on_mismatch() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(1.0 m, 1.0 s)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }

    #[test]
    fn exact_assert_passes_for_alias_units() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("exact_assert(5.0 m, 5.0 meter)").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn exact_assert_fails_when_conversion_would_be_required() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("exact_assert(1.0 km, 1000.0 m)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "exact_assert", .. }));
    }

    #[test]
    fn assert_rejects_non_quantity_arguments() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(\"a\", \"b\")").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
    }

    #[test]
    fn stable_and_experimental_decorators_do_not_affect_evaluation() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("@stable\nfn f(x) = x * 2.0\nf(3.0)").unwrap();
        match results.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 6.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 6.0),
            other => panic!("expected numeric value, got {other:?}"),
        }

        let mut interp2 = PhsInterpreter::default();
        let results2 = interp2.eval_str("@experimental\nfn g(x) = x * 3.0\ng(2.0)").unwrap();
        match results2.last().unwrap() {
            PhsValue::Number(n) => assert_eq!(*n, 6.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 6.0),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn an_asymmetric_measurement_refuses_instead_of_using_half_of_it() {
        // The notation parses so the grammar is settled, but nothing propagates a third
        // moment yet. Evaluating it would keep the upper half and report a symmetric
        // measurement — the one answer the notation exists to avoid.
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("x = 12.3 +/- (0.5, 0.4) m").unwrap_err();
        assert!(err.to_string().contains("cannot be evaluated yet"), "{err}");

        interp.eval_str("y = 12.3 +/- 0.5 m").expect("a symmetric measurement still evaluates");
    }

    #[test]
    fn test_unary_minus_multiline_if_and_guard_return() {
        let mut interp = PhsInterpreter::default();
        let code = r#"
CERO_A = 0 A
CERO_V = 0 V

circuito_abierto(V: V, I: A) =
    if I == CERO_A
    then return 0
    if V == CERO_V
    then
        - 1
    else
        1

r1 = circuito_abierto(5 V, 0 A)
r2 = circuito_abierto(0 V, 2 A)
r3 = circuito_abierto(5 V, 2 A)
"#;
        interp.eval_str(code).unwrap();
        let num = |name: &str| match interp.get_var(name).unwrap() {
            PhsValue::Quantity(q) => q.value.mean(),
            PhsValue::Number(n) => *n,
            other => panic!("expected numeric value for {name}, got {other:?}"),
        };
        assert_eq!(num("r1"), 0.0);
        assert_eq!(num("r2"), -1.0);
        assert_eq!(num("r3"), 1.0);
    }

    #[test]
    fn test_round_honors_decimals_argument() {
        // Numeric literals evaluate to a dimensionless PhsValue::Quantity (not
        // PhsValue::Number), so round()'s decimals argument must accept that
        // shape too, or it silently falls back to 0 decimals.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("x = round(3.14159, 2)").unwrap();
        match interp.get_var("x").unwrap() {
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 3.14),
            PhsValue::Number(n) => assert_eq!(*n, 3.14),
            other => panic!("expected numeric value, got {other:?}"),
        }
    }

    #[test]
    fn test_kinetic_energy() {
        let mut interp = PhsInterpreter::default();
        
        let statements = vec![
            Statement::FunctionDef(FunctionDefNode {
                name: "kinetic_energy".to_string(),
                params: vec!["m".to_string(), "v".to_string()],
                param_units: vec![None, None],
                body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 0.5,
                            uncertainty: None,
                            uncertainty_lower: None,
                            is_sigma: false,
                            unit: None,
                        })),
                        right: Box::new(Expr::Identifier("m".to_string())),
                    }),
                    right: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Pow,
                        left: Box::new(Expr::Identifier("v".to_string())),
                        right: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 2.0,
                            uncertainty: None,
                            uncertainty_lower: None,
                            is_sigma: false,
                            unit: None,
                        })),
                    })
                })],
                body_lines: vec![],
                decorators: Vec::new(),
                doc: None,
            }),
            Statement::Assignment(AssignmentNode {
                name: "m".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 10.0,
                    uncertainty: None,
                    uncertainty_lower: None,
                    is_sigma: false,
                    unit: Some("kg".to_string()),
                }),
                decorators: Vec::new(),
            }),
            Statement::Assignment(AssignmentNode {
                name: "v".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 2.0,
                    uncertainty: None,
                    uncertainty_lower: None,
                    is_sigma: false,
                    unit: Some("m/s".to_string()),
                }),
                decorators: Vec::new(),
            }),
            Statement::Assignment(AssignmentNode {
                name: "E".to_string(),
                value: Expr::FunctionCall {
                    name: "kinetic_energy".to_string(),
                    args: vec![
                        Expr::Identifier("m".to_string()),
                        Expr::Identifier("v".to_string()),
                    ],
                    kwargs: Vec::new(),
                },
                decorators: Vec::new(),
            }),
        ];
        
        let env = interp.eval_program(&Program { statements, lines: vec![] }).unwrap();
        
        let e_val = env.get("E").unwrap();
        if let PhsValue::Quantity(q) = e_val {
            assert_eq!(q.value.mean(), 20.0);
            
            // Check that it's equivalent to 20 J
            let parsed_j = UnitParser::parse_expression("J").unwrap();
            assert!(q.unit.same_dimensions(&parsed_j));
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_uncertainty_propagation() {
        let mut interp = PhsInterpreter::default();
        let program = Program {
            statements: vec![
                Statement::Assignment(AssignmentNode {
                    name: "m".to_string(),
                    value: Expr::Quantity(QuantityNode {
                        magnitude: 75.0,
                        uncertainty: Some(0.5),
                        uncertainty_lower: None,
                        is_sigma: false,
                        unit: Some("kg".to_string()),
                    }),
                    decorators: Vec::new(),
                }),
            ],
            lines: vec![],
        };
        let env = interp.eval_program(&program).unwrap();
        let m_val = env.get("m").unwrap();
        if let PhsValue::Quantity(q) = m_val {
            assert_eq!(q.value.mean(), 75.0);
            assert_eq!(q.value.std_dev(), 0.5);
            assert_eq!(q.unit.__repr__(), "kg");
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_register_fn_dispatch() {
        let mut interp = PhsInterpreter::default();
        interp.register_fn("double", |args: &[PhsValue]| match args.first() {
            Some(PhsValue::Quantity(q)) => Ok(PhsValue::Number(q.value.mean() * 2.0)),
            Some(PhsValue::Number(n)) => Ok(PhsValue::Number(n * 2.0)),
            _ => Err(PhysureError::Generic("double expects a number".into())),
        });

        let results = interp.eval_str("double(21)").unwrap();
        assert_eq!(results, vec![PhsValue::Number(42.0)]);
    }

    #[test]
    fn test_prime_function_call_eval() {
        let mut interp = PhsInterpreter::default();
        interp.eval_str("f(x) = x^3 - 3*x").unwrap();
        
        let res_symbolic = interp.eval_str("f'(x)").unwrap();
        assert_eq!(res_symbolic, vec![PhsValue::String("3 * x^2 - 3".to_string())]);

        let res_num1 = interp.eval_str("f'(2)").unwrap();
        match &res_num1[0] {
            PhsValue::Number(n) => assert_eq!(*n, 9.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 9.0),
            _ => panic!("Expected number or quantity"),
        }

        let res_num2 = interp.eval_str("f''(2)").unwrap();
        match &res_num2[0] {
            PhsValue::Number(n) => assert_eq!(*n, 12.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 12.0),
            _ => panic!("Expected number or quantity"),
        }
    }

    #[test]
    fn test_virtual_module_import() {
        let mut resolver = MemoryModuleResolver::new();
        let mut export = ModuleExport {
            symbols: HashMap::new(),
            functions: HashMap::new(),
        };
        export.symbols.insert("G".to_string(), Expr::Quantity(QuantityNode {
            magnitude: 6.674e-11,
            uncertainty: None,
            uncertainty_lower: None,
            is_sigma: false,
            unit: Some("m^3 / (kg * s^2)".to_string()),
        }));
        resolver.add_module("constants".to_string(), export);
        
        let mut interp = PhsInterpreter::new(Arc::new(resolver));
        let program = Program {
            statements: vec![
                Statement::Import(ImportNode {
                    path: "constants".to_string(),
                    specifier: ImportSpecifier::Wildcard,
                })
            ],
            lines: vec![],
        };
        let env = interp.eval_program(&program).unwrap();
        let g_val = env.get("G").unwrap();
        if let PhsValue::Quantity(q) = g_val {
            assert_eq!(q.value.mean(), 6.674e-11);
        } else {
            panic!("Expected quantity");
        }
    }

    fn assert_is_five(val: &PhsValue) {
        match val {
            PhsValue::Number(n) => assert_eq!(*n, 5.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 5.0),
            PhsValue::String(s) => assert_eq!(s, "5"),
            other => panic!("Expected solve(...) to resolve to 5, got {:?}", other),
        }
    }

    #[test]
    fn test_callable_equation_solves_when_unknown_lands_on_lhs() {
        // "R = V / I" * "I" simplifies to "I * R = V" -- the unknown (V) ends
        // up on the RHS-of-assignment side, not the LHS. Calling with I and R
        // bound must still solve for V by evaluating the fully-bound side.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("var = \"R = V / I\" * \"I\"").unwrap();
        let results = interp.eval_str("var(I = 3 A, R = 3 Ohm)").unwrap();
        match &results[0] {
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 9.0),
            other => panic!("Expected quantity, got {:?}", other),
        }
    }

    #[test]
    fn test_domain_gated_calc_builtins() {
        // Ungated call fails with "Undefined function".
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str("solve(\"2 * x = 10\", \"x\")")
            .unwrap_err();
        assert!(
            err.to_string().contains("Undefined function"),
            "unexpected error: {}",
            err
        );

        // `use solve from calc` unlocks it.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("use solve from calc").unwrap();
        let results = interp.eval_str("solve(\"2 * x = 10\", \"x\")").unwrap();
        assert_is_five(&results[0]);

        // `use * from calc` unlocks every member.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("use * from calc").unwrap();
        interp.eval_str("deriv(\"x^2\", \"x\")").unwrap();
        interp.eval_str("integral(\"2 * x\", \"x\")").unwrap();
        let results = interp.eval_str("solve(\"2 * x = 10\", \"x\")").unwrap();
        assert_is_five(&results[0]);

        // Requesting an unknown domain member errors clearly.
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("use bogus_fn from calc").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no such function"), "unexpected error: {}", msg);
        assert!(msg.contains("in domain"), "unexpected error: {}", msg);
    }

    #[test]
    fn test_exhaustive_interpreter_features() {
        let mut interp = PhsInterpreter::default();
        interp.eval_str("f(x) = sin(x)").unwrap();
        
        assert_eq!(interp.eval_str("f'(x)").unwrap()[0], PhsValue::String("cos(x)".into()));
        assert_eq!(interp.eval_str("f''(x)").unwrap()[0], PhsValue::String("-1 * sin(x)".into()));
        
        match &interp.eval_str("f'(0)").unwrap()[0] {
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 1.0),
            _ => panic!("Expected quantity"),
        }
        
        match &interp.eval_str("f''(3)").unwrap()[0] {
            PhsValue::Quantity(q) => assert!((q.value.mean() - -0.1411200080598672).abs() < 1e-10),
            _ => panic!("Expected quantity"),
        }
        
        // function composition with derivatives
        interp.eval_str("g(x) = sin(cos(x))").unwrap();
        let res = interp.eval_str("g'(x)").unwrap();
        assert_eq!(res[0], PhsValue::String("cos(cos(x)) * -1 * sin(x)".into()));
        
        // solve multi-variable
        interp.eval_str("use solve from calc").unwrap();
        let res = interp.eval_str("solve(\"2*x + 3 = 11\", \"x\")").unwrap();
        match &res[0] {
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 4.0),
            _ => panic!("Expected quantity 4"),
        }
        
        // uncertainty propagation through trig and exp
        interp.eval_str("u = 0 +/- 0.1").unwrap();
        let res_sin = interp.eval_str("sin(u)").unwrap();
        match &res_sin[0] {
            PhsValue::Quantity(q) => {
                assert_eq!(q.value.mean(), 0.0);
                assert_eq!(q.value.std_dev(), 0.1);
            }
            _ => panic!("Expected quantity"),
        }
        
        interp.eval_str("v = 1 +/- 0.1").unwrap();
        let res_exp = interp.eval_str("exp(v)").unwrap();
        match &res_exp[0] {
            PhsValue::Quantity(q) => {
                assert!((q.value.mean() - 2.718281828459045).abs() < 1e-10);
                assert!((q.value.std_dev() - 0.27182818284590454).abs() < 1e-10);
            }
            _ => panic!("Expected quantity"),
        }
    }

    #[test]
    fn test_interpreter_for_expr() {
        let mut interp = PhsInterpreter::default();
        let stmts = crate::parser::parse_phs("res = for i in 1..4 { i * 2 }").unwrap();
        interp.run_statements(&stmts).unwrap();
        let val = interp.get_var("res").unwrap();
        assert!(matches!(val, PhsValue::Vector(_)));
    }

    #[test]
    fn test_interpreter_for_expr_large_scale() {
        let mut interp = PhsInterpreter::default();
        let stmts = crate::parser::parse_phs("res = for i in 1..100000 { i + 1 }").unwrap();
        interp.run_statements(&stmts).unwrap();
        let val = interp.get_var("res").unwrap();
        if let PhsValue::Vector(v) = val {
            assert_eq!(v.len(), 99999);
        } else {
            panic!("expected vector");
        }
    }

    #[test]
    fn for_expr_parallel_and_sequential_paths_agree() {
        let script = "res = for i in 1..20000 { i * 3 + 1 }";
        let stmts = crate::parser::parse_phs(script).unwrap();

        let seq_val = {
            let _guard = physure_core::settings::scoped(usize::MAX);
            let mut interp_seq = PhsInterpreter::default();
            interp_seq.run_statements(&stmts).unwrap();
            interp_seq.get_var("res").unwrap().clone()
        };

        let par_val = {
            let _guard = physure_core::settings::scoped(0);
            let mut interp_par = PhsInterpreter::default();
            interp_par.run_statements(&stmts).unwrap();
            interp_par.get_var("res").unwrap().clone()
        };

        assert_eq!(seq_val, par_val);
    }

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

    #[test]
    fn test_interpreter_while_loop_and_max_iter() {
        let mut interp = PhsInterpreter::default();
        let stmts = crate::parser::parse_phs("i = 0\nwhile i < 5 { i = i + 1 }").unwrap();
        interp.run_statements(&stmts).unwrap();
        let val = interp.get_var("i").unwrap();
        assert_eq!(val.to_string(), "5.0");

        let infinite = crate::parser::parse_phs("i = 0\nwhile true { i = i + 1 }").unwrap();
        assert!(interp.run_statements(&infinite).is_err());
    }

    #[test]
    fn test_is_truthy_negative_numbers() {
        let mut interp = PhsInterpreter::default();
        let stmts = crate::parser::parse_phs("i = -5\ncount = 0\nwhile i { count = count + 1\n i = i + 1 }").unwrap();
        assert!(interp.run_statements(&stmts).is_ok());
    }
}
