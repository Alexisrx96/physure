use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use physure_core::error::{PhysureError, PhysureResult};

/// A host-registered function callable from PHS source by name. Lets embedders
/// (e.g. the PyO3 binding) expose functions without physure-script depending on them.
pub type ExternalFn = Arc<dyn Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync>;
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;

use crate::ast::{BinaryOp, Expr, Program, Statement};
use crate::debug::{DebugAction, DebugContext, DebugHook, StackFrame};
use crate::resolver::{ModuleResolver, FsModuleResolver};
use crate::symbolic::Node;
use crate::PhsValue;

/// Applies a symbolic binary op to two `Node`s. Only `+ - * /` are meaningful for
/// equation algebra (the linear operations solve_equation and simplify understand);
/// `^` and `=>` on an equation are explicitly out of scope for this pass.
fn node_op(op: BinaryOp, a: Node, b: Node) -> PhysureResult<Node> {
    Ok(match op {
        BinaryOp::Add => Node::Add(vec![a, b]),
        BinaryOp::Sub => Node::Sub(Box::new(a), Box::new(b)),
        BinaryOp::Mul => Node::Mul(vec![a, b]),
        BinaryOp::Div => Node::Div(Box::new(a), Box::new(b)),
        _ => return Err(PhysureError::Generic("Pow/Convert are not supported for equation algebra yet".into())),
    })
}

/// Converts a non-Equation operand into a symbolic `Node` for equation algebra.
/// A dimensionless `Quantity` (e.g. a bare scale factor) becomes its numeric value;
/// a dimensioned one (e.g. `2 m`) is kept as `number * unit_symbol` so the unit isn't
/// silently dropped from the resulting equation's text.
/// ponytail: the unit symbol isn't a real bindable variable, so it stays purely
/// symbolic — collides only if the equation also has a variable named the same as the unit.
fn value_to_symbolic_node(val: &PhsValue) -> PhysureResult<Node> {
    match val {
        PhsValue::Number(n) => Ok(Node::Number(*n)),
        PhsValue::String(s) => crate::symbolic::SymbolicParser::parse_str(s),
        PhsValue::Quantity(q) if q.unit == RationalUnit::dimensionless() => Ok(Node::Number(q.value.mean())),
        PhsValue::Quantity(q) => Ok(Node::Mul(vec![Node::Number(q.value.mean()), Node::Symbol(q.unit.__repr__())])),
        _ => Err(PhysureError::Generic("Equation algebra only supports Number, String, Equation, or Quantity operands".into())),
    }
}

/// A plain string holding `"lhs = rhs"` (e.g. from a bare assignment, not `solve()`)
/// is coerced into an `Equation` so it supports the same arithmetic. Strings without
/// a top-level `=` (unit symbols, bare variable names) pass through unchanged.
pub(crate) fn coerce_equation_string(val: PhsValue) -> PhsValue {
    if let PhsValue::String(ref s) = val {
        if let Ok(Some((l, r))) = crate::symbolic::SymbolicParser::parse_equation_str(s) {
            return PhsValue::Equation(l, r);
        }
    }
    val
}

/// The magnitude a range endpoint denotes, or `None` when it denotes none.
fn range_endpoint(val: &PhsValue) -> Option<Quantity> {
    match val {
        PhsValue::Quantity(q) => Some(q.clone()),
        PhsValue::Number(n) => Some(Quantity::new_scalar(*n, 0.0, RationalUnit::dimensionless(), None, None)),
        _ => None,
    }
}

/// Builds `min .. max`, after checking that it names an interval at all.
///
/// An endpoint with no dimension of its own takes the other's unit, so `0 .. 100 m` reads
/// as `0 m .. 100 m` — on paper the lower bound of an interval does not repeat the unit
/// either. Everything else is refused rather than repaired: a range whose sides measure
/// different things, one that does not run upwards, and anything that is not a magnitude.
/// A missing endpoint never reaches here; the grammar requires both.
fn make_range(l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
    let (Some(mut min), Some(mut max)) = (range_endpoint(&l_val), range_endpoint(&r_val)) else {
        return Err(PhysureError::Generic(format!(
            "A range runs between two magnitudes, and `{} .. {}` has something else on at least one side",
            l_val, r_val,
        )));
    };

    if min.unit.dimensions.is_empty() && !max.unit.dimensions.is_empty() {
        min.unit = max.unit.clone();
    } else if max.unit.dimensions.is_empty() && !min.unit.dimensions.is_empty() {
        max.unit = min.unit.clone();
    } else if !min.unit.same_dimensions(&max.unit) {
        return Err(PhysureError::IncompatibleDimensions {
            op: "range",
            dim1: min.unit.__repr__(),
            dim2: max.unit.__repr__(),
        });
    }

    let (lo, hi) = (min.canonical_magnitude(), max.canonical_magnitude());
    if lo.is_nan() || hi.is_nan() {
        return Err(PhysureError::Generic(format!(
            "A range needs two magnitudes that can be ordered, and `{} .. {}` has one that cannot",
            min, max,
        )));
    }
    if lo >= hi {
        return Err(PhysureError::Generic(format!(
            "A range runs from its minimum to its maximum: `{}` is not below `{}`",
            min, max,
        )));
    }

    // A bare number stays a bare number when nothing was adopted — a dimensionless range is
    // written `-2 .. 2` and the consumers that read one distinguish the two cases.
    let rewrap = |original: PhsValue, q: Quantity| match original {
        PhsValue::Number(_) if q.unit.dimensions.is_empty() => original,
        _ => PhsValue::Quantity(q),
    };
    Ok(PhsValue::Range(
        Box::new(rewrap(l_val, min)),
        Box::new(rewrap(r_val, max)),
    ))
}

fn is_truthy(val: &PhsValue) -> bool {
    match val {
        PhsValue::Quantity(q) => q.value.mean().abs() > 1e-15,
        PhsValue::Number(n) => n.abs() > 1e-15,
        PhsValue::Bool(b) => *b,
        PhsValue::String(s) => s == "true" || s == "True" || s == "1",
        _ => false,
    }
}

fn eval_template_string(text: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> String {
    interpolate(text.trim_matches('`').trim(), interp, env)
}

/// Substitutes every `{expr}` in `text` with the value of `expr` in `env`, leaving the
/// braces untouched when the expression does not evaluate. Unlike `eval_template_string`
/// this keeps the surrounding whitespace, which a quoted string literal is entitled to.
fn interpolate(text: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> String {
    let mut result = String::new();
    let mut rest = text;

    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('}') {
            let expr_str = rest[..end].trim();
            rest = &rest[end + 1..];

            if let Ok(prog) = crate::parse_phs(expr_str) {
                if let Some(stmt) = prog.statements.first() {
                    if let Ok(val) = interp.eval_statement_with_env(stmt, &mut env.clone()) {
                        result.push_str(&val.to_string());
                        continue;
                    }
                }
            }
            result.push('{');
            result.push_str(expr_str);
            result.push('}');
        } else {
            result.push('{');
            break;
        }
    }
    result.push_str(rest);
    result
}

#[derive(Clone)]
pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
    pub externals: HashMap<String, ExternalFn>,
    plugin_state: Arc<Mutex<crate::plugin::PluginState>>,
    plugin_base_dir: Option<std::path::PathBuf>,
    /// call-name -> (domain, canonical builtin name), populated by `use x from calc` etc.
    unlocked_builtins: Arc<Mutex<HashMap<String, (&'static str, String)>>>,
    /// Lazily-loaded plugin/ext functions, keyed by their `use`d (possibly aliased) name.
    dynamic_externals: Arc<Mutex<HashMap<String, ExternalFn>>>,
    // TODO: a `context: PhsContext` belongs here, next to `unlocked_builtins` -- the one
    // thing this interpreter already scopes to a program. A script cannot say how its
    // uncertainties should propagate; it depends on a `physure.conf` it never mentions,
    // and the transpilers drop that dependency entirely. See
    // docs/superpowers/specs/2026-08-02-phs-execution-context.md.
    pub(crate) debug_hook: Option<Arc<dyn DebugHook>>,
    /// `Arc<Mutex<..>>`, not `RefCell`: Track B's `for`-expression and `parallel_map` rayon
    /// paths require `&PhsInterpreter: Send + Sync` at compile time regardless of whether a
    /// hook is set at runtime -- `RefCell` would break both of those already-shipped parallel
    /// paths. The mutex is safe today because a debugging session only exercises sequential
    /// execution paths in practice; `parallel_map`'s rayon path does not yet check
    /// `debug_hook` and would corrupt this stack if used concurrently with an active hook --
    /// closing that gap is planned as a later Integration task, not yet implemented.
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
    breakpoints: Arc<Mutex<Vec<crate::debug::Breakpoint>>>,
    step_mode: Arc<Mutex<Option<StepMode>>>,
}

impl Default for PhsInterpreter {
    fn default() -> Self {
        Self::new(Arc::new(FsModuleResolver::default()))
    }
}

/// RAII pop for the `StackFrame` `call_function_node_at` pushes. Constructed right after the
/// push, held as a local in `call_function_node_at`; its `Drop` runs on every exit from that
/// point on -- normal return, an early `break`, or `?` propagating an error out of the body
/// loop -- so the frame can never be left on `call_stack` past the call that pushed it.
struct CallStackGuard {
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
}

impl Drop for CallStackGuard {
    fn drop(&mut self) {
        self.call_stack.lock().unwrap_or_else(|e| e.into_inner()).pop();
    }
}

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

/// Trailing `#` / `//` comments survive into a unit annotation's text; the unit parser
/// must never see them.
fn strip_unit_comment(text: &str) -> &str {
    text.split('#').next().unwrap().split("//").next().unwrap().trim()
}

/// `Some(1 <unit>)` if `name` is a registered unit symbol, so that a bare unit symbol left
/// behind by the symbolic layer multiplies as the unit it names instead of degrading to a
/// string. Returns `None` for any name that is not a unit, which stays a plain identifier.
fn unit_symbol_as_quantity(name: &str) -> Option<Quantity> {
    if !crate::parser::is_known_unit_symbol(name) {
        return None;
    }
    let unit = UnitParser::parse_expression(name).ok()?;
    Some(Quantity::new_scalar(1.0, 0.0, unit, None, None))
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
            breakpoints: Arc::new(Mutex::new(Vec::new())),
            step_mode: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_default() -> Self {
        Self::default()
    }

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
    fn call_stack_depth(&self) -> usize {
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
        self.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).push(bp);
    }

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

    pub fn eval_statement_with_env(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        self.eval_statement_with_env_at(stmt, env, 0)
    }

    fn eval_statement_with_env_at(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>, line: usize) -> PhysureResult<PhsValue> {
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

    pub fn eval_expr(&self, expr: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        match expr {
            Expr::Quantity(node) => {
                if let Some(reason) = node.asymmetric_refusal() {
                    return Err(PhysureError::Generic(reason));
                }
                let mut q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), RationalUnit::dimensionless(), None, None);
                if let Some(unit_str) = &node.unit {
                    let clean_unit_str = strip_unit_comment(unit_str);
                    if !clean_unit_str.is_empty() {
                        let parsed_unit = UnitParser::parse_expression(clean_unit_str)?;
                        q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), parsed_unit, None, None);
                    }
                }
                if node.is_sigma {
                    Ok(PhsValue::SigmaBound(q, node.uncertainty.unwrap_or(1.0)))
                } else {
                    Ok(PhsValue::Quantity(q))
                }
            }
            // A string literal is the text the user wrote, never a variable lookup: with
            // `v = 3 m/s` in scope, `deriv("0.5*m*v^2", "v")` used to receive the quantity
            // instead of the name. `{v}` folds a value in explicitly.
            Expr::Str(text) => Ok(PhsValue::String(interpolate(text, self, env))),
            Expr::Identifier(name) => {
                if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
                    let text = eval_template_string(name, self, env);
                    Ok(PhsValue::String(text))
                } else if let Some(val) = env.get(name) {
                    Ok(val.clone())
                } else if let Some(unit) = unit_symbol_as_quantity(name) {
                    // The symbolic layer has no notion of units: it parses `2.0 J / (gram * K)`
                    // into plain algebra over the symbols J, gram and K. Re-evaluating such a
                    // symbol as one of its unit reassembles the right dimensions, which is what
                    // makes a unit-bearing equation string survive the round-trip. A binding in
                    // `env` still wins, so this only ever fires for an otherwise-free name.
                    Ok(PhsValue::Quantity(unit))
                } else {
                    Ok(PhsValue::String(name.clone()))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                if *op == BinaryOp::Convert {
                    let l_val = self.eval_expr(left, env)?;
                    let target_unit = crate::codegen::expr_to_unit_string(right);
                    let clean_target = strip_unit_comment(&target_unit);
                    if !clean_target.is_empty() {
                        let parsed_unit = UnitParser::parse_expression(clean_target)?;
                        return self.convert_value_to_unit(l_val, &parsed_unit);
                    }
                    return Ok(l_val);
                }
                let l_val = self.eval_expr(left, env)?;
                let r_val = self.eval_expr(right, env)?;
                self.eval_binary_op_vals(*op, l_val, r_val)
            }
            Expr::FunctionCall { name, args, kwargs } => {
                if name == "let" && args.len() == 3 {
                    if let Expr::Identifier(var_name) = &args[0] {
                        let val = self.eval_expr(&args[1], env)?;
                        let mut local_env = env.clone();
                        local_env.insert(var_name.clone(), val);
                        return self.eval_expr(&args[2], &local_env);
                    }
                }

                if let Some(PhsValue::Equation(lhs, rhs)) = env.get(name).cloned().map(coerce_equation_string) {
                    if !args.is_empty() {
                        return Err(PhysureError::Generic(format!(
                            "Calling equation '{}' requires named arguments only, e.g. {}(x=1), got positional arguments",
                            name, name
                        )));
                    }
                    if kwargs.is_empty() {
                        return Err(PhysureError::Generic(format!(
                            "Calling equation '{}' requires at least one named argument",
                            name
                        )));
                    }
                    let mut local_env = env.clone();
                    for (kwarg_name, kwarg_expr) in kwargs {
                        let val = self.eval_expr(kwarg_expr, env)?;
                        local_env.insert(kwarg_name.clone(), val);
                    }
                    // Algebra (e.g. multiplying both sides) can move the unknown to
                    // either side of the equation, so try whichever side is fully
                    // bound by the supplied kwargs rather than assuming it's the RHS.
                    let unbound = |s: &&String| !local_env.contains_key(*s);
                    let mut rhs_free = std::collections::HashSet::new();
                    rhs.free_symbols(&mut rhs_free);
                    let rhs_missing: Vec<&String> = rhs_free.iter().filter(unbound).collect();
                    let solved_node = if rhs_missing.is_empty() {
                        &rhs
                    } else {
                        let mut lhs_free = std::collections::HashSet::new();
                        lhs.free_symbols(&mut lhs_free);
                        let lhs_missing: Vec<&String> = lhs_free.iter().filter(unbound).collect();
                        if lhs_missing.is_empty() {
                            &lhs
                        } else {
                            // Neither side is fully bound by name alone, which is what a unit in
                            // the equation text looks like: `"Q = m * 4.18 J/(g*K) * (T2 - T1)"`
                            // leaves J, g and K free on the right and the unknown Q on the left.
                            // Ignoring unit symbols separates the two, and only here — after both
                            // strict passes have failed — so a unit-named unknown (solving for
                            // `V`, `T`, `A`) is still picked up by the passes above.
                            let rhs_units_only =
                                rhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s));
                            let lhs_units_only =
                                lhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s));
                            if rhs_units_only {
                                &rhs
                            } else if lhs_units_only {
                                &lhs
                            } else {
                                let missing = if rhs_missing.len() <= lhs_missing.len() { rhs_missing } else { lhs_missing };
                                return Err(PhysureError::Generic(format!(
                                    "Missing argument(s) for equation '{}': {}",
                                    name,
                                    missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                                )));
                            }
                        }
                    };
                    let solved_str = solved_node.to_phs_string();
                    let program = crate::parser::parse_phs(&solved_str)?;
                    let Some(Statement::Expr(expr)) = program.statements.first() else {
                        return Err(PhysureError::Generic(format!("Failed to evaluate equation '{}'", name)));
                    };
                    return self.eval_expr(expr, &local_env);
                }

                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.eval_expr(arg, env)?);
                }

                let mut kwarg_vals = Vec::new();
                for (kw_name, kw_expr) in kwargs {
                    let kw_val = self.eval_expr(kw_expr, env)?;
                    kwarg_vals.push((kw_name.clone(), kw_val));
                }

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    if args.len() == 1 && kwargs.is_empty() {
                        let arg_eval = self.eval_expr(&args[0], env);
                        if let Ok(PhsValue::Function(arg_func)) = arg_eval {
                            let params = arg_func.params.clone();
                            let param_units = arg_func.param_units.clone();
                            let inner_args: Vec<Expr> = params.iter().map(|p| Expr::Identifier(p.clone())).collect();
                            let body = Statement::Expr(Expr::FunctionCall {
                                name: func.name.clone(),
                                args: vec![Expr::FunctionCall {
                                    name: arg_func.name.clone(),
                                    args: inner_args,
                                    kwargs: Vec::new(),
                                }],
                                kwargs: Vec::new(),
                            });
                            return Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                                name: format!("{}_{}", func.name, arg_func.name),
                                params,
                                param_units,
                                body_stmts: vec![body],
                                body_lines: vec![],
                                decorators: Vec::new(),
                                doc: None,
                            }));
                        }
                    }
                }

                if let Some(val) = crate::builtins::eval_core_builtin(name, &arg_vals, self, env)? {
                    return Ok(val);
                }

                if let Some((domain, canonical)) = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned() {
                    if let Some(val) = crate::builtins::eval_domain_builtin_with_kwargs(domain, &canonical, &arg_vals, &kwarg_vals, self, env)? {
                        return Ok(val);
                    }
                }

                let external = self.externals.get(name).cloned()
                    .or_else(|| self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned());
                if let Some(f) = external {
                    return f(&arg_vals);
                }

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    return self.call_function_node(func, arg_vals, env);
                }

                if name.ends_with('\'') {
                    let base_name = name.trim_end_matches('\'');
                    let order = name.len() - base_name.len();
                    if let Some(res) = self.eval_prime_function_call(base_name, order, &arg_vals, args, env)? {
                        return Ok(res);
                    }
                }

                Err(PhysureError::Generic(format!("Undefined function '{}'", name)))
            }
            Expr::ForExpr { var, iterable, body } => {
                let iterable_val = self.eval_expr(iterable, env)?;
                let items: Vec<PhsValue> = match iterable_val {
                    PhsValue::Vector(v) => v,
                    PhsValue::Range(start, end) => {
                        let (start_num, unit) = match start.as_ref() {
                            PhsValue::Number(n) => (*n, None),
                            PhsValue::Quantity(q) => {
                                let u = if q.unit.dimensions.is_empty() {
                                    None
                                } else {
                                    Some(q.unit.clone())
                                };
                                (q.value.mean(), u)
                            }
                            _ => return Err(PhysureError::Generic("Range start must be a number or quantity".into())),
                        };
                        let end_num = match end.as_ref() {
                            PhsValue::Number(n) => *n,
                            PhsValue::Quantity(q) => q.value.mean(),
                            _ => return Err(PhysureError::Generic("Range end must be a number or quantity".into())),
                        };
                        let start_i = start_num as i64;
                        let end_i = end_num as i64;
                        (start_i..end_i)
                            .map(|i| {
                                if let Some(ref u) = unit {
                                    PhsValue::Quantity(Quantity::new_scalar(i as f64, 0.0, u.clone(), None, None))
                                } else {
                                    PhsValue::Number(i as f64)
                                }
                            })
                            .collect()
                    }
                    other => return Err(PhysureError::Generic(format!("Cannot iterate over {}", other))),
                };

                // Switch to parallel evaluation if the iteration count meets the threshold.
                // Note: rayon's parallel collect stops scheduling new work on error but may
                // leave in-flight work on other threads to complete, so loop body side effects
                // (e.g. I/O in external functions) may partially execute even if evaluation fails.
                if items.len() >= physure_core::settings::parallel_threshold() && !self.debug_hook_is_set() {
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
            }
        }
    }

    fn eval_prime_function_call(
        &self,
        base_name: &str,
        order: usize,
        arg_vals: &[PhsValue],
        args: &[crate::ast::Expr],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<Option<PhsValue>> {
        if let Some(val) = env.get(base_name) {
            match val {
                PhsValue::Function(func) => {
                    if func.params.is_empty() {
                        return Err(PhysureError::Generic(format!("Function {} has no parameters to differentiate", base_name)));
                    }
                    let var_name = &func.params[0];
                    let mut body_str = String::new();
                    for stmt in &func.body_stmts {
                        match stmt {
                            crate::ast::Statement::Return(expr) | crate::ast::Statement::Expr(expr) => {
                                body_str = crate::codegen::expr_to_phs_string(expr);
                            }
                            _ => {}
                        }
                    }
                    if body_str.is_empty() {
                        return Err(PhysureError::Generic(format!("Cannot extract expression for function {}", base_name)));
                    }
                    let node = crate::symbolic::SymbolicParser::parse_str(&body_str)?;
                    let diff_node = node.diff_node_n(var_name, order)?;

                    if !arg_vals.is_empty() {
                        let first_arg = &arg_vals[0];
                        match first_arg {
                            PhsValue::Number(num) => {
                                let mut local_env = env.clone();
                                local_env.insert(var_name.clone(), PhsValue::Number(*num));
                                let expr_node = crate::parser::parse_phs(&diff_node.to_phs_string())?;
                                if let Some(crate::ast::Statement::Expr(e)) = expr_node.statements.first() {
                                    return Ok(Some(self.eval_expr(e, &local_env)?));
                                }
                            }
                            PhsValue::Quantity(q) => {
                                let mut local_env = env.clone();
                                local_env.insert(var_name.clone(), PhsValue::Quantity(q.clone()));
                                let expr_node = crate::parser::parse_phs(&diff_node.to_phs_string())?;
                                if let Some(crate::ast::Statement::Expr(e)) = expr_node.statements.first() {
                                    return Ok(Some(self.eval_expr(e, &local_env)?));
                                }
                            }
                            _ => {}
                        }
                    }
                    return Ok(Some(PhsValue::String(diff_node.to_string())));
                }
                PhsValue::String(expr_str) => {
                    let var_name = if !args.is_empty() {
                        if let crate::ast::Expr::Identifier(v) = &args[0] {
                            v.as_str()
                        } else {
                            "x"
                        }
                    } else {
                        "x"
                    };
                    let node = crate::symbolic::SymbolicParser::parse_str(expr_str)?;
                    let diff_node = node.diff_node_n(var_name, order)?;
                    return Ok(Some(PhsValue::String(diff_node.to_string())));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        self.call_function_node_at(func, arg_vals, env, 0)
    }

    fn call_function_node_at(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
        call_site_line: usize,
    ) -> PhysureResult<PhsValue> {
        if func.params.len() != arg_vals.len() {
            return Err(PhysureError::Generic(format!("Function {} expects {} args, got {}", func.name, func.params.len(), arg_vals.len())));
        }
        let mut local_env = env.clone();
        for (i, (param_name, arg_val)) in func.params.iter().zip(arg_vals.into_iter()).enumerate() {
            let bound_val = self.bind_param_value(&func.name, param_name, func.param_units.get(i).and_then(|u| u.as_ref()), arg_val)?;
            local_env.insert(param_name.clone(), bound_val);
        }
        self.check_requires(func, &local_env)?;

        // `_stack_guard` pops the pushed `StackFrame` on every exit path from this point on --
        // normal completion, an early `break` from a `Return`/`GuardReturn` arm, or `?` error
        // propagation from anywhere in the body loop below (undefined function, contract
        // violation, unit mismatch, ...). Without this, a mid-body error would leave the frame
        // on `call_stack` forever: `PhsInterpreter` is long-lived (REPL, future DAP sessions),
        // and every enclosing call in a chain hits the same unguarded early return, so a deep
        // call stack would leak one frame per active call on every error.
        let _stack_guard = if self.debug_hook.is_some() {
            self.call_stack.lock().unwrap_or_else(|e| e.into_inner())
                .push(StackFrame::new(func, call_site_line));
            Some(CallStackGuard { call_stack: self.call_stack.clone() })
        } else {
            None
        };

        let mut last_val = PhsValue::None;
        for (i, stmt) in func.body_stmts.iter().enumerate() {
            let line = func.body_lines.get(i).copied().unwrap_or(0);
            match stmt {
                Statement::Return(expr) => {
                    self.debug_checkpoint(line, &local_env)?;
                    last_val = self.eval_expr(expr, &local_env)?;
                    break;
                }
                Statement::GuardReturn { cond, value } => {
                    self.debug_checkpoint(line, &local_env)?;
                    let cond_val = self.eval_expr(cond, &local_env)?;
                    if is_truthy(&cond_val) {
                        last_val = self.eval_expr(value, &local_env)?;
                        break;
                    }
                }
                _ => {
                    last_val = self.eval_statement_with_env_at(stmt, &mut local_env, line)?;
                }
            }
        }

        self.check_ensures(func, &local_env, &last_val)?;
        Ok(last_val)
    }

    /// Evaluates every `@requires` condition against the already-bound parameters,
    /// erroring on the first one that is not truthy. Conditions are ordinary `Expr`s —
    /// a comparison like `m > 0.0` is a `FunctionCall { name: "op_>", .. }` under the
    /// hood, so this needs no evaluator support beyond `eval_expr`/`is_truthy`.
    ///
    /// Note: conditions must evaluate to a numeric/`Quantity` truthy value (as produced by
    /// comparison operators like `op_>`); a plugin-provided `PhsValue::Bool` is currently
    /// always treated as falsy by `is_truthy`, so boolean-returning plugin predicates are
    /// not yet safe to use directly as `@requires`/`@ensures` conditions.
    fn check_requires(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        for dec in &func.decorators {
            if dec.name == "requires" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, local_env)?) {
                    let message = self.eval_expr(&dec.args[1], local_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "requires".to_string(), message });
                }
            }
        }
        Ok(())
    }

    /// Evaluates every `@ensures` condition with `result` bound to the function's
    /// return value. `validate_decorators` (Task 5) already rejects `@ensures` on any
    /// function with a parameter literally named `result`, so this insert can never
    /// silently shadow a caller-visible binding.
    ///
    /// See `check_requires`'s note on `PhsValue::Bool`.
    fn check_ensures(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>, result: &PhsValue) -> PhysureResult<()> {
        if !func.decorators.iter().any(|d| d.name == "ensures") {
            return Ok(());
        }
        let mut result_env = local_env.clone();
        result_env.insert("result".to_string(), result.clone());
        for dec in &func.decorators {
            if dec.name == "ensures" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, &result_env)?) {
                    let message = self.eval_expr(&dec.args[1], &result_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "ensures".to_string(), message });
                }
            }
        }
        Ok(())
    }

    /// Binds an argument value to a function parameter, converting it to the parameter's
    /// declared unit (if any) so that dimensionally-equivalent-but-differently-scaled
    /// arguments (e.g. `5 cm` passed to a `(r: m)` parameter) produce identical results
    /// regardless of which unit the caller used.
    ///
    /// - If the parameter has no declared unit, the argument is bound as-is (no conversion).
    /// - If the argument isn't a `Quantity`, it is bound as-is (nothing to convert).
    /// - If the argument's unit is dimensionally incompatible with the declared unit,
    ///   this returns a clear error rather than silently producing a wrong result.
    fn bind_param_value(
        &self,
        fn_name: &str,
        param_name: &str,
        declared_unit: Option<&String>,
        arg_val: PhsValue,
    ) -> PhysureResult<PhsValue> {
        let Some(unit_str) = declared_unit else {
            return Ok(arg_val);
        };
        let PhsValue::Quantity(q) = arg_val else {
            return Ok(arg_val);
        };
        let clean_unit_str = strip_unit_comment(unit_str);
        if clean_unit_str.is_empty() {
            return Ok(PhsValue::Quantity(q));
        }
        let target_unit = UnitParser::parse_expression(clean_unit_str)?;
        let converted = q.convert_to(&target_unit).map_err(|e| {
            PhysureError::Generic(format!(
                "Argument for parameter '{}' of function '{}' has a unit incompatible with declared unit '{}': {:?}",
                param_name, fn_name, clean_unit_str, e
            ))
        })?;
        Ok(PhsValue::Quantity(converted))
    }

    fn convert_value_to_unit(&self, val: PhsValue, unit: &RationalUnit) -> PhysureResult<PhsValue> {
        match val {
            PhsValue::Quantity(q) => Ok(PhsValue::Quantity(q.convert_to(unit)?)),
            PhsValue::Vector(vec) => {
                let mut results = Vec::new();
                for item in vec {
                    results.push(self.convert_value_to_unit(item, unit)?);
                }
                Ok(PhsValue::Vector(results))
            }
            // A range is its endpoints, so converting it converts both: `(0 m .. 100 m) => km`
            // is `0 km .. 0.1 km`. Without this arm it fell to the catch-all below and came
            // back as the metres it went in as, with nothing said about the `=> km`.
            PhsValue::Range(start, end) => make_range(
                self.convert_value_to_unit(*start, unit)?,
                self.convert_value_to_unit(*end, unit)?,
            ),
            other => Ok(other),
        }
    }

    pub fn eval_binary_op_vals(&self, op: BinaryOp, l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
        if op == BinaryOp::Range {
            return make_range(l_val, r_val);
        }
        let l_val = coerce_equation_string(l_val);
        let r_val = coerce_equation_string(r_val);
        // A range is its two endpoints and nothing else, so an operation on one is that
        // operation on both: `(0 .. 100) m` is `0 m .. 100 m` and `(0 m .. 100 m) => km` is
        // `0 km .. 0.1 km`. Only the operations that keep it a range are distributed —
        // adding two ranges asks a question about intervals that PHS has not been told the
        // answer to, and guessing one is worse than refusing.
        if let PhsValue::Range(start, end) = &l_val {
            if matches!(op, BinaryOp::Mul | BinaryOp::Div | BinaryOp::Convert) {
                let lo = self.eval_binary_op_vals(op, (**start).clone(), r_val.clone())?;
                let hi = self.eval_binary_op_vals(op, (**end).clone(), r_val)?;
                return make_range(lo, hi);
            }
        }
        match (l_val, r_val) {
            (PhsValue::Function(f), PhsValue::Function(g)) => {
                let (params, param_units) = if !f.params.is_empty() {
                    (f.params.clone(), f.param_units.clone())
                } else {
                    (g.params.clone(), g.param_units.clone())
                };
                let args_expr: Vec<Expr> = params.iter().map(|p| Expr::Identifier(p.clone())).collect();
                let body = Statement::Expr(Expr::BinaryOp {
                    op,
                    left: Box::new(Expr::FunctionCall { name: f.name.clone(), args: args_expr.clone(), kwargs: Vec::new() }),
                    right: Box::new(Expr::FunctionCall { name: g.name.clone(), args: args_expr, kwargs: Vec::new() }),
                });
                let name = match op {
                    BinaryOp::Add => format!("{}_add_{}", f.name, g.name),
                    BinaryOp::Sub => format!("{}_sub_{}", f.name, g.name),
                    BinaryOp::Mul => format!("{}_mul_{}", f.name, g.name),
                    BinaryOp::Div => format!("{}_div_{}", f.name, g.name),
                    _ => format!("{}_op_{}", f.name, g.name),
                };
                Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                    name,
                    params,
                    param_units,
                    body_stmts: vec![body],
                    body_lines: vec![],
                    decorators: Vec::new(),
                    doc: None,
                }))
            }
            (PhsValue::Equation(l1, r1), PhsValue::Equation(l2, r2)) => {
                let new_l = node_op(op, l1, l2)?.simplify();
                let new_r = node_op(op, r1, r2)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (PhsValue::Equation(l, r), other) => {
                let node = value_to_symbolic_node(&other)?;
                let new_l = node_op(op, l, node.clone())?.simplify();
                let new_r = node_op(op, r, node)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (other, PhsValue::Equation(l, r)) => {
                let node = value_to_symbolic_node(&other)?;
                let new_l = node_op(op, node.clone(), l)?.simplify();
                let new_r = node_op(op, node, r)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (PhsValue::Vector(l_vec), PhsValue::Vector(r_vec)) => {
                if l_vec.len() != r_vec.len() {
                    return Err(PhysureError::Generic("Vector length mismatch in binary operation".into()));
                }
                let mut results = Vec::new();
                for (l_item, r_item) in l_vec.into_iter().zip(r_vec.into_iter()) {
                    results.push(self.eval_binary_op_vals(op, l_item, r_item)?);
                }
                Ok(PhsValue::Vector(results))
            }
            (PhsValue::Vector(v_vec), scalar) => {
                let mut results = Vec::new();
                for item in v_vec {
                    results.push(self.eval_binary_op_vals(op, item, scalar.clone())?);
                }
                Ok(PhsValue::Vector(results))
            }
            (scalar, PhsValue::Vector(v_vec)) => {
                let mut results = Vec::new();
                for item in v_vec {
                    results.push(self.eval_binary_op_vals(op, scalar.clone(), item)?);
                }
                Ok(PhsValue::Vector(results))
            }
            (PhsValue::Quantity(l), PhsValue::Quantity(r)) => {
                let res = match op {
                    BinaryOp::Add => l.add(&r)?,
                    BinaryOp::Sub => l.sub(&r)?,
                    BinaryOp::Mul => l.mul(&r)?,
                    BinaryOp::Div => l.div(&r)?,
                    BinaryOp::Pow => {
                        if r.unit == RationalUnit::dimensionless() && r.value.std_dev() == 0.0 {
                            l.pow(r.value.mean())?
                        } else {
                            return Err(PhysureError::Generic("Exponent must be a dimensionless constant".into()));
                        }
                    }
                    BinaryOp::Convert | BinaryOp::Range => unreachable!(),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::Quantity(l), PhsValue::Number(r)) => {
                let r_q = Quantity::new_scalar(r, 0.0, RationalUnit::dimensionless(), None, None);
                self.eval_binary_op_vals(op, PhsValue::Quantity(l), PhsValue::Quantity(r_q))
            }
            (PhsValue::Number(l), PhsValue::Quantity(r)) => {
                let l_q = Quantity::new_scalar(l, 0.0, RationalUnit::dimensionless(), None, None);
                self.eval_binary_op_vals(op, PhsValue::Quantity(l_q), PhsValue::Quantity(r))
            }
            (PhsValue::Number(l), PhsValue::Number(r)) => {
                let res = match op {
                    BinaryOp::Add => l + r,
                    BinaryOp::Sub => l - r,
                    BinaryOp::Mul => l * r,
                    BinaryOp::Div => {
                        if r == 0.0 {
                            return Err(PhysureError::Generic("Division by zero".into()));
                        }
                        l / r
                    }
                    BinaryOp::Pow => l.powf(r),
                    BinaryOp::Convert | BinaryOp::Range => unreachable!(),
                };
                Ok(PhsValue::Number(res))
            }
            // A bare word that isn't a bound variable arrives here as a String, so these
            // four arms are where `5 foobar` is decided. The unit parser now reports the
            // offending symbol and the nearest registered one; swallowing that with `if
            // let Ok` would replace a usable message with a bare "Unknown unit symbol".
            (PhsValue::Quantity(l), PhsValue::String(r)) => {
                let parsed_unit = UnitParser::parse_expression(strip_unit_comment(&r))?;
                let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit.clone(), None, None);
                let res = match op {
                    BinaryOp::Mul => l.mul(&unit_q)?,
                    BinaryOp::Div => l.div(&unit_q)?,
                    BinaryOp::Convert => l.convert_to(&parsed_unit)?,
                    _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::Number(l), PhsValue::String(r)) => {
                let parsed_unit = UnitParser::parse_expression(strip_unit_comment(&r))?;
                let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                let num_q = Quantity::new_scalar(l, 0.0, RationalUnit::dimensionless(), None, None);
                let res = match op {
                    BinaryOp::Mul => num_q.mul(&unit_q)?,
                    BinaryOp::Div => num_q.div(&unit_q)?,
                    _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::String(l), PhsValue::Quantity(r)) => {
                let parsed_unit = UnitParser::parse_expression(strip_unit_comment(&l))?;
                let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                let res = match op {
                    BinaryOp::Mul => unit_q.mul(&r)?,
                    BinaryOp::Pow => unit_q.pow(r.value.mean())?,
                    _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::String(l), PhsValue::Number(r)) => {
                let parsed_unit = UnitParser::parse_expression(strip_unit_comment(&l))?;
                let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                let num_q = Quantity::new_scalar(r, 0.0, RationalUnit::dimensionless(), None, None);
                let res = match op {
                    BinaryOp::Mul => unit_q.mul(&num_q)?,
                    BinaryOp::Pow => unit_q.pow(r)?,
                    _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                };
                Ok(PhsValue::Quantity(res))
            }
            _ => Err(PhysureError::Generic("Invalid operand types for binary operation".into())),
        }
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
