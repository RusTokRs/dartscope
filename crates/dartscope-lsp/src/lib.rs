//! Language Server Protocol bridge for DartScope.
//!
//! This crate provides the editor-facing LSP layer on top of the
//! deterministic `dartscope-index` analysis. It is intentionally
//! isolated from filesystem I/O: the server holds an incremental
//! `DartWorkspaceIndex` and receives document contents via LSP
//! `textDocument/didOpen` / `didChange` notifications.
//!
//! The crate is `0.1` and `planned` in `docs/development/dartscope-library-plan.md#DS-LSP-001`;
//! the initial implementation covers:
//! - LSP lifecycle (`initialize` / `initialized` / `shutdown` / `exit`)
//! - incremental document synchronization (full and incremental `didChange`)
//! - UTF-16 ↔ UTF-8 coordinate conversion (LF, CRLF, surrogate pairs)
//! - definition / references / hover / documentSymbol backed by `dartscope-index`
//!   without inventing member/type results unavailable from the index.
//!
//! Positions are `crate::types::Position` (0-indexed line, 0-indexed UTF-16 character)
 //! and are converted to `dartscope_core::SourceSpan` (1-indexed line, 1-indexed char
 //! column, byte offsets) via `coordinates`.

pub mod coordinates;
pub mod server;
pub mod types;

pub use coordinates::{
    byte_offset_to_lsp_position, lsp_position_to_byte_offset, lsp_range_to_source_span,
    source_span_to_lsp_range,
};
pub use server::{DartLspServer, LspError};
