# X12 837 Stream Parser - Design Document

## Overview

This is a production-ready, no_std Rust parser for X12 837 (Healthcare Claim) documents. The design is inspired by `httparse` and optimized for streaming, zero-copy parsing.

## Design Goals

1. **Zero-Copy**: All parsed data references the original buffer
2. **Streaming**: Parse one segment at a time without buffering
3. **Ephemeral**: No long-term memory storage
4. **Efficient**: Minimal allocations, suitable for embedded systems
5. **Standard-Compliant**: Follows X12 837 specification closely

## Architecture

### Core Components

#### 1. Parser (`Parser`)

- **Responsibility**: State machine for parsing X12 segments
- **State**:
  - `Initial`: Waiting for ISA segment to extract delimiters
  - `Processing`: Parsing regular segments with known delimiters
- **Key Methods**:
  - `parse_segment()`: Main entry point, returns bytes consumed or `Incomplete`
  - `parse_isa_segment()`: Special handling for ISA (fixed-width format)
  - `parse_regular_segment()`: Handles all other segments

#### 2. Segment (`Segment<'a>`)

- **Responsibility**: Zero-copy representation of a parsed segment
- **Design Choice**: Single struct with array of optional elements
- **Why not enum/dedicated structs?**
  - Flexibility: Single type handles all segment types
  - Performance: No allocations, fixed size on stack
  - Simplicity: Easier to work with in callbacks
  - Trade-off: Less type safety vs. performance and flexibility

#### 3. Element (`Element<'a>`)

- **Responsibility**: Reference to a single element within a segment
- **Features**:
  - Access as bytes or UTF-8 string
  - Split into sub-components
  - Zero-copy operations

#### 4. SegmentHandler (trait)

- **Responsibility**: Process parsed segments in streaming fashion
- **Design Pattern**: Callback interface
- **Lifetime**: Segment reference valid only during callback
- **Use Cases**:
  - Validation (SNIP levels 1-7)
  - State tracking (hierarchical levels, control numbers)
  - Business logic processing

### Memory Model

```text
Host Application Buffer:
┌────────────────────────────────────────┐
│ ISA*00*...*~GS*HC*...*~ST*837*...*~   │
└────────────────────────────────────────┘
        ↑           ↑          ↑
        │           │          │
Parser reads slice: │          │
        │           │          │
    ┌───┴───────┬───┴──────┬───┴──────┐
    │ Segment 1 │ Segment 2│ Segment 3│
    └───────────┴──────────┴──────────┘
           ↓
    Segment<'buf> {
        id: &'buf [u8],
        elements: [Option<Element<'buf>>; MAX_ELEMENTS],
        ...
    }
           ↓
    Handler.handle(&segment)
           ↓
    Processing must complete before buffer changes
```

### Flow Diagram

```text
┌──────────────┐
│ Host App     │
│ (allocator)  │
└──────┬───────┘
       │ Provides buffer slice
       ↓
┌──────────────────────────────────────────┐
│ Parser::parse_segment(buf, &mut handler) │
└──────┬───────────────────────────────────┘
       │
       ├─→ [State::Initial] ──→ parse_isa_segment()
       │                         │
       │                         ├─→ Extract delimiters
       │                         ├─→ Parse fixed-width fields
       │                         └─→ Transition to State::Processing
       │
       └─→ [State::Processing] ─→ parse_regular_segment()
                                   │
                                   ├─→ Find segment terminator
                                   ├─→ Extract segment ID
                                   ├─→ Split elements
                                   └─→ Build Segment<'buf>
                                        │
                                        ↓
                              ┌─────────────────────┐
                              │ handler.handle(seg) │
                              └─────────────────────┘
                                        │
                                        ↓
                              ┌─────────────────────┐
                              │ Return Ok(bytes)    │
                              └─────────────────────┘
```

## Key Design Decisions

### 1. Why Single Segment Struct Instead of Enums?

**Considered Options:**

```rust
// Option A: Enum with dedicated structs (more type-safe)
enum Segment<'a> {
    ISA(IsaSegment<'a>),
    GS(GsSegment<'a>),
    ST(StSegment<'a>),
    // ... 100+ segment types
}

// Option B: Single struct (chosen)
struct Segment<'a> {
    id: &'a [u8],
    elements: [Option<Element<'a>>; MAX_ELEMENTS],
    // ...
}
```

> **Chosen: Option B**

**Rationale:**

- **Flexibility**: X12 has 100+ segment types, dedicated structs would be unwieldy
- **Performance**: Fixed-size struct on stack, no heap allocations
- **Streaming**: Handler can process any segment type uniformly
- **Extensibility**: Easy to add new segment types without code changes
- **Trade-off**: Less compile-time type safety, but runtime validation ensures correctness

### 2. Error Handling: Incomplete vs. Error

**Design:**

```rust
enum Error {
    Incomplete,      // Need more data (expected, not fatal)
    InvalidSegment,  // Parsing failed (error)
    // ...
}
```

**Rationale:**

- `Incomplete` is a normal condition in streaming (like httparse)
- Host can resize/refill buffer and retry
- Clear distinction between "need more data" and "malformed data"

### 3. ISA Special Handling

**Why separate `parse_isa_segment()`?**

- ISA uses **fixed-width fields** (not delimiter-separated)
- Defines delimiters for rest of document
- Must be parsed first to initialize parser state
- Cannot use regular parsing logic

### 4. Maximum Element Count

**Constant:**

```rust
pub const MAX_ELEMENTS: usize = 512;
```

**Rationale:**

- X12 standard limit is typically < 100 elements per segment
- 512 provides safety margin
- Fixed-size array avoids allocations (critical for no_std)
- Trade-off: ~4KB stack space per Segment, acceptable for most systems

### 5. Delimiter Configuration

**Storage:**

```rust
pub struct Delimiters {
    element: u8,      // *
    subelement: u8,   // :
    segment: u8,      // ~
    repetition: u8,   // ^
}
```

**Rationale:**

- Extracted from ISA segment (positions 3, 104, 105)
- Configurable per document (X12 allows customization)
- Stored in parser state for efficient access
- Passed to Segment for sub-element splitting

## SNIP Validation Levels

The `SegmentHandler` trait enables implementation of all 7 SNIP validation levels:

| Level | Description | Implementation |
|-------|-------------|----------------|
| 1 | Syntax | Parser enforces syntax automatically |
| 2 | Business Scenario | Handler validates segment sequences |
| 3 | Implementation | Handler checks implementation-specific rules |
| 4 | External Code Sets | Handler validates against external codes |
| 5 | Data Value | Handler checks valid data ranges |
| 6 | Situational Data | Handler enforces conditional requirements |
| 7 | Inter-segment | Handler tracks relationships between segments |

> **Example: Level 7 Validation in Handler**

```rust
impl SegmentHandler for MyHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        match segment.id_str()? {
            "HL" => {
                // Track hierarchical structure
                // Validate parent-child relationships
            }
            "SE" => {
                // Verify segment count matches SE01
            }
            // ... other validations
        }
        Ok(())
    }
}
```

## Performance Characteristics

- **Time Complexity**: O(n) where n = segment length
- **Space Complexity**: O(1) - fixed-size structures
- **Allocations**: Zero (in no_std mode)
- **Memory Usage**:
  - Parser: ~16 bytes
  - Segment: ~4KB (mostly element array)
  - No heap usage

## Usage Pattern

### Typical Flow

```rust
// 1. Host allocates buffer
let mut buffer = [0u8; 8192];

// 2. Load data into buffer
let bytes_read = file.read(&mut buffer)?;
let mut data = &buffer[..bytes_read];

// 3. Create parser and handler
let mut parser = Parser::new();
let mut handler = MyHandler::new();

// 4. Parse loop
loop {
    match parser.parse_segment(data, &mut handler) {
        Ok(consumed) => {
            data = &data[consumed..];
            if data.is_empty() {
                // Need more data
                break;
            }
        }
        Err(Error::Incomplete) => {
            // Move unparsed data to buffer start
            // Load more data
            // Retry
        }
        Err(e) => {
            // Handle error
            return Err(e);
        }
    }
}
```

### Buffer Management Strategies

**Strategy 1: Ring Buffer** (recommended for continuous streaming)

```rust
let mut ring = RingBuffer::new(8192);
loop {
    let slice = ring.writable_slice();
    let n = source.read(slice)?;
    ring.advance_write(n);
    
    while let Some(view) = ring.readable_slice() {
        match parser.parse_segment(view, &mut handler) {
            Ok(consumed) => ring.advance_read(consumed),
            Err(Error::Incomplete) => break,
            Err(e) => return Err(e),
        }
    }
}
```

**Strategy 2: Sliding Window** (simpler, periodic compaction)

```rust
let mut buffer = vec![0u8; 8192];
let mut start = 0;
let mut end = 0;

loop {
    // Compact if needed
    if start > buffer.len() / 2 {
        buffer.copy_within(start..end, 0);
        end -= start;
        start = 0;
    }
    
    // Read more data
    let n = source.read(&mut buffer[end..])?;
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

## Extension Points

### Custom Segment Types

While the parser uses a generic `Segment` struct, handlers can implement type-specific logic:

```rust
impl SegmentHandler for TypedHandler {
    fn handle(&mut self, segment: &Segment) -> Result<()> {
        match segment.id_str()? {
            "NM1" => self.handle_nm1(segment),
            "CLM" => self.handle_clm(segment),
            // ...
        }
    }
}

impl TypedHandler {
    fn handle_nm1(&mut self, seg: &Segment) -> Result<()> {
        let entity_id = seg.required_element(0)?;
        let entity_type = seg.required_element(1)?;
        // Type-specific validation and processing
        Ok(())
    }
}
```

### Custom Validations

Handlers maintain state for complex validations:

```rust
struct ValidationHandler {
    control_numbers: HashMap<u32, SegmentType>,
    hierarchical_stack: Vec<HierarchicalLevel>,
    // ...
}
```

## Testing Strategy

1. **Unit Tests**: Individual functions (element parsing, delimiters)
2. **Integration Tests**: Full segment parsing with sample data
3. **Property Tests**: Fuzz testing with random inputs
4. **Compliance Tests**: Official X12 test files
5. **Performance Tests**: Large file parsing benchmarks

## Future Enhancements

1. **Optional `std` Features**:
   - String allocations for convenience
   - HashMap for segment lookup
   - Error context with backtrace

2. **Validation Library**:
   - Pre-built handlers for common validations
   - Configurable validation levels

3. **Code Generation**:
   - Generate typed segment structs from X12 schema
   - Type-safe element access

4. **Performance Optimizations**:
   - SIMD for delimiter searching
   - Parallel segment parsing (for batch files)

## Security Considerations

1. **Buffer Overflow Protection**: All indexing is bounds-checked
2. **Integer Overflow**: Parser uses checked arithmetic
3. **Resource Limits**: MAX_ELEMENTS prevents memory exhaustion
4. **UTF-8 Validation**: Safe conversion with error handling
5. **No Unsafe Code**: Pure safe Rust (except potentially in optimizations)

## Comparison with Alternatives

| Feature | This Parser | Traditional Parser | SAX-style Parser |
|---------|-------------|-------------------|------------------|
| Memory | O(1) | O(n) | O(1) |
| Copies | 0 | Multiple | 0 |
| Streaming | Yes | No | Yes |
| Type Safety | Runtime | Compile-time | Runtime |
| no_std | Yes | Usually No | Varies |
| Complexity | Low | Medium | Medium |

## Conclusion

This design prioritizes:

1. **Efficiency**: Zero-copy, minimal allocations
2. **Flexibility**: Works in any environment (embedded, server, WASM)
3. **Correctness**: Standard-compliant parsing
4. **Usability**: Simple API inspired by httparse

The trade-off of runtime type checking vs. compile-time safety is acceptable given the performance benefits and streaming requirements.
