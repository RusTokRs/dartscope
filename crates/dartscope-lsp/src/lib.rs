//! Language Server Protocol bridge for DartScope.
//!
//! This crate provides the editor-facing LSP layer on top of the
//! deterministic `dartscope-index` analysis. The server and the protocol layer are isolated from
//! filesystem I/O: the server holds an incremental `DartWorkspaceIndex` and receives document
//! contents via LSP `textDocument/didOpen` / `didChange` notifications and the files of the project
//! as text from a `WorkspaceSource`. `FsWorkspace` is the one adapter that reads the filesystem.
//!
//! The crate is `0.1`; its status and remaining gaps are tracked in
//! `docs/development/dartscope-library-plan.md#DS-LSP-001`. It covers:
//! - LSP lifecycle (`initialize` / `initialized` / `shutdown` / `exit`) with the protocol's error
//!   codes and exit codes, and `Content-Length` framing with a bounded message size (`rpc`)
//! - incremental document synchronization (full and incremental `didChange`, clamped ranges)
//! - `publishDiagnostics` after every open and change
//! - UTF-16 ↔ UTF-8 coordinate conversion (`\n`, `\r\n`, `\r`, surrogate pairs)
//! - definition / references / hover / documentSymbol backed by `dartscope-index`
//!   without inventing member/type results unavailable from the index
//! - a workspace model (`docs/development/lsp.md`): the Dart files of the project, its
//!   `pubspec.yaml` and `.dart_tool/package_config.json` are loaded when the client is ready, so
//!   navigation and `workspace/symbol` see files that are not open and `package:` imports resolve;
//!   `workspace/didChangeWatchedFiles` keeps them current.
//!
//! Positions are `crate::types::Position` (0-indexed line, 0-indexed UTF-16 character)
//! and are converted to `dartscope_core::SourceSpan` (1-indexed line, 1-indexed char
//! column, byte offsets) via `coordinates`.

pub mod coordinates;
pub mod fs_workspace;
pub mod rpc;
pub mod server;
pub mod types;

pub use coordinates::{
    LineIndex, byte_offset_to_lsp_position, lsp_position_to_byte_offset, lsp_range_to_source_span,
    source_span_to_lsp_range,
};
pub use fs_workspace::FsWorkspace;
pub use server::{
    DartLspServer, LspError, NoWorkspace, WorkspaceFile, WorkspaceScan, WorkspaceSource,
};
