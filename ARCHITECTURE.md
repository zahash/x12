# X12 Parser Architecture

Comprehensive architectural overview and design decisions.

## 🏛️ System Architecture

```text
┌─────────────────────────────────────────────────┐
│                  Host Application               │
│  ┌───────────────────────────────────────────┐  │
│  │  Chunked File Reader                      │  │
│  │  - Dynamic buffer sizing                  │  │
│  │  - Compaction strategy                    │  │
│  └──────────────┬────────────────────────────┘  │
│                 │                                │
│                 ▼                                │
│  ┌───────────────────────────────────────────┐  │
│  │  Validation Suite                         │  │
│  │  - Composable validators                  │  │
│  │  - Error accumulation                     │  │
│  │  - Implements SegmentHandler              │  │
│  └──────────────┬────────────────────────────┘  │
└─────────────────┼────────────────────────────────┘
                  │
                  │ Segment<'a>
                  │
┌─────────────────┼────────────────────────────────┐
│                 ▼                                │
│  ┌───────────────────────────────────────────┐  │
│  │  Parser (no_std)                          │  │
│  │  - State machine                          │  │
│  │  - Zero-copy parsing                      │  │
│  │  - Returns Incomplete/Error               │  │
│  └───────────────────────────────────────────┘  │
│                                                  │
│                segment crate                     │
└──────────────────────────────────────────────────┘
```

## 🎯 Design Goals

### 1. **No Allocations in Core Parser**

The `segment` crate parser never allocates:

- All references into original buffer
- Lifetimes enforce safety
- `no_std` compatible

### 2. **Separation of Concerns**

Three distinct layers:

1. **Parser** - Syntax and structure (no_std)
2. **Validation** - Semantics and rules (no_std with alloc)
3. **Host** - File I/O and orchestration (std)

### 3. **Error Accumulation**

Validators collect **all** errors, not just the first:

- Better user experience
- Complete validation report
- Handlers don't return errors for validation

### 4. **Composability**

Independent validators that can be mixed:

```rust
let mut suite = ValidationSuite::new();
suite.add(Box::new(Snip1Validator::new()));
suite.add(Box::new(Snip7Validator::new()));
suite.add(Box::new(CustomValidator::new()));
```

## 🔄 Data Flow

### Parsing Flow

```text
File
  │
  ▼
[Read Chunk] ──────────► Buffer (8KB → 16KB → ...)
  │                           │
  │                           ▼
  │                      [Parse Segment]
  │                           │
  │                           ├─► Ok(consumed) ─► Advance buffer
  │                           │
  │                           ├─► Err(Incomplete) ─► Double buffer
  │                           │                      or read more
  │                           │
  │                           └─► Err(Invalid) ─► Report error
  │
  └─► [No more data] ─► Finish
```

### Buffer Management

```text
Initial State:
┌────────────┬───────────────────────────────────┐
│    8KB     │           Empty                   │
└────────────┴───────────────────────────────────┘
 buffer_start                              buffer_end
      │                                         │
      └─────────────────────────────────────────┘
                    unparsed data


After Parsing Some Segments:
┌──────┬─────┬──────────────────────────────────┐
│Parsed│Unprs│          Empty                   │
└──────┴─────┴──────────────────────────────────┘
       ▲     ▲                                  ▲
       │     │                                  │
   buffer   buffer                          buffer
   _start   _start                            _end
  (before)  (after)


Compaction (when >50% consumed):
┌─────┬──────────────────────────────────────────┐
│Unprs│             Empty                        │
└─────┴──────────────────────────────────────────┘
 ▲    ▲
 │    └─ buffer_end
 └─ buffer_start


Doubling (when Incomplete and buffer full):
┌─────┬────────────────────────────────────────────────────┐
│Unprs│                   Empty (16KB)                     │
└─────┴────────────────────────────────────────────────────┘
```

## 🧩 Component Details

### Parser State Machine

```rust
enum State {
    Initial,        // Start state, expects ISA
    Processing,     // After ISA, processing segments
}
```

**Transitions:**

```text
Initial ──[ISA]──► Processing ──[segment]──► Processing
                      │
                      └──[IEA]──► Initial
```

**ISA Handling:**

- Special case: Fixed-width format (106 chars)
- Extracts delimiters from ISA segment
- Used for all subsequent segments

### Segment Handler Trait

```rust
pub trait SegmentHandler {
    type Error: From<ParserError>;
    
    fn handle(&mut self, segment: &Segment) -> Result<(), Self::Error>;
}
```

**Design Rationale:**

1. **Associated Error Type**
   - Handlers define their own error types
   - Must implement `From<ParserError>` for conversion
   - Allows rich error context without coupling

2. **Why Not Generic Enum?**

   ```rust
   // ❌ This doesn't work:
   enum ParserError<H> {
       Incomplete,
       Handler(H),
   }
   ```

   Problems:
   - Can't make trait object `dyn SegmentHandler`
   - Generic parameter creates variance issues
   - Couples parser to handler implementation

3. **Result Return**
   - `Ok(())` - Continue parsing
   - `Err(e)` - Stop parsing (catastrophic only)
   - Validation errors accumulated internally

### Validator Architecture

```rust
pub trait Validator {
    fn validate(&mut self, segment: &Segment);
    fn errors(&self) -> &[ValidationError];
    fn clear(&mut self);
    fn name(&self) -> &str;
}
```

**Key Design Points:**

1. **No Result Return**
   - `validate()` never fails
   - Errors accumulated in `errors` field
   - Retrieved after parsing completes

2. **Common Error Type**

   ```rust
   pub struct ValidationError {
       pub severity: Severity,
       pub kind: ErrorKind,
       pub segment_id: [u8; 3],
       pub element: Option<usize>,
       pub message: String,
       pub segment_position: Option<usize>,
   }
   ```

   Benefits:
   - Uniform error reporting
   - Easy to sort/filter/group
   - Rich context for debugging

3. **Dynamic Dispatch**

   ```rust
   validators: Vec<Box<dyn Validator>>
   ```

   - Allows mixing different validator types
   - Runtime composition
   - Small overhead acceptable for validation

### ValidationSuite

Implements both `Validator` and `SegmentHandler`:

```rust
impl SegmentHandler for ValidationSuite {
    type Error = ParserError;
    
    fn handle(&mut self, segment: &Segment) -> Result<(), Self::Error> {
        for validator in &mut self.validators {
            validator.validate(segment);
        }
        Ok(())  // Never returns validation errors
    }
}
```

**Design Philosophy:**

- Aggregates multiple validators
- Implements `SegmentHandler` for parser integration
- Only returns `Err` for parser errors, never validation
- Call `finish()` to get all accumulated errors

## 🚀 Performance Optimizations

### 1. Zero-Copy Parsing

All data structures reference original buffer:

```rust
pub struct Segment<'a> {
    buffer: &'a [u8],      // Original buffer
    id: &'a [u8],          // Slice into buffer
    elements_start: usize,  // Offset, not copy
    // ...
}
```

### 2. Lifetime Tracking

Compiler enforces buffer lifetime:

```rust
fn parse_segment<'a, H>(
    buffer: &'a [u8],
    handler: &mut H
) -> Result<usize, H::Error>
where H: SegmentHandler
```

Segment<'a> can't outlive buffer.

### 3. Minimal Allocation

**Parser**: Zero allocations
**Validator**: Only for error accumulation
**Host**: Only for buffer management

### 4. Buffer Strategy

- **Initial size**: 8KB (fits 99% of segments)
- **Doubling**: Up to 16MB maximum
- **Compaction**: When >50% consumed
- **Early stop**: At 16MB if still incomplete

### 5. SIMD Opportunities

Current implementation is byte-by-byte, but:

- Delimiter search could use SIMD
- ISA fixed-width parsing could be vectorized
- Future optimization opportunity

## 🔒 Safety Guarantees

### 1. No Unsafe Code

Entire codebase is safe Rust:

- All bounds checks preserved
- Lifetime tracking enforced
- No unsafe blocks needed

### 2. Lifetime Safety

```rust
pub struct Segment<'a> {
    buffer: &'a [u8],  // Tied to buffer lifetime
}

impl<'a> Segment<'a> {
    pub fn element(&self, index: usize) -> Option<Element<'a>> {
        // Element<'a> also tied to buffer lifetime
    }
}
```

Can't use segment after buffer modified.

### 3. Bounds Checking

All array accesses checked:

```rust
if index < self.element_count {
    // Safe access
}
```

### 4. UTF-8 Validation

Uses `core::str::from_utf8` which validates:

```rust
pub fn id_str(&self) -> Option<&str> {
    core::str::from_utf8(self.id).ok()
}
```

## 🧪 Testing Strategy

### Unit Tests

- Individual function testing
- Edge cases (empty, malformed, etc.)
- Delimiter handling
- ISA special cases

### Integration Tests

- Full segment parsing
- Control number validation
- Error accumulation
- Incomplete handling

### Property Tests (Future)

- Fuzzing with arbitrary input
- Round-trip parsing
- Invariant checking

## 📈 Future Enhancements

### 1. Additional Validators

Implement remaining SNIP levels:

- Level 2: Business scenario
- Level 3: Implementation conventions
- Level 4: External code sets
- Level 5: Data value validation
- Level 6: Situational data elements

### 2. Performance Optimizations

- SIMD for delimiter search
- Parallel validation (rayon)
- Memory-mapped file I/O
- Custom allocator for validators

### 3. Additional Features

- Segment building (not just parsing)
- X12 generation
- EDI to JSON conversion
- Schema validation

### 4. Ergonomics

- Builder patterns for validators
- Macro for handler implementation
- More examples and documentation

## 🎓 Learning Resources

**For understanding the design:**

1. Read `segment/src/lib.rs` - Core parser
2. Read `validation/src/lib.rs` - Validator architecture
3. Read `host/src/main.rs` - File handling
4. Study lifetimes in `Segment<'a>` and `Element<'a>`

**For using the library:**

1. Start with examples in `segment/examples/`
2. Review integration tests
3. Try parsing real X12 files
4. Implement custom validators

## 🤔 Design Questions & Answers

### Q: Why not return validation errors from `handle()`?

**A:** To collect ALL errors, not just the first one. Validation errors are accumulated internally and retrieved after parsing. Only catastrophic errors (parse failures) return `Err`.

### Q: Why `From<ParserError>` bound on Error type?

**A:** Allows automatic conversion of parser errors to handler errors using `.into()`. Handlers can define rich error types while still accepting parser errors.

### Q: Why separate `segment` and `validation` crates?

**A:** Parser can be used without validation (e.g., just extract data). Validation requires allocations for error collection, but parser is pure `no_std`.

### Q: Why dynamic dispatch for validators?

**A:** Allows runtime composition of validators. User can mix built-in and custom validators without generic complexity.

### Q: Why not use async I/O?

**A:** Synchronous I/O is simpler and sufficient. File reading is not the bottleneck (parsing is). Async would add complexity without benefit.

### Q: Could this be made parallel?

**A:** Yes, but carefully:

- Parser must be sequential (stateful)
- Validation could be parallelized per transaction set
- File reading could use multiple buffers
- Complexity vs benefit trade-off

## 📊 Comparison with Alternatives

| Feature | This Parser | nom-based | serde-based | Line-by-line |
|---------|-------------|-----------|-------------|--------------|
| no_std | ✅ Yes | ✅ Yes | ❌ No | ✅ Yes |
| Zero-copy | ✅ Yes | ⚠️ Partial | ❌ No | ⚠️ Partial |
| Streaming | ✅ Yes | ⚠️ Limited | ❌ No | ✅ Yes |
| Error accumulation | ✅ Yes | ❌ No | ❌ No | ⚠️ Manual |
| Validator composition | ✅ Yes | ⚠️ Manual | ❌ No | ⚠️ Manual |
| Incomplete handling | ✅ Explicit | ⚠️ Via nom::Err | ❌ N/A | ⚠️ Manual |
| Type safety | ✅ Strong | ✅ Strong | ✅ Strong | ⚠️ Weak |

This parser optimizes for:

- Large file handling
- Complete error reporting
- Composable validation
- Explicit incomplete handling
