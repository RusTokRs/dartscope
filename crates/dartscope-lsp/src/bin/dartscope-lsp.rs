//! Minimal stdio LSP server for DartScope.
//!
//! Reads LSP `Content-Length` framed JSON-RPC from stdin and writes
//! responses/notifications to stdout. No async runtime, no hidden
//! filesystem scans — every `textDocument/*` carries its content.
//!
//! Usage: `dartscope-lsp [root] [--lints]`. The optional `--lints` flag only
//! works when the binary is built with the crate's `lints` feature; it runs the
//! all-rules `dartscope-lints` configuration after every sync event and routes
//! the findings through the server's supplemental diagnostics port.

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use dartscope_lsp::DartLspServer;
use dartscope_lsp::types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, HoverParams, InitializeParams, ReferenceParams,
    TextDocumentPositionParams, Url, WorkspaceSymbolParams,
};
use serde_json::{Value, json};

fn main() -> io::Result<()> {
    let mut root = String::from(".");
    #[cfg(feature = "lints")]
    let mut with_lints = false;
    #[cfg(not(feature = "lints"))]
    let with_lints = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--lints" => {
                #[cfg(feature = "lints")]
                {
                    with_lints = true;
                }
                #[cfg(not(feature = "lints"))]
                {
                    eprintln!("dartscope-lsp: `--lints` needs the `lints` cargo feature; ignoring");
                }
            }
            other => root = other.to_string(),
        }
    }
    let mut server = DartLspServer::new(root);
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut stdout = io::stdout();

    loop {
        let mut headers = HashMap::new();
        let mut line = String::new();
        // Read headers until the empty separator line.
        loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                return Ok(());
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let len = headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        if len == 0 {
            continue;
        }
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body)?;
        let msg: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_exit = msg.get("method").and_then(|m| m.as_str()) == Some("exit");
        for response in handle_message_with_options(&mut server, &msg, with_lints) {
            let body = serde_json::to_string(&response).unwrap_or_else(|_| "null".to_string());
            write!(stdout, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
            stdout.flush()?;
        }
        if is_exit {
            // The LSP exit notification asks for a zero exit code only after a
            // successful shutdown; an abrupt exit leaves the status non-zero.
            if !server.is_shutdown() {
                std::process::exit(1);
            }
            break;
        }
    }
    Ok(())
}

/// Handles one inbound message and returns every outbound message to write,
/// in order: a response for requests plus any server-initiated notifications
/// (currently `textDocument/publishDiagnostics`).
#[cfg(test)]
fn handle_message(server: &mut DartLspServer, msg: &Value) -> Vec<Value> {
    handle_message_with_options(server, msg, false)
}

fn handle_message_with_options(
    server: &mut DartLspServer,
    msg: &Value,
    with_lints: bool,
) -> Vec<Value> {
    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
        return Vec::new();
    };
    let id = msg.get("id").cloned();
    if !server.is_initialized() && method != "initialize" {
        // Requests before `initialize` are answered with the protocol error; stray
        // notifications are dropped instead of mutating server state.
        if id.is_some() {
            return vec![error_response(id, -32002, "ServerNotInitialized")];
        }
        return Vec::new();
    }
    if server.is_shutdown() {
        // After `shutdown` the server stays alive for the `exit` notification only.
        if id.is_some() {
            return vec![error_response(id, -32600, "InvalidRequest")];
        }
        return Vec::new();
    }
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let outgoing = match method {
        "initialize" => {
            let params: InitializeParams = serde_json::from_value(params).unwrap_or_default();
            let result = match server.initialize(params) {
                Ok(result) => serde_json::to_value(result).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            vec![response(id, result)]
        }
        "initialized" => {
            server.initialized();
            Vec::new()
        }
        "shutdown" => {
            let _ = server.shutdown();
            vec![response(id, Value::Null)]
        }
        "exit" | "$/cancelRequest" => Vec::new(),
        "textDocument/didOpen" => {
            let mut outgoing = Vec::new();
            if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_open(params);
                outgoing.push(publish_diagnostics(server, &uri));
            }
            outgoing
        }
        "textDocument/didChange" => {
            let mut outgoing = Vec::new();
            if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_change(params);
                outgoing.push(publish_diagnostics(server, &uri));
            }
            outgoing
        }
        "textDocument/didClose" => {
            let mut outgoing = Vec::new();
            if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(params) {
                let uri = params.text_document.uri.clone();
                server.did_close(params);
                // LSP clients keep the last published set until told otherwise.
                outgoing.push(publish_empty_diagnostics(&uri));
            }
            outgoing
        }
        "textDocument/definition" => {
            let result = serde_json::from_value::<TextDocumentPositionParams>(params)
                .ok()
                .and_then(|params| {
                    server
                        .definition(&params.text_document.uri, params.position)
                        .ok()
                        .flatten()
                })
                .map_or(Value::Null, to_result);
            vec![response(id, result)]
        }
        "textDocument/references" => {
            let result = serde_json::from_value::<ReferenceParams>(params)
                .ok()
                .and_then(|params| {
                    server
                        .references(
                            &params.text_document_position.text_document.uri,
                            params.text_document_position.position,
                        )
                        .ok()
                        .flatten()
                })
                .map_or(Value::Null, to_result);
            vec![response(id, result)]
        }
        "textDocument/hover" => {
            let result = serde_json::from_value::<HoverParams>(params)
                .ok()
                .and_then(|params| {
                    server
                        .hover(
                            &params.text_document_position_params.text_document.uri,
                            params.text_document_position_params.position,
                        )
                        .ok()
                        .flatten()
                })
                .map_or(Value::Null, to_result);
            vec![response(id, result)]
        }
        "textDocument/documentSymbol" => {
            let result = serde_json::from_value::<DocumentSymbolParams>(params)
                .ok()
                .and_then(|params| {
                    server
                        .document_symbols(&params.text_document.uri)
                        .ok()
                        .flatten()
                })
                .map_or(Value::Null, to_result);
            vec![response(id, result)]
        }
        "workspace/symbol" => {
            let result = serde_json::from_value::<WorkspaceSymbolParams>(params)
                .map(|params| server.workspace_symbols(&params))
                .map_or(Value::Null, to_result);
            vec![response(id, result)]
        }
        _ => {
            // Honest empty result for unsupported requests with an id.
            if id.is_some() {
                vec![response(id, Value::Null)]
            } else {
                Vec::new()
            }
        }
    };
    apply_lints_hook(server, msg, method, outgoing, with_lints)
}

/// Post-sync hook for the `lints` feature: after `didOpen`/`didChange` the
/// server reruns the lint adapter over its own snapshot and the publish
/// notification in `outgoing` is regenerated so parser diagnostics and
/// supplemental lint entries reach the client merged and versioned.
fn apply_lints_hook(
    server: &mut DartLspServer,
    msg: &Value,
    method: &str,
    outgoing: Vec<Value>,
    with_lints: bool,
) -> Vec<Value> {
    #[cfg(not(feature = "lints"))]
    let _ = (&server, msg, method, with_lints);
    #[cfg(feature = "lints")]
    {
        if with_lints && matches!(method, "textDocument/didOpen" | "textDocument/didChange") {
            server.refresh_lint_diagnostics(&dartscope_lints::DartLintConfig::all_rules());
            if let Some(uri) = synced_uri(msg) {
                return outgoing
                    .into_iter()
                    .map(|entry| {
                        if entry.get("method").and_then(Value::as_str)
                            == Some("textDocument/publishDiagnostics")
                        {
                            publish_diagnostics(server, &uri)
                        } else {
                            entry
                        }
                    })
                    .collect();
            }
        }
    }
    outgoing
}

#[cfg(feature = "lints")]
fn synced_uri(msg: &Value) -> Option<Url> {
    let uri = msg
        .get("params")?
        .get("textDocument")?
        .get("uri")?
        .as_str()?;
    Url::parse(uri).ok()
}

fn to_result<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn publish_diagnostics(server: &DartLspServer, uri: &Url) -> Value {
    let mut params = json!({
        "uri": uri,
        "diagnostics": server.diagnostics(uri)
    });
    if let Some(version) = server.document_version(uri) {
        params["version"] = json!(version);
    }
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": params
    })
}

fn publish_empty_diagnostics(uri: &Url) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": []
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the spec-required `initialize` handshake so lifecycle-gated
    /// requests and notifications below are accepted by the server.
    fn initialize(server: &mut DartLspServer) {
        let msg = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} });
        handle_message(server, &msg);
    }

    #[test]
    fn initialize_response_uses_camel_case_wire_shape() {
        let mut server = DartLspServer::new(".");
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        let outgoing = handle_message(&mut server, &msg);
        assert_eq!(outgoing.len(), 1);
        let result = &outgoing[0]["result"];
        assert_eq!(
            result["capabilities"]["workspaceSymbolProvider"],
            json!(true)
        );
        assert_eq!(result["capabilities"]["textDocumentSync"], json!(2));
        assert_eq!(result["serverInfo"]["name"], json!("dartscope-lsp"));
        assert_eq!(outgoing[0]["id"], json!(1));
    }

    #[test]
    fn did_open_publishes_diagnostics_notification() {
        let mut server = DartLspServer::new(".");
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "rootUri": "file:///workspace" }
        });
        handle_message(&mut server, &msg);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///workspace/lib/main.dart",
                    "languageId": "dart",
                    "version": 1,
                    "text": "class Foo {\n  new();\n}\n"
                }
            }
        });
        let outgoing = handle_message(&mut server, &msg);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(
            outgoing[0]["method"],
            json!("textDocument/publishDiagnostics")
        );
        assert_eq!(
            outgoing[0]["params"]["uri"],
            json!("file:///workspace/lib/main.dart")
        );
        assert!(
            !outgoing[0]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn did_close_clears_diagnostics() {
        let mut server = DartLspServer::new(".");
        initialize(&mut server);
        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///lib/main.dart",
                    "languageId": "dart",
                    "version": 1,
                    "text": "class A {}\n"
                }
            }
        });
        handle_message(&mut server, &open);
        let close = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": "file:///lib/main.dart" } }
        });
        let outgoing = handle_message(&mut server, &close);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(
            outgoing[0]["method"],
            json!("textDocument/publishDiagnostics")
        );
        assert_eq!(outgoing[0]["params"]["diagnostics"], json!([]));
    }

    #[test]
    fn workspace_symbol_request_filters_open_documents() {
        let mut server = DartLspServer::new(".");
        initialize(&mut server);
        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///lib/payments.dart",
                    "languageId": "dart",
                    "version": 1,
                    "text": "class PaymentGateway {}\nclass Config {}\n"
                }
            }
        });
        handle_message(&mut server, &open);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "workspace/symbol",
            "params": { "query": "gateway" }
        });
        let outgoing = handle_message(&mut server, &msg);
        assert_eq!(outgoing.len(), 1);
        let symbols = outgoing[0]["result"].as_array().unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0]["name"], json!("PaymentGateway"));
        assert_eq!(symbols[0]["kind"], json!(5));
        assert_eq!(
            symbols[0]["location"]["uri"],
            json!("file:///lib/payments.dart")
        );
    }

    #[test]
    fn unsupported_request_returns_null_result() {
        let mut server = DartLspServer::new(".");
        initialize(&mut server);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/completion",
            "params": {}
        });
        let outgoing = handle_message(&mut server, &msg);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0]["result"], Value::Null);
        let msg = json!({ "jsonrpc": "2.0", "method": "window/logMessage", "params": {} });
        assert!(handle_message(&mut server, &msg).is_empty());
    }

    #[test]
    fn requests_before_initialize_get_not_initialized_error() {
        let mut server = DartLspServer::new(".");
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","id":1,"method":"textDocument/definition","params":{}}),
        );
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0]["error"]["code"], json!(-32002));
        // Notifications without an id are dropped instead of mutating server state.
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument": {
                    "uri": "file:///lib/x.dart",
                    "languageId": "dart",
                    "version": 1,
                    "text": "class X {}"
                }
            }}),
        );
        assert!(outgoing.is_empty());
    }

    #[test]
    fn after_shutdown_requests_are_rejected_until_exit() {
        let mut server = DartLspServer::new(".");
        handle_message(
            &mut server,
            &json!({
                "jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"rootUri":"file:///workspace"}
            }),
        );
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        );
        assert_eq!(outgoing.len(), 1);
        assert!(outgoing[0].get("error").is_none());
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{}}),
        );
        assert_eq!(outgoing[0]["error"]["code"], json!(-32600));
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{}}),
        );
        assert!(outgoing.is_empty());
    }

    #[cfg(feature = "lints")]
    #[test]
    fn lints_feature_routes_findings_into_publish_diagnostics() {
        let mut server = DartLspServer::new(".");
        initialize(&mut server);
        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///lib/a.dart",
                    "languageId": "dart",
                    "version": 3,
                    "text": "part 'missing.dart';\nclass A {}\n"
                }
            }
        });
        let outgoing = handle_message_with_options(&mut server, &open, true);
        assert_eq!(outgoing.len(), 1);
        let diags = outgoing[0]["params"]["diagnostics"].as_array().unwrap();
        assert!(
            diags.iter().any(|d| {
                d["code"] == json!("dartscope.unresolved_part")
                    && d["source"] == json!("dartscope-lints")
            }),
            "expected the lint finding in the published set: {diags:?}"
        );
        assert_eq!(outgoing[0]["params"]["version"], json!(3));
        // Fixing the file clears the supplemental entry on the next publish.
        let change = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": "file:///lib/a.dart", "version": 4 },
                "contentChanges": [{ "text": "class A {}\n" }]
            }
        });
        let outgoing = handle_message_with_options(&mut server, &change, true);
        let diags = outgoing[0]["params"]["diagnostics"].as_array().unwrap();
        assert!(
            !diags
                .iter()
                .any(|d| d["source"] == json!("dartscope-lints"))
        );
        assert_eq!(outgoing[0]["params"]["version"], json!(4));
    }

    #[test]
    fn publish_diagnostics_carries_document_version() {
        let mut server = DartLspServer::new(".");
        handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        );
        let outgoing = handle_message(
            &mut server,
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument": {
                    "uri": "file:///lib/v.dart",
                    "languageId": "dart",
                    "version": 7,
                    "text": "class V { new(); }"
                }
            }}),
        );
        assert_eq!(outgoing.len(), 1);
        assert_eq!(
            outgoing[0]["method"],
            json!("textDocument/publishDiagnostics")
        );
        assert_eq!(outgoing[0]["params"]["version"], json!(7));
    }
}
