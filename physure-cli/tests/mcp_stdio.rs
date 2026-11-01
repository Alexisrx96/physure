//! `phs mcp` as a real process: a client's session over stdin and stdout, with `evaluate` and `check` running
//! their programs in child `phs` processes.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

/// Sends every request, closes stdin, and returns the replies in order.
fn session(requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_phs"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start phs mcp");
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "phs mcp ended with {:?}: {}", output.status, String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().lines().map(|line| serde_json::from_str(line).expect("every line is JSON")).collect()
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

#[test]
fn a_whole_session_with_the_four_tools_and_the_resource() {
    let replies = session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        call(3, "evaluate", json!({ "source": "d = 120 m\nt = 8 s\nd / t" })),
        call(4, "evaluate", json!({ "source": "1 m + 1 s" })),
        call(5, "check", json!({ "source": "1 m + 1 s" })),
        call(6, "convert", json!({ "value": 36, "from": "km/h", "to": "m/s" })),
        call(7, "describe_unit", json!({ "symbol": "kWh" })),
        json!({ "jsonrpc": "2.0", "id": 8, "method": "resources/read", "params": { "uri": "phs://language" } }),
    ]);
    assert_eq!(replies.len(), 8, "the notification gets no reply: {replies:?}");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 4);

    let speed = &replies[2]["result"]["structuredContent"];
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(speed["ok"], true, "{speed}");
    assert_eq!(speed["results"][2]["value"], 15.0);
    assert_eq!(speed["results"][2]["dimension"], "L·T⁻¹");

    let wrong = &replies[3]["result"]["structuredContent"];
    assert_eq!(wrong["ok"], false);
    assert_eq!(wrong["diagnostics"][0]["code"], "dimension-mismatch");

    let checked = &replies[4]["result"]["structuredContent"];
    assert_eq!((checked["ok"].clone(), checked["findings"][0]["code"].clone()), (json!(false), json!("dimension-mismatch")));

    assert_eq!(replies[5]["result"]["structuredContent"]["value"], 10.0);
    assert_eq!(replies[6]["result"]["structuredContent"]["dimension"], "L²·M·T⁻²");
    assert!(replies[7]["result"]["contents"][0]["text"].as_str().unwrap().len() > 500);
}

#[test]
fn the_sandbox_holds_through_the_server() {
    let replies = session(&[
        call(1, "evaluate", json!({ "source": "fn f(x) = f(x)\nf(1)" })),
        call(2, "evaluate", json!({ "source": "use export3d from plot\nexport3d(\"sin(x)*cos(y)\", \"mcp_must_not_write.stl\", \"stl\")" })),
        call(3, "evaluate", json!({ "source": "2 m" })),
    ]);
    assert_eq!(replies[0]["result"]["structuredContent"]["diagnostics"][0]["code"], "limit");
    assert_eq!(replies[1]["result"]["structuredContent"]["diagnostics"][0]["code"], "forbidden");
    assert!(!std::path::Path::new("mcp_must_not_write.stl").exists());
    // and the server was still there for the third
    assert_eq!(replies[2]["result"]["structuredContent"]["ok"], true);
}

#[test]
fn garbage_on_the_wire_is_an_error_reply_and_the_server_goes_on() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_phs")).arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "this is not json").unwrap();
    writeln!(stdin, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" })).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["result"], json!({}));
}
