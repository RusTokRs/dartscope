//! JSON-RPC 2.0 message handling and `Content-Length` framing for the DartScope language server.
//!
//! [`handle_message`] is the protocol state machine and does no I/O: it maps one decoded message
//! to the messages the server sends in answer. [`serve`] is the loop that reads frames from a
//! reader, hands them to it and writes the answers to a writer; the `dartscope-lsp` binary is
//! `serve` over stdin and stdout.
//!
//! Lifecycle, as the protocol defines it:
//!
//! - Before `initialize`, requests are answered with `ServerNotInitialized` (-32002) and
//!   notifications other than `exit` are dropped.
//! - After `shutdown`, requests are answered with `InvalidRequest` (-32600) and only `exit` is
//!   meaningful. `exit` ends the session with code 0 after `shutdown` and with 1 without it.
//! - A message that is not valid JSON is answered with `ParseError` (-32700) and a `null` id; a
//!   frame that cannot be read ends the session with code 1, because the stream can no longer be
//!   trusted to be aligned on message boundaries.

use std::fmt;
use std::io::{self, BufRead, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::server::DartLspServer;
use crate::types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, HoverParams, InitializeParams, PublishDiagnosticsParams, ReferenceParams,
    TextDocumentPositionParams, Url,
};

/// The largest message body the server reads. A longer `Content-Length` ends the session instead
/// of allocating what a peer claims to send.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// The longest header line the server reads.
const MAX_HEADER_LINE_BYTES: u64 = 8 * 1024;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const SERVER_NOT_INITIALIZED: i64 = -32002;

/// What handling one message produced.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Responses and notifications to send, in order.
    pub messages: Vec<Value>,
    /// The exit code, once the client has sent `exit`.
    pub exit: Option<i32>,
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    /// The headers ended without a `Content-Length`.
    MissingContentLength,
    /// The `Content-Length` is not a number.
    InvalidContentLength(String),
    /// The `Content-Length` is larger than [`MAX_MESSAGE_BYTES`].
    TooLarge(usize),
    /// A header line is longer than the server reads.
    HeaderTooLong,
    /// The input ended inside a frame.
    Truncated,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Io(error) => write!(formatter, "cannot read a message: {error}"),
            FrameError::MissingContentLength => {
                formatter.write_str("the message headers have no Content-Length")
            }
            FrameError::InvalidContentLength(value) => {
                write!(formatter, "Content-Length is not a number: `{value}`")
            }
            FrameError::TooLarge(length) => write!(
                formatter,
                "Content-Length {length} exceeds the limit of {MAX_MESSAGE_BYTES} bytes"
            ),
            FrameError::HeaderTooLong => formatter.write_str("a message header line is too long"),
            FrameError::Truncated => formatter.write_str("the input ended inside a message"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Runs the server until the client sends `exit` or the input ends, and returns the exit code the
/// process should use: 0 for `exit` after `shutdown`, 1 otherwise.
///
/// # Errors
///
/// Returns the I/O error when the reader or the writer fails.
pub fn serve<R: BufRead, W: Write>(
    server: &mut DartLspServer,
    reader: &mut R,
    writer: &mut W,
) -> io::Result<i32> {
    loop {
        let body = match read_frame(reader) {
            Ok(Some(body)) => body,
            Ok(None) => return Ok(if server.is_shutting_down() { 0 } else { 1 }),
            Err(FrameError::Io(error)) => return Err(error),
            Err(error) => {
                let response = error_response(Value::Null, INVALID_REQUEST, &error.to_string());
                write_message(writer, &response)?;
                return Ok(1);
            }
        };
        let outcome = match serde_json::from_slice::<Value>(&body) {
            Ok(message) => handle_message(server, message),
            Err(error) => Outcome {
                messages: vec![error_response(
                    Value::Null,
                    PARSE_ERROR,
                    &format!("invalid JSON: {error}"),
                )],
                exit: None,
            },
        };
        for message in &outcome.messages {
            write_message(writer, message)?;
        }
        if let Some(code) = outcome.exit {
            return Ok(code);
        }
    }
}

/// Reads one `Content-Length` framed message body; `None` at a clean end of input.
///
/// # Errors
///
/// A [`FrameError`] when the headers are malformed, the length exceeds [`MAX_MESSAGE_BYTES`] or
/// the input ends inside the frame.
pub fn read_frame<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>, FrameError> {
    let mut content_length = None;
    let mut started = false;
    loop {
        let mut line = Vec::new();
        let read = read_header_line(reader, &mut line)?;
        if read == 0 {
            return if started {
                Err(FrameError::Truncated)
            } else {
                Ok(None)
            };
        }
        started = true;
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            break;
        }
        if let Some((name, value)) = text.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            let value = value.trim();
            let length = value
                .parse::<usize>()
                .map_err(|_| FrameError::InvalidContentLength(value.to_string()))?;
            content_length = Some(length);
        }
    }
    let length = content_length.ok_or(FrameError::MissingContentLength)?;
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            FrameError::Truncated
        } else {
            FrameError::Io(error)
        }
    })?;
    Ok(Some(body))
}

fn read_header_line<R: BufRead>(reader: &mut R, line: &mut Vec<u8>) -> Result<usize, FrameError> {
    let read = reader
        .by_ref()
        .take(MAX_HEADER_LINE_BYTES)
        .read_until(b'\n', line)
        .map_err(FrameError::Io)?;
    if read as u64 == MAX_HEADER_LINE_BYTES && line.last() != Some(&b'\n') {
        return Err(FrameError::HeaderTooLong);
    }
    Ok(read)
}

/// Writes one message with its `Content-Length` header and flushes it.
///
/// # Errors
///
/// The writer's I/O error.
pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

/// Handles one decoded message and returns what the server sends in answer.
pub fn handle_message(server: &mut DartLspServer, message: Value) -> Outcome {
    let mut outcome = Outcome::default();
    let Value::Object(object) = message else {
        outcome.messages.push(error_response(
            Value::Null,
            INVALID_REQUEST,
            "a message must be a JSON object; batches are not part of the protocol",
        ));
        return outcome;
    };
    let id = object.get("id").filter(|id| !id.is_null()).cloned();
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        // The server sends no requests, so a message without a method is either a stray response,
        // which is ignored, or a request that names no method.
        let is_response = object.contains_key("result") || object.contains_key("error");
        if let (Some(id), false) = (id, is_response) {
            outcome.messages.push(error_response(
                id,
                INVALID_REQUEST,
                "the request has no method",
            ));
        }
        return outcome;
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    match id {
        Some(id) => handle_request(server, method, params, id, &mut outcome),
        None => handle_notification(server, method, params, &mut outcome),
    }
    outcome
}

fn handle_request(
    server: &mut DartLspServer,
    method: &str,
    params: Value,
    id: Value,
    outcome: &mut Outcome,
) {
    let reply = if method == "initialize" {
        if server.is_initialized() {
            Err(RpcError::new(
                INVALID_REQUEST,
                "initialize was already requested",
            ))
        } else {
            parse_params::<InitializeParams>(params).and_then(|params| {
                let result = server
                    .initialize(params)
                    .map_err(|error| RpcError::new(INTERNAL_ERROR, error.to_string()))?;
                to_result(&result)
            })
        }
    } else if !server.is_initialized() {
        Err(RpcError::new(
            SERVER_NOT_INITIALIZED,
            "the server has not been initialized",
        ))
    } else if server.is_shutting_down() {
        Err(RpcError::new(
            INVALID_REQUEST,
            "the server is shutting down",
        ))
    } else {
        dispatch_request(server, method, params)
    };
    outcome.messages.push(match reply {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => error_response(id, error.code, &error.message),
    });
}

fn dispatch_request(
    server: &mut DartLspServer,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    match method {
        "shutdown" => {
            server
                .shutdown()
                .map_err(|error| RpcError::new(INTERNAL_ERROR, error.to_string()))?;
            Ok(Value::Null)
        }
        "textDocument/definition" => {
            let params = parse_params::<TextDocumentPositionParams>(params)?;
            // A document the client never opened has nothing to resolve; the answer is "no result".
            let locations = server
                .definition(&params.text_document.uri, params.position)
                .ok()
                .flatten();
            to_result(&locations)
        }
        "textDocument/references" => {
            let params = parse_params::<ReferenceParams>(params)?;
            let locations = server
                .references_with_declaration(
                    &params.text_document_position.text_document.uri,
                    params.text_document_position.position,
                    params.context.include_declaration,
                )
                .ok()
                .flatten();
            to_result(&locations)
        }
        "textDocument/hover" => {
            let params = parse_params::<HoverParams>(params)?;
            let hover = server
                .hover(
                    &params.text_document_position_params.text_document.uri,
                    params.text_document_position_params.position,
                )
                .ok()
                .flatten();
            to_result(&hover)
        }
        "textDocument/documentSymbol" => {
            let params = parse_params::<DocumentSymbolParams>(params)?;
            let uri = params.text_document.uri.clone();
            let symbols = server.document_symbols(&uri, params).ok().flatten();
            to_result(&symbols)
        }
        _ => Err(RpcError::new(
            METHOD_NOT_FOUND,
            format!("unsupported request `{method}`"),
        )),
    }
}

fn handle_notification(
    server: &mut DartLspServer,
    method: &str,
    params: Value,
    outcome: &mut Outcome,
) {
    if method == "exit" {
        outcome.exit = Some(if server.is_shutting_down() { 0 } else { 1 });
        return;
    }
    if method == "initialized" {
        server.initialized();
        return;
    }
    // Notifications before `initialize` and after `shutdown` are dropped, as the protocol says;
    // so is every notification this server has no use for (`$/cancelRequest`, `didSave`, ...).
    if !server.is_initialized() || server.is_shutting_down() {
        return;
    }
    match method {
        "textDocument/didOpen" => {
            if let Ok(params) = parse_params::<DidOpenTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_open(params);
                publish_diagnostics(server, &uri, outcome);
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) = parse_params::<DidChangeTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_change(params);
                publish_diagnostics(server, &uri, outcome);
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = parse_params::<DidCloseTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_close(params);
                // The client discards what is shown for a closed document.
                push_diagnostics(outcome, uri, None, Vec::new());
            }
        }
        _ => {}
    }
}

/// Publishes the diagnostics of an open document after it changed.
fn publish_diagnostics(server: &DartLspServer, uri: &Url, outcome: &mut Outcome) {
    let version = server.document_version(uri);
    push_diagnostics(outcome, uri.clone(), version, server.diagnostics(uri));
}

fn push_diagnostics(
    outcome: &mut Outcome,
    uri: Url,
    version: Option<i32>,
    diagnostics: Vec<crate::types::Diagnostic>,
) {
    let params = PublishDiagnosticsParams {
        uri,
        version,
        diagnostics,
    };
    if let Ok(params) = serde_json::to_value(params) {
        outcome.messages.push(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": params,
        }));
    }
}

struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params)
        .map_err(|error| RpcError::new(INVALID_PARAMS, format!("invalid params: {error}")))
}

fn to_result<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|error| {
        RpcError::new(INTERNAL_ERROR, format!("cannot encode the result: {error}"))
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

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

    /// Runs a whole session over in-memory pipes: the exit code and every message sent.
    fn session(input: Vec<u8>) -> (i32, Vec<Value>) {
        let mut server = DartLspServer::new(".");
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        let code = serve(&mut server, &mut reader, &mut output).unwrap();
        let mut sent = Vec::new();
        let mut cursor = Cursor::new(output);
        while let Some(body) = read_frame(&mut cursor).unwrap() {
            sent.push(serde_json::from_slice(&body).unwrap());
        }
        (code, sent)
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
    fn initialize_advertises_capabilities_with_the_protocol_names() {
        let mut input = handshake();
        input.push(request(2, "shutdown", Value::Null));
        input.push(notification("exit", Value::Null));
        let (code, sent) = session(frames(&input));

        assert_eq!(code, 0);
        assert_eq!(sent.len(), 2);
        let capabilities = &sent[0]["result"]["capabilities"];
        assert_eq!(
            capabilities["textDocumentSync"],
            json!({ "openClose": true, "change": 2 })
        );
        for provider in [
            "definitionProvider",
            "referencesProvider",
            "hoverProvider",
            "documentSymbolProvider",
        ] {
            assert_eq!(capabilities[provider], json!(true), "{provider}");
        }
        assert_eq!(sent[0]["result"]["serverInfo"]["name"], "dartscope-lsp");
        assert!(capabilities.get("text_document_sync").is_none());
        assert_eq!(
            sent[1],
            json!({ "jsonrpc": "2.0", "id": 2, "result": null })
        );
    }

    #[test]
    fn exit_without_shutdown_is_a_failure_and_the_end_of_input_too() {
        let mut input = handshake();
        input.push(notification("exit", Value::Null));
        assert_eq!(session(frames(&input)).0, 1);
        assert_eq!(session(frames(&handshake())).0, 1);

        let mut after_shutdown = handshake();
        after_shutdown.push(request(2, "shutdown", Value::Null));
        // The end of the input after `shutdown` is a clean end.
        assert_eq!(session(frames(&after_shutdown)).0, 0);
    }

    #[test]
    fn requests_follow_the_lifecycle() {
        let hover = json!({
            "textDocument": { "uri": "file:///a.dart" },
            "position": { "line": 0, "character": 0 }
        });
        let (_, before) = session(frames(&[
            request(1, "textDocument/hover", hover.clone()),
            notification("exit", Value::Null),
        ]));
        assert_eq!(before[0]["error"]["code"], -32002);
        assert_eq!(before[0]["id"], 1);

        let mut input = handshake();
        input.push(request(2, "initialize", json!({})));
        input.push(request(3, "shutdown", Value::Null));
        input.push(request(4, "textDocument/hover", hover));
        input.push(notification("exit", Value::Null));
        let (code, sent) = session(frames(&input));
        assert_eq!(code, 0);
        assert_eq!(sent[1]["id"], 2);
        assert_eq!(sent[1]["error"]["code"], -32600, "initialize twice");
        assert_eq!(sent[2]["result"], Value::Null);
        assert_eq!(sent[3]["id"], 4);
        assert_eq!(sent[3]["error"]["code"], -32600, "request after shutdown");
    }

    #[test]
    fn protocol_errors_are_answered_and_never_dropped() {
        let mut input = handshake();
        input.push(request(2, "textDocument/rename", json!({})));
        input.push(request(3, "$/unknown", json!({})));
        input.push(request(4, "textDocument/hover", json!({ "position": 3 })));
        input.push(json!([{ "jsonrpc": "2.0", "id": 5, "method": "shutdown" }]));
        input.push(json!({ "jsonrpc": "2.0", "id": 6 }));
        // A stray response and unknown notifications are ignored without an answer.
        input.push(json!({ "jsonrpc": "2.0", "id": 7, "result": null }));
        input.push(notification("$/cancelRequest", json!({ "id": 2 })));
        input.push(notification("workspace/didChangeConfiguration", json!({})));
        input.push(request(8, "shutdown", Value::Null));
        input.push(notification("exit", Value::Null));
        let (code, sent) = session(frames(&input));

        assert_eq!(code, 0);
        let codes: Vec<_> = sent
            .iter()
            .skip(1)
            .map(|message| (message["id"].clone(), message["error"]["code"].clone()))
            .collect();
        assert_eq!(
            codes,
            [
                (json!(2), json!(-32601)),
                (json!(3), json!(-32601)),
                (json!(4), json!(-32602)),
                (Value::Null, json!(-32600)),
                (json!(6), json!(-32600)),
                (json!(8), Value::Null),
            ]
        );
    }

    #[test]
    fn invalid_json_is_a_parse_error_with_a_null_id_and_the_session_continues() {
        let mut input = Vec::new();
        input.extend(b"Content-Length: 5\r\n\r\n{nope");
        input.extend(frames(&handshake()));
        input.extend(frames(&[
            request(2, "shutdown", Value::Null),
            notification("exit", Value::Null),
        ]));
        let (code, sent) = session(input);

        assert_eq!(code, 0);
        assert_eq!(sent[0]["error"]["code"], -32700);
        assert_eq!(sent[0]["id"], Value::Null);
        assert_eq!(sent[1]["id"], 1);
    }

    #[test]
    fn a_frame_that_cannot_be_read_ends_the_session_with_a_null_id_error() {
        let cases: [(&[u8], &str); 5] = [
            (b"Content-Type: x\r\n\r\n{}", "no Content-Length"),
            (b"Content-Length: abc\r\n\r\n", "not a number"),
            (b"Content-Length: 99999999999\r\n\r\n", "exceeds the limit"),
            (b"Content-Length: 50\r\n\r\n{}", "ended inside a message"),
            (b"Content-Length: 5\r\n", "ended inside a message"),
        ];
        for (input, expected) in cases {
            let (code, sent) = session(input.to_vec());
            assert_eq!(code, 1, "{expected}");
            assert_eq!(sent.len(), 1, "{expected}");
            assert_eq!(sent[0]["id"], Value::Null);
            let message = sent[0]["error"]["message"].as_str().unwrap();
            assert!(message.contains(expected), "{message} / {expected}");
        }
    }

    #[test]
    fn header_names_are_case_insensitive_and_other_headers_are_ignored() {
        let body = serde_json::to_vec(&request(1, "shutdown", Value::Null)).unwrap();
        let mut input = format!(
            "content-length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
            body.len()
        )
        .into_bytes();
        input.extend(body);
        let mut cursor = Cursor::new(input);
        let read = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&read).unwrap()["method"],
            "shutdown"
        );
        assert!(read_frame(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn a_header_line_without_a_newline_is_not_buffered_without_bound() {
        let mut cursor = Cursor::new(vec![b'a'; 1024 * 1024]);
        assert!(matches!(
            read_frame(&mut cursor),
            Err(FrameError::HeaderTooLong)
        ));
    }

    #[test]
    fn documents_are_synchronized_and_diagnostics_are_published() {
        let uri = "file:///work/app/lib/main.dart";
        let mut input = handshake();
        input.push(notification(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri, "languageId": "dart", "version": 1,
                "text": "<<<<<<< HEAD\nclass A {}\n"
            }}),
        ));
        input.push(notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 1, "character": 0 } },
                    "text": ""
                }]
            }),
        ));
        input.push(notification(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        ));
        input.push(request(2, "shutdown", Value::Null));
        input.push(notification("exit", Value::Null));
        let (code, sent) = session(frames(&input));

        assert_eq!(code, 0);
        let published: Vec<_> = sent
            .iter()
            .filter(|message| message["method"] == "textDocument/publishDiagnostics")
            .collect();
        assert_eq!(published.len(), 3);
        // Opened with a conflict marker: one diagnostic on line 0, for version 1.
        assert_eq!(published[0]["params"]["uri"], uri);
        assert_eq!(published[0]["params"]["version"], 1);
        let diagnostics = published[0]["params"]["diagnostics"].as_array().unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["code"], "merge_conflict_marker");
        assert_eq!(diagnostics[0]["severity"], 2);
        assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
        // After the marker line is deleted there is nothing to report.
        assert_eq!(published[1]["params"]["version"], 2);
        assert_eq!(published[1]["params"]["diagnostics"], json!([]));
        // Closing clears what the client shows.
        assert_eq!(published[2]["params"]["uri"], uri);
        assert_eq!(published[2]["params"]["diagnostics"], json!([]));
    }

    #[test]
    fn navigation_answers_use_the_uris_and_utf16_columns_of_the_client() {
        // The declaring file is named with a percent-encoded non-ASCII letter, as editors write it.
        let lib = "file:///work/app/lib/%C3%BCber.dart";
        let main = "file:///work/app/lib/main.dart";
        let main_text = "import '\u{fc}ber.dart';\n/* \u{1F600} */ void run() { Widget(); }\n";
        // The emoji is one character, two UTF-16 units and four bytes before the call.
        let line = main_text.lines().nth(1).unwrap();
        let at = line[..line.find("Widget").unwrap()].encode_utf16().count();
        let mut input = handshake();
        input.push(notification(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": lib, "languageId": "dart", "version": 1,
                                       "text": "class Widget {}\n" }}),
        ));
        input.push(notification(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": main, "languageId": "dart", "version": 1,
                                       "text": main_text }}),
        ));
        input.push(request(
            2,
            "textDocument/definition",
            json!({ "textDocument": { "uri": main },
                    "position": { "line": 1, "character": at } }),
        ));
        input.push(request(
            3,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": lib } }),
        ));
        input.push(request(
            4,
            "textDocument/hover",
            json!({ "textDocument": { "uri": "file:///never/opened.dart" },
                    "position": { "line": 0, "character": 0 } }),
        ));
        input.push(request(5, "shutdown", Value::Null));
        input.push(notification("exit", Value::Null));
        let (code, sent) = session(frames(&input));
        assert_eq!(code, 0);

        let answer = |id: i64| {
            sent.iter()
                .find(|message| message["id"] == id)
                .unwrap_or_else(|| panic!("no answer for {id}"))
        };
        let definition = &answer(2)["result"];
        assert_eq!(definition[0]["uri"], lib);
        assert_eq!(
            definition[0]["range"],
            json!({ "start": { "line": 0, "character": 6 }, "end": { "line": 0, "character": 12 } })
        );
        let symbols = &answer(3)["result"];
        assert_eq!(symbols[0]["name"], "Widget");
        assert_eq!(symbols[0]["kind"], 5);
        assert_eq!(symbols[0]["selectionRange"]["start"]["character"], 6);
        // A document the client never opened has no result; it is not an error.
        assert_eq!(answer(4)["result"], Value::Null);
    }
}
