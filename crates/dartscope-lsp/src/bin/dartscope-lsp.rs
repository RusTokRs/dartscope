//! Minimal stdio LSP server for DartScope.
//!
//! Reads LSP `Content-Length` framed JSON-RPC from stdin and writes
//! responses/notifications to stdout. No async runtime, no hidden
//! filesystem scans — every `textDocument/*` carries its content.

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
    let mut server = DartLspServer::new(".");
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
            let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');
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
        for response in handle_message(&mut server, &msg) {
            let body = serde_json::to_string(&response).unwrap_or_else(|_| "null".to_string());
            write!(stdout, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
            stdout.flush()?;
        }
        if is_exit {
            break;
        }
    }
    Ok(())
}

/// Handles one inbound message and returns every outbound message to write,
/// in order: a response for requests plus any server-initiated notifications
/// (currently `textDocument/publishDiagnostics`).
fn handle_message(server: &mut DartLspServer, msg: &Value) -> Vec<Value> {
    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
        return Vec::new();
    };
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    match method {
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
    }
}

fn to_result<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn publish_diagnostics(server: &DartLspServer, uri: &Url) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": server.diagnostics(uri)
        }
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
                    "text": "class Foo { Foo.new(); }\n"
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
            outgoing[0]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .len()
                >= 1
        );
    }

    #[test]
    fn did_close_clears_diagnostics() {
        let mut server = DartLspServer::new(".");
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
}
