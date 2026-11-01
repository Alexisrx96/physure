//! What an interpreter is allowed to do. The default is unrestricted (apart from a call-depth ceiling that
//! turns a native stack overflow into an error); [`Limits::sandboxed`] is for text from outside.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Nested user-function calls any interpreter allows. A net for hosts with a large stack: measured in release, a PHS
/// call costs 15 to 20 KB of native stack (about 100 KB in a debug build), so 1 MiB (a Windows main thread) holds about 60
/// levels and 8 MiB about 400; only [`Limits::max_stack_bytes`] protects a small stack.
pub const DEFAULT_MAX_CALL_DEPTH: usize = 1_000;
/// Nested calls a sandboxed interpreter allows.
pub const SANDBOX_MAX_CALL_DEPTH: usize = 200;
/// Native stack a sandboxed run may use, counted from where its first statement starts.
pub const SANDBOX_STACK_BUDGET: usize = 48 << 20;
/// Size of the thread [`run_sandboxed`] evaluates on: the budget plus headroom for the native frames between two
/// checks (a builtin, the symbolic engine).
pub const SANDBOX_THREAD_STACK_BYTES: usize = 64 << 20;
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
    /// Native stack the evaluation may use, counted from the statement that started it; one more is an error
    /// with the prefix `stack limit`. `None` does not look at the stack.
    pub max_stack_bytes: Option<usize>,
    /// Whether a builtin may write a file (the mesh export).
    pub allow_files: bool,
    /// Whether `for` expressions and `parallel_map` must stay on one thread (reproducible random draws).
    pub sequential: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_call_depth: DEFAULT_MAX_CALL_DEPTH, deadline: None, max_output_bytes: None, max_stack_bytes: None, allow_files: true, sequential: false }
    }
}

impl Limits {
    pub fn sandboxed(timeout: Duration) -> Self {
        Self {
            max_call_depth: SANDBOX_MAX_CALL_DEPTH,
            deadline: Some(Instant::now() + timeout),
            max_output_bytes: Some(SANDBOX_MAX_OUTPUT_BYTES),
            max_stack_bytes: Some(SANDBOX_STACK_BUDGET),
            allow_files: false,
            sequential: true,
        }
    }
}

thread_local! {
    static CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Held for the duration of one user-function call; the depth is per thread because each thread has its own stack.
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
    fn drop(&mut self) {
        CALL_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

thread_local! {
    /// Lowest stack address the current evaluation may reach; `0` when none is being tracked.
    static STACK_FLOOR: Cell<usize> = const { Cell::new(0) };
}

fn stack_address() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// Marks where an evaluation starts so [`check_stack`] can tell how much stack it has used. Nested scopes restore
/// the outer floor on drop. The stack grows downwards on every target we build for.
pub(crate) struct StackScope(usize);

impl StackScope {
    pub(crate) fn begin(budget: Option<usize>) -> Option<Self> {
        let budget = budget?;
        let previous = STACK_FLOOR.with(|floor| floor.replace(stack_address().saturating_sub(budget)));
        Some(StackScope(previous))
    }
}

impl Drop for StackScope {
    fn drop(&mut self) {
        STACK_FLOOR.with(|floor| floor.set(self.0));
    }
}

/// Errors when the stack in use has passed the budget of the enclosing [`StackScope`] (or, with none, of the first
/// check on this thread). A no-op when `budget` is `None`.
pub(crate) fn check_stack(budget: Option<usize>) -> Result<(), String> {
    let Some(budget) = budget else { return Ok(()) };
    let here = stack_address();
    STACK_FLOOR.with(|floor| {
        if floor.get() == 0 {
            floor.set(here.saturating_sub(budget));
            Ok(())
        } else if here < floor.get() {
            Err(format!("stack limit of {} MiB reached (nesting too deep)", budget >> 20))
        } else {
            Ok(())
        }
    })
}

/// Runs `f` with a sandboxed interpreter on a thread whose stack is big enough for the limits' stack budget.
/// A panic inside `f` is re-raised here.
pub fn run_sandboxed<R: Send + 'static>(
    limits: Limits,
    f: impl FnOnce(crate::PhsInterpreter) -> R + Send + 'static,
) -> R {
    let handle = std::thread::Builder::new()
        .stack_size(SANDBOX_THREAD_STACK_BYTES)
        .spawn(move || f(crate::PhsInterpreter::sandboxed(limits)))
        .expect("the operating system refused a thread for the sandbox");
    match handle.join() {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
