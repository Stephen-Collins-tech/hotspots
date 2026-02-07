//! AST adapter layer for function discovery
//!
//! Global invariants enforced:
//! - Deterministic traversal order by (file, span.start)
//! - Formatting, comments, and whitespace must not affect results

use crate::language::SourceSpan;
use swc_ecma_ast::*;

/// Function identifier: (file_index, local_index)
///
/// IDs are internal only and must never appear in user output.
/// Generated deterministically during traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId {
    pub file_index: usize,
    pub local_index: usize,
}

/// Stable abstraction for a function node in the AST
#[derive(Debug, Clone)]
pub struct FunctionNode {
    pub id: FunctionId,
    pub name: Option<String>,
    pub span: SourceSpan,
    pub body: BlockStmt,
    pub suppression_reason: Option<String>,
}

impl FunctionNode {
    /// Extract the start line number from the span
    ///
    /// Note: For backwards compatibility, this method still exists but now
    /// simply returns the line number from the SourceSpan.
    pub fn start_line(&self, _source_map: &swc_common::SourceMap) -> u32 {
        self.span.start_line
    }

    /// Get the start line number directly from the span
    pub fn line(&self) -> u32 {
        self.span.start_line
    }
}
