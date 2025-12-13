# X12 837 Stream Parser - Quick Start Guide

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
segment = { path = "./segment" }
```

For `std` features (examples, String support):

```toml
[dependencies]
segment = { path = "./segment", features = ["std"] }
```

## Basic Usage

### 1. Simple Parsing

```rust
use segment::{Parser, Segment, SegmentHandler, Result};

// Create a handler
struct MyHandler;

impl SegmentHandler for MyHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        println!("Parsed segment: {:?}", segment.id_str());
        Ok(())
    }
}

// Parse data
let mut parser = Parser::new();
let mut handler = MyHandler;
let data = b"ISA*00*...*~GS*HC*...~";

let bytes_consumed = parser.parse_segment(data, &mut handler)?;
```

### 2. Streaming from Buffer

```rust
let mut parser = Parser::new();
let mut handler = MyHandler;
let mut buffer = /* your buffer */;

loop {
    match parser.parse_segment(buffer, &mut handler) {
        Ok(consumed) => {
            buffer = &buffer[consumed..];
            if buffer.is_empty() {
                break; // Need more data
            }
        }
        Err(Error::Incomplete) => {
            // Load more data into buffer
            break;
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
            break;
        }
    }
}
```

### 3. Accessing Element Data

```rust
impl SegmentHandler for MyHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        match segment.id_str()? {
            "NM1" => {
                // Access by index (0-based, excludes segment ID)
                if let Some(entity_code) = segment.element(0) {
                    println!("Entity: {:?}", entity_code.as_str());
                }
                
                // Required element (returns error if missing)
                let name = segment.required_element(2)?;
                println!("Name: {:?}", name.as_str());
            }
            "CLM" => {
                let claim_id = segment.required_element(0)?;
                let amount = segment.required_element(1)?;
                println!("Claim {} for ${:?}", 
                    claim_id.as_str().unwrap_or("?"),
                    amount.as_str().unwrap_or("?"));
            }
            _ => {}
        }
        Ok(())
    }
}
```

### 4. Splitting Sub-Elements

```rust
// For composite elements like "11:B:1"
if let Some(elem) = segment.element(3) {
    for component in elem.split_components(b':') {
        println!("Component: {:?}", std::str::from_utf8(component));
    }
}
```

## Common Patterns

### Pattern 1: Validation Handler

```rust
struct ValidationHandler {
    errors: Vec<String>,
}

impl SegmentHandler for ValidationHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        // Validate element count
        match segment.id_str()? {
            "ISA" => {
                if segment.element_count != 16 {
                    self.errors.push(
                        format!("ISA should have 16 elements, found {}",
                            segment.element_count));
                }
            }
            "ST" => {
                if segment.element_count < 2 {
                    self.errors.push("ST missing required elements".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}
```

### Pattern 2: State Tracking

```rust
struct StatefulHandler {
    in_claim: bool,
    current_claim_id: Option<String>,
}

impl SegmentHandler for StatefulHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        match segment.id_str()? {
            "CLM" => {
                self.in_claim = true;
                if let Some(id) = segment.element(0) {
                    self.current_claim_id = 
                        id.as_str().map(|s| s.to_string());
                }
            }
            "SE" => {
                self.in_claim = false;
                self.current_claim_id = None;
            }
            "LX" if self.in_claim => {
                println!("Service line in claim: {:?}", 
                    self.current_claim_id);
            }
            _ => {}
        }
        Ok(())
    }
}
```

### Pattern 3: Counting and Statistics

```rust
struct StatsHandler {
    segment_counts: HashMap<String, usize>,
    total_segments: usize,
}

impl SegmentHandler for StatsHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        self.total_segments += 1;
        
        if let Some(id) = segment.id_str() {
            *self.segment_counts.entry(id.to_string())
                .or_insert(0) += 1;
        }
        
        Ok(())
    }
}
```

## Error Handling

### Handling Incomplete Data

```rust
match parser.parse_segment(buffer, &mut handler) {
    Ok(consumed) => {
        // Success: advance buffer
        buffer = &buffer[consumed..];
    }
    Err(Error::Incomplete) => {
        // Expected: need more data
        // Move remaining data to start of buffer
        // Read more data
        // Try again
    }
    Err(Error::InvalidSegment) => {
        // Malformed segment
        // Log error, skip, or abort
    }
    Err(e) => {
        // Other errors
        eprintln!("Parse error: {:?}", e);
    }
}
```

### Handler Errors

```rust
impl SegmentHandler for MyHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        // Return error to stop parsing
        if segment.id_str()? == "BAD" {
            return Err(Error::InvalidSegment);
        }
        
        // Or use required_element which returns Result
        let required = segment.required_element(0)?;
        
        Ok(())
    }
}
```

## Buffer Management

### Strategy 1: Fixed Buffer with Compaction

```rust
let mut buffer = vec![0u8; 8192];
let mut start = 0;
let mut end = 0;

loop {
    // Compact when half full
    if start > buffer.len() / 2 {
        buffer.copy_within(start..end, 0);
        end -= start;
        start = 0;
    }
    
    // Read more data
    let n = reader.read(&mut buffer[end..])?;
    if n == 0 { break; }
    end += n;
    
    // Parse
    loop {
        match parser.parse_segment(&buffer[start..end], &mut handler) {
            Ok(consumed) => start += consumed,
            Err(Error::Incomplete) => break,
            Err(e) => return Err(e),
        }
    }
}
```

### Strategy 2: Growing Buffer

```rust
let mut buffer = Vec::new();
let mut chunk = [0u8; 4096];

loop {
    let n = reader.read(&mut chunk)?;
    if n == 0 { break; }
    buffer.extend_from_slice(&chunk[..n]);
    
    let mut offset = 0;
    loop {
        match parser.parse_segment(&buffer[offset..], &mut handler) {
            Ok(consumed) => offset += consumed,
            Err(Error::Incomplete) => break,
            Err(e) => return Err(e),
        }
    }
    
    // Remove processed data
    buffer.drain(..offset);
}
```

## Performance Tips

1. **Use large buffers**: 8KB-16KB is optimal
2. **Batch reads**: Read large chunks from I/O
3. **Minimize allocations**: Reuse buffers
4. **Process inline**: Don't store all segments in memory
5. **Use references**: Don't clone element data

## no_std Usage

The parser works in no_std environments:

```rust
#![no_std]

use segment::{Parser, Segment, SegmentHandler, Result};

// Stack-allocated buffer
let mut buffer = [0u8; 512];
let mut parser = Parser::new();

struct NoStdHandler {
    count: u32,
}

impl SegmentHandler for NoStdHandler {
    fn handle(&mut self, _segment: &Segment) -> Result<()> {
        self.count += 1;
        Ok(())
    }
}
```

## Common X12 837 Segments

| Segment | Description | Key Elements |
|---------|-------------|--------------|
| ISA | Interchange Header | [0]=Auth Qual, [5]=Sender, [7]=Receiver |
| GS | Functional Group | [0]=Func ID, [1]=Sender, [2]=Receiver |
| ST | Transaction Set | [0]=Trans ID ("837"), [1]=Control # |
| BHT | Transaction Header | [0]=Structure, [1]=Purpose, [2]=Ref# |
| NM1 | Entity Name | [0]=Entity ID, [1]=Type, [2]=Name |
| N3 | Address | [0]=Address Line 1, [1]=Address Line 2 |
| N4 | City/State/ZIP | [0]=City, [1]=State, [2]=ZIP |
| CLM | Claim | [0]=Claim ID, [1]=Amount |
| HI | Diagnosis | [0]=Diagnosis codes (composite) |
| LX | Service Line | [0]=Line number |
| SV1 | Professional Service | [0]=Procedure (composite), [1]=Amount |
| SE | Transaction Trailer | [0]=Segment count, [1]=Control # |
| GE | Group Trailer | [0]=Transaction count, [1]=Control # |
| IEA | Interchange Trailer | [0]=Group count, [1]=Control # |

## Debugging

Enable logging in your handler:

```rust
impl SegmentHandler for DebugHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        println!("Segment: {:?}", segment.id_str());
        println!("  Elements: {}", segment.element_count);
        
        for (i, elem) in segment.iter_elements().enumerate() {
            println!("  [{}]: {:?}", i, elem.as_str());
        }
        
        Ok(())
    }
}
```

## Testing

```rust
#[test]
fn test_my_handler() {
    let mut parser = Parser::new();
    let mut handler = MyHandler::new();
    
    let data = b"ISA*00*...*~ST*837*0001~SE*1*0001~GE*1*1~IEA*1*1~";
    let mut buffer = data.as_slice();
    
    while !buffer.is_empty() {
        let consumed = parser.parse_segment(buffer, &mut handler).unwrap();
        buffer = &buffer[consumed..];
    }
    
    assert_eq!(handler.segment_count, 5);
}
```

## Resources

- [X12 837 Specification](https://x12.org/)
- [Implementation Guides](https://www.cms.gov/medicare/regulations-guidance)
- [Design Document](DESIGN.md)
- [Benchmarks](BENCHMARKS.md)
- [Examples](segment/examples/)

## Next Steps

1. Run the examples: `cargo run --example parse_837 --features std`
2. Read the design document: [DESIGN.md](DESIGN.md)
3. Implement your own handler for your use case
4. Optimize buffer management for your workload

## Support

For issues or questions:

- Check the examples in `segment/examples/`
- Read the design documentation
- Review the integration tests
- Open an issue on GitHub
