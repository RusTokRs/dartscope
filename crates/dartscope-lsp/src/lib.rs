//! Language Server Protocol bridge for DartScope.
//!
//! This crate provides the editor-facing LSP layer on top of the
//! deterministic `dartscope-index` analysis. It is intentionally
//! isolated from filesystem I/O and from higher-level crates such as
//! `dartscope-lints`: the server holds an incremental `DartWorkspaceIndex`
//! and receives document contents via LSP `textDocument/didOpen` /
//! `didChange` notifications.
//!
//! The crate is versioned `0.1` and tracks
//! `docs/development/dartscope-library-plan.md` (`DS-LSP-001`). The initial
//! implementation covers:
//! - LSP lifecycle (`initialize` / `initialized` / `shutdown` / `exit`)
//! - incremental document synchronization (full and incremental `didChange`)
//! - UTF-16 ↔ UTF-8 coordinate conversion (LF, CRLF, surrogate pairs)
//! - definition / references / hover / `documentSymbol` / `workspace/symbol`
//!   backed by `dartscope-index` without inventing member/type results
//!   unavailable from the index
//! - `textDocument/publishDiagnostics` pushed by the stdio binary after each
//!   sync event, plus `set_supplemental_diagnostics` as the explicit port for
//!   downstream lint adapters (this crate never depends on `dartscope-lints`)
//! - `snapshot_generation` as the staleness signal for cached editor answers.
//!
//! Positions are `crate::types::Position` (0-indexed line, 0-indexed UTF-16
//! character) and are converted to `dartscope_core::SourceSpan` (1-indexed
//! line, 1-indexed char column, byte offsets) via `coordinates`. Wire JSON
//! matches the `lsp-types` 0.97 camelCase shape for every handled method.

pub mod coordinates;
pub mod server;
pub mod types;

pub use coordinates::{
    byte_offset_to_lsp_position, lsp_position_to_byte_offset, lsp_range_to_source_span,
    source_span_to_lsp_range,
};
pub use server::{DartLspServer, LspError};
