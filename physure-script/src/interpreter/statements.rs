use std::collections::HashMap;
use std::sync::Arc;
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::{Program, Statement};
use crate::value::PhsValue;
use crate::debug::{DebugAction, DebugContext};
use super::{PhsInterpreter, ExternalFn};
use super::helpers::is_truthy;

/// Tracks what a `Step*`/`Pause` `DebugAction` committed the interpreter to doing next, so a
/// later `debug_checkpoint` call can decide whether to fire the hook even when no `Breakpoint`
/// matches -- this is what actually makes `step`/`next`/`finish` do something once at least one
/// breakpoint is registered (previously the `DebugAction` a hook returned was thrown away
/// entirely, so those commands were indistinguishable from `Continue`). `None` means "no step
/// pending" -- either nothing has been returned yet, or the last action was `Continue`.
#[derive(Clone, Copy)]
pub(crate) enum StepMode {
    /// Fire on the very next checkpoint, whatever its call-stack depth.
    Into,
    /// Fire once `call_stack` depth is back down to (or shallower than) the depth recorded when
    /// this was issued -- skips over anything deeper (a nested call).
    Over(usize),
    /// Fire once `call_stack` depth is strictly shallower than the depth recorded when this was
    /// issued -- i.e. only after the current frame has actually returned.
    Out(usize),
}

impl PhsInterpreter {
    pub fn eval_str(&mut self, code: &str) -> PhysureResult<Vec<PhsValue>> {
        let prog = crate::parse_phs(code)?;
        let mut results = Vec::new();
        for stmt in &prog.statements {
            results.push(self.eval_statement(stmt)?);
        }
        Ok(results)
    }

    pub fn run_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        self.eval_statement(stmt)
    }

    pub fn run_statements(&mut self, program: &Program) -> PhysureResult<PhsValue> {
        let mut last = PhsValue::None;
        for stmt in &program.statements {
            last = self.eval_statement(stmt)?;
        }
        Ok(last)
    }

    /// Like `run_statements`, but executes against `program.lines` so `debug_checkpoint` sees
    /// real source lines instead of `0`. This is what `phs debug` uses; `run_statements` stays
    /// as-is for every other caller (Python/WASM/Java bindings, the plain REPL) that doesn't
    /// have line-accurate debugging as a goal.
    pub fn run_statements_with_lines(&mut self, program: &Program) -> PhysureResult<PhsValue> {
        let mut env = self.env.clone();
        let mut last = PhsValue::None;
        for (i, stmt) in program.statements.iter().enumerate() {
            let line = program.lines.get(i).copied().unwrap_or(0);
            last = self.eval_statement_with_env_at(stmt, &mut env, line)?;
        }
        self.env = env;
        Ok(last)
    }

    pub fn get_var(&self, name: &str) -> Option<&PhsValue> {
        self.env.get(name)
    }

    pub fn env(&self) -> &HashMap<String, PhsValue> {
        &self.env
    }

    /// Test-only: `call_stack` stays private (not part of the public debugger API C1 defines --
    /// a debugger observes it indirectly, through `DebugContext::call_stack` in `on_statement`),
    /// but a leak regression test needs to assert its depth from outside any hook callback,
    /// after an error has already propagated back out. Compiled out of production builds.
    #[cfg(test)]
    pub(crate) fn call_stack_depth(&self) -> usize {
        self.call_stack.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn get_fn_params(&self, name: &str) -> Option<Vec<String>> {
        if let Some(PhsValue::Function(f)) = self.env.get(name) {
            Some(f.params.clone())
        } else {
            None
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> PhysureResult<HashMap<String, PhsValue>> {
        for stmt in &program.statements {
            self.eval_statement(stmt)?;
        }
        Ok(self.env.clone())
    }

    pub fn add_breakpoint(&self, bp: crate::debug::Breakpoint) {
        let mut guard = self.breakpoints.lock().unwrap_or_else(|e| e.into_inner());
        let mut updated = (**guard).clone();
        updated.push(bp);
        *guard = Arc::new(updated);
    }

    pub(crate) fn debug_checkpoint(&self, line: usize, env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
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
        for bp in &*breakpoints {
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

    pub fn eval_statement_with_env(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        self.eval_statement_with_env_at(stmt, env, 0)
    }

    pub(crate) fn eval_statement_with_env_at(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>, line: usize) -> PhysureResult<PhsValue> {
        self.debug_checkpoint(line, env)?;
        match stmt {
            Statement::Assignment(node) => {
                let val = self.eval_expr(&node.value, env)?;
                env.insert(node.name.clone(), val.clone());
                Ok(val)
            }
            Statement::FunctionDef(node) => {
                env.insert(node.name.clone(), PhsValue::Function(node.clone()));
                Ok(PhsValue::None)
            }
            Statement::Expr(expr) => {
                self.eval_expr(expr, env)
            }
            Statement::Import(node) => self.resolve_use(node, env),
            Statement::Export(_node) => Ok(PhsValue::None),
            Statement::Return(expr) => self.eval_expr(expr, env),
            Statement::GuardReturn { cond, value } => {
                let cond_val = self.eval_expr(cond, env)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(value, env)
                } else {
                    Ok(PhsValue::None)
                }
            }
            Statement::While { cond, body, body_lines } => {
                const DEFAULT_MAX_LOOP_ITERATIONS: usize = 10_000;
                let mut count = 0;
                let mut last_val = PhsValue::None;
                while is_truthy(&self.eval_expr(cond, env)?) {
                    if count >= DEFAULT_MAX_LOOP_ITERATIONS {
                        return Err(PhysureError::Generic(format!(
                            "while loop did not converge after {} iterations",
                            DEFAULT_MAX_LOOP_ITERATIONS
                        )));
                    }
                    count += 1;
                    for (i, stmt) in body.iter().enumerate() {
                        let line = body_lines.get(i).copied().unwrap_or(0);
                        last_val = self.eval_statement_with_env_at(stmt, env, line)?;
                    }
                }
                Ok(last_val)
            }
        }
    }

    /// Resolves a `use` statement against, in order: builtin domains (`core` is
    /// always on, so this only matters for `calc`/`plot`/`array`), `.phs` modules,
    /// and native plugin stems (`<base_dir>/ext/<stem>.<DLL_EXTENSION>`, dlopen'd lazily).
    fn resolve_use(&self, node: &crate::ast::ImportNode, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        use crate::ast::ImportSpecifier;

        if let Some(members) = crate::builtins::domain_members(&node.path) {
            let domain: &'static str = match node.path.as_str() {
                "calc" => "calc",
                "plot" => "plot",
                "array" => "array",
                _ => unreachable!("domain_members returned Some for unknown domain"),
            };
            let mut unlocked = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner());
            match &node.specifier {
                ImportSpecifier::Wildcard => {
                    for &member in members {
                        unlocked.insert(member.to_string(), (domain, member.to_string()));
                    }
                }
                ImportSpecifier::Symbols(syms) => {
                    for sym in syms {
                        if !members.contains(&sym.name.as_str()) {
                            return Err(PhysureError::Generic(format!("no such function '{}' in domain '{}'", sym.name, node.path)));
                        }
                        let call_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                        unlocked.insert(call_name, (domain, sym.name.clone()));
                    }
                }
                ImportSpecifier::ModuleAlias(_alias) => {
                    return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
                }
            }
            return Ok(PhsValue::None);
        }

        if let Ok(export) = self.resolver.resolve(&node.path) {
            match &node.specifier {
                ImportSpecifier::Wildcard => {
                    for (name, expr) in export.symbols {
                        let val = self.eval_expr(&expr, env)?;
                        env.insert(name, val);
                    }
                    for (name, func) in export.functions {
                        env.insert(name, PhsValue::Function(func));
                    }
                }
                ImportSpecifier::Symbols(syms) => {
                    for sym in syms {
                        if let Some(expr) = export.symbols.get(&sym.name) {
                            let val = self.eval_expr(expr, env)?;
                            let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                            env.insert(target_name, val);
                        } else if let Some(func) = export.functions.get(&sym.name) {
                            let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                            env.insert(target_name, PhsValue::Function(func.clone()));
                        } else {
                            return Err(PhysureError::Generic(format!("Symbol {} not found in module {}", sym.name, node.path)));
                        }
                    }
                }
                ImportSpecifier::ModuleAlias(_alias) => {
                    return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
                }
            }
            return Ok(PhsValue::None);
        }

        if let Some(base_dir) = &self.plugin_base_dir {
            let plugin_state = self.plugin_state.clone();
            let mut state = plugin_state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(functions) = state.ensure_stem_loaded(base_dir, &node.path)? {
                self.install_dynamic_externals(&node.specifier, &node.path, &functions)?;
                return Ok(PhsValue::None);
            }
        }

        Err(PhysureError::Generic(format!("No such module or domain '{}'", node.path)))
    }

    /// Installs the requested (possibly aliased) subset of `functions` into
    /// `dynamic_externals`, erroring if a requested name isn't exported.
    fn install_dynamic_externals(&self, specifier: &crate::ast::ImportSpecifier, module: &str, functions: &HashMap<String, ExternalFn>) -> PhysureResult<()> {
        use crate::ast::ImportSpecifier;
        let mut dynamic = self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner());
        match specifier {
            ImportSpecifier::Wildcard => {
                for (name, f) in functions {
                    dynamic.insert(name.clone(), f.clone());
                }
            }
            ImportSpecifier::Symbols(syms) => {
                for sym in syms {
                    let f = functions.get(&sym.name).ok_or_else(|| {
                        PhysureError::Generic(format!("Symbol {} not found in module {}", sym.name, module))
                    })?;
                    let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                    dynamic.insert(target_name, f.clone());
                }
            }
            ImportSpecifier::ModuleAlias(_alias) => {
                return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
            }
        }
        Ok(())
    }

    pub fn eval_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        let mut env = self.env.clone();
        let res = self.eval_statement_with_env(stmt, &mut env)?;
        self.env = env;
        Ok(res)
    }

}
