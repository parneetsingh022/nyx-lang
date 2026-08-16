// SPDX-License-Identifier: MIT OR Apache-2.0

use miette::SourceSpan;

/// A half-open byte range within a source file.
///
/// The span represents `start..end`, where `end` is the first byte
/// immediately after the covered source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Byte index where this span starts.
    start: u32,

    /// Byte index immediately after this span ends.
    end: u32,
}

impl Span {
    /// Creates a new span covering `start..end`.
    ///
    /// # Panics
    ///
    /// Panics if `start` is greater than `end`.
    pub const fn new(start: u32, end: u32) -> Self {
        assert!(start <= end, "span start must not exceed span end");
        Self { start, end }
    }

    /// Creates a span covering from the start of `first`
    /// through the end of `last`.
    ///
    /// The spans are expected to appear in source order.
    ///
    /// # Panics
    ///
    /// Panics if `first` starts after `last`.
    pub const fn from_bounds(first: Self, last: Self) -> Self {
        assert!(
            first.start <= last.start,
            "spans must appear in source order"
        );

        Self::new(first.start, last.end)
    }

    /// Returns the length of this span in bytes.
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns whether this span covers no bytes.
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Converts the span into a range usable for indexing source text.
    pub const fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }

    /// Returns the start offset of span
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Returns the end offset of span
    pub const fn end(self) -> u32 {
        self.end
    }
}

impl From<Span> for SourceSpan {
    fn from(span: Span) -> Self {
        // Calculate the length from your end and start offsets
        let offset = span.start as usize;
        let length = span.len() as usize;

        Self::new(offset.into(), length)
    }
}

#[cfg(test)]
mod tests {
    use super::Span;
    use miette::SourceSpan;

    #[test]
    fn new_creates_half_open_span() {
        let span = Span::new(3, 8);

        assert_eq!(span.start, 3);
        assert_eq!(span.end, 8);
    }

    #[test]
    fn new_allows_empty_span() {
        let span = Span::new(5, 5);

        assert_eq!(span.start, 5);
        assert_eq!(span.end, 5);
        assert!(span.is_empty());
    }

    #[test]
    #[should_panic(expected = "span start must not exceed span end")]
    fn new_panics_for_reversed_span() {
        Span::new(8, 3);
    }

    #[test]
    fn from_bounds_combines_spans() {
        let first = Span::new(2, 5);
        let last = Span::new(8, 12);

        assert_eq!(Span::from_bounds(first, last), Span::new(2, 12));
    }

    #[test]
    fn from_bounds_allows_touching_spans() {
        let first = Span::new(2, 5);
        let last = Span::new(5, 8);

        assert_eq!(Span::from_bounds(first, last), Span::new(2, 8));
    }

    #[test]
    fn from_bounds_allows_overlapping_spans() {
        let first = Span::new(2, 8);
        let last = Span::new(5, 10);

        assert_eq!(Span::from_bounds(first, last), Span::new(2, 10));
    }

    #[test]
    fn from_bounds_allows_same_start() {
        let first = Span::new(4, 6);
        let last = Span::new(4, 9);

        assert_eq!(Span::from_bounds(first, last), Span::new(4, 9));
    }

    #[test]
    #[should_panic(expected = "spans must appear in source order")]
    fn from_bounds_panics_when_first_starts_after_last() {
        let first = Span::new(8, 10);
        let last = Span::new(3, 5);

        Span::from_bounds(first, last);
    }

    #[test]
    fn len_returns_half_open_byte_length() {
        assert_eq!(Span::new(0, 0).len(), 0);
        assert_eq!(Span::new(0, 1).len(), 1);
        assert_eq!(Span::new(3, 8).len(), 5);
    }

    #[test]
    fn is_empty_detects_empty_spans() {
        assert!(Span::new(4, 4).is_empty());
        assert!(!Span::new(4, 5).is_empty());
    }

    #[test]
    fn range_returns_usize_range() {
        let span = Span::new(3, 8);

        assert_eq!(span.range(), 3usize..8usize);
    }

    #[test]
    fn range_can_index_source_text() {
        let source = "hello world";
        let span = Span::new(6, 11);

        assert_eq!(&source[span.range()], "world");
    }

    #[test]
    fn default_span_is_empty_at_start() {
        let span = Span::default();

        assert_eq!(span, Span::new(0, 0));
        assert!(span.is_empty());
    }

    #[test]
    fn converts_to_miette_source_span() {
        let span = Span::new(3, 8);
        let source_span = SourceSpan::from(span);

        assert_eq!(source_span.offset(), 3);
        assert_eq!(source_span.len(), 5);
    }

    #[test]
    fn empty_span_converts_to_zero_length_source_span() {
        let span = Span::new(7, 7);
        let source_span = SourceSpan::from(span);

        assert_eq!(source_span.offset(), 7);
        assert_eq!(source_span.len(), 0);
    }
}
