//! The sandboxed interpreter: what text from outside cannot make the engine do.

use physure_script::{parse_phs, Limits, PhsInterpreter, PhsValue};
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
