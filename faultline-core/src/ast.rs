//! AST adapter layer for function discovery
//!
//! Global invariants enforced:
//! - Deterministic traversal order by (file, span.start)
//! - Formatting, comments, and whitespace must not affect results

use swc_common::Span;
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
    pub span: Span,
    pub body: BlockStmt,
    pub suppression_reason: Option<String>,
}

impl FunctionNode {
    /// Extract the start line number from the span
    pub fn start_line(&self, source_map: &swc_common::SourceMap) -> u32 {
        let loc = source_map.lookup_char_pos(self.span.lo);
        loc.line as u32
    }
}
