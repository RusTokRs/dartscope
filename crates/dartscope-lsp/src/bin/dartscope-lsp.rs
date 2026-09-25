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
    DocumentSymbolParams, HoverParams, InitializeParams, ReferenceParams, TextDocumentPositionParams,
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
        // Read headers until empty line
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
        if let Some(response) = handle_message(&mut server, &msg) {
            let body = serde_json::to_string(&response).unwrap();
            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )?;
            stdout.flush()?;
        }
        if msg.get("method").and_then(|m| m.as_str()) == Some("exit") {
            break;
        }
    }
    Ok(())
}

fn handle_message(server: &mut DartLspServer, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str())?;
    let id = msg.get("id").cloned();
    // Notifications have no id; requests have id
    match method {
        "initialize" => {
            let params: InitializeParams =
                serde_json::from_value(msg.get("params").cloned().unwrap_or(Value::Null))
                    .unwrap_or_default();
            let result = server.initialize(params).ok()?;
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        }
        "initialized" => {
            server.initialized();
            None
        }
        "shutdown" => {
            let _ = server.shutdown();
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": Value::Null }))
        }
        "exit" => None,
        "$/cancelRequest" => None,
        "textDocument/didOpen" => {
            if let Ok(params) =
                serde_json::from_value::<DidOpenTextDocumentParams>(msg.get("params").cloned().unwrap_or(Value::Null))
            {
                server.did_open(params);
            }
            None
        }
        "textDocument/didChange" => {
            if let Ok(params) =
                serde_json::from_value::<DidChangeTextDocumentParams>(msg.get("params").cloned().unwrap_or(Value::Null))
            {
                server.did_change(params);
            }
            None
        }
        "textDocument/didClose" => {
            if let Ok(params) =
                serde_json::from_value::<DidCloseTextDocumentParams>(msg.get("params").cloned().unwrap_or(Value::Null))
            {
                server.did_close(params);
            }
            None
        }
        "textDocument/definition" => {
            let params: TextDocumentPositionParams =
                serde_json::from_value(msg.get("params").cloned().unwrap_or(Value::Null)).ok()?;
            let result = server
                .definition(&params.text_document.uri, params.position)
                .ok()
                .flatten()
                .unwrap_or_default();
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        }
        "textDocument/references" => {
            let params: ReferenceParams =
                serde_json::from_value(msg.get("params").cloned().unwrap_or(Value::Null)).ok()?;
            let result = server
                .references(&params.text_document_position.text_document.uri, params.text_document_position.position)
                .ok()
                .flatten()
                .unwrap_or_default();
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        }
        "textDocument/hover" => {
            let params: HoverParams =
                serde_json::from_value(msg.get("params").cloned().unwrap_or(Value::Null)).ok()?;
            let result = server
                .hover(&params.text_document_position_params.text_document.uri, params.text_document_position_params.position)
                .ok()
                .flatten();
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams =
                serde_json::from_value(msg.get("params").cloned().unwrap_or(Value::Null)).ok()?;
            let result = server
                .document_symbols(&params.text_document.uri, params)
                .ok()
                .flatten()
                .unwrap_or_default();
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        }
        _ => {
            // Honest empty for unsupported requests
            if id.is_some() {
                Some(json!({ "jsonrpc": "2.0", "id": id, "result": Value::Null }))
            } else {
                None
            }
        }
    }
}
