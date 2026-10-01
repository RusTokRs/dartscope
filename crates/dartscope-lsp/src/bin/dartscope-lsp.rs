//! Minimal stdio LSP server for DartScope.
//!
//! Reads `Content-Length` framed JSON-RPC from stdin and writes responses and notifications to
//! stdout; see `dartscope_lsp::rpc` for the protocol handling. No async runtime: the server is
//! single-threaded. The documents the client opens arrive as text, and the files of the project
//! (Dart sources, `pubspec.yaml`, `.dart_tool/package_config.json`) are read once, when the client
//! says it is ready, and again whenever the client reports that one changed.
//!
//! Exit code: 0 after `shutdown` and `exit`, 1 when the session ends any other way.

use std::io;
use std::process::ExitCode;

use dartscope_lsp::rpc::serve_with;
use dartscope_lsp::{DartLspServer, FsWorkspace};

fn main() -> ExitCode {
    let mut server = DartLspServer::new(".");
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    match serve_with(&mut server, &FsWorkspace, &mut reader, &mut writer) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("dartscope-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}
