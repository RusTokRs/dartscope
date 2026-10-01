//! The `dartscope-lsp` binary driven the way an editor drives it: framed JSON-RPC over pipes.

use std::io::{Cursor, Write};
use std::process::{Command, Stdio};
use std::thread;

use dartscope_lsp::rpc::read_frame;
use serde_json::{Value, json};

fn frame(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).unwrap();
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// Runs the server on `input`, closes its stdin and returns its exit code and every message it
/// wrote to stdout.
fn run(input: Vec<u8>) -> (Option<i32>, Vec<Value>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dartscope-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the language server starts");
    let mut stdin = child.stdin.take().unwrap();
    // Feeding stdin from another thread keeps a full stdout pipe from blocking the test.
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().expect("the server ends");
    writer.join().unwrap();

    let mut sent = Vec::new();
    let mut cursor = Cursor::new(output.stdout);
    while let Some(body) = read_frame(&mut cursor).expect("the server writes whole frames") {
        sent.push(serde_json::from_slice(&body).expect("the server writes JSON"));
    }
    (
        output.status.code(),
        sent,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn frames(messages: &[Value]) -> Vec<u8> {
    messages.iter().flat_map(frame).collect()
}

fn handshake() -> Vec<Value> {
    vec![
        request(
            1,
            "initialize",
            json!({ "processId": null, "rootUri": "file:///work/app", "capabilities": {} }),
        ),
        notification("initialized", json!({})),
    ]
}

#[test]
fn an_editor_session_works_end_to_end() {
    let uri = "file:///work/app/lib/%E2%98%83.dart";
    let text = "class Snow {\n  int flakes = 0;\n  void melt() {}\n}\n<<<<<<< HEAD\n";
    let mut input = handshake();
    input.push(notification(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "dart", "version": 7, "text": text } }),
    ));
    input.push(request(
        2,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    ));
    input.push(request(3, "shutdown", Value::Null));
    input.push(notification("exit", Value::Null));

    let (code, sent, stderr) = run(frames(&input));

    assert_eq!(code, Some(0), "stderr: {stderr}");
    // initialize, publishDiagnostics, documentSymbol, shutdown.
    assert_eq!(sent.len(), 4, "{sent:?}");
    let capabilities = &sent[0]["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"]["change"], 2);
    assert_eq!(capabilities["documentSymbolProvider"], true);

    let published = &sent[1];
    assert_eq!(published["method"], "textDocument/publishDiagnostics");
    // The client's own spelling of the URI comes back, percent-escapes and all.
    assert_eq!(published["params"]["uri"], uri);
    assert_eq!(published["params"]["version"], 7);
    let diagnostics = published["params"]["diagnostics"].as_array().unwrap();
    let marker = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "merge_conflict_marker")
        .expect("a diagnostic for the conflict marker");
    assert_eq!(marker["range"]["start"]["line"], 4);

    let outline = &sent[2]["result"];
    assert_eq!(outline[0]["name"], "Snow");
    assert_eq!(
        outline[0]["selectionRange"]["start"],
        json!({ "line": 0, "character": 6 })
    );
    let members: Vec<_> = outline[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["name"].as_str().unwrap())
        .collect();
    assert_eq!(members, ["flakes", "melt"]);
}

#[test]
fn exit_codes_follow_the_lifecycle() {
    let mut clean = handshake();
    clean.push(request(2, "shutdown", Value::Null));
    clean.push(notification("exit", Value::Null));
    assert_eq!(run(frames(&clean)).0, Some(0));

    let mut abrupt = handshake();
    abrupt.push(notification("exit", Value::Null));
    assert_eq!(run(frames(&abrupt)).0, Some(1));

    // The editor went away without saying anything.
    assert_eq!(run(frames(&handshake())).0, Some(1));
    assert_eq!(run(Vec::new()).0, Some(1));
}

#[test]
fn an_oversized_message_is_refused_without_being_read() {
    let (code, sent, _) = run(b"Content-Length: 99999999999\r\n\r\n".to_vec());

    assert_eq!(code, Some(1));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["id"], Value::Null);
    assert_eq!(sent[0]["error"]["code"], -32600);
}

#[test]
fn malformed_input_gets_a_parse_error_and_the_session_goes_on() {
    let mut input = b"Content-Length: 3\r\n\r\n{{{".to_vec();
    input.extend(frames(&handshake()));
    input.extend(frames(&[
        request(2, "shutdown", Value::Null),
        notification("exit", Value::Null),
    ]));

    let (code, sent, _) = run(input);

    assert_eq!(code, Some(0));
    assert_eq!(sent[0]["error"]["code"], -32700);
    assert_eq!(sent[1]["id"], 1);
}
