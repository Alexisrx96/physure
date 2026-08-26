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
mod tests;
