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

use std::path::PathBuf;
use std::env;
use std::process;

use x12_host::{ChunkedParser, ChunkedParseConfig};
use x12_validation::ValidationSuite;

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
    
    // Configure parser
    let config = ChunkedParseConfig {
        initial_buffer_size: 8 * 1024,       // 8KB
        max_buffer_size: 16 * 1024 * 1024,   // 16MB
        resize_multiplier: 2,
    };
    
    let validator = ValidationSuite::all_snip_levels();
    let mut parser = ChunkedParser::new(validator, config);
    
    // Parse file
    match parser.parse_file(&path) {
        Ok(()) => {
            let duration = start.elapsed();
            let stats = parser.statistics();
            
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
            let errors = parser.handler_mut().errors();
            
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
        Err(_) => {
            eprintln!("Error: Parsing halted due to catastrophic error");
            process::exit(1);
        }
    }
}
