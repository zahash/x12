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

/// Parsed X12 segment with zero-copy element references
#[derive(Debug)]
pub struct Segment<'a> {
    /// Segment identifier (e.g., "ISA", "GS", "ST", "NM1")
    pub id: &'a [u8],
    /// Raw segment data containing elements
    data: &'a [u8],
    /// Delimiter configuration
    pub delimiters: Delimiters,
}

impl<'a> Segment<'a> {
    /// Create a new segment
    fn new(id: &'a [u8], data: &'a [u8], delimiters: Delimiters) -> Self {
        Self {
            id,
            data,
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
        self.elements().nth(index)
    }

    /// Get required element at index, returns error if missing or empty
    #[inline]
    pub fn required_element(&self, index: usize) -> Result<Element<'a>, ParserError> {
        match self.element(index) {
            Some(elem) if !elem.is_empty() => Ok(elem),
            _ => Err(ParserError::MissingRequiredElement),
        }
    }

    /// Get element count
    pub fn element_count(&self) -> usize {
        self.elements().count()
    }

    /// Iterate over all elements
    pub fn elements(&self) -> ElementIter<'a> {
        ElementIter {
            data: self.data,
            separator: self.delimiters.element,
            pos: 0,
        }
    }
}

/// Iterator over segment elements
pub struct ElementIter<'a> {
    data: &'a [u8],
    separator: u8,
    pos: usize,
}

impl<'a> Iterator for ElementIter<'a> {
    type Item = Element<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos > self.data.len() {
            return None;
        }

        let start = self.pos;
        let remaining = &self.data[start..];

        if let Some(idx) = remaining.iter().position(|&b| b == self.separator) {
            self.pos = start + idx + 1;
            Some(Element::new(&remaining[..idx]))
        } else if start < self.data.len() {
            self.pos = self.data.len() + 1;
            Some(Element::new(remaining))
        } else if start == self.data.len() && start > 0 {
            // Handle trailing separator
            self.pos = self.data.len() + 1;
            Some(Element::new(&[]))
        } else {
            None
        }
    }
}

/// Catastrophic error indicating parsing must halt immediately
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Halt;

/// Trait for handling parsed segments
///
/// Implement this trait to process segments as they are parsed.
/// The segment lifetime is tied to the buffer, so all processing
/// must complete before the buffer is modified.
///
/// # Design Philosophy
///
/// Handlers should accumulate validation errors internally.
/// Only return `Err(Halt)` for catastrophic errors that make it
/// impossible to continue parsing (e.g., missing GE but next GS started,
/// out of memory, corrupted structure).
///
/// For validation errors (wrong counts, missing fields, etc.),
/// accumulate them and report after parsing completes.
pub trait SegmentHandler {
    /// Handle a successfully parsed segment
    ///
    /// This method is called for each complete segment parsed.
    /// The segment reference is only valid during this call.
    ///
    /// # Returns
    ///
    /// - `Ok(())` to continue parsing
    /// - `Err(Halt)` only for catastrophic structural errors
    ///
    /// Accumulate validation errors internally and expose via
    /// a separate method (e.g., `errors()` or `report()`).
    fn handle(&mut self, segment: &Segment) -> Result<(), Halt>;
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
    ) -> Result<usize, Halt> {
        if buffer.is_empty() {
            return Err(Halt);
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
    ) -> Result<usize, Halt> {
        // ISA has a standard structure - search for segment terminator
        // which should be around position 105-106
        if buffer.len() < 106 {
            return Err(Halt);
        }

        // Verify ISA identifier
        if &buffer[0..3] != b"ISA" {
            return Err(Halt);
        }

        // Extract delimiters from standard positions
        // Position 3: element separator
        // Position 104: sub-element separator
        // Position 105: segment terminator
        let element_sep = buffer[3];
        let subelement_sep = buffer[104];
        let segment_term = buffer[105];

        self.delimiters = Delimiters {
            element: element_sep,
            subelement: subelement_sep,
            segment: segment_term,
            repetition: b'^', // Default
        };

        // Get the data between ISA and segment terminator
        let data = &buffer[4..105];
        let segment = Segment::new(b"ISA", data, self.delimiters);

        // Extract repetition separator from ISA11 (element at index 10)
        if let Some(elem) = segment.element(10) {
            if let Some(&rep) = elem.as_bytes().first() {
                self.delimiters.repetition = rep;
            }
        }

        self.state = State::Processing;
        handler.handle(&segment)?;
        Ok(106)
    }

    /// Parse a regular segment (non-ISA)
    fn parse_regular_segment<H: SegmentHandler>(
        &mut self,
        buffer: &[u8],
        handler: &mut H,
    ) -> Result<usize, Halt> {
        // Find segment terminator
        let segment_end = buffer
            .iter()
            .position(|&b| b == self.delimiters.segment)
            .ok_or(Halt)?;

        let segment_data = &buffer[..segment_end];

        // Find first element separator to extract segment ID
        let id_end = segment_data
            .iter()
            .position(|&b| b == self.delimiters.element)
            .unwrap_or(segment_data.len());

        if id_end == 0 {
            return Err(Halt);
        }

        let segment_id = &segment_data[..id_end];

        // Get element data (everything after segment ID and separator)
        let elements_data = if id_end < segment_data.len() {
            &segment_data[id_end + 1..]
        } else {
            &[]
        };

        let segment = Segment::new(segment_id, elements_data, self.delimiters);
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
        fn handle(&mut self, _segment: &Segment) -> Result<(), Halt> {
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
        assert_eq!(handler.segments, 1);
        assert!(parser.is_initialized());
    }

    #[test]
    fn test_parse_incomplete() {
        let mut parser = Parser::new();
        let mut handler = TestHandler::new();

        let data = b"ISA*00*          *00*";

        let result = parser.parse_segment(data, &mut handler);
        assert_eq!(result, Err(Halt));
    }

    #[test]
    fn test_parse_regular_segment() {
        let mut parser = Parser::new();
        parser.state = State::Processing; // Skip ISA for this test

        let mut handler = TestHandler::new();
        let data = b"ST*837*0001*005010X222A1~";

        let result = parser.parse_segment(data, &mut handler);
        assert!(result.is_ok());
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
