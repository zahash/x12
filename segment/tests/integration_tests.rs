use segment::{Halt, Parser, ParserError, Segment, SegmentHandler};

/// Minimal handler that just counts segments
struct CountingHandler {
    count: usize,
}

impl CountingHandler {
    fn new() -> Self {
        Self { count: 0 }
    }
}

impl SegmentHandler for CountingHandler {
    fn handle(&mut self, _segment: &Segment) -> core::result::Result<(), Halt> {
        self.count += 1;
        Ok(())
    }
}

#[test]
fn test_complete_837_document() {
    let x12_data = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~\
                     GS*HC*SENDER*RECEIVER*20231213*1430*1*X*005010X222A1~\
                     ST*837*0001*005010X222A1~\
                     SE*1*0001~\
                     GE*1*1~\
                     IEA*1*000000001~";

    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();
    let mut buffer = x12_data.as_slice();

    while !buffer.is_empty() {
        match parser.parse_segment(buffer, &mut handler) {
            Ok(consumed) => {
                buffer = &buffer[consumed..];
            }
            Err(e) => {
                panic!("Parse error: {:?}", e);
            }
        }
    }

    assert_eq!(handler.count, 6);
}

#[test]
fn test_incomplete_isa() {
    let partial_isa = b"ISA*00*          *00*";

    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();

    let result = parser.parse_segment(partial_isa, &mut handler);
    assert_eq!(result, Err(ParserError::Incomplete));
}

#[test]
fn test_incomplete_regular_segment() {
    let mut parser = Parser::new();
    parser.set_delimiters(segment::Delimiters::default());

    let mut handler = CountingHandler::new();
    let partial_st = b"ST*837*0001";

    let result = parser.parse_segment(partial_st, &mut handler);
    assert_eq!(result, Err(ParserError::Incomplete));
}

#[test]
fn test_invalid_segment_id() {
    // Parser no longer validates segment IDs - that's the validator's job
    // Parser just parses the structure
    let mut parser = Parser::new();
    parser.set_delimiters(segment::Delimiters::default());

    let mut handler = CountingHandler::new();
    let bad_segment = b"1*element~";

    // Parser should successfully parse the structure
    let result = parser.parse_segment(bad_segment, &mut handler);
    assert!(result.is_ok());
    assert_eq!(handler.count, 1);
}

#[test]
fn test_element_splitting() {
    use segment::Element;

    let element = Element::new(b"VALUE1:VALUE2:VALUE3");
    let components: Vec<_> = element.split_components(b':').collect();

    assert_eq!(components.len(), 3);
    assert_eq!(components[0], b"VALUE1");
    assert_eq!(components[1], b"VALUE2");
    assert_eq!(components[2], b"VALUE3");
}

#[test]
fn test_empty_elements() {
    use segment::Element;

    // Test trailing empty elements
    let element = Element::new(b"A:B:");
    let components: Vec<_> = element.split_components(b':').collect();

    assert_eq!(components.len(), 3);
    assert_eq!(components[0], b"A");
    assert_eq!(components[1], b"B");
    assert_eq!(components[2], b"");
}

#[test]
fn test_segment_element_access() {
    let x12_data = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~";

    struct ElementCaptureHandler {
        captured: bool,
        element_5: Option<Vec<u8>>,
    }

    impl SegmentHandler for ElementCaptureHandler {
        fn handle(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
            // ISA06 is the sender ID (element 6 in new numbering)
            if let Some(elem) = segment.element(6) {
                self.element_5 = Some(elem.as_bytes().to_vec());
                self.captured = true;
            }
            Ok(())
        }
    }

    let mut parser = Parser::new();
    let mut handler = ElementCaptureHandler {
        captured: false,
        element_5: None,
    };

    parser.parse_segment(x12_data, &mut handler).unwrap();

    assert!(handler.captured);
    assert!(handler.element_5.is_some());
}

#[test]
fn test_multiple_segments_in_sequence() {
    let x12_data = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~\
                     GS*HC*SENDER*RECEIVER*20231213*1430*1*X*005010X222A1~\
                     ST*837*0001~";

    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();
    let mut buffer = x12_data.as_slice();

    // Parse ISA
    let consumed = parser.parse_segment(buffer, &mut handler).unwrap();
    assert_eq!(consumed, 106);
    buffer = &buffer[consumed..];

    // Parse GS
    let consumed = parser.parse_segment(buffer, &mut handler).unwrap();
    assert!(consumed > 0);
    buffer = &buffer[consumed..];

    // Parse ST
    let consumed = parser.parse_segment(buffer, &mut handler).unwrap();
    assert!(consumed > 0);

    assert_eq!(handler.count, 3);
}

#[test]
fn test_custom_delimiters() {
    // Use | as element separator and # as segment terminator
    let x12_data = b"ISA|00|          |00|          |ZZ|SENDER         |ZZ|RECEIVER       |231213|1430|^|00501|000000001|0|P|:#";

    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();

    let result = parser.parse_segment(x12_data, &mut handler);

    // Should successfully parse with custom delimiters
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 106);

    // Verify delimiters were extracted
    let delims = parser.delimiters();
    assert_eq!(delims.element, b'|');
    assert_eq!(delims.segment, b'#');
}

#[test]
fn test_parser_reset() {
    let isa = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~";

    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();

    parser.parse_segment(isa, &mut handler).unwrap();
    assert!(parser.is_initialized());

    parser.reset();
    assert!(!parser.is_initialized());
}

#[test]
fn test_utf8_element_conversion() {
    let isa = b"ISA*00*          *00*          *ZZ*SENDER         *ZZ*RECEIVER       *231213*1430*^*00501*000000001*0*P*:~";

    struct Utf8Handler {
        sender_found: bool,
    }

    impl SegmentHandler for Utf8Handler {
        fn handle(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
            // ISA06 is the sender ID (element 6 in new numbering)
            if let Some(elem) = segment.element(6) {
                if let Some(s) = elem.as_str() {
                    if s.contains("SENDER") {
                        self.sender_found = true;
                    }
                }
            }
            Ok(())
        }
    }

    let mut parser = Parser::new();
    let mut handler = Utf8Handler {
        sender_found: false,
    };

    parser.parse_segment(isa, &mut handler).unwrap();
    assert!(handler.sender_found);
}

#[test]
fn test_segment_id_validation() {
    // Parser no longer validates segment ID format - that's the validator's job
    // Parser just parses structure regardless of segment ID validity
    let mut parser = Parser::new();
    parser.set_delimiters(segment::Delimiters::default());

    let mut handler = CountingHandler::new();

    // All segment IDs should parse successfully
    // Validation of ID format happens in validators, not parser

    // One character
    let result = parser.parse_segment(b"A*element~", &mut handler);
    assert!(result.is_ok());

    // Four characters
    let result = parser.parse_segment(b"ABCD*element~", &mut handler);
    assert!(result.is_ok());

    // Two characters
    let result = parser.parse_segment(b"ST*element~", &mut handler);
    assert!(result.is_ok());

    // Three characters
    let result = parser.parse_segment(b"NM1*element~", &mut handler);
    assert!(result.is_ok());

    assert_eq!(handler.count, 4);
}

#[test]
fn test_empty_buffer() {
    let mut parser = Parser::new();
    let mut handler = CountingHandler::new();

    let result = parser.parse_segment(b"", &mut handler);
    assert_eq!(result, Err(ParserError::Incomplete));
}

#[test]
fn test_segment_with_no_elements() {
    let mut parser = Parser::new();
    parser.set_delimiters(segment::Delimiters::default());

    let mut handler = CountingHandler::new();

    let result = parser.parse_segment(b"SE~", &mut handler);
    assert!(result.is_ok());
}

#[test]
fn test_segment_with_empty_elements() {
    let mut parser = Parser::new();
    parser.set_delimiters(segment::Delimiters::default());

    struct ElementCountHandler {
        count: usize,
    }

    impl SegmentHandler for ElementCountHandler {
        fn handle(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
            self.count = segment.element_count();
            Ok(())
        }
    }

    let mut handler = ElementCountHandler { count: 0 };

    // Segment with empty elements: NM1***VALUE3
    // element_count includes: NM1-00 (segment ID), NM1-01 (empty), NM1-02 (empty), NM1-03 (VALUE3)
    let result = parser.parse_segment(b"NM1***VALUE3~", &mut handler);
    assert!(result.is_ok());
    assert_eq!(handler.count, 4); // Four elements: segment ID + three data elements
}
