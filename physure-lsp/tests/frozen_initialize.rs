//! The language server under a frozen configuration that cannot be used: `initialize` answers (the server does not
//! crash, which would only make the client restart it in a loop), the problem is shown once as an error message, and
//! an opened document gets one error diagnostic that says why it is not analysed.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn frame(json: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{json}", json.len()).into_bytes()
}

/// The `Content-Length` of the next message's headers (0 when they name none), or `None` at the end of the stream.
fn content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            return Some(length);
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
}

/// The body of the next framed message, or `None` at the end of the stream.
fn read_message(reader: &mut impl BufRead) -> Option<String> {
    let mut body = vec![0; content_length(reader)?];
    reader.read_exact(&mut body).ok()?;
    Some(String::from_utf8_lossy(&body).into_owned())
}

/// Reads framed messages from the server's stdout into a channel.
fn messages(stdout: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Some(message) = read_message(&mut reader) {
            if send.send(message).is_err() {
                return;
            }
        }
    });
    receive
}

/// The messages received until one satisfies `done`, or a failure after 60 seconds.
fn until(receive: &mpsc::Receiver<String>, done: impl Fn(&str) -> bool) -> Vec<String> {
    let mut seen = Vec::new();
    loop {
        let message = receive
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("the server did not answer; received so far: {seen:#?}"));
        let finished = done(&message);
        seen.push(message);
        if finished {
            return seen;
        }
    }
}

#[test]
fn an_unusable_frozen_configuration_is_reported_and_the_server_keeps_running() {
    let dir = std::env::temp_dir().join(format!("physure_lsp_frozen_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("physure.frozen.conf"), "not a frozen configuration").unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_physure-lsp"))
        .current_dir(&dir)
        .env_remove("PHYSURE_CONF")
        .env_remove("PHYSURE_REQUIRE_FROZEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the language server starts");
    let mut stdin = server.stdin.take().unwrap();
    let receive = messages(server.stdout.take().unwrap());

    stdin.write_all(&frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#)).unwrap();
    stdin.flush().unwrap();
    let seen = until(&receive, |m| m.contains(r#""id":1"#));
    assert!(seen.last().unwrap().contains("capabilities"), "initialize answered: {seen:#?}");

    stdin.write_all(&frame(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)).unwrap();
    stdin.flush().unwrap();
    let seen = until(&receive, |m| m.contains("window/showMessage"));
    let shown = seen.last().unwrap();
    assert!(shown.contains("physure.frozen.conf") && shown.contains(r#""type":1"#), "one error message names the file: {shown}");
    let open = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///x.phs","languageId":"phs","version":1,"text":"x = 3 m\n"}}}"#;
    stdin.write_all(&frame(open)).unwrap();
    stdin.flush().unwrap();
    let seen = until(&receive, |m| m.contains("textDocument/publishDiagnostics"));
    let diagnostics = seen.last().unwrap();
    assert!(diagnostics.contains("physure.frozen.conf") && diagnostics.contains(r#""severity":1"#), "{diagnostics}");

    // still alive: it answers a request after all that
    stdin.write_all(&frame(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#)).unwrap();
    stdin.flush().unwrap();
    until(&receive, |m| m.contains(r#""id":2"#));
    stdin.write_all(&frame(r#"{"jsonrpc":"2.0","method":"exit"}"#)).unwrap();
    drop(stdin);
    let status = server.wait().unwrap();
    assert!(status.success(), "{status:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
