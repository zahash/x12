//! X12 File Parser Host Application
//!
//! Parses large X12 files efficiently using chunked reading with
//! dynamic buffer resizing for incomplete segments.
//!
//! # Features
//!
//! - Handles multi-gigabyte files
//! - Chunks reading to minimize memory usage
//! - Dynamically doubles buffer size when segments don't fit
//! - Accumulates all validation errors
//! - Comprehensive error reporting

use std::env;
use std::fs::File;

use x12_host::StreamingParser;
use x12_validation::ValidationSuite;

const BUFFER_SIZE: usize = 4096;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filepath = env::args().skip(1).next().ok_or("Usage: x12 <file.x12>")?;
    let mut file = File::open(&filepath)?;

    let validator = ValidationSuite::all_snip_levels();
    let mut parser = StreamingParser::<_, BUFFER_SIZE>::new(validator);

    let bytes_parsed = parser.parse_reader(&mut file)?;
    println!("{}", bytes_parsed);

    Ok(())
}
