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
