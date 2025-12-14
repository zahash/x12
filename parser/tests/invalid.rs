//! Integration tests for invalid X12 documents
//!
//! These tests verify that the parser correctly detects and handles errors in malformed
//! X12 documents. The reconstruction should contain only valid segments before the error.

mod common;

use common::SegmentCollector;
use parser::{Halt, Segment, SegmentHandler, SegmentParser, SegmentParserError};

use pretty_assertions::assert_eq;

#[test]
fn test_incomplete_isa_segment() {
    // ISA requires exactly 106 bytes, this is too short
    let input = "ISA*00*          *00*          *ZZ*SENDER";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let err = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap_err();

    assert!(matches!(err, SegmentParserError::Incomplete));
    assert_eq!(collector.segment_count(), 0);
    assert_eq!(collector.reconstruct(), "");
}

#[test]
fn test_invalid_isa_header() {
    // Does not start with "ISA"
    let input = "GS *00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let SegmentParserError::Halt(Halt { message }) = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap_err()
    else {
        panic!("Expected Halt error");
    };

    assert!(message.contains("ISA"));
    assert_eq!(collector.segment_count(), 0);
    assert_eq!(collector.reconstruct(), "");
}

#[test]
fn test_empty_segment_id() {
    let isa = String::from(
        "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~",
    );
    let input = isa.clone() + "*ELEMENT1*ELEMENT2~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let SegmentParserError::Halt(Halt { message }) = parser
        .parse_segments(input.as_bytes(), &mut collector)
        .unwrap_err()
    else {
        panic!("Expected Halt error");
    };

    assert!(message.contains("segment ID cannot be empty"));

    // ISA should have been parsed successfully before the error
    assert_eq!(collector.segment_count(), 1);
    assert_eq!(collector.reconstruct(), isa);
}

#[test]
fn test_incomplete_regular_segment() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010";
    // Missing segment terminator

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    // Parser returns Ok with bytes consumed when it successfully parses ISA,
    // then encounters incomplete segment - this is streaming behavior
    assert!(
        result.is_ok(),
        "Parser should return Ok with partial parse in streaming mode"
    );

    // ISA should have been parsed
    assert_eq!(collector.segment_count(), 1);

    let reconstructed = collector.reconstruct();
    let expected = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~";
    assert_eq!(reconstructed, expected);
}

#[test]
fn test_error_mid_document() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                 ST*837*0001~\
                 *INVALID*SEGMENT~\
                 SE*1*0001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_err());

    // Should have parsed 3 segments before error
    assert_eq!(collector.segment_count(), 3);

    let reconstructed = collector.reconstruct();
    let expected = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                    GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                    ST*837*0001~";
    assert_eq!(reconstructed, expected);
}

#[test]
fn test_no_segment_terminator_in_buffer() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    // Streaming behavior: returns Ok with ISA parsed, incomplete GS is left in buffer
    assert!(result.is_ok(), "Parser returns Ok in streaming mode");
    assert_eq!(collector.segment_count(), 1, "Only ISA parsed");
}

#[test]
fn test_handler_error_propagation() {
    // Test that handler errors stop parsing immediately
    struct ErrorOnSecondSegment {
        count: usize,
    }

    impl SegmentHandler for ErrorOnSecondSegment {
        fn handle(&mut self, _segment: &Segment) -> Result<(), Halt> {
            self.count += 1;
            if self.count == 2 {
                Err(Halt::new("Handler requested halt"))
            } else {
                Ok(())
            }
        }
    }

    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                 ST*837*0001~";

    let mut parser = SegmentParser::init();
    let mut handler = ErrorOnSecondSegment { count: 0 };

    let result = parser.parse_segments(input.as_bytes(), &mut handler);

    assert!(result.is_err());
    match result {
        Err(SegmentParserError::Halt(halt)) => {
            assert_eq!(halt.message, "Handler requested halt");
        }
        _ => panic!("Expected Halt error"),
    }

    assert_eq!(handler.count, 2, "Handler called exactly twice");
}

#[test]
fn test_buffer_contains_only_whitespace() {
    let input = "     ";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    // Should fail because it doesn't start with ISA
    assert!(result.is_err());
    assert_eq!(collector.segment_count(), 0);
}

#[test]
fn test_empty_buffer() {
    let input = "";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    // Empty buffer is not an error - just returns 0 bytes consumed
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
    assert_eq!(collector.segment_count(), 0);
}
