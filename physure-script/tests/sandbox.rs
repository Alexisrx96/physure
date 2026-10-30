//! The sandboxed interpreter: what text from outside cannot make the engine do.

use physure_script::{parse_phs, run_sandboxed, Limits, PhsInterpreter, PhsValue};
use std::time::Duration;

/// Runs `source` statement by statement in `interp`, returning every non-empty value.
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

/// Runs `f` on a thread with a stack of `bytes`.
fn on_a_stack<R: Send + 'static>(bytes: usize, f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new().stack_size(bytes).spawn(f).unwrap().join().expect("the evaluation thread panicked")
}

const RUNAWAY: &str = "fn f(x) = f(x)
f(1)";

fn down(depth: usize) -> String {
    format!("fn down(n) =
  if n <= 0 then return 0
  return down(n - 1)
down({depth})")
}

fn sandbox() -> Limits {
    Limits::sandboxed(Duration::from_secs(60))
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash_in_the_sandbox() {
    let error = run_sandboxed(sandbox(), |interp| run(&interp, RUNAWAY)).unwrap_err();
    assert!(error.contains("call depth limit"), "{error}");
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash_in_the_default_interpreter_on_a_big_stack() {
    // 1000 calls need about 100 MB in a debug build, so this thread is generous; a release build needs far less
    let error = on_a_stack(256 << 20, || run(&PhsInterpreter::default(), RUNAWAY)).unwrap_err();
    assert!(error.contains("call depth limit"), "{error}");
}

#[test]
fn recursion_within_the_limit_still_works() {
    let source = down(150);
    assert_eq!(run_sandboxed(sandbox(), move |interp| run(&interp, &source)).unwrap().len(), 1);
}

#[test]
fn the_depth_counter_is_released_after_an_error() {
    let source = down(150);
    run_sandboxed(sandbox(), move |interp| {
        assert!(run(&interp, RUNAWAY).is_err());
        assert!(run(&interp, &source).is_ok(), "depth leaked from the failed run");
    });
}

#[test]
fn the_stack_budget_stops_what_the_depth_does_not_count() {
    // a call depth far past anything reached, and a one megabyte budget: the stack is what runs out
    let mut limits = sandbox();
    limits.max_call_depth = 100_000;
    limits.max_stack_bytes = Some(1 << 20);
    let source = down(5_000);
    let error = run_sandboxed(limits, move |interp| run(&interp, &source)).unwrap_err();
    assert!(error.contains("stack limit"), "{error}");
}

#[test]
fn deeply_nested_expressions_inside_recursion_end_in_an_error() {
    // a call depth of 200 at 40 nested parentheses per level would need far more than the 64 MiB thread has
    let nested = format!("{}f(x){}", "(".repeat(40), ")".repeat(40));
    let source = format!("fn f(x) = {nested}
f(1)");
    let error = run_sandboxed(sandbox(), move |interp| run(&interp, &source)).unwrap_err();
    assert!(error.contains("stack limit") || error.contains("call depth limit"), "{error}");
}
