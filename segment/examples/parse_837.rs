use segment::{Parser, ParserError, Segment, SegmentHandler};

/// Example segment handler that validates X12 837 structure
struct X12Handler {
    interchange_count: u32,
    group_count: u32,
    transaction_count: u32,
    in_transaction: bool,
    in_group: bool,
    in_interchange: bool,
}

impl X12Handler {
    fn new() -> Self {
        Self {
            interchange_count: 0,
            group_count: 0,
            transaction_count: 0,
            in_transaction: false,
            in_group: false,
            in_interchange: false,
        }
    }

    fn validate_isa(&self, segment: &Segment) -> core::result::Result<(), ParserError> {
        // ISA must have 16 elements
        if segment.element_count != 16 {
            return Err(ParserError::InvalidElementCount);
        }

        // Validate ISA05/ISA07 (Interchange ID Qualifiers)
        let isa05 = segment.required_element(4)?;
        if isa05.as_bytes().len() != 2 {
            return Err(ParserError::InvalidSegment);
        }

        println!("ISA: Interchange Control Header");
        println!(
            "  Sender: {:?}",
            segment.element(5).and_then(|e| e.as_str())
        );
        println!(
            "  Receiver: {:?}",
            segment.element(7).and_then(|e| e.as_str())
        );
        println!("  Date: {:?}", segment.element(8).and_then(|e| e.as_str()));
        println!("  Time: {:?}", segment.element(9).and_then(|e| e.as_str()));
        println!(
            "  Control Number: {:?}",
            segment.element(12).and_then(|e| e.as_str())
        );

        Ok(())
    }

    fn validate_gs(&self, segment: &Segment) -> core::result::Result<(), ParserError> {
        // GS must have at least 8 elements
        if segment.element_count < 8 {
            return Err(ParserError::InvalidElementCount);
        }

        let functional_id = segment.required_element(0)?;
        println!("GS: Functional Group Header");
        println!("  Functional ID Code: {:?}", functional_id.as_str());
        println!(
            "  Application Sender: {:?}",
            segment.element(1).and_then(|e| e.as_str())
        );
        println!(
            "  Application Receiver: {:?}",
            segment.element(2).and_then(|e| e.as_str())
        );
        println!("  Date: {:?}", segment.element(3).and_then(|e| e.as_str()));
        println!(
            "  Control Number: {:?}",
            segment.element(5).and_then(|e| e.as_str())
        );

        Ok(())
    }

    fn validate_st(&self, segment: &Segment) -> core::result::Result<(), ParserError> {
        // ST must have at least 2 elements
        if segment.element_count < 2 {
            return Err(ParserError::InvalidElementCount);
        }

        let transaction_set = segment.required_element(0)?;
        let control_number = segment.required_element(1)?;

        println!("ST: Transaction Set Header");
        println!("  Transaction Set ID: {:?}", transaction_set.as_str());
        println!("  Control Number: {:?}", control_number.as_str());

        // For 837, transaction set should be "837"
        if transaction_set.as_bytes() == b"837" {
            println!("  Type: Healthcare Claim (837)");
        }

        Ok(())
    }

    fn handle_nm1(&self, segment: &Segment) -> core::result::Result<(), ParserError> {
        // NM1 - Entity Identifier
        println!("NM1: Entity Identifier");

        if let Some(entity_id) = segment.element(0) {
            println!("  Entity ID Code: {:?}", entity_id.as_str());
        }

        if let Some(entity_type) = segment.element(1) {
            println!("  Entity Type: {:?}", entity_type.as_str());
        }

        if let Some(name) = segment.element(2) {
            println!("  Name: {:?}", name.as_str());
        }

        Ok(())
    }

    fn handle_clm(&self, segment: &Segment) -> core::result::Result<(), ParserError> {
        // CLM - Claim Information
        println!("CLM: Claim Information");

        if let Some(claim_id) = segment.element(0) {
            println!("  Claim ID: {:?}", claim_id.as_str());
        }

        if let Some(amount) = segment.element(1) {
            println!("  Claim Amount: {:?}", amount.as_str());
        }

        Ok(())
    }
}

impl SegmentHandler for X12Handler {
    type Error = ParserError;

    fn handle(&mut self, segment: &Segment) -> core::result::Result<(), Self::Error> {
        let id = segment.id_str().ok_or(ParserError::InvalidSegmentId)?;

        match id {
            "ISA" => {
                self.validate_isa(segment)?;
                self.in_interchange = true;
                self.interchange_count += 1;
            }
            "IEA" => {
                println!("IEA: Interchange Control Trailer");
                self.in_interchange = false;
            }
            "GS" => {
                if !self.in_interchange {
                    return Err(ParserError::InvalidSegment);
                }
                self.validate_gs(segment)?;
                self.in_group = true;
                self.group_count += 1;
            }
            "GE" => {
                println!("GE: Functional Group Trailer");
                self.in_group = false;
            }
            "ST" => {
                if !self.in_group {
                    return Err(ParserError::InvalidSegment);
                }
                self.validate_st(segment)?;
                self.in_transaction = true;
                self.transaction_count += 1;
            }
            "SE" => {
                println!("SE: Transaction Set Trailer");
                self.in_transaction = false;
            }
            "BHT" => {
                println!("BHT: Beginning of Hierarchical Transaction");
            }
            "REF" => {
                println!("REF: Reference Identification");
            }
            "NM1" => {
                self.handle_nm1(segment)?;
            }
            "N3" => {
                println!("N3: Address Information");
            }
            "N4" => {
                println!("N4: Geographic Location");
            }
            "PER" => {
                println!("PER: Administrative Communications Contact");
            }
            "CLM" => {
                self.handle_clm(segment)?;
            }
            "HI" => {
                println!("HI: Health Care Diagnosis Code");
            }
            "LX" => {
                println!("LX: Service Line Number");
            }
            "SV1" | "SV2" => {
                println!("{}: Professional/Institutional Service", id);
            }
            "DTP" => {
                println!("DTP: Date or Time Period");
            }
            "HL" => {
                println!("HL: Hierarchical Level");
            }
            _ => {
                println!("{}: (Segment ID: {})", id, id);
            }
        }

        Ok(())
    }
}

fn main() {
    println!("X12 837 Stream Parser Example\n");

    // Example X12 837 data (simplified)
    let x12_data = b"ISA*00*          *00*          *ZZ*SENDER123      *ZZ*RECEIVER456    *231213*1430*^*00501*000000001*0*P*:~\
                     GS*HC*SENDER*RECEIVER*20231213*1430*1*X*005010X222A1~\
                     ST*837*0001*005010X222A1~\
                     BHT*0019*00*123456*20231213*1430*CH~\
                     NM1*41*2*PROVIDER CLINIC*****46*12345~\
                     PER*IC*JOHN DOE*TE*5551234567~\
                     NM1*40*2*INSURANCE CO*****46*67890~\
                     HL*1**20*1~\
                     NM1*85*2*BILLING PROVIDER*****XX*1234567890~\
                     N3*123 MAIN ST~\
                     N4*CITY*ST*12345~\
                     REF*EI*123456789~\
                     HL*2*1*22*0~\
                     NM1*IL*1*DOE*JANE****MI*123456789~\
                     N3*456 OAK AVE~\
                     N4*TOWN*ST*54321~\
                     CLM*PATIENT123*100.00***11:B:1*Y*A*Y*Y~\
                     DTP*431*D8*20231201~\
                     HI*ABK:Z1234~\
                     LX*1~\
                     SV1*HC:99213*50.00*UN*1***1~\
                     DTP*472*D8*20231201~\
                     SE*22*0001~\
                     GE*1*1~\
                     IEA*1*000000001~";

    let mut parser = Parser::new();
    let mut handler = X12Handler::new();
    let mut buffer = x12_data.as_slice();
    let mut total_bytes = 0;

    loop {
        match parser.parse_segment(buffer, &mut handler) {
            Ok(bytes_read) => {
                total_bytes += bytes_read;
                buffer = &buffer[bytes_read..];
                println!("  -> Consumed {} bytes\n", bytes_read);

                if buffer.is_empty() {
                    break;
                }
            }
            Err(ParserError::Incomplete) => {
                println!("\nIncomplete segment, need more data");
                break;
            }
            Err(e) => {
                println!("\nError parsing segment: {:?}", e);
                break;
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total bytes parsed: {}", total_bytes);
    println!("Interchanges: {}", handler.interchange_count);
    println!("Functional Groups: {}", handler.group_count);
    println!("Transaction Sets: {}", handler.transaction_count);
}
