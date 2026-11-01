//! `phs mcp`: a Model Context Protocol server over stdio, so an agent can check its work against the engine.
//!
//! JSON-RPC 2.0, one message per line, hand-written on `serde_json` (no SDK). Tools, all read-only:
//! `evaluate` and `check` run the program in a **child process** (`phs eval -` / `phs check -`) that is killed if it
//! does not finish, so a crash or a hang in an evaluation cannot take the server down; `convert` and `describe_unit`
//! call the core in-process, since they run no user code. The resource `phs://language` is the `phs doc` reference.

use physure_script::report::{self, DEFAULT_SEED};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Protocol versions this server speaks, oldest first. The tools and resources used here are the same in all three.
const SUPPORTED_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];
const LANGUAGE_URI: &str = "phs://language";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 30_000;
/// How long after the evaluation's own deadline the child process is given before it is killed.
const KILL_GRACE: Duration = Duration::from_secs(3);
const MAX_SOURCE_BYTES: usize = 1 << 20;

/// A JSON-RPC error: code and message.
type RpcError = (i64, String);

/// How a tool runs `phs` in another process.
pub trait Runner {
    /// Runs `phs <args>` with `stdin` as its standard input and returns (exit code, stdout, stderr), or says why it
    /// could not within `limit`.
    fn run(&self, args: &[String], stdin: &str, limit: Duration) -> Result<(i32, String, String), String>;
}

/// Runs the current executable.
pub struct ChildRunner {
    exe: PathBuf,
}

impl ChildRunner {
    pub fn current() -> std::io::Result<Self> {
        Ok(Self { exe: std::env::current_exe()? })
    }
}

impl Runner for ChildRunner {
    fn run(&self, args: &[String], stdin: &str, limit: Duration) -> Result<(i32, String, String), String> {
        run_child(&self.exe, args, stdin, limit)
    }
}

/// Spawns `exe args`, feeds it `stdin`, and waits for it; past `limit` it is killed.
fn run_child(exe: &std::path::Path, args: &[String], stdin: &str, limit: Duration) -> Result<(i32, String, String), String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", exe.display()))?;
    // the pipes are served from threads: a child that fills its output before reading its input must not deadlock us
    let mut input = child.stdin.take().expect("stdin was piped");
    let text = stdin.to_string();
    std::thread::spawn(move || {
        let _ = input.write_all(text.as_bytes());
    });
    let read_all = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let stdout = read_all(Box::new(child.stdout.take().expect("stdout was piped")));
    let stderr = read_all(Box::new(child.stderr.take().expect("stderr was piped")));
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => break status,
            None if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the evaluation process did not finish within {} ms and was stopped", limit.as_millis()));
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    Ok((status.code().unwrap_or(-1), stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default()))
}

pub fn run_mcp() {
    let runner = ChildRunner::current().unwrap_or_else(|error| {
        eprintln!("phs mcp: cannot find its own executable: {error}");
        std::process::exit(2);
    });
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(stdin.lock(), &mut stdout.lock(), &runner);
}

/// Reads one JSON-RPC message per line from `input` and writes the replies to `output`, until `input` ends.
pub fn serve(input: impl BufRead, output: &mut impl Write, runner: &dyn Runner) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_line(&line, runner) {
            let _ = writeln!(output, "{reply}");
            let _ = output.flush();
        }
    }
}

fn reply(id: Value, outcome: Result<Value, RpcError>) -> Value {
    match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    }
}

fn handle_line(line: &str, runner: &dyn Runner) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(reply(Value::Null, Err((-32700, format!("parse error: {error}"))))),
    };
    let Some(object) = message.as_object() else {
        return Some(reply(Value::Null, Err((-32600, "a message is a JSON object".to_string()))));
    };
    let id = object.get("id").cloned();
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        // an answer from the client to something we never asked is ignored; anything else is not a request
        let is_answer = object.contains_key("result") || object.contains_key("error");
        return if is_answer { None } else { Some(reply(id.unwrap_or(Value::Null), Err((-32600, "no method".to_string())))) };
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let outcome = dispatch(method, &params, runner);
    // a notification has no id and gets no reply, whatever came of it
    Some(reply(id?, outcome))
}

fn dispatch(method: &str, params: &Value, runner: &dyn Runner) -> Result<Value, RpcError> {
    match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = SUPPORTED_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&SUPPORTED_VERSIONS[SUPPORTED_VERSIONS.len() - 1]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": { "name": "phs", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Check a physics computation against the engine: `evaluate` runs PhysureScript and returns value, unit, uncertainty and dimension per statement plus diagnostics; `check` finds dimension errors without running; `convert` and `describe_unit` answer from the unit catalog. The language reference is the resource phs://language."
            }))
        }
        "ping" => Ok(json!({})),
        m if m.starts_with("notifications/") => Ok(Value::Null),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(params, runner),
        "resources/list" => Ok(json!({ "resources": [{
            "uri": LANGUAGE_URI,
            "name": "PhysureScript language reference",
            "description": "The syntax, units, builtins and decorators of PHS, generated from the implementation (`phs doc`).",
            "mimeType": "text/markdown"
        }] })),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
            if uri != LANGUAGE_URI {
                return Err((-32002, format!("resource not found: {uri}")));
            }
            Ok(json!({ "contents": [{ "uri": LANGUAGE_URI, "mimeType": "text/markdown", "text": crate::generate_language_docs_md() }] }))
        }
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn tools() -> Value {
    let read_only = |title: &str| json!({ "title": title, "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false });
    json!([
        {
            "name": "evaluate",
            "description": "Run a PhysureScript program in a sandbox (no files, no plugins, bounded time and output) and return, per statement, its value, unit, uncertainty and physical dimension, plus diagnostics (dimension errors, unknown units, syntax and runtime errors). Deterministic for a given seed.",
            "inputSchema": { "type": "object", "properties": {
                "source": { "type": "string", "description": "The program, e.g. \"d = 120 m\\nt = 8 s\\nd / t\"" },
                "seed": { "type": "integer", "minimum": 0, "description": "Seed for Monte Carlo draws (default 0)" },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_MS, "description": "Deadline in milliseconds (default 5000)" }
            }, "required": ["source"] },
            "annotations": read_only("Evaluate PhysureScript")
        },
        {
            "name": "check",
            "description": "Find dimension errors, unknown units and bad exponents in a PhysureScript program without running it.",
            "inputSchema": { "type": "object", "properties": { "source": { "type": "string" } }, "required": ["source"] },
            "annotations": read_only("Check dimensions")
        },
        {
            "name": "convert",
            "description": "Convert a value between units of the same dimension (absolute temperatures included).",
            "inputSchema": { "type": "object", "properties": {
                "value": { "type": "number" },
                "from": { "type": "string", "description": "Unit expression, e.g. \"km/h\"" },
                "to": { "type": "string", "description": "Unit expression, e.g. \"m/s\"" }
            }, "required": ["value", "from", "to"] },
            "annotations": read_only("Convert units")
        },
        {
            "name": "describe_unit",
            "description": "What the unit catalog knows of a unit: dimension, whether it is an absolute temperature scale, its shortest name and its form in base units.",
            "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] },
            "annotations": read_only("Describe a unit")
        }
    ])
}

fn call_tool(params: &Value, runner: &dyn Runner) -> Result<Value, RpcError> {
    let name = params.get("name").and_then(Value::as_str).ok_or((-32602, "tools/call needs a tool name".to_string()))?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let outcome = match name {
        "evaluate" => tool_evaluate(&arguments, runner),
        "check" => tool_check(&arguments, runner),
        "convert" => tool_convert(&arguments),
        "describe_unit" => tool_describe_unit(&arguments),
        other => return Err((-32602, format!("unknown tool '{other}'"))),
    };
    // a program that is wrong is a normal answer (`ok: false`); an error here is the tool failing to run
    let (structured, is_error) = match outcome {
        Ok(value) => (value, false),
        Err(message) => (json!({ "error": message }), true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": structured.to_string() }], "structuredContent": structured, "isError": is_error }))
}

fn text_arg<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments.get(name).and_then(Value::as_str).ok_or_else(|| format!("the argument '{name}' is required and must be a string"))
}

fn source_arg(arguments: &Value) -> Result<&str, String> {
    let source = text_arg(arguments, "source")?;
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!("the program is {} bytes; the limit is {MAX_SOURCE_BYTES}", source.len()));
    }
    Ok(source)
}

/// The JSON a child wrote when it ended normally (0 or 1); an error describing it otherwise.
fn child_json(code: i32, stdout: &str, stderr: &str) -> Result<Value, String> {
    if code == 0 || code == 1 {
        if let Ok(value) = serde_json::from_str(stdout) {
            return Ok(value);
        }
    }
    let detail: String = stderr.trim().chars().take(300).collect();
    Err(format!("the evaluation process ended abnormally (exit code {code}){}", if detail.is_empty() { String::new() } else { format!(": {detail}") }))
}

fn tool_evaluate(arguments: &Value, runner: &dyn Runner) -> Result<Value, String> {
    let source = source_arg(arguments)?;
    let seed = arguments.get("seed").and_then(Value::as_u64).unwrap_or(DEFAULT_SEED);
    let timeout_ms = arguments.get("timeout_ms").and_then(Value::as_u64).unwrap_or(DEFAULT_TIMEOUT_MS).clamp(1, MAX_TIMEOUT_MS);
    let args = ["eval", "-", "--seed", &seed.to_string(), "--timeout-ms", &timeout_ms.to_string()].map(String::from);
    let (code, stdout, stderr) = runner.run(&args, source, Duration::from_millis(timeout_ms) + KILL_GRACE)?;
    child_json(code, &stdout, &stderr)
}

fn tool_check(arguments: &Value, runner: &dyn Runner) -> Result<Value, String> {
    let source = source_arg(arguments)?;
    let args = ["check", "-", "--json"].map(String::from);
    let (code, stdout, stderr) = runner.run(&args, source, Duration::from_millis(DEFAULT_TIMEOUT_MS) + KILL_GRACE)?;
    match code {
        0 | 1 => Ok(json!({ "ok": code == 0, "findings": serde_json::from_str::<Value>(&stdout).map_err(|e| format!("unreadable check output: {e}"))? })),
        // exit 2: the program does not parse
        2 => Ok(json!({ "ok": false, "findings": [{ "line": 0, "severity": "error", "code": "syntax", "message": stderr.trim() }] })),
        _ => Err(child_json(code, &stdout, &stderr).unwrap_err()),
    }
}

fn tool_convert(arguments: &Value) -> Result<Value, String> {
    let value = arguments.get("value").and_then(Value::as_f64).ok_or("the argument 'value' is required and must be a number")?;
    let converted = report::convert(value, text_arg(arguments, "from")?, text_arg(arguments, "to")?)?;
    serde_json::to_value(converted).map_err(|error| error.to_string())
}

fn tool_describe_unit(arguments: &Value) -> Result<Value, String> {
    let description = report::describe_unit(text_arg(arguments, "symbol")?)?;
    serde_json::to_value(description).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers every run with a canned result and remembers what it was asked.
    struct Fake {
        answer: Result<(i32, String, String), String>,
        asked: RefCell<Vec<(Vec<String>, String)>>,
    }

    impl Fake {
        fn answering(code: i32, stdout: &str) -> Self {
            Fake { answer: Ok((code, stdout.to_string(), String::new())), asked: RefCell::new(Vec::new()) }
        }
    }

    impl Runner for Fake {
        fn run(&self, args: &[String], stdin: &str, _limit: Duration) -> Result<(i32, String, String), String> {
            self.asked.borrow_mut().push((args.to_vec(), stdin.to_string()));
            self.answer.clone()
        }
    }

    fn ask(runner: &dyn Runner, message: Value) -> Option<Value> {
        handle_line(&message.to_string(), runner)
    }

    fn call(runner: &dyn Runner, name: &str, arguments: Value) -> Value {
        ask(runner, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })).unwrap()
    }

    #[test]
    fn initialize_agrees_on_a_version_the_server_speaks() {
        let fake = Fake::answering(0, "");
        let init = |version: &str| ask(&fake, json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": version } })).unwrap();
        assert_eq!(init("2025-03-26")["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init("1999-01-01")["result"]["protocolVersion"], "2025-06-18", "an unknown version gets the newest we speak");
        let result = &init("2025-06-18")["result"];
        assert_eq!(result["serverInfo"]["name"], "phs");
        assert!(result["capabilities"]["tools"].is_object() && result["capabilities"]["resources"].is_object());
    }

    #[test]
    fn a_notification_gets_no_reply_and_ping_gets_one() {
        let fake = Fake::answering(0, "");
        assert!(ask(&fake, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        assert_eq!(ask(&fake, json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })).unwrap()["result"], json!({}));
    }

    #[test]
    fn the_four_tools_are_listed_and_all_read_only() {
        let fake = Fake::answering(0, "");
        let reply = ask(&fake, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["evaluate", "check", "convert", "describe_unit"]);
        assert!(tools.iter().all(|t| t["annotations"]["readOnlyHint"] == true && t["inputSchema"]["type"] == "object"));
    }

    #[test]
    fn evaluate_runs_the_program_in_a_child_and_returns_its_report() {
        let fake = Fake::answering(1, r#"{"ok":false,"results":[],"diagnostics":[{"line":1,"severity":"error","code":"dimension-mismatch","message":"m"}]}"#);
        let reply = call(&fake, "evaluate", json!({ "source": "1 m + 1 s", "seed": 9, "timeout_ms": 250 }));
        let result = &reply["result"];
        assert_eq!(result["isError"], false, "a wrong program is a normal answer");
        assert_eq!(result["structuredContent"]["diagnostics"][0]["code"], "dimension-mismatch");
        assert!(result["content"][0]["text"].as_str().unwrap().contains("dimension-mismatch"));
        let (args, stdin) = fake.asked.borrow()[0].clone();
        assert_eq!(args, ["eval", "-", "--seed", "9", "--timeout-ms", "250"]);
        assert_eq!(stdin, "1 m + 1 s");
    }

    #[test]
    fn a_child_that_crashes_or_hangs_is_a_tool_error_and_the_server_goes_on() {
        let crashed = Fake { answer: Ok((-1073741571, String::new(), "stack overflow".into())), asked: RefCell::new(Vec::new()) };
        let reply = call(&crashed, "evaluate", json!({ "source": "1 m" }));
        assert_eq!(reply["result"]["isError"], true);
        assert!(reply["result"]["structuredContent"]["error"].as_str().unwrap().contains("stack overflow"));

        let hung = Fake { answer: Err("the evaluation process did not finish within 8000 ms and was stopped".into()), asked: RefCell::new(Vec::new()) };
        let reply = call(&hung, "evaluate", json!({ "source": "1 m" }));
        assert_eq!(reply["result"]["isError"], true);
        // and the next request is served
        assert!(ask(&hung, json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" })).unwrap().get("result").is_some());
    }

    #[test]
    fn check_reports_findings_and_a_program_that_does_not_parse() {
        let findings = Fake::answering(1, r#"[{"file":"-","line":1,"severity":"error","code":"dimension-mismatch","message":"m"}]"#);
        let reply = call(&findings, "check", json!({ "source": "1 m + 1 s" }));
        assert_eq!(reply["result"]["structuredContent"]["ok"], false);
        assert_eq!(reply["result"]["structuredContent"]["findings"][0]["code"], "dimension-mismatch");
        assert_eq!(findings.asked.borrow()[0].0, ["check", "-", "--json"]);

        let clean = Fake::answering(0, "[]");
        assert_eq!(call(&clean, "check", json!({ "source": "1 m" }))["result"]["structuredContent"]["ok"], true);

        let unparsable = Fake { answer: Ok((2, "[]".into(), "-: does not parse: oops".into())), asked: RefCell::new(Vec::new()) };
        let reply = call(&unparsable, "check", json!({ "source": "((" }));
        assert_eq!(reply["result"]["structuredContent"]["findings"][0]["code"], "syntax");
    }

    #[test]
    fn convert_and_describe_unit_answer_in_process_without_a_child() {
        let fake = Fake::answering(0, "");
        let converted = call(&fake, "convert", json!({ "value": 2.5, "from": "km", "to": "m" }));
        assert_eq!(converted["result"]["isError"], false);
        assert_eq!(converted["result"]["structuredContent"]["value"], 2500.0);
        let described = call(&fake, "describe_unit", json!({ "symbol": "N" }));
        assert_eq!(described["result"]["structuredContent"]["dimension"], "L·M·T⁻²");
        assert!(fake.asked.borrow().is_empty(), "no child process for a catalog lookup");
        // a refusal from the core is a tool error with its message
        let refused = call(&fake, "convert", json!({ "value": 1, "from": "m", "to": "s" }));
        assert_eq!(refused["result"]["isError"], true);
    }

    #[test]
    fn bad_arguments_are_a_tool_error_that_names_the_argument() {
        let fake = Fake::answering(0, "");
        let reply = call(&fake, "evaluate", json!({}));
        assert_eq!(reply["result"]["isError"], true);
        assert!(reply["result"]["structuredContent"]["error"].as_str().unwrap().contains("source"));
        let too_big = call(&fake, "check", json!({ "source": "x".repeat(MAX_SOURCE_BYTES + 1) }));
        assert_eq!(too_big["result"]["isError"], true);
        assert!(fake.asked.borrow().is_empty());
    }

    #[test]
    fn protocol_errors_have_the_json_rpc_codes() {
        let fake = Fake::answering(0, "");
        assert_eq!(handle_line("{not json", &fake).unwrap()["error"]["code"], -32700);
        assert_eq!(handle_line("[1,2]", &fake).unwrap()["error"]["code"], -32600);
        assert_eq!(ask(&fake, json!({ "jsonrpc": "2.0", "id": 1, "method": "nope" })).unwrap()["error"]["code"], -32601);
        assert_eq!(call(&fake, "no_such_tool", json!({}))["error"]["code"], -32602);
        let missing = ask(&fake, json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/read", "params": { "uri": "phs://other" } })).unwrap();
        assert_eq!(missing["error"]["code"], -32002);
        // an answer from the client is not a request
        assert!(handle_line(r#"{"jsonrpc":"2.0","id":3,"result":{}}"#, &fake).is_none());
    }

    #[test]
    fn the_language_reference_is_a_resource() {
        let fake = Fake::answering(0, "");
        let listed = ask(&fake, json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list" })).unwrap();
        assert_eq!(listed["result"]["resources"][0]["uri"], LANGUAGE_URI);
        let read = ask(&fake, json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/read", "params": { "uri": LANGUAGE_URI } })).unwrap();
        let text = read["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.len() > 500 && text.contains('#'), "{} bytes", text.len());
    }

    #[test]
    fn serve_answers_each_line_and_stops_at_the_end_of_input() {
        let fake = Fake::answering(0, "");
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n";
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &fake);
        let lines: Vec<&str> = std::str::from_utf8(&output).unwrap().lines().collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("\"id\":1") && lines[1].contains("\"id\":2"));
    }

    #[test]
    fn a_child_that_outlives_its_limit_is_killed() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("ping", ["-n", "30", "127.0.0.1"].map(String::from).to_vec())
        } else {
            ("sleep", vec!["30".to_string()])
        };
        let started = Instant::now();
        let error = run_child(std::path::Path::new(program), &args, "", Duration::from_millis(300)).unwrap_err();
        assert!(error.contains("did not finish"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
    }

    #[test]
    fn a_child_that_ends_returns_its_exit_code_and_output() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd", ["/C", "echo hello"].map(String::from).to_vec())
        } else {
            ("sh", ["-c", "echo hello"].map(String::from).to_vec())
        };
        let (code, stdout, _) = run_child(std::path::Path::new(program), &args, "", Duration::from_secs(20)).unwrap();
        assert_eq!(code, 0);
        assert!(stdout.contains("hello"), "{stdout:?}");
    }
}
