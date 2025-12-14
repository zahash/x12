//! Edge case tests for the X12 parser
//!
//! These tests verify parser behavior under boundary conditions and unusual
//! but valid scenarios.

mod common;

use common::SegmentCollector;
use parser::SegmentParser;

use pretty_assertions::assert_eq;

#[test]
fn test_isa_exactly_106_bytes() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~";

    assert_eq!(input.len(), 106, "ISA test input must be exactly 106 bytes");

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let bytes_parsed = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap();

    assert_eq!(bytes_parsed, input.len());
    assert_eq!(collector.reconstruct(), input);
    assert_eq!(collector.segment_count(), 1);
}

#[test]
fn test_delimiter_extraction_from_isa() {
    let input = "ISA|00|          |00|          |ZZ|SENDER         |ZZ|RECEIVER       |210101|1200|*|00501|000000001|0|P|#!";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let bytes_parsed = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap();

    assert_eq!(bytes_parsed, input.len());
    assert_eq!(collector.reconstruct(), input);
    assert_eq!(collector.segment_count(), 1);

    let isa = collector.get_segment(0).unwrap();

    assert_eq!(isa.delimiters.element, b'|', "Element separator");
    assert_eq!(isa.delimiters.subelement, b'#', "Sub-element separator");
    assert_eq!(isa.delimiters.segment, b'!', "Segment terminator");
    assert_eq!(isa.delimiters.repetition, b'*', "Repetition separator");
}

#[test]
fn test_consecutive_segments() {
    let input = "\
            ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
            GS*HC~\
            ";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let bytes_parsed = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap();

    assert_eq!(bytes_parsed, input.len());
    assert_eq!(collector.reconstruct(), input);
    assert_eq!(collector.segment_count(), 2);
}

#[test]
fn test_large_segment_with_max_elements() {
    // Create a segment with many elements to test iterator performance
    let mut input = String::from(
        "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~",
    );
    input.push_str("DMG");
    for i in 0..100 {
        input.push_str(&format!("*ELEM{}", i));
    }
    input.push('~');

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let bytes_parsed = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap();

    assert_eq!(bytes_parsed, input.len());
    assert_eq!(collector.reconstruct(), input);
    assert_eq!(collector.segment_count(), 2);

    let dmg = collector.get_segment(1).unwrap();
    assert_eq!(dmg.elements.len(), 100);
}

#[test]
fn test_binary_data_in_elements() {
    // Test that parser can handle non-UTF8 data (treated as binary)
    let mut input = Vec::from("ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~".as_bytes());

    let non_utf8 = &[0xFF, 0xFE, 0xFD];
    assert!({
        #[allow(invalid_from_utf8)]
        std::str::from_utf8(non_utf8).is_err()
    });

    input.extend_from_slice(b"BIN*");
    input.extend_from_slice(non_utf8); // Invalid UTF-8
    input.push(b'~');

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(&input, &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let bin_segment = collector.get_segment(1).unwrap();
    assert_eq!(bin_segment.elements.len(), 1);
    assert_eq!(bin_segment.elements[0], non_utf8);
}
