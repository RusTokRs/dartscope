//! Minimal stdio LSP server for DartScope.
//!
//! Reads `Content-Length` framed JSON-RPC from stdin and writes responses and notifications to
//! stdout; see `dartscope_lsp::rpc` for the protocol handling. No async runtime and no
//! filesystem scans: every document the server knows was sent by the client.
//!
//! Exit code: 0 after `shutdown` and `exit`, 1 when the session ends any other way.

use std::io;
use std::process::ExitCode;

use dartscope_lsp::DartLspServer;
use dartscope_lsp::rpc::serve;

fn main() -> ExitCode {
    let mut server = DartLspServer::new(".");
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    match serve(&mut server, &mut reader, &mut writer) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("dartscope-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}
