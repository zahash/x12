//! X12 Host Library
//!
//! Provides utilities for parsing large X12 files with chunked reading.

use std::fs::File;
use std::io::{Read, BufReader};
use std::path::Path;

use segment::{Parser, ParserError, SegmentHandler};

/// Configuration for chunked file parsing
#[derive(Debug, Clone)]
pub struct ChunkedParseConfig {
    /// Initial buffer size in bytes
    pub initial_buffer_size: usize,
    /// Maximum buffer size in bytes
    pub max_buffer_size: usize,
    /// Buffer size multiplier when resizing (e.g., 2 for doubling)
    pub resize_multiplier: usize,
}

impl Default for ChunkedParseConfig {
    fn default() -> Self {
        Self {
            initial_buffer_size: 8 * 1024,       // 8KB
            max_buffer_size: 16 * 1024 * 1024,   // 16MB
            resize_multiplier: 2,
        }
    }
}

/// Statistics collected during parsing
#[derive(Debug, Default, Clone)]
pub struct ParseStatistics {
    /// Total bytes read from file
    pub bytes_read: u64,
    /// Number of segments successfully parsed
    pub segments_parsed: usize,
    /// Number of times buffer was resized
    pub buffer_resizes: usize,
    /// Maximum buffer size reached
    pub max_buffer_size: usize,
}

/// Chunked file parser
pub struct ChunkedParser<H: SegmentHandler> {
    parser: Parser,
    handler: H,
    config: ChunkedParseConfig,
    stats: ParseStatistics,
    buffer: Vec<u8>,
    buffer_start: usize,
    buffer_end: usize,
}

impl<H: SegmentHandler> ChunkedParser<H> {
    /// Create a new chunked parser with custom configuration
    pub fn new(handler: H, config: ChunkedParseConfig) -> Self {
        let buffer = vec![0u8; config.initial_buffer_size];
        let max_buffer_size = config.initial_buffer_size;
        
        Self {
            parser: Parser::new(),
            handler,
            config,
            stats: ParseStatistics {
                max_buffer_size,
                ..Default::default()
            },
            buffer,
            buffer_start: 0,
            buffer_end: 0,
        }
    }

    /// Create a new chunked parser with default configuration
    pub fn with_default_config(handler: H) -> Self {
        Self::new(handler, ChunkedParseConfig::default())
    }

    /// Parse a file from a path
    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), H::Error> {
        let file = File::open(path).map_err(|_| {
            ParserError::InvalidSegment // Convert IO error
        })?;
        let mut reader = BufReader::new(file);
        self.parse_reader(&mut reader)
    }

    /// Parse from a reader
    pub fn parse_reader<R: Read>(&mut self, reader: &mut R) -> Result<(), H::Error> {
        loop {
            // Compact buffer if needed
            if self.buffer_start > self.buffer.len() / 2 && self.buffer_start > 0 {
                self.buffer.copy_within(self.buffer_start..self.buffer_end, 0);
                self.buffer_end -= self.buffer_start;
                self.buffer_start = 0;
            }

            // Read more data
            let bytes_read = reader.read(&mut self.buffer[self.buffer_end..])
                .map_err(|_| ParserError::InvalidSegment)?;
            
            self.stats.bytes_read += bytes_read as u64;

            if bytes_read == 0 && self.buffer_start == self.buffer_end {
                // End of file
                break;
            }

            self.buffer_end += bytes_read;

            // Parse segments
            'parse: loop {
                let unparsed = &self.buffer[self.buffer_start..self.buffer_end];

                if unparsed.is_empty() {
                    break 'parse;
                }

                // Try to parse a segment
                // We handle Incomplete specially by retrying with more data
                let result = self.parser.parse_segment(unparsed, &mut self.handler);
                
                match result {
                    Ok(consumed) => {
                        self.buffer_start += consumed;
                        self.stats.segments_parsed += 1;
                    }
                    Err(e) => {
                        // Since H::Error: From<ParserError>, we know the error
                        // came from either the parser or handler
                        // 
                        // Strategy: If we haven't consumed anything and there's
                        // more data coming, assume Incomplete and retry with bigger buffer
                        
                        let remaining = self.buffer_end - self.buffer_start;
                        
                        if bytes_read > 0 && remaining >= self.buffer.len() {
                            // Buffer is full and we read data, likely Incomplete
                            // Try to resize buffer
                            self.resize_buffer()?;
                            break 'parse; // Retry with more data
                        } else if bytes_read > 0 {
                            // More data available, try reading more
                            break 'parse;
                        } else {
                            // No more data, this is a real error
                            return Err(e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Resize the buffer
    fn resize_buffer(&mut self) -> Result<(), H::Error> {
        let new_size = (self.buffer.len() * self.config.resize_multiplier)
            .min(self.config.max_buffer_size);

        if new_size == self.buffer.len() {
            // Can't resize further
            return Err(ParserError::InvalidSegment.into());
        }

        let remaining = self.buffer_end - self.buffer_start;
        let mut new_buffer = vec![0u8; new_size];
        new_buffer[..remaining].copy_from_slice(&self.buffer[self.buffer_start..self.buffer_end]);
        
        self.buffer = new_buffer;
        self.buffer_end = remaining;
        self.buffer_start = 0;

        self.stats.buffer_resizes += 1;
        self.stats.max_buffer_size = self.stats.max_buffer_size.max(new_size);

        Ok(())
    }

    /// Get parsing statistics
    pub fn statistics(&self) -> &ParseStatistics {
        &self.stats
    }

    /// Get mutable reference to handler
    pub fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }

    /// Consume parser and return handler
    pub fn into_handler(self) -> H {
        self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use segment::{Segment, ParserError};

    struct TestHandler {
        count: usize,
    }

    impl SegmentHandler for TestHandler {
        type Error = ParserError;

        fn handle(&mut self, _segment: &Segment) -> Result<(), Self::Error> {
            self.count += 1;
            Ok(())
        }
    }

    #[test]
    fn test_chunked_parser_creation() {
        let handler = TestHandler { count: 0 };
        let parser = ChunkedParser::with_default_config(handler);
        assert_eq!(parser.stats.segments_parsed, 0);
    }
}
