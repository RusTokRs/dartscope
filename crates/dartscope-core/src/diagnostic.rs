//! Diagnostics, their severity and confidence.

use serde::{Deserialize, Serialize};

use crate::path::normalize_path;
use crate::span::SourceSpan;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DartDiagnostic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

impl DartDiagnostic {
    pub fn info(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            path: None,
            code: code.into(),
            severity: DiagnosticSeverity::Info,
            message: message.into(),
            span,
            confidence: None,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            path: None,
            code: code.into(),
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            span,
            confidence: None,
        }
    }

    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            path: None,
            code: code.into(),
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            span,
            confidence: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(normalize_path(path.into()));
        self
    }

    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}
