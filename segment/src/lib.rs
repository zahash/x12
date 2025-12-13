#![no_std]

//! X12 837 Stream Parser
//!
//! A no_std, zero-copy, streaming parser for X12 837 healthcare claims documents.
//! Inspired by httparse, this parser processes segments incrementally from a buffer
//! provided by the host application.
//!
//! # Design Philosophy
//! - Zero-copy: All data references point to the original buffer
//! - Streaming: Parse one segment at a time without buffering
//! - Ephemeral: No long-term storage, all references are short-lived
//! - Efficient: Minimal allocations, optimal for embedded systems
//!
//! # Usage
//! ```ignore
//! let mut parser = Parser::new();
//! let mut handler = MyHandler::new();
//!
//! loop {
//!     match parser.parse_segment(buffer, &mut handler) {
//!         Ok(bytes_read) => {
//!             // Consume bytes from buffer
//!             buffer = &buffer[bytes_read..];
//!         }
//!         Err(ParserError::Incomplete) => {
//!             // Load more data into buffer
//!             break;
//!         }
//!         Err(e) => {
//!             // Handle error
//!             break;
//!         }
//!     }
//! }
//! ```

/// Parser errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserError {
    /// Not enough data in buffer to parse a complete segment
    Incomplete,
    /// Invalid segment structure or format
    InvalidSegment,
    /// Invalid delimiter configuration in ISA segment
    InvalidDelimiters,
    /// Segment identifier is invalid or missing
    InvalidSegmentId,
    /// Element count doesn't match segment requirements
    InvalidElementCount,
    /// Required element is missing
    MissingRequiredElement,
}

/// X12 delimiters extracted from ISA segment
#[derive(Debug, Clone, Copy)]
pub struct Delimiters {
    /// Element separator (position 3 in ISA, typically '*')
    pub element: u8,
    /// Sub-element separator (position 104 in ISA, typically ':')
    pub subelement: u8,
    /// Segment terminator (position 105 in ISA, typically '~')
    pub segment: u8,
    /// Repetition separator (optional, typically '^')
    pub repetition: u8,
}

impl Default for Delimiters {
    fn default() -> Self {
        Self {
            element: b'*',
            subelement: b':',
            segment: b'~',
            repetition: b'^',
        }
    }
}

/// Represents a parsed segment element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element<'a> {
    /// Raw element data
    data: &'a [u8],
}

impl<'a> Element<'a> {
    /// Create a new element from a byte slice
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Get the raw bytes of this element
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Check if element is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get element as string (if valid UTF-8)
    #[inline]
    pub fn as_str(&self) -> Option<&'a str> {
        core::str::from_utf8(self.data).ok()
    }

    /// Split element by sub-element separator
    pub fn split_components(&self, separator: u8) -> ComponentIter<'a> {
        ComponentIter {
            data: self.data,
            separator,
            pos: 0,
        }
    }
}

/// Iterator over sub-element components
pub struct ComponentIter<'a> {
    data: &'a [u8],
    separator: u8,
    pos: usize,
}

impl<'a> Iterator for ComponentIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos > self.data.len() {
            return None;
        }

        let start = self.pos;
        let remaining = &self.data[start..];

        if let Some(idx) = remaining.iter().position(|&b| b == self.separator) {
            self.pos = start + idx + 1;
            Some(&remaining[..idx])
        } else if start < self.data.len() {
            self.pos = self.data.len() + 1;
            Some(remaining)
        } else if start == self.data.len() && start > 0 {
            // Handle trailing separator
            self.pos = self.data.len() + 1;
            Some(&[])
        } else {
            None
        }
    }
}

/// Maximum number of elements in a segment (X12 standard limit)
pub const MAX_ELEMENTS: usize = 512;

/// Parsed X12 segment with zero-copy element references
#[derive(Debug)]
pub struct Segment<'a> {
    /// Segment identifier (e.g., "ISA", "GS", "ST", "NM1")
    pub id: &'a [u8],
    /// Segment elements (not including the segment ID)
    pub elements: [Option<Element<'a>>; MAX_ELEMENTS],
    /// Number of elements in this segment
    pub element_count: usize,
    /// Delimiter configuration
    pub delimiters: Delimiters,
}

impl<'a> Segment<'a> {
    /// Create a new empty segment
    fn new(id: &'a [u8], delimiters: Delimiters) -> Self {
        Self {
            id,
            elements: [None; MAX_ELEMENTS],
            element_count: 0,
            delimiters,
        }
    }

    /// Get segment ID as string (if valid UTF-8)
    #[inline]
    pub fn id_str(&self) -> Option<&'a str> {
        core::str::from_utf8(self.id).ok()
    }

    /// Get element at index (0-based, not including segment ID)
    #[inline]
    pub fn element(&self, index: usize) -> Option<Element<'a>> {
        if index < self.element_count {
            self.elements[index]
        } else {
            None
        }
    }

    /// Get required element at index, returns error if missing or empty
    #[inline]
    pub fn required_element(&self, index: usize) -> Result<Element<'a>, ParserError> {
        match self.element(index) {
            Some(elem) if !elem.is_empty() => Ok(elem),
            _ => Err(ParserError::MissingRequiredElement),
        }
    }

    /// Iterate over all elements
    pub fn iter_elements(&self) -> impl Iterator<Item = Element<'a>> + '_ {
        self.elements[..self.element_count]
            .iter()
            .filter_map(|e| *e)
    }
}

/// Trait for handling parsed segments
///
/// Implement this trait to process segments as they are parsed.
/// The segment lifetime is tied to the buffer, so all processing
/// must complete before the buffer is modified.
///
/// # Design Philosophy
///
/// This trait does NOT return errors for validation failures. Instead,
/// handlers should accumulate errors internally and provide them via
/// a separate method after parsing completes. This allows collecting
/// ALL errors in a document, not just the first one.
///
/// Only return Err for catastrophic errors that prevent further processing
/// (e.g., out of memory, I/O failure).
pub trait SegmentHandler {
    /// Associated error type for catastrophic handler errors
    ///
    /// Use this only for errors that prevent further processing.
    /// For validation errors, accumulate them internally.
    type Error: From<ParserError>;

    /// Handle a successfully parsed segment
    ///
    /// This method is called for each complete segment parsed.
    /// The segment reference is only valid during this call.
    ///
    /// # Returns
    ///
    /// - `Ok(())` to continue parsing
    /// - `Err(e)` only for catastrophic errors that prevent further processing
    ///
    /// For validation errors, accumulate them internally and expose via
    /// a separate method (e.g., `errors()` or `finalize()`).
    fn handle(&mut self, segment: &Segment) -> Result<(), Self::Error>;
}

/// Parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// Waiting for ISA segment to extract delimiters
    Initial,
    /// Processing segments with known delimiters
    Processing,
}

/// X12 837 stream parser
///
/// Parses X12 837 documents one segment at a time from a byte buffer.
/// The parser maintains minimal state and performs zero-copy parsing.
pub struct Parser {
    state: State,
    delimiters: Delimiters,
}

impl Parser {
    /// Create a new parser instance
    pub fn new() -> Self {
        Self {
            state: State::Initial,
            delimiters: Delimiters::default(),
        }
    }

    /// Reset parser to initial state
    pub fn reset(&mut self) {
        self.state = State::Initial;
        self.delimiters = Delimiters::default();
    }

    /// Parse one segment from the buffer and invoke handler
    ///
    /// Returns the number of bytes consumed on success.
    /// Returns Err(Incomplete) if buffer doesn't contain a complete segment.
    /// Returns other errors for parsing failures.
    ///
    /// # Arguments
    /// * `buffer` - Byte slice containing X12 data
    /// * `handler` - Segment handler to process parsed segment
    pub fn parse_segment<H: SegmentHandler>(
        &mut self,
        buffer: &[u8],
        handler: &mut H,
    ) -> Result<usize, H::Error> {
        if buffer.is_empty() {
            return Err(ParserError::Incomplete.into());
        }

        match self.state {
            State::Initial => self.parse_isa_segment(buffer, handler),
            State::Processing => self.parse_regular_segment(buffer, handler),
        }
    }

    /// Parse the ISA (Interchange Control Header) segment
    ///
    /// The ISA segment is special because it has fixed-width fields and
    /// defines the delimiters used for the rest of the document.
    fn parse_isa_segment<H: SegmentHandler>(
        &mut self,
        buffer: &[u8],
        handler: &mut H,
    ) -> Result<usize, H::Error> {
        // ISA segment is exactly 106 characters including terminator
        const ISA_LENGTH: usize = 106;

        if buffer.len() < ISA_LENGTH {
            return Err(ParserError::Incomplete.into());
        }

        // Verify ISA identifier
        if buffer.len() < 3 || &buffer[0..3] != b"ISA" {
            return Err(ParserError::InvalidSegmentId.into());
        }

        // Extract delimiters from ISA segment
        // Position 3: element separator
        // Position 104: sub-element separator
        // Position 105: segment terminator
        let element_sep = buffer[3];
        let subelement_sep = buffer[104];
        let segment_term = buffer[105];

        // Validate delimiters are different
        if element_sep == subelement_sep
            || element_sep == segment_term
            || subelement_sep == segment_term
        {
            return Err(ParserError::InvalidDelimiters.into());
        }

        self.delimiters = Delimiters {
            element: element_sep,
            subelement: subelement_sep,
            segment: segment_term,
            repetition: b'^', // Default, can be overridden by ISA16
        };

        // Parse ISA elements using fixed positions
        let mut segment = Segment::new(b"ISA", self.delimiters);

        // ISA has 16 elements with fixed widths
        let positions = [
            (4, 6),   // ISA01: Authorization Information Qualifier (2)
            (6, 16),  // ISA02: Authorization Information (10)
            (16, 18), // ISA03: Security Information Qualifier (2)
            (18, 28), // ISA04: Security Information (10)
            (28, 30), // ISA05: Interchange ID Qualifier (2)
            (30, 45), // ISA06: Interchange Sender ID (15)
            (45, 47), // ISA07: Interchange ID Qualifier (2)
            (47, 62), // ISA08: Interchange Receiver ID (15)
            (62, 68), // ISA09: Interchange Date (6)
            (68, 72), // ISA10: Interchange Time (4)
            (72, 73), // ISA11: Repetition Separator (1)
            (73, 78), // ISA12: Interchange Control Version Number (5)
            (78, 87), // ISA13: Interchange Control Number (9)
            (87, 88), // ISA14: Acknowledgment Requested (1)
            (88, 89), // ISA15: Usage Indicator (1)
            (89, 90), // ISA16: Component Element Separator (1)
        ];

        for (i, &(start, end)) in positions.iter().enumerate() {
            if end <= buffer.len() {
                segment.elements[i] = Some(Element::new(&buffer[start..end]));
                segment.element_count += 1;
            } else {
                return Err(ParserError::Incomplete.into());
            }
        }

        // Extract repetition separator from ISA11
        if let Some(elem) = segment.element(10) {
            if let Some(&rep) = elem.as_bytes().first() {
                self.delimiters.repetition = rep;
            }
        }

        self.state = State::Processing;
        handler.handle(&segment)?;
        Ok(ISA_LENGTH)
    }

    /// Parse a regular segment (non-ISA)
    fn parse_regular_segment<H: SegmentHandler>(
        &mut self,
        buffer: &[u8],
        handler: &mut H,
    ) -> Result<usize, H::Error> {
        // Find segment terminator
        let segment_end = buffer
            .iter()
            .position(|&b| b == self.delimiters.segment)
            .ok_or(ParserError::Incomplete)?;

        let segment_data = &buffer[..segment_end];

        // Find first element separator to extract segment ID
        let id_end = segment_data
            .iter()
            .position(|&b| b == self.delimiters.element)
            .unwrap_or(segment_data.len());

        if id_end == 0 {
            return Err(ParserError::InvalidSegmentId.into());
        }

        let segment_id = &segment_data[..id_end];

        // Validate segment ID (2-3 uppercase alphanumeric characters)
        if segment_id.len() < 2 || segment_id.len() > 3 {
            return Err(ParserError::InvalidSegmentId.into());
        }

        for &b in segment_id {
            if !b.is_ascii_alphanumeric() {
                return Err(ParserError::InvalidSegmentId.into());
            }
        }

        let mut segment = Segment::new(segment_id, self.delimiters);

        // Parse elements after segment ID
        if id_end < segment_data.len() {
            let elements_data = &segment_data[id_end + 1..];
            let mut start = 0;

            for (i, &b) in elements_data.iter().enumerate() {
                if b == self.delimiters.element {
                    if segment.element_count >= MAX_ELEMENTS {
                        return Err(ParserError::InvalidElementCount.into());
                    }
                    segment.elements[segment.element_count] =
                        Some(Element::new(&elements_data[start..i]));
                    segment.element_count += 1;
                    start = i + 1;
                }
            }

            // Add final element
            if start <= elements_data.len() {
                if segment.element_count >= MAX_ELEMENTS {
                    return Err(ParserError::InvalidElementCount.into());
                }
                segment.elements[segment.element_count] =
                    Some(Element::new(&elements_data[start..]));
                segment.element_count += 1;
            }
        }

        handler.handle(&segment)?;
        Ok(segment_end + 1) // +1 for segment terminator
    }

    /// Get current delimiter configuration
    #[inline]
    pub fn delimiters(&self) -> Delimiters {
        self.delimiters
    }

    /// Check if parser has been initialized with ISA segment
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.state == State::Processing
    }

    /// Set custom delimiters and force parser into processing state
    ///
    /// This is useful when you want to parse segments without first
    /// processing an ISA segment. Not recommended for production use.
    #[doc(hidden)]
    pub fn set_delimiters(&mut self, delimiters: Delimiters) {
        self.delimiters = delimiters;
        self.state = State::Processing;
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        segments: usize,
    }

    impl TestHandler {
        fn new() -> Self {
            Self { segments: 0 }
        }
    }

    impl SegmentHandler for TestHandler {
        type Error = ParserError;

        fn handle(&mut self, _segment: &Segment) -> Result<(), Self::Error> {
            self.segments += 1;
            Ok(())
        }
    }

    #[test]
    fn test_parse_isa_segment() {
        let mut parser = Parser::new();
        let mut handler = TestHandler::new();

        let data = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~";

        let result = parser.parse_segment(data, &mut handler);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 106);
        assert_eq!(handler.segments, 1);
        assert!(parser.is_initialized());
    }

    #[test]
    fn test_parse_incomplete() {
        let mut parser = Parser::new();
        let mut handler = TestHandler::new();

        let data = b"ISA*00*          *00*";

        let result = parser.parse_segment(data, &mut handler);
        assert_eq!(result, Err(ParserError::Incomplete));
    }

    #[test]
    fn test_parse_regular_segment() {
        let mut parser = Parser::new();
        parser.state = State::Processing; // Skip ISA for this test

        let mut handler = TestHandler::new();
        let data = b"ST*837*0001*005010X222A1~";

        let result = parser.parse_segment(data, &mut handler);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 25);
        assert_eq!(handler.segments, 1);
    }

    #[test]
    fn test_element_access() {
        let element = Element::new(b"TEST");
        assert_eq!(element.as_bytes(), b"TEST");
        assert_eq!(element.as_str(), Some("TEST"));
        assert!(!element.is_empty());
    }

    #[test]
    fn test_component_split() {
        let element = Element::new(b"AA:BB:CC");
        let components: alloc::vec::Vec<_> = element.split_components(b':').collect();
        assert_eq!(components.len(), 3);
        assert_eq!(components[0], b"AA");
        assert_eq!(components[1], b"BB");
        assert_eq!(components[2], b"CC");
    }
}

// Test allocator for unit tests
#[cfg(test)]
extern crate alloc;
