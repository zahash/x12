#![no_std]

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

    /// Iterate over all elements
    pub fn elements(&self) -> ElementIter<'a> {
        ElementIter {
            data: self.data,
            separator: self.delimiters.element,
            pos: 0,
        }
    }

    /// Get element by X12 element number
    ///
    /// Uses domain-specific numbering:
    /// - `element(0)` returns the segment ID (e.g., ISA-00)
    /// - `element(1)` returns the first data element (e.g., ISA-01)
    /// - `element(2)` returns the second data element (e.g., ISA-02)
    ///
    /// This matches X12 standard conventions and prevents off-by-one errors.
    #[inline]
    pub fn element(&self, element_number: usize) -> Option<Element<'a>> {
        match element_number {
            0 => Some(Element::new(self.id)), // Element 0 is the segment ID itself
            _ => self.elements().nth(element_number - 1), // Elements 1+ are data elements (0-indexed internally)
        }
    }

    /// Get total element count including segment ID
    ///
    /// Returns count where segment ID is element 0.
    /// For example, ISA has 17 elements total (ISA-00 through ISA-16).
    pub fn element_count(&self) -> usize {
        // Add 1 to include the segment ID as element 0
        self.elements().count() + 1
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

/// Represents a parsed segment element
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    /// Raw element data
    data: &'a [u8],
}

impl<'a> Element<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.data
    }

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

/// Catastrophic error indicating parsing must halt immediately
///
/// Contains context about what caused the unrecoverable error.
#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct Halt {
    /// Human-readable error message
    pub message: &'static str,
}

impl Halt {
    /// Create a new Halt error with a message
    #[inline]
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

/// Parser error type distinguishing recoverable from catastrophic errors
#[derive(thiserror::Error, Debug)]
pub enum SegmentParserError {
    /// Need more data in buffer to complete parsing
    ///
    /// This is a recoverable error - the caller should:
    /// 1. Grow the buffer (if possible)
    /// 2. Read more data from the source
    /// 3. Retry parsing
    #[error("Incomplete segment - need more data")]
    Incomplete,

    /// Catastrophic error - parsing cannot continue
    ///
    /// This indicates a structural problem that makes it
    /// impossible to continue parsing (e.g., invalid ISA header,
    /// empty segment ID, handler returned error).
    #[error("{0}")]
    Halt(Halt),
}

impl From<Halt> for SegmentParserError {
    fn from(halt: Halt) -> Self {
        SegmentParserError::Halt(halt)
    }
}

/// X12 837 stream parser
///
/// Parses X12 837 documents one segment at a time from a byte buffer.
/// The parser maintains minimal state and performs zero-copy parsing.
pub enum SegmentParser {
    /// Waiting for ISA segment to extract delimiters
    Initial,

    /// Processing segments with known delimiters
    Processing(Delimiters),
}

impl SegmentParser {
    pub fn init() -> Self {
        Self::Initial
    }

    /// Skip leading newlines (\\r and \\n) at the start of buffer
    ///
    /// This handles the case where segment terminators are followed by newlines for readability,
    /// and the buffer boundary falls in the middle of those newlines.
    /// Advances the buffer and returns the number of bytes skipped.
    #[inline]
    fn skip_lf_crlf(buffer: &mut &[u8]) -> usize {
        let skipped = buffer
            .iter()
            .take_while(|&&b| b == b'\r' || b == b'\n')
            .count();
        *buffer = &buffer[skipped..];
        skipped
    }

    /// Parse multiple segments from the buffer and invoke handler for each.
    ///
    /// Returns the number of bytes consumed on success.
    ///
    /// # Errors
    ///
    /// - `ParserError::Incomplete` - Buffer doesn't contain even a single complete segment.
    ///   Caller should grow buffer and read more data.
    /// - `ParserError::Halt` - Catastrophic error (invalid structure, handler error).
    ///   Parsing cannot continue.
    ///
    /// # Arguments
    /// * `buffer` - Byte slice containing X12 data
    /// * `handler` - Segment handler to process parsed segment
    pub fn parse_segments<H: SegmentHandler>(
        &mut self,
        mut buffer: &[u8],
        handler: &mut H,
    ) -> Result<usize, SegmentParserError> {
        let mut total_bytes_parsed = 0;

        // Skip any leading newlines at the start of this buffer chunk.
        // This handles the case where newlines after a segment terminator
        // were split across buffer boundaries.
        total_bytes_parsed += Self::skip_lf_crlf(&mut buffer);

        while !buffer.is_empty() {
            total_bytes_parsed += match self {
                SegmentParser::Initial => {
                    let (bytes_parsed, delimiters) = Self::parse_isa_segment(&mut buffer, handler)?;
                    *self = SegmentParser::Processing(delimiters);
                    bytes_parsed
                }
                SegmentParser::Processing(delimiters) => {
                    match Self::parse_regular_segment(&mut buffer, handler, *delimiters) {
                        Ok(consumed) => consumed,
                        Err(e) => match e {
                            SegmentParserError::Incomplete if total_bytes_parsed > 0 => {
                                /* some segments were parsed but need more data for next */
                                break;
                            }
                            _ => return Err(e),
                        },
                    }
                }
            };

            // Skip any trailing newlines after the segment we just parsed.
            // This ensures we don't include them in the next segment.
            total_bytes_parsed += Self::skip_lf_crlf(&mut buffer);
        }

        Ok(total_bytes_parsed)
    }

    /// Parse the ISA (Interchange Control Header) segment
    ///
    /// The ISA segment is special because it has fixed-width fields and
    /// defines the delimiters used for the rest of the document.
    /// Advances the buffer and returns delimiters and bytes consumed.
    fn parse_isa_segment<H: SegmentHandler>(
        buffer: &mut &[u8],
        handler: &mut H,
    ) -> Result<(usize, Delimiters), SegmentParserError> {
        // including segment terminator
        const ISA_SIZE_BYTES: usize = 106;

        if buffer.len() < ISA_SIZE_BYTES {
            return Err(SegmentParserError::Incomplete);
        }

        // Verify ISA identifier
        if &buffer[0..3] != b"ISA" {
            return Err(SegmentParserError::Halt(Halt::new(
                "Invalid ISA header: first three bytes must be 'ISA'",
            )));
        }

        // Get the data between ISA* and segment terminator
        let data = &buffer[4..105];
        let mut segment = Segment::new(
            b"ISA",
            data,
            Delimiters {
                // Extract delimiters from standard positions
                element: buffer[3],
                subelement: buffer[104],
                segment: buffer[105],
                ..Default::default() // repetetion default for now. will be extracted from ISA-11 below
            },
        );

        // Extract repetition separator from ISA11
        segment.delimiters.repetition = *segment
            .element(11)
            .and_then(|ele| ele.as_bytes().first())
            .ok_or(SegmentParserError::Halt(Halt::new(
                "Missing repetition separator in ISA-11",
            )))?;

        handler.handle(&segment)?;
        *buffer = &buffer[ISA_SIZE_BYTES..];
        Ok((ISA_SIZE_BYTES, segment.delimiters))
    }

    /// Parse a regular segment (non-ISA)
    /// Advances the buffer and returns the number of bytes consumed.
    fn parse_regular_segment<H: SegmentHandler>(
        buffer: &mut &[u8],
        handler: &mut H,
        delimiters: Delimiters,
    ) -> Result<usize, SegmentParserError> {
        // Find segment terminator
        let segment_end = buffer
            .iter()
            .position(|&b| b == delimiters.segment)
            .ok_or(SegmentParserError::Incomplete)?;

        let segment_data = &buffer[..segment_end];

        // Find first element separator to extract segment ID
        let id_end = segment_data
            .iter()
            .position(|&b| b == delimiters.element)
            .unwrap_or(segment_data.len());

        if id_end == 0 {
            return Err(SegmentParserError::Halt(Halt::new(
                "Invalid segment: segment ID cannot be empty",
            )));
        }

        let segment_id = &segment_data[..id_end];

        // Get element data (everything after segment ID and separator)
        let elements_data = if id_end < segment_data.len() {
            &segment_data[id_end + 1..]
        } else {
            &[]
        };

        let segment = Segment::new(segment_id, elements_data, delimiters);
        handler.handle(&segment)?;

        let consumed = segment_end + 1; // +1 for segment terminator
        *buffer = &buffer[consumed..];
        Ok(consumed)
    }
}
