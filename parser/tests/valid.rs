//! Integration tests for valid X12 documents
//!
//! These tests verify that the parser correctly processes valid X12 documents
//! by reconstructing the input from parsed segments and comparing them.
//! For all valid inputs: input == reconstructed output

mod common;

use common::SegmentCollector;
use parser::{SegmentParser, SegmentParserError};

use pretty_assertions::assert_eq;

#[test]
fn test_minimal_isa_segment() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should succeed on valid ISA");
    assert_eq!(result.unwrap(), input.len(), "Should consume entire input");
    assert_eq!(
        collector.segment_count(),
        1,
        "Should parse exactly 1 segment"
    );

    let reconstructed = collector.reconstruct();
    assert_eq!(
        input, reconstructed,
        "Reconstructed output must match input"
    );
}

#[test]
fn test_complete_minimal_interchange() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                 ST*837*0001*005010X222A1~\
                 SE*1*0001~\
                 GE*1*1~\
                 IEA*1*000000001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok(), "Parser should succeed on valid interchange");
    assert_eq!(result.unwrap(), input.len());
    assert_eq!(collector.segment_count(), 6, "Should parse 6 segments");

    let reconstructed = collector.reconstruct();
    assert_eq!(
        input, reconstructed,
        "Reconstructed output must match input"
    );
}

#[test]
fn test_multiple_transactions_in_group() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                 ST*837*0001*005010X222A1~\
                 SE*1*0001~\
                 ST*837*0002*005010X222A1~\
                 SE*1*0002~\
                 ST*837*0003*005010X222A1~\
                 SE*1*0003~\
                 GE*3*1~\
                 IEA*1*000000001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 10, "Should parse 10 segments");

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_multiple_groups_in_interchange() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                 ST*837*0001*005010X222A1~\
                 SE*1*0001~\
                 GE*1*1~\
                 GS*HC*SENDER*RECEIVER*20210101*1201*2*X*005010~\
                 ST*837*0002*005010X222A1~\
                 SE*1*0002~\
                 GE*1*2~\
                 IEA*2*000000001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 10, "Should parse 10 segments");

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_empty_elements() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 NM1*IL*1**LAST*FIRST**MI~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    // Verify empty elements are preserved
    let nm1_segment = collector.get_segment(1).unwrap();
    assert_eq!(nm1_segment.elements.len(), 7);
    assert_eq!(nm1_segment.elements[2], b""); // Empty element
}

#[test]
fn test_trailing_element_separator() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 REF*D9*12345*~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    // Verify trailing separator creates empty element
    let ref_segment = collector.get_segment(1).unwrap();
    assert_eq!(ref_segment.elements.len(), 3);
    assert_eq!(ref_segment.elements[2], b""); // Trailing empty element
}

#[test]
fn test_segment_with_many_elements() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 DMG*D8*19800101*M*W*2*3*4*5*6*7*8*9*10~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    let dmg_segment = collector.get_segment(1).unwrap();
    assert_eq!(dmg_segment.elements.len(), 13);
}

#[test]
fn test_segment_with_no_data_elements() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 BHT~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    let bht_segment = collector.get_segment(1).unwrap();
    assert_eq!(bht_segment.id, b"BHT");
    assert_eq!(bht_segment.elements.len(), 0, "BHT has no data elements");
}

#[test]
fn test_sub_elements_composite_elements() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 N3*123 MAIN ST:APT 4B~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    // Verify sub-element separator is preserved in element data
    let n3_segment = collector.get_segment(1).unwrap();
    assert_eq!(n3_segment.elements[0], b"123 MAIN ST:APT 4B");
}

#[test]
fn test_special_characters_in_data() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 NTE*ADD*PATIENT HAS DIABETES & HYPERTENSION~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_numeric_data_preservation() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 AMT*T*1234.56~\
                 QTY*PT*00042~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 3);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);

    // Verify numeric formatting is preserved
    let amt_segment = collector.get_segment(1).unwrap();
    assert_eq!(amt_segment.elements[1], b"1234.56");

    let qty_segment = collector.get_segment(2).unwrap();
    assert_eq!(qty_segment.elements[1], b"00042", "Leading zeros preserved");
}

#[test]
fn test_alternative_delimiters() {
    // Using | as element separator, ~ as segment terminator (less common but valid)
    let input = "ISA|00|          |00|          |ZZ|SENDER         |ZZ|RECEIVER       |210101|1200|^|00501|000000001|0|P|:~\
                 GS|HC|SENDER|RECEIVER|20210101|1200|1|X|005010~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_whitespace_in_elements() {
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 NM1*IL*1*  SMITH  *  JOHN  ~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 2);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed, "Whitespace must be preserved exactly");
}

#[test]
fn test_long_segment_id() {
    // Some segments have 3-character IDs
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 BHT*0019*00*123*20210101*1200*CH~\
                 REF*D9*12345~\
                 NM1*IL*1*SMITH*JOHN~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 4);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_two_character_segment_id() {
    // Some segments have 2-character IDs (rare but valid)
    let input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                 ST*837*0001~\
                 SE*1*0001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    let result = parser.parse_segments(input.as_bytes(), &mut collector);

    assert!(result.is_ok());
    assert_eq!(collector.segment_count(), 3);

    let reconstructed = collector.reconstruct();
    assert_eq!(input, reconstructed);
}

#[test]
fn test_chunked_parsing_simulation() {
    // Simulate reading data in chunks (like streaming from a file)
    let full_input = "ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *210101*1200*^*00501*000000001*0*P*:~\
                      GS*HC*SENDER*RECEIVER*20210101*1200*1*X*005010~\
                      ST*837*0001~\
                      SE*1*0001~\
                      GE*1*1~\
                      IEA*1*000000001~";

    let mut parser = SegmentParser::init();
    let mut collector = SegmentCollector::new();

    // Parse in chunks
    let chunk_size = 120;
    let mut offset = 0;

    while offset < full_input.len() {
        let end = (offset + chunk_size).min(full_input.len());
        let chunk = &full_input.as_bytes()[offset..end];

        match parser.parse_segments(chunk, &mut collector) {
            Ok(consumed) => {
                offset += consumed;
            }
            Err(SegmentParserError::Incomplete) => {
                // Need more data - in real scenario would read more
                // For this test, we just move to next chunk
                offset = end;
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    assert_eq!(collector.segment_count(), 6);

    let reconstructed = collector.reconstruct();
    assert_eq!(full_input, reconstructed);
}
