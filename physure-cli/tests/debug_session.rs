use std::io::Write;
use std::process::{Command, Stdio};

/// Drives `phs debug` as a real subprocess, feeding it a scripted sequence of debugger
/// commands over stdin and asserting on stdout -- the same black-box shape a human at a
/// terminal would exercise, since `run_debug`'s `CliDebugHook` is a blocking stdin loop with
/// no seam for in-process mocking (matching how `run_repl` is untested in-process too).
#[test]
fn function_entry_breakpoint_pauses_and_inspect_decomposes_a_local() {
    let script_dir = std::env::temp_dir();
    let script_path = script_dir.join("phs_debug_session_test.phs");
    // PHS function bodies are indentation-delimited, not brace-delimited -- see the note on
    // the C0.1 test. Lines: 1 "fn simulate(v) =", 2 "  speed = v => km/h", 3 "  speed" (the
    // function's implicit return), 4 "result = simulate(5.0 m/s)".
    std::fs::write(
        &script_path,
        "fn simulate(v) =\n  speed = v => km/h\n  speed\nresult = simulate(5.0 m/s)\n",
    )
    .unwrap();

    // Break on line 3 ("speed", the bare-expression return), not on function entry (line 2,
    // "speed = v => km/h"): a checkpoint fires *before* its own statement runs, so pausing at
    // line 2 would catch `speed` before that assignment has happened. Line 3 doesn't assign
    // anything itself, so by the time its checkpoint fires, line 2 has already fully executed
    // and `speed` is in scope to inspect.
    let mut child = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["debug", script_path.to_str().unwrap(), "--break", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn phs debug");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "inspect speed").unwrap();
        writeln!(stdin, "continue").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Paused at line 3"), "expected a pause banner, got:\n{stdout}");
    assert!(stdout.contains("kind        : Scalar"), "expected inspect output, got:\n{stdout}");
    assert!(stdout.contains("Program finished."), "expected the script to run to completion after continue, got:\n{stdout}");

    let _ = std::fs::remove_file(&script_path);
}
