//! Language-agnostic source span representation

use serde::{Deserialize, Serialize};

/// Language-agnostic source code span
///
/// Represents a contiguous region of source code, independent of the parser used.
/// All parsers must convert their native span types to this representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Byte offset of the start of the span (inclusive)
    pub start: usize,
    /// Byte offset of the end of the span (exclusive)
    pub end: usize,
    /// Line number of the start (1-indexed)
    pub start_line: u32,
    /// Line number of the end (1-indexed)
    pub end_line: u32,
    /// Column number of the start (0-indexed, in bytes)
    pub start_col: u32,
}

impl SourceSpan {
    /// Create a new source span
    pub fn new(start: usize, end: usize, start_line: u32, end_line: u32, start_col: u32) -> Self {
        SourceSpan {
            start,
            end,
            start_line,
            end_line,
            start_col,
        }
    }

    /// Get the length of the span in bytes
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Check if this span contains another span
    pub fn contains(&self, other: &SourceSpan) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Check if this span overlaps with another span
    ///
    /// Zero-width spans (where start == end) don't overlap with anything,
    /// including themselves.
    pub fn overlaps(&self, other: &SourceSpan) -> bool {
        // Zero-width spans don't overlap
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.start < other.end && other.start < self.end
    }
}

/// Convert from SWC Span to SourceSpan
impl From<swc_common::Span> for SourceSpan {
    fn from(span: swc_common::Span) -> Self {
        // Note: We can't get line/col from a bare Span without SourceMap
        // This will be handled by the parser which has access to SourceMap
        SourceSpan {
            start: span.lo.0 as usize,
            end: span.hi.0 as usize,
            start_line: 0, // To be filled in by parser
            end_line: 0,   // To be filled in by parser
            start_col: 0,  // To be filled in by parser
        }
    }
}

/// Helper to convert SWC Span to SourceSpan with line/column info
pub fn span_with_location(
    span: swc_common::Span,
    source_map: &swc_common::SourceMap,
) -> SourceSpan {
    let start_loc = source_map.lookup_char_pos(span.lo);
    let end_loc = source_map.lookup_char_pos(span.hi);
    SourceSpan {
        start: span.lo.0 as usize,
        end: span.hi.0 as usize,
        start_line: start_loc.line as u32,
        end_line: end_loc.line as u32,
        start_col: start_loc.col.0 as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let span = SourceSpan::new(10, 20, 1, 3, 5);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.start_line, 1);
        assert_eq!(span.end_line, 3);
        assert_eq!(span.start_col, 5);
    }

    #[test]
    fn test_len() {
        let span = SourceSpan::new(10, 20, 1, 3, 5);
        assert_eq!(span.len(), 10);

        let empty_span = SourceSpan::new(10, 10, 1, 1, 5);
        assert_eq!(empty_span.len(), 0);
    }

    #[test]
    fn test_is_empty() {
        let span = SourceSpan::new(10, 20, 1, 3, 5);
        assert!(!span.is_empty());

        let empty_span = SourceSpan::new(10, 10, 1, 1, 5);
        assert!(empty_span.is_empty());

        let backwards_span = SourceSpan::new(20, 10, 1, 1, 5);
        assert!(backwards_span.is_empty());
    }

    #[test]
    fn test_contains() {
        let outer = SourceSpan::new(10, 30, 1, 5, 5);
        let inner = SourceSpan::new(15, 25, 2, 4, 10);
        let outside = SourceSpan::new(5, 15, 1, 2, 0);

        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
        assert!(!outer.contains(&outside));

        // Span contains itself
        assert!(outer.contains(&outer));
    }

    #[test]
    fn test_overlaps() {
        let span1 = SourceSpan::new(10, 20, 1, 3, 5);
        let span2 = SourceSpan::new(15, 25, 2, 4, 10);
        let span3 = SourceSpan::new(25, 30, 4, 5, 20);

        assert!(span1.overlaps(&span2));
        assert!(span2.overlaps(&span1));
        assert!(!span1.overlaps(&span3));
        assert!(!span3.overlaps(&span1));

        // Non-empty span overlaps with itself
        assert!(span1.overlaps(&span1));
    }

    #[test]
    fn test_edge_cases() {
        // Adjacent spans don't overlap
        let span1 = SourceSpan::new(10, 20, 1, 3, 5);
        let span2 = SourceSpan::new(20, 30, 3, 5, 15);
        assert!(!span1.overlaps(&span2));

        // Zero-width span
        let zero_span = SourceSpan::new(15, 15, 2, 2, 10);
        assert!(span1.contains(&zero_span));
        assert!(!zero_span.overlaps(&span1)); // Zero-width doesn't overlap
    }
}
