//! Complete workflow example: Parse file with custom handler
//!
//! This example demonstrates:
//! - Custom handler with error accumulation
//! - Chunked file parsing
//! - Statistics reporting
//! - Error reporting
//!
//! Run with:
//! cargo run --example complete_workflow -- path/to/file.x12

use std::env;
use std::process;

use segment::{Halt, Segment, SegmentHandler};
use x12_host::{ChunkedParseConfig, ChunkedParser};

/// Custom handler that collects segment statistics
struct StatsHandler {
    segment_counts: std::collections::HashMap<String, usize>,
    total_elements: usize,
}

impl StatsHandler {
    fn new() -> Self {
        Self {
            segment_counts: std::collections::HashMap::new(),
            total_elements: 0,
        }
    }

    fn report(&self) {
        println!("\n=== Segment Statistics ===\n");

        let mut segments: Vec<_> = self.segment_counts.iter().collect();
        segments.sort_by_key(|(id, _)| *id);

        for (id, count) in segments {
            println!("  {}: {}", id, count);
        }

        println!("\nTotal elements parsed: {}", self.total_elements);
    }
}

impl SegmentHandler for StatsHandler {
    fn handle(&mut self, segment: &Segment) -> Result<(), Halt> {
        // Count segment type
        if let Some(id) = segment.id_str() {
            *self.segment_counts.entry(id.to_string()).or_insert(0) += 1;
        }

        // Count elements
        self.total_elements += segment.element_count();

        Ok(())
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file.x12>", args[0]);
        process::exit(1);
    }

    let path = &args[1];

    println!("Parsing X12 file: {}", path);
    println!();

    // Configure parser
    let config = ChunkedParseConfig {
        initial_buffer_size: 8 * 1024,
        max_buffer_size: 16 * 1024 * 1024,
        resize_multiplier: 2,
    };

    let handler = StatsHandler::new();
    let mut parser = ChunkedParser::new(handler, config);

    let start = std::time::Instant::now();

    // Parse file
    if let Err(e) = parser.parse_file(path) {
        eprintln!("Error: {:?}", e);
        process::exit(1);
    }

    let duration = start.elapsed();

    // Get statistics (clone since we need it after consuming parser)
    let stats = parser.statistics().clone();

    println!("=== Parsing Complete ===\n");
    println!("Statistics:");
    println!("  Bytes read:      {}", stats.bytes_read);
    println!("  Segments:        {}", stats.segments_parsed);
    println!("  Buffer resizes:  {}", stats.buffer_resizes);
    println!("  Max buffer:      {} KB", stats.max_buffer_size / 1024);
    println!("  Time:            {:.2}s", duration.as_secs_f64());
    println!(
        "  Throughput:      {:.2} MB/s",
        stats.bytes_read as f64 / duration.as_secs_f64() / 1_000_000.0
    );

    // Consume parser and get handler
    let handler = parser.into_handler();
    handler.report();
}
