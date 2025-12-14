//! Tests for newline and whitespace handling in X12 documents
//!
//! X12 standard specifies segment terminator (usually ~) but does not mandate newlines.
//! However, many real-world X12 files include newlines after segment terminators for readability.
//! These tests verify that the parser correctly skips trailing newlines after segment terminators.

mod common;

use common::SegmentCollector;
use parser::SegmentParser;
use pretty_assertions::assert_eq;

#[test]
fn test_segment_terminator_with_single_newline() {
    // Common real-world format: segment terminator followed by newline
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\n";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle ~\\n pattern");
    assert_eq!(collector.segment_count(), 2, "Should parse 2 segments");

    // Verify segment IDs are correct (no leading newlines)
    let isa = collector.get_segment(0).unwrap();
    assert_eq!(isa.id, b"ISA", "ISA segment ID should be correct");

    let gs = collector.get_segment(1).unwrap();
    assert_eq!(
        gs.id, b"GS",
        "GS segment ID should not have leading newline"
    );
    assert_eq!(gs.elements[0], b"HC", "GS first element should be correct");
}

#[test]
fn test_segment_terminator_with_crlf() {
    // Windows-style line endings
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\r\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\r\n";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle ~\\r\\n pattern");
    assert_eq!(collector.segment_count(), 2, "Should parse 2 segments");

    // Verify segment IDs are correct (no leading \r or \n)
    let isa = collector.get_segment(0).unwrap();
    assert_eq!(isa.id, b"ISA", "ISA segment ID should be correct");

    let gs = collector.get_segment(1).unwrap();
    assert_eq!(gs.id, b"GS", "GS segment ID should not have leading CRLF");
    assert_eq!(gs.elements[0], b"HC", "GS first element should be correct");
}

#[test]
fn test_multiple_newlines_between_segments() {
    // Some editors might add blank lines for readability
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\n\n\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle multiple newlines");
    assert_eq!(collector.segment_count(), 2, "Should parse 2 segments");

    let gs = collector.get_segment(1).unwrap();
    assert_eq!(
        gs.id, b"GS",
        "GS segment ID should be correct after multiple newlines"
    );
}

#[test]
fn test_no_newlines_compact_format() {
    // Standard X12 format without any newlines (all segments concatenated)
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~ST*837*0001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle compact format");
    assert_eq!(collector.segment_count(), 3);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_whitespace_after_segment_terminator() {
    // Space or tab after segment terminator should NOT be skipped (only newlines)
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~  GS*HC~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    // This should parse but segment ID will include leading spaces
    // We only skip newlines, not other whitespace
    assert!(result.is_ok());

    let gs = collector.get_segment(1).unwrap();
    // Spaces should be part of segment ID (we don't skip them)
    assert_eq!(
        gs.id, b"  GS",
        "Spaces should not be skipped, only newlines"
    );
}

#[test]
fn test_newline_as_actual_segment_terminator() {
    // What if someone tries to use \n as the segment terminator itself?
    // ISA byte 105 would be \n
    // This is technically allowed by the standard but very unusual

    let mut input = Vec::from(
        "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:",
    );
    input.push(b'\n'); // Position 105 is newline

    input.extend_from_slice(b"GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010");
    input.push(b'\n');

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(&input, &mut collector);

    assert!(result.is_ok(), "Parser should handle \\n as terminator");
    assert_eq!(collector.segment_count(), 2, "Should parse 2 segments");

    // When \n is the actual terminator, no additional newlines should be skipped
    let isa = collector.get_segment(0).unwrap();
    assert_eq!(isa.id, b"ISA");

    let gs = collector.get_segment(1).unwrap();
    assert_eq!(gs.id, b"GS");
}

#[test]
fn test_mixed_terminators_and_newlines() {
    // Real-world: ~ as terminator, but newlines present after for formatting
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\nST*837*0001~\nBHT*0019*00*123*20210101*1200*CH~\nSE*2*0001~\nGE*1*1~\nIEA*1*000000001~\n";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle real-world format");
    assert_eq!(collector.segment_count(), 7, "Should parse all 7 segments");

    // Verify all segment IDs are correct
    let expected_ids = ["ISA", "GS", "ST", "BHT", "SE", "GE", "IEA"];
    for (i, expected_id) in expected_ids.iter().enumerate() {
        let seg = collector.get_segment(i).unwrap();
        assert_eq!(
            seg.id,
            expected_id.as_bytes(),
            "Segment {} ID should be {}",
            i,
            expected_id
        );
    }
}

#[test]
fn test_mixed_crlf_and_lf() {
    // Mix of \r\n and \n in same file
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\r\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\nST*837*0001~\r\n";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle mixed line endings");
    assert_eq!(collector.segment_count(), 3);

    assert_eq!(collector.get_segment(0).unwrap().id, b"ISA");
    assert_eq!(collector.get_segment(1).unwrap().id, b"GS");
    assert_eq!(collector.get_segment(2).unwrap().id, b"ST");
}

#[test]
fn test_multiple_crlf_between_segments() {
    // Multiple CRLF sequences
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\r\n\r\n\r\nGS*HC~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should handle multiple CRLF");
    assert_eq!(collector.segment_count(), 2);

    assert_eq!(collector.get_segment(1).unwrap().id, b"GS");
}

#[test]
fn test_mixed_newlines_and_crlf() {
    // Combinations of \n, \r\n, and \n\r (unusual but possible)
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\n\r\nGS*HC~\r\n\nST*837*0001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(
        result.is_ok(),
        "Parser should handle mixed newline patterns"
    );
    assert_eq!(collector.segment_count(), 3);

    assert_eq!(collector.get_segment(0).unwrap().id, b"ISA");
    assert_eq!(collector.get_segment(1).unwrap().id, b"GS");
    assert_eq!(collector.get_segment(2).unwrap().id, b"ST");
}

#[test]
fn test_newlines_split_across_buffer_boundary() {
    let isa = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~";

    // Add 10 newlines after ISA segment terminator
    let mut chunk1 = Vec::from(&isa[..]);
    chunk1.extend_from_slice(b"\n\n\n\n\n"); // First 5 newlines in chunk 1

    // Remaining 5 newlines and GS segment in chunk 2
    let chunk2 = b"\n\n\n\n\nGS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    // Parse first chunk
    let result1 = parser.parse_segments(&chunk1, &mut collector);
    assert!(result1.is_ok(), "Should parse first chunk successfully");
    assert_eq!(
        collector.segment_count(),
        1,
        "Should have parsed ISA segment"
    );

    // Parse second chunk - this is where the bug manifests
    // Without the fix, GS segment ID will have leading newlines
    let result2 = parser.parse_segments(chunk2, &mut collector);
    assert!(result2.is_ok(), "Should parse second chunk successfully");
    assert_eq!(
        collector.segment_count(),
        2,
        "Should have parsed both segments"
    );

    // Verify GS segment ID is correct (no leading newlines)
    let gs = collector.get_segment(1).unwrap();
    assert_eq!(
        gs.id, b"GS",
        "GS segment ID should not have leading newlines from previous chunk"
    );
    assert_eq!(gs.elements[0], b"HC", "GS first element should be correct");
}
