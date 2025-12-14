use std::io::{self, Read};

use segment::{Halt, SegmentHandler, SegmentParser, SegmentParserError};

/// Buffer for streaming parse operations
struct Buffer<const N: usize> {
    buffer: [u8; N],
    start: usize,
    end: usize, // exclusive
}

impl<const N: usize> Default for Buffer<N> {
    fn default() -> Self {
        Self {
            buffer: [0u8; N],
            start: 0,
            end: 0,
        }
    }
}

impl<const N: usize> Buffer<N> {
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn unparsed_slice(&self) -> &[u8] {
        &self.buffer[self.start..self.end]
    }

    /// Mark bytes as parsed, advancing the start pointer
    #[inline]
    fn mark_parsed(&mut self, bytes: usize) {
        self.start += bytes;
    }

    /// Read data from a reader into the buffer.
    /// replaces already parsed data with new data.
    ///
    /// Returns the number of bytes read, or an error if the read fails.
    fn read_from<R: Read>(&mut self, reader: &mut R) -> io::Result<usize> {
        // make space by moving unparsed data to the beginning
        if self.start > 0 {
            self.buffer.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
        }

        let bytes_read = reader.read(&mut self.buffer[self.end..])?;

        self.end += bytes_read;
        Ok(bytes_read)
    }
}

/// Chunked file parser
pub struct StreamingParser<H: SegmentHandler, const BUFFER_SIZE: usize> {
    parser: SegmentParser,
    handler: H,
    buffer: Buffer<BUFFER_SIZE>,
}

#[derive(thiserror::Error, Debug)]
pub enum StreamingParserError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Parsing halted: {0}")]
    Halt(#[from] Halt),
}

impl From<SegmentParserError> for StreamingParserError {
    fn from(err: SegmentParserError) -> Self {
        match err {
            SegmentParserError::Incomplete => StreamingParserError::Halt(Halt {
                message: "Insufficient buffer size",
            }),
            SegmentParserError::Halt(halt) => StreamingParserError::Halt(halt),
        }
    }
}

impl<H: SegmentHandler, const BUFFER_SIZE: usize> StreamingParser<H, BUFFER_SIZE> {
    /// Create a new chunked parser with custom configuration
    pub fn new(handler: H) -> Self {
        Self {
            parser: SegmentParser::init(),
            handler,
            buffer: Buffer::new(),
        }
    }

    pub fn parse_reader<R: Read>(&mut self, reader: &mut R) -> Result<usize, StreamingParserError> {
        let mut total_bytes_read = 0;

        while let bytes_read = self.buffer.read_from(reader)?
            && bytes_read > 0
        {
            total_bytes_read += bytes_read;
            let bytes_parsed = self
                .parser
                .parse_segments(self.buffer.unparsed_slice(), &mut self.handler)?;
            self.buffer.mark_parsed(bytes_parsed);
        }

        Ok(total_bytes_read)
    }
}
