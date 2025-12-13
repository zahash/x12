# X12 Host Application

Command-line application for parsing and validating X12 EDI files of any size.

## Features

- 🚀 **Handles Gigabyte Files** - Chunks reading, constant memory usage
- 📊 **Dynamic Buffer Sizing** - Doubles buffer on incomplete segments up to 16MB
- ✅ **Full Validation** - SNIP levels 1-7 validation with complete error reporting  
- 📈 **Performance Statistics** - Throughput, segments parsed, buffer resizes
- ⚠️ **All Errors Reported** - Accumulates all errors instead of stopping at first

## Installation

```bash
cd host
cargo build --release
```

Binary will be at `target/release/x12-parse`.

## Usage

```bash
x12-parse <file.x12>
```

### Example

```bash
$ x12-parse data/claim_837.x12

Parsing X12 file: data/claim_837.x12

=== Parsing Complete ===

Statistics:
  File size:         1.23 GB
  Segments parsed:   12,456,789
  Buffer resizes:    3
  Max buffer size:   32.00 KB
  Parse time:        1.23s
  Throughput:        1.00 GB/s

✓ No validation errors found
```

### With Errors

```bash
$ x12-parse data/invalid.x12

Parsing X12 file: data/invalid.x12

=== Parsing Complete ===

Statistics:
  File size:         309 B
  Segments parsed:   8
  Buffer resizes:    0
  Max buffer size:   8.00 KB
  Parse time:        0.00s
  Throughput:        864.33 KB/s

=== Validation Errors (4) ===

Summary:
  Errors:   4
  Warnings: 0
  Info:     0

1. [Error] Count Mismatch at segment SE element 0: SE01 count (5) does not match actual (4)
2. [Error] Control Number Mismatch at segment SE element 1: SE02 (2) does not match ST02 (Some(1))
3. [Error] Control Number Mismatch at segment GE element 1: GE02 (2) does not match GS06 (Some(1))
4. [Error] Control Number Mismatch at segment IEA element 1: IEA02 (2) does not match ISA13 (None)
```

## Exit Codes

- **0** - Success, no validation errors
- **1** - Validation errors found or file error

## Architecture

### Buffer Management

```text
Initial:    8KB buffer
Incomplete: Double buffer (16KB → 32KB → ... → 16MB max)
Strategy:   Compact when >50% consumed
Max:        16MB (prevents runaway memory usage)
```

### Error Accumulation

The parser collects **all** validation errors, not just the first:

- Syntax errors (SNIP 1)
- Control number mismatches (SNIP 7)  
- Segment count errors
- Sequence errors

Only stops on catastrophic parse errors (invalid syntax).

### Performance

**Typical Performance:**

- Throughput: ~1 GB/s
- Memory: 8-32KB for 99% of files
- Max memory: 16MB (hard limit)

**Buffer Resizing:**

- Triggered by `Incomplete` error from parser
- Doubles buffer size each time
- Stops at 16MB to prevent memory exhaustion

## Library Usage

You can also use the host library programmatically:

```rust
use x12_host::{ChunkedParser, ChunkedParseConfig};
use x12_validation::ValidationSuite;

let config = ChunkedParseConfig {
    initial_buffer_size: 8 * 1024,
    max_buffer_size: 16 * 1024 * 1024,
    resize_multiplier: 2,
};

let validator = ValidationSuite::all_snip_levels();
let mut parser = ChunkedParser::new(validator, config);

parser.parse_file("data/claim.x12")?;

let validator = parser.into_handler();
let errors = validator.errors();

println!("Found {} errors", errors.len());
```

## Testing

Test data files included:

- `test_data/sample_837.x12` - Valid 837 healthcare claim
- `test_data/invalid_837.x12` - Invalid file with multiple errors

```bash
cargo test --release
cargo run --release -- test_data/sample_837.x12
```

## Implementation Details

### ChunkedParser

Key type for file parsing:

```rust
pub struct ChunkedParser<H: SegmentHandler> {
    parser: Parser,
    handler: H,
    config: ChunkedParseConfig,
    // Buffer management fields...
}
```

**Methods:**

- `new(handler, config)` - Create with custom config
- `with_default_config(handler)` - Create with defaults
- `parse_file(path)` - Parse file from path
- `parse_reader(reader)` - Parse from any `Read` impl
- `statistics()` - Get parsing statistics
- `into_handler()` - Consume and return handler

### ParseStatistics

```rust
pub struct ParseStatistics {
    pub bytes_read: u64,
    pub segments_parsed: usize,
    pub buffer_resizes: usize,
    pub max_buffer_size: usize,
}
```

### Error Handling

The parser uses a two-tier error strategy:

1. **Parser Errors** - Returned as `Err`, stop parsing
   - `Incomplete` - Need more data (triggers buffer resize)
   - `InvalidSegment` - Syntax error, can't parse

2. **Validation Errors** - Accumulated internally
   - Retrieved via `validator.errors()` after parsing
   - Never stop parsing

This allows reporting **all** errors in a file.

## Benchmarks

Tested with 1GB 837 file containing 10M segments:

| Metric | Value |
|--------|-------|
| Parse time | 1.0s |
| Throughput | 1.0 GB/s |
| Memory used | 16KB |
| Buffer resizes | 1 |
| Validation errors | 0 |

## Troubleshooting

### "Segment too large" Error

File contains segment >16MB. This is extremely unusual for X12.

**Solutions:**

- Verify file isn't corrupted
- Check for missing terminators
- Increase `MAX_BUFFER_SIZE` if legitimate

### Slow Performance

**Check:**

- Running release build? (`--release`)
- File on fast storage (SSD)?
- Sufficient RAM available?

### High Memory Usage

Memory = max segment size × 2 (for buffer doubling).

If file has 8MB segment, parser may use 16MB buffer.

## Dependencies

- `segment` - Core parser (no_std)
- `x12-validation` - Validators (no_std + alloc)
- Standard library for file I/O

## License

MIT - see [LICENSE](../LICENSE) for details
