use segment::{Halt, Parser, Segment, SegmentHandler};

/// A more sophisticated handler that maintains state for SNIP validation
/// and hierarchical structure validation
struct ValidationHandler {
    // Envelope tracking
    isa_control_number: Option<u32>,
    gs_control_number: Option<u32>,
    st_control_number: Option<u32>,

    // Counters for validation
    isa_segment_count: u32,
    gs_segment_count: u32,
    st_segment_count: u32,

    // Expected counts from trailers
    expected_groups: Option<u32>,
    expected_transactions: Option<u32>,
    expected_segments: Option<u32>,

    // Hierarchical level tracking
    current_hl_id: Option<u32>,
    hl_stack: [Option<u32>; 16],
    hl_stack_depth: usize,

    // State flags
    in_interchange: bool,
    in_group: bool,
    in_transaction: bool,

    // SNIP Level validations
    validation_level: SnipLevel,
    errors: [Option<ValidationError>; 64],
    error_count: usize,
}

#[derive(Debug, Clone, Copy)]
enum SnipLevel {
    Level1, // Syntax validation
    Level2, // Business scenario validation
    Level3, // Implementation-specific validation
    Level4, // External code set validation
    Level5, // Data value validation
    Level6, // Situational data element validation
    Level7, // Inter-segment validation
}

#[derive(Debug, Clone, Copy)]
struct ValidationError {
    segment_id: [u8; 3],
    error_type: ErrorType,
    element_position: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
enum ErrorType {
    // MandatorySegmentMissing,
    SegmentSequenceError,
    // MandatoryElementMissing,
    InvalidElementValue,
    ControlNumberMismatch,
    CountMismatch,
}

impl ValidationHandler {
    fn new(level: SnipLevel) -> Self {
        Self {
            isa_control_number: None,
            gs_control_number: None,
            st_control_number: None,
            isa_segment_count: 0,
            gs_segment_count: 0,
            st_segment_count: 0,
            expected_groups: None,
            expected_transactions: None,
            expected_segments: None,
            current_hl_id: None,
            hl_stack: [None; 16],
            hl_stack_depth: 0,
            in_interchange: false,
            in_group: false,
            in_transaction: false,
            validation_level: level,
            errors: [None; 64],
            error_count: 0,
        }
    }

    fn add_error(&mut self, segment_id: &[u8], error_type: ErrorType, element: Option<u8>) {
        if self.error_count < self.errors.len() {
            let mut id_bytes = [0u8; 3];
            let len = segment_id.len().min(3);
            id_bytes[..len].copy_from_slice(&segment_id[..len]);

            self.errors[self.error_count] = Some(ValidationError {
                segment_id: id_bytes,
                error_type,
                element_position: element,
            });
            self.error_count += 1;
        }
    }

    fn validate_isa(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        // SNIP Level 1: Syntax validation
        // ISA has 17 elements total: ISA-00 (segment ID) through ISA-16
        if segment.element_count() != 17 {
            self.add_error(segment.id, ErrorType::SegmentSequenceError, None);
        }

        // Extract and store control number (ISA13)
        if let Some(control) = segment.element(13) {
            if let Some(ctrl_str) = control.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    self.isa_control_number = Some(num);
                }
            }
        }

        // SNIP Level 2: Validate ISA01 and ISA03 qualifiers
        if let Some(isa01) = segment.element(1) {
            if !is_valid_qualifier(isa01.as_bytes(), &[b"00", b"03"]) {
                self.add_error(segment.id, ErrorType::InvalidElementValue, Some(1));
            }
        }

        // SNIP Level 5: Validate ISA15 (Usage Indicator)
        if let Some(isa15) = segment.element(15) {
            if !is_valid_qualifier(isa15.as_bytes(), &[b"T", b"P", b"I"]) {
                self.add_error(segment.id, ErrorType::InvalidElementValue, Some(15));
            }
        }

        self.in_interchange = true;
        self.isa_segment_count = 0;
        Ok(())
    }

    fn validate_iea(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        // IEA01 - Number of Included Functional Groups
        if let Some(count_elem) = segment.element(1) {
            if let Some(count_str) = count_elem.as_str() {
                if let Ok(count) = parse_u32(count_str.as_bytes()) {
                    self.expected_groups = Some(count);
                }
            }
        }

        // IEA02 - Interchange Control Number (must match ISA13)
        if let Some(control_elem) = segment.element(2) {
            if let Some(ctrl_str) = control_elem.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    if let Some(isa_ctrl) = self.isa_control_number {
                        if num != isa_ctrl {
                            self.add_error(segment.id, ErrorType::ControlNumberMismatch, Some(2));
                        }
                    }
                }
            }
        }

        self.in_interchange = false;
        Ok(())
    }

    fn validate_gs(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        if !self.in_interchange {
            self.add_error(segment.id, ErrorType::SegmentSequenceError, None);
            // Validation error recorded
        }

        // SNIP Level 1: GS requires at least 9 elements (GS-00 through GS-08)
        if segment.element_count() < 9 {
            // Validation error recorded
        }

        // Extract GS06 - Group Control Number
        if let Some(control) = segment.element(6) {
            if let Some(ctrl_str) = control.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    self.gs_control_number = Some(num);
                }
            }
        }

        // SNIP Level 2: Validate GS01 Functional Identifier Code
        if let Some(gs01) = segment.element(1) {
            // For 837, should be "HC" (Health Care Claim)
            if gs01.as_bytes() != b"HC" {
                println!("Warning: GS01 is not 'HC' for healthcare claim");
            }
        }

        // SNIP Level 5: Validate GS08 Version
        if let Some(version) = segment.element(8) {
            if let Some(ver_str) = version.as_str() {
                if !ver_str.starts_with("00501") {
                    println!("Warning: Non-standard version: {}", ver_str);
                }
            }
        }

        self.in_group = true;
        self.gs_segment_count = 0;
        Ok(())
    }

    fn validate_ge(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        // GE01 - Number of Transaction Sets
        if let Some(count_elem) = segment.element(1) {
            if let Some(count_str) = count_elem.as_str() {
                if let Ok(count) = parse_u32(count_str.as_bytes()) {
                    self.expected_transactions = Some(count);
                }
            }
        }

        // GE02 - Group Control Number (must match GS06)
        if let Some(control_elem) = segment.element(2) {
            if let Some(ctrl_str) = control_elem.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    if let Some(gs_ctrl) = self.gs_control_number {
                        if num != gs_ctrl {
                            self.add_error(segment.id, ErrorType::ControlNumberMismatch, Some(2));
                        }
                    }
                }
            }
        }

        self.in_group = false;
        Ok(())
    }

    fn validate_st(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        if !self.in_group {
            self.add_error(segment.id, ErrorType::SegmentSequenceError, None);
            // Validation error recorded
        }

        // Extract ST02 - Transaction Set Control Number
        if let Some(control) = segment.element(2) {
            if let Some(ctrl_str) = control.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    self.st_control_number = Some(num);
                }
            }
        }

        // Validate ST01 - Transaction Set Identifier Code
        if let Some(st01) = segment.element(1) {
            if st01.as_bytes() != b"837" {
                println!("Warning: Not an 837 transaction set");
            }
        }

        self.in_transaction = true;
        self.st_segment_count = 1; // ST counts as first segment
        Ok(())
    }

    fn validate_se(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        self.st_segment_count += 1; // SE counts in the total

        // SE01 - Number of Included Segments
        if let Some(count_elem) = segment.element(1) {
            if let Some(count_str) = count_elem.as_str() {
                if let Ok(count) = parse_u32(count_str.as_bytes()) {
                    if count != self.st_segment_count {
                        self.add_error(segment.id, ErrorType::CountMismatch, Some(1));
                        println!(
                            "Warning: SE01 count {} doesn't match actual count {}",
                            count, self.st_segment_count
                        );
                    }
                }
            }
        }

        // SE02 - Transaction Set Control Number (must match ST02)
        if let Some(control_elem) = segment.element(2) {
            if let Some(ctrl_str) = control_elem.as_str() {
                if let Ok(num) = parse_u32(ctrl_str.as_bytes()) {
                    if let Some(st_ctrl) = self.st_control_number {
                        if num != st_ctrl {
                            self.add_error(segment.id, ErrorType::ControlNumberMismatch, Some(2));
                        }
                    }
                }
            }
        }

        self.in_transaction = false;
        Ok(())
    }

    fn validate_hl(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        // SNIP Level 7: Inter-segment validation for hierarchical structure

        // HL01 - Hierarchical ID Number
        if let Some(hl_id) = segment.element(1) {
            if let Some(id_str) = hl_id.as_str() {
                if let Ok(id) = parse_u32(id_str.as_bytes()) {
                    self.current_hl_id = Some(id);
                }
            }
        }

        // HL02 - Hierarchical Parent ID Number
        if let Some(parent_elem) = segment.element(2) {
            if !parent_elem.is_empty() {
                if let Some(parent_str) = parent_elem.as_str() {
                    if let Ok(parent_id) = parse_u32(parent_str.as_bytes()) {
                        // Verify parent exists in stack
                        let mut found = false;
                        for i in 0..self.hl_stack_depth {
                            if self.hl_stack[i] == Some(parent_id) {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            println!("Warning: Parent HL ID {} not found", parent_id);
                        }
                    }
                }
            }
        }

        // Add current HL to stack
        if let Some(current) = self.current_hl_id {
            if self.hl_stack_depth < self.hl_stack.len() {
                self.hl_stack[self.hl_stack_depth] = Some(current);
                self.hl_stack_depth += 1;
            }
        }

        Ok(())
    }

    fn print_validation_summary(&self) {
        println!("\n=== Validation Summary ===");
        println!("SNIP Level: {:?}", self.validation_level);
        println!("Errors found: {}", self.error_count);

        for i in 0..self.error_count {
            if let Some(error) = self.errors[i] {
                let id_str = core::str::from_utf8(&error.segment_id)
                    .unwrap_or("???")
                    .trim_end_matches('\0');
                print!("  {} - {:?}", id_str, error.error_type);
                if let Some(pos) = error.element_position {
                    print!(" at element {}", pos);
                }
                println!();
            }
        }
    }
}

impl SegmentHandler for ValidationHandler {
    fn handle(&mut self, segment: &Segment) -> core::result::Result<(), Halt> {
        let id = segment.id_str().unwrap_or("???");

        // Count all segments within transaction
        if self.in_transaction && id != "ST" && id != "SE" {
            self.st_segment_count += 1;
        }

        match id {
            "ISA" => self.validate_isa(segment),
            "IEA" => self.validate_iea(segment),
            "GS" => self.validate_gs(segment),
            "GE" => self.validate_ge(segment),
            "ST" => self.validate_st(segment),
            "SE" => self.validate_se(segment),
            "HL" => self.validate_hl(segment),
            _ => Ok(()),
        }
    }
}

// Helper functions
fn parse_u32(bytes: &[u8]) -> core::result::Result<u32, ()> {
    let s = core::str::from_utf8(bytes).map_err(|_| ())?;
    let trimmed = s.trim();

    let mut result = 0u32;
    for byte in trimmed.bytes() {
        if byte < b'0' || byte > b'9' {
            return Err(());
        }
        result = result.checked_mul(10).ok_or(())?;
        result = result.checked_add((byte - b'0') as u32).ok_or(())?;
    }
    Ok(result)
}

fn is_valid_qualifier(value: &[u8], valid_values: &[&[u8]]) -> bool {
    let trimmed = trim_bytes(value);
    valid_values.iter().any(|&v| v == trimmed)
}

fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|&b| b != b' ').unwrap_or(0);
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ')
        .map(|i| i + 1)
        .unwrap_or(0);
    &bytes[start..end]
}

fn main() {
    println!("X12 837 Streaming Validation Example\n");

    let x12_data = b"ISA*00*          *00*          *ZZ*SENDER123      *ZZ*RECEIVER456    *231213*1430*^*00501*000000001*0*P*:~\
                     GS*HC*SENDER*RECEIVER*20231213*1430*1*X*005010X222A1~\
                     ST*837*0001*005010X222A1~\
                     BHT*0019*00*123456*20231213*1430*CH~\
                     NM1*41*2*PROVIDER*****46*12345~\
                     HL*1**20*1~\
                     HL*2*1*22*0~\
                     CLM*CLAIM001*100.00***11:B:1*Y*A*Y*Y~\
                     SE*8*0001~\
                     GE*1*1~\
                     IEA*1*000000001~";

    let mut parser = Parser::new();
    let mut handler = ValidationHandler::new(SnipLevel::Level7);
    let mut buffer = x12_data.as_slice();
    let mut segment_count = 0;

    println!("Starting validation...\n");

    loop {
        match parser.parse_segment(buffer, &mut handler) {
            Ok(bytes_read) => {
                segment_count += 1;
                buffer = &buffer[bytes_read..];

                if buffer.is_empty() {
                    break;
                }
            }
            Err(Halt) => {
                println!("Parsing halted");
                break;
            }
        }
    }

    println!("\nTotal segments processed: {}", segment_count);
    handler.print_validation_summary();
}
