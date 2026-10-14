//! Process-level smoke tests: spawn the real stdio binary, speak raw
//! `Content-Length`-framed JSON-RPC to it, and assert protocol-visible
//! behavior end to end (framing, lifecycle, publications, exit codes).

use std::io::{Read, Write};
use std::process::{ChildStdin, Command, Stdio};

use serde_json::{Value, json};

fn send(stdin: &mut ChildStdin, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdin.flush().unwrap();
}

fn read_all<R: Read>(mut stream: R) -> Vec<Value> {
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let mut messages = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let header_end = buf[pos..]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("header terminator");
        let header = std::str::from_utf8(&buf[pos..pos + header_end]).unwrap();
        let len = header
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .expect("content-length header");
        let start = pos + header_end + 4;
        let body = &buf[start..start + len];
        messages.push(serde_json::from_slice::<Value>(body).unwrap());
        pos = start + len;
    }
    messages
}

fn spawn_server() -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_dartscope-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn dartscope-lsp")
}

fn message_with_id<'a>(messages: &'a [Value], id: i64) -> &'a Value {
    messages
        .iter()
        .find(|msg| msg.get("id").and_then(Value::as_i64) == Some(id))
        .unwrap_or_else(|| panic!("no response with id {id} in {messages:?}"))
}

#[test]
fn initialize_to_shutdown_round_trips_over_real_framing() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"rootUri":"file:///smoke"}}),
    );
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let main_text = "class Widget {}\nclass App { void go() { Widget(); } }\n";
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{
            "uri":"file:///smoke/lib/main.dart",
            "languageId":"dart",
            "version":42,
            "text":main_text
        }}}),
    );
    // UTF-16 character position of `Widget` inside `Widget();` on line 2.
    assert_eq!(&main_text.lines().nth(1).unwrap()[24..30], "Widget");
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover",
                "params":{"textDocument":{"uri":"file:///smoke/lib/main.dart"},
                          "position":{"line":1,"character":25}}}),
    );
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
    );
    send(&mut stdin, &json!({"jsonrpc":"2.0","method":"exit"}));
    drop(stdin);
    let mut stdout = child.stdout.take().unwrap();
    let messages = read_all(&mut stdout);
    let status = child.wait().unwrap();

    let init = message_with_id(&messages, 1);
    assert_eq!(
        init["result"]["capabilities"]["workspaceSymbolProvider"],
        json!(true)
    );
    assert_eq!(init["result"]["serverInfo"]["name"], json!("dartscope-lsp"));

    let published = messages
        .iter()
        .find(|msg| msg.get("method") == Some(&json!("textDocument/publishDiagnostics")))
        .expect("publishDiagnostics notification");
    assert_eq!(published["params"]["version"], json!(42));

    let hover = message_with_id(&messages, 2);
    let contents = hover["result"]["contents"].as_array().unwrap();
    assert!(
        contents
            .iter()
            .any(|entry| entry["language"] == json!("dart")
                && entry["value"]
                    .as_str()
                    .is_some_and(|v| v.contains("Widget"))),
        "expected a dart code block in {contents:?}"
    );

    assert_eq!(
        message_with_id(&messages, 3),
        &json!({"jsonrpc":"2.0","id":3,"result":null})
    );
    assert!(status.success(), "clean shutdown must exit 0, got {status}");
}

#[test]
fn exit_without_shutdown_is_reported_as_an_abrupt_stop() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    send(&mut stdin, &json!({"jsonrpc":"2.0","method":"exit"}));
    drop(stdin);
    let mut stdout = child.stdout.take().unwrap();
    let _ = read_all(&mut stdout);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(1), "abrupt exit must be non-zero");
}

#[test]
fn end_of_input_closes_the_loop_cleanly() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
    );
    drop(stdin);
    let mut stdout = child.stdout.take().unwrap();
    let messages = read_all(&mut stdout);
    let status = child.wait().unwrap();
    assert_eq!(messages.len(), 2);
    assert!(status.success(), "EOF after shutdown exits 0, got {status}");
}

#[test]
fn pre_initialize_requests_are_rejected_by_the_process() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":5,"method":"textDocument/definition","params":{}}),
    );
    send(&mut stdin, &json!({"jsonrpc":"2.0","method":"exit"}));
    drop(stdin);
    let mut stdout = child.stdout.take().unwrap();
    let messages = read_all(&mut stdout);
    let _ = child.wait().unwrap();
    let error = message_with_id(&messages, 5);
    assert_eq!(error["error"]["code"], json!(-32002));
}
