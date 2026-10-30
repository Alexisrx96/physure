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

#[test]
fn a_deadline_that_has_passed_refuses_the_first_statement() {
    let interp = PhsInterpreter::sandboxed(Limits::sandboxed(Duration::ZERO));
    let error = run(&interp, "1 m + 1 m").unwrap_err();
    assert!(error.contains("time limit"), "{error}");
}

#[test]
fn slow_work_is_stopped_at_the_deadline() {
    // 10,000 iterations of a 150-deep recursion take seconds; the deadline is 100 ms
    let source = format!("{}\nx = 0\nwhile true {{ x = down(150) }}", down(0).lines().take(3).collect::<Vec<_>>().join("\n"));
    let started = std::time::Instant::now();
    let error = run_sandboxed(Limits::sandboxed(Duration::from_millis(100)), move |interp| run(&interp, &source)).unwrap_err();
    assert!(error.contains("time limit"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}

#[test]
fn a_deadline_far_away_changes_nothing() {
    let interp = PhsInterpreter::sandboxed(Limits::sandboxed(Duration::from_secs(3600)));
    assert_eq!(run(&interp, "1 m + 2 m").unwrap().len(), 1);
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("physure_sandbox_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_mesh_export_writes_a_file_normally_and_is_refused_in_the_sandbox() {
    let dir = temp_dir("export");
    let path = dir.join("mesh.stl");
    let source = format!("use export3d from plot\nexport3d(\"sin(x)*cos(y)\", \"{}\", \"stl\")", path.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/"));

    let error = run(&PhsInterpreter::sandboxed(sandbox()), &source).unwrap_err();
    assert!(error.contains("forbidden"), "{error}");
    assert!(!path.exists(), "the sandbox wrote a file");

    run(&PhsInterpreter::default(), &source).unwrap();
    assert!(path.exists(), "the unrestricted interpreter no longer writes");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_import_reads_a_file_normally_and_nothing_in_the_sandbox() {
    // the unrestricted interpreter resolves `import` against the working directory (the crate's, in a test)
    let module = format!("zz_sandbox_probe_{}", std::process::id());
    let file = format!("{module}.phs");
    std::fs::write(&file, "x = 5 m\n").unwrap();
    let source = format!("use x from {module}\nx + 1 m");

    let normal = run(&PhsInterpreter::default(), &source);
    let sandboxed = run(&PhsInterpreter::sandboxed(sandbox()), &source);
    let _ = std::fs::remove_file(&file);
    assert_eq!(normal.unwrap().len(), 1, "the control did not read the module");
    assert!(sandboxed.is_err(), "the sandbox read {file}");
}

#[test]
fn a_sandboxed_for_expression_stays_on_one_thread() {
    let threads = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut interp = PhsInterpreter::sandboxed(sandbox());
    let seen = threads.clone();
    interp.register_fn("probe", move |args| {
        seen.lock().unwrap().insert(std::thread::current().id());
        Ok(args[0].clone())
    });
    let values = run_sandboxed_with(interp, "res = for i in 1..20000 { probe(i) }\nres");
    values.unwrap();
    let used = threads.lock().unwrap().len();
    assert_eq!(used, 1, "the loop spread over {used} threads");
}


/// Runs `source` in `interp` on a thread big enough for a sandbox.
fn run_sandboxed_with(interp: PhsInterpreter, source: &'static str) -> Result<Vec<PhsValue>, String> {
    on_a_stack(physure_script::limits::SANDBOX_THREAD_STACK_BYTES, move || run(&interp, source))
}
