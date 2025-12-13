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

use std::fs::File;
use std::io::{Read, BufReader};
use std::path::PathBuf;
use std::env;
use std::process;

use segment::{Parser, ParserError};
use x12_validation::ValidationSuite;

/// Initial buffer size (8KB)
const INITIAL_BUFFER_SIZE: usize = 8 * 1024;

/// Maximum buffer size before giving up (16MB)
const MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Statistics about the parsing operation
#[derive(Debug, Default)]
struct ParseStats {
    bytes_read: u64,
    segments_parsed: usize,
    buffer_resizes: usize,
    max_buffer_size: usize,
}

/// Result type for parse operations
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Parse a large X12 file with chunked reading
fn parse_file(path: &PathBuf) -> Result<(ValidationSuite, ParseStats)> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    
    let mut parser = Parser::new();
    let mut validator = ValidationSuite::all_snip_levels();
    let mut stats = ParseStats::default();
    
    // Start with initial buffer size
    let mut buffer = vec![0u8; INITIAL_BUFFER_SIZE];
    let mut buffer_start = 0;  // Start of unparsed data in buffer
    let mut buffer_end = 0;    // End of valid data in buffer
    
    stats.max_buffer_size = INITIAL_BUFFER_SIZE;
    
    'read_loop: loop {
        // Compact buffer if we've consumed more than half
        if buffer_start > buffer.len() / 2 && buffer_start > 0 {
            buffer.copy_within(buffer_start..buffer_end, 0);
            buffer_end -= buffer_start;
            buffer_start = 0;
        }
        
        // Read more data into buffer
        let bytes_read = reader.read(&mut buffer[buffer_end..])?;
        stats.bytes_read += bytes_read as u64;
        
        if bytes_read == 0 && buffer_start == buffer_end {
            // End of file and no data left in buffer
            break 'read_loop;
        }
        
        buffer_end += bytes_read;
        
        // Parse segments from buffer
        'parse_loop: loop {
            let unparsed = &buffer[buffer_start..buffer_end];
            
            if unparsed.is_empty() {
                break 'parse_loop;
            }
            
            match parser.parse_segment(unparsed, &mut validator) {
                Ok(consumed) => {
                    buffer_start += consumed;
                    stats.segments_parsed += 1;
                }
                Err(ParserError::Incomplete) => {
                    // Segment doesn't fit in current buffer
                    
                    if bytes_read == 0 {
                        // End of file but incomplete segment
                        eprintln!("Warning: Incomplete segment at end of file");
                        break 'read_loop;
                    }
                    
                    // Check if we need to resize buffer
                    let remaining = buffer_end - buffer_start;
                    
                    if remaining >= buffer.len() {
                        // Buffer is full and still incomplete - need bigger buffer
                        let new_size = (buffer.len() * 2).min(MAX_BUFFER_SIZE);
                        
                        if new_size == buffer.len() {
                            return Err(format!(
                                "Segment too large: exceeds maximum buffer size of {} bytes",
                                MAX_BUFFER_SIZE
                            ).into());
                        }
                        
                        // Resize buffer
                        let mut new_buffer = vec![0u8; new_size];
                        new_buffer[..remaining].copy_from_slice(&buffer[buffer_start..buffer_end]);
                        buffer = new_buffer;
                        buffer_end = remaining;
                        buffer_start = 0;
                        
                        stats.buffer_resizes += 1;
                        stats.max_buffer_size = stats.max_buffer_size.max(new_size);
                        
                        eprintln!("Resized buffer to {} bytes", new_size);
                    }
                    
                    // Need to read more data
                    break 'parse_loop;
                }
                Err(e) => {
                    return Err(format!("Parse error: {:?}", e).into());
                }
            }
        }
    }
    
    Ok((validator, stats))
}

/// Format bytes in human-readable form
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Print usage information
fn print_usage() {
    eprintln!("Usage: x12-parse <file.x12>");
    eprintln!();
    eprintln!("Parse and validate X12 files of any size.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -h, --help     Show this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        process::exit(1);
    }
    
    let path = PathBuf::from(&args[1]);
    
    if !path.exists() {
        eprintln!("Error: File not found: {}", path.display());
        process::exit(1);
    }
    
    println!("Parsing X12 file: {}", path.display());
    println!();
    
    let start = std::time::Instant::now();
    
    match parse_file(&path) {
        Ok((validator, stats)) => {
            let duration = start.elapsed();
            
            println!("=== Parsing Complete ===");
            println!();
            println!("Statistics:");
            println!("  File size:         {}", format_bytes(stats.bytes_read));
            println!("  Segments parsed:   {}", stats.segments_parsed);
            println!("  Buffer resizes:    {}", stats.buffer_resizes);
            println!("  Max buffer size:   {}", format_bytes(stats.max_buffer_size as u64));
            println!("  Parse time:        {:.2}s", duration.as_secs_f64());
            println!("  Throughput:        {}/s", 
                format_bytes((stats.bytes_read as f64 / duration.as_secs_f64()) as u64));
            println!();
            
            // Get all validation errors
            let errors = validator.errors();
            
            if errors.is_empty() {
                println!("✓ No validation errors found");
            } else {
                println!("=== Validation Errors ({}) ===", errors.len());
                println!();
                
                // Group errors by severity
                let mut error_count = 0;
                let mut warning_count = 0;
                let mut info_count = 0;
                
                for error in &errors {
                    match error.severity {
                        x12_validation::Severity::Error => error_count += 1,
                        x12_validation::Severity::Warning => warning_count += 1,
                        x12_validation::Severity::Info => info_count += 1,
                    }
                }
                
                println!("Summary:");
                println!("  Errors:   {}", error_count);
                println!("  Warnings: {}", warning_count);
                println!("  Info:     {}", info_count);
                println!();
                
                // Print first 20 errors
                let max_display = 20;
                for (i, error) in errors.iter().take(max_display).enumerate() {
                    println!("{}. {}", i + 1, error);
                }
                
                if errors.len() > max_display {
                    println!();
                    println!("... and {} more errors", errors.len() - max_display);
                }
                
                // Exit with error code if there are errors
                if error_count > 0 {
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
