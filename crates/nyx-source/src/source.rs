// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use miette::{MietteError, NamedSource, SourceCode, SourceSpan, SpanContents};

use crate::Span;

/// Validates that a source length fits within the supported offset range.
///
/// Source offsets and spans are represented as `u32` throughout the source,
/// lexer, and diagnostic infrastructure. Validating the source length once at
/// construction time allows those components to safely convert byte offsets to
/// `u32` without repeating checked conversions.
///
/// # Panics
///
/// Panics if `len` exceeds [`u32::MAX`].
fn validate_source_len(len: usize) {
    u32::try_from(len).expect("source file exceeds the maximum supported size");
}

/// Shared source text used by the lexer, parser, and diagnostics.
///
/// The underlying [`NamedSource`] stores the source contents and filename used
/// by `miette` when rendering labeled spans and code snippets.
///
/// Wrapping it in an [`Arc`] allows the source file to be shared cheaply across
/// the lexer, parser, and multiple diagnostics without duplicating the source
/// text.
#[derive(Debug, Clone)]
pub struct SourceFile(Arc<SourceFileInner>);

#[derive(Debug)]
struct SourceFileInner {
    source: NamedSource<String>,
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Creates a shared source file with the given name and contents.
    ///
    /// The source text is stored in a [`NamedSource`] and wrapped in an [`Arc`]
    /// so it can be cloned and shared cheaply.
    ///
    /// # Panics
    ///
    /// Panics if the source contents exceed the maximum supported size of
    /// [u32::MAX] bytes.
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        let name = name.into();
        let content = content.into();

        let mut line_starts = vec![0];

        validate_source_len(content.len());

        for (offset, byte) in content.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push((offset + 1) as u32);
            }
        }

        Self(Arc::new(SourceFileInner {
            source: NamedSource::new(name, content),
            line_starts,
        }))
    }

    /// Returns the complete source text.
    ///
    /// The returned string slice is borrowed from this source file.
    pub fn contents(&self) -> &str {
        self.0.source.inner().as_str()
    }

    /// Returns the source text covered by `span`.
    ///
    /// # Panics
    ///
    /// Panics if the span is out of bounds, reversed, or does not lie on valid
    /// UTF-8 character boundaries.
    pub fn slice(&self, span: Span) -> &str {
        &self.contents()[span.range()]
    }

    /// Returns the one-based line and character column for a byte offset.
    ///
    /// The offset is interpreted as a byte offset into the source text.
    ///
    /// Computing the column is linear in the number of characters between the
    /// start of the line and `offset`.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is out of bounds or is not a UTF-8 character boundary.
    pub fn location(&self, offset: u32) -> Location {
        let line_index = self.line_index(offset);
        let line_start = self.0.line_starts[line_index] as usize;

        let offset = offset as usize;

        // `offset` and `line_start` are byte offsets. Count `char`s in the line
        // prefix so multibyte UTF-8 characters contribute one column, then add one
        // because source locations use one-based columns.
        let column = self.contents()[line_start..offset].chars().count() + 1;
        Location {
            line: (line_index + 1) as u32,
            column: column as u32,
        }
    }

    /// Returns the one-based line number containing `offset`.
    ///
    /// The offset is interpreted as a byte offset into the source text.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is out of bounds or is not a valid UTF-8 character
    /// boundary.
    pub fn line_of(&self, offset: u32) -> u32 {
        (self.line_index(offset) + 1) as u32
    }

    /// Returns whether two byte offsets are on the same source line.
    ///
    /// # Panics
    ///
    /// Panics if either offset is out of bounds or is not a valid UTF-8 character
    /// boundary.
    pub fn is_same_line(&self, first: u32, second: u32) -> bool {
        self.line_index(first) == self.line_index(second)
    }

    /// Returns the index in `line_starts` for the line containing `offset`.
    ///
    /// Each entry in `line_starts` is the byte offset at which a source line
    /// begins. The returned index is therefore also the zero-based line number.
    ///
    /// `offset` is interpreted as a byte offset into the source text. If it equals
    /// a value in `line_starts`, the index of that value is returned. Otherwise,
    /// the index of the nearest preceding line start is returned.
    ///
    /// The end of the source is a valid offset and belongs to the final line.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is greater than the source length or is not a valid
    /// UTF-8 character boundary.
    fn line_index(&self, offset: u32) -> usize {
        let offset_usize = offset as usize;
        let contents = self.contents();

        assert!(
            offset_usize <= contents.len(),
            "source offset is out of bounds"
        );

        assert!(
            contents.is_char_boundary(offset_usize),
            "source offset is not a UTF-8 character boundary"
        );

        match self.0.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => {
                debug_assert!(index > 0);
                index - 1
            }
        }
    }
}

impl SourceCode for SourceFile {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        self.0
            .source
            .read_span(span, context_lines_before, context_lines_after)
    }
}

/// A one-based location within source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// One-based line number.
    pub line: u32,

    /// One-based column measured in Unicode scalar values.
    pub column: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_maximum_supported_source_length() {
        validate_source_len(u32::MAX as usize);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "source file exceeds the maximum supported size")]
    fn rejects_source_larger_than_maximum_supported_size() {
        validate_source_len(u32::MAX as usize + 1);
    }

    #[test]
    fn stores_source_contents() {
        let source = SourceFile::new("test.nyx", "let answer = 42;");

        assert_eq!(source.contents(), "let answer = 42;");
    }

    #[test]
    fn cloned_source_has_same_contents_and_locations() {
        let source = SourceFile::new("test.nyx", "first\nsecond");
        let cloned = source.clone();

        assert_eq!(cloned.contents(), source.contents());
        assert_eq!(cloned.location(6), source.location(6));
    }

    #[test]
    fn empty_source_has_one_empty_line() {
        let source = SourceFile::new("empty.nyx", "");

        assert_eq!(source.line_of(0), 1);
        assert_eq!(source.location(0), Location { line: 1, column: 1 });
    }

    #[test]
    fn location_returns_one_based_line_and_column() {
        let source = SourceFile::new("test.nyx", "abc\ndef");

        assert_eq!(source.location(0), Location { line: 1, column: 1 });

        assert_eq!(source.location(2), Location { line: 1, column: 3 });

        assert_eq!(source.location(3), Location { line: 1, column: 4 });

        assert_eq!(source.location(4), Location { line: 2, column: 1 });

        assert_eq!(source.location(7), Location { line: 2, column: 4 });
    }

    #[test]
    fn newline_byte_belongs_to_preceding_line() {
        let source = SourceFile::new("test.nyx", "abc\ndef");

        assert_eq!(source.line_of(3), 1);
        assert_eq!(source.location(3), Location { line: 1, column: 4 });
    }

    #[test]
    fn offset_after_newline_belongs_to_next_line() {
        let source = SourceFile::new("test.nyx", "abc\ndef");

        assert_eq!(source.line_of(4), 2);
        assert_eq!(source.location(4), Location { line: 2, column: 1 });
    }

    #[test]
    fn trailing_newline_creates_final_empty_line() {
        let source = SourceFile::new("test.nyx", "abc\n");

        assert_eq!(source.line_of(4), 2);
        assert_eq!(source.location(4), Location { line: 2, column: 1 });
    }

    #[test]
    fn handles_consecutive_newlines() {
        let source = SourceFile::new("test.nyx", "a\n\nb");

        assert_eq!(source.line_of(0), 1);
        assert_eq!(source.line_of(2), 2);
        assert_eq!(source.line_of(3), 3);

        assert_eq!(source.location(2), Location { line: 2, column: 1 });

        assert_eq!(source.location(3), Location { line: 3, column: 1 });
    }

    #[test]
    fn location_counts_unicode_characters_not_bytes() {
        let source = SourceFile::new("unicode.nyx", "aé日\nx");

        // Byte layout:
        // a   = byte 0
        // é   = bytes 1..3
        // 日  = bytes 3..6
        // \n  = byte 6
        // x   = byte 7

        assert_eq!(source.location(1), Location { line: 1, column: 2 });

        assert_eq!(source.location(3), Location { line: 1, column: 3 });

        assert_eq!(source.location(6), Location { line: 1, column: 4 });

        assert_eq!(source.location(7), Location { line: 2, column: 1 });
    }

    #[test]
    fn line_of_returns_one_based_line_number() {
        let source = SourceFile::new("test.nyx", "one\ntwo\nthree");

        assert_eq!(source.line_of(0), 1);
        assert_eq!(source.line_of(3), 1);
        assert_eq!(source.line_of(4), 2);
        assert_eq!(source.line_of(7), 2);
        assert_eq!(source.line_of(8), 3);
        assert_eq!(source.line_of(13), 3);
    }

    #[test]
    fn is_same_line_returns_true_for_offsets_on_same_line() {
        let source = SourceFile::new("test.nyx", "abc\ndef");

        assert!(source.is_same_line(0, 3));
        assert!(source.is_same_line(4, 7));
    }

    #[test]
    fn is_same_line_returns_false_for_offsets_on_different_lines() {
        let source = SourceFile::new("test.nyx", "abc\ndef");

        assert!(!source.is_same_line(0, 4));
        assert!(!source.is_same_line(3, 4));
    }

    #[test]
    fn end_of_source_is_a_valid_offset() {
        let source = SourceFile::new("test.nyx", "abc");

        assert_eq!(source.line_of(3), 1);
        assert_eq!(source.location(3), Location { line: 1, column: 4 });
    }

    #[test]
    #[should_panic(expected = "source offset is out of bounds")]
    fn location_panics_for_out_of_bounds_offset() {
        let source = SourceFile::new("test.nyx", "abc");

        source.location(4);
    }

    #[test]
    #[should_panic(expected = "source offset is out of bounds")]
    fn line_of_panics_for_out_of_bounds_offset() {
        let source = SourceFile::new("test.nyx", "abc");

        source.line_of(4);
    }

    #[test]
    #[should_panic(expected = "source offset is out of bounds")]
    fn is_same_line_panics_for_out_of_bounds_offset() {
        let source = SourceFile::new("test.nyx", "abc");

        source.is_same_line(0, 4);
    }

    #[test]
    #[should_panic(expected = "source offset is not a UTF-8 character boundary")]
    fn location_panics_for_offset_inside_utf8_character() {
        let source = SourceFile::new("unicode.nyx", "é");

        // `é` occupies bytes 0 and 1. Byte offset 1 is not a character boundary.
        source.location(1);
    }

    #[test]
    #[should_panic(expected = "source offset is not a UTF-8 character boundary")]
    fn line_of_panics_for_offset_inside_utf8_character() {
        let source = SourceFile::new("unicode.nyx", "é");

        source.line_of(1);
    }
}
