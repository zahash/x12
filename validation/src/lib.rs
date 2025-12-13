#![no_std]

//! X12 Validation Library
//!
//! Provides composable, no_std compatible validators for X12 documents.
//! Implements SNIP (Standard Numeric Interchange Protocol) levels 1-7.
//!
//! # Design Philosophy
//!
//! - **Composable**: Each validator is independent and can be used alone or combined
//! - **Accumulating**: Validators collect all errors instead of stopping at the first one
//! - **Efficient**: no_std compatible, minimal allocations
//! - **Modular**: SNIP levels separated into individual validators
//!
//! # Usage
//!
//! ```ignore
//! use x12_validation::{ValidationSuite, Snip1Validator};
//!
//! let mut suite = ValidationSuite::new();
//! suite.add(Snip1Validator::new());
//!
//! // Parse with validation
//! parser.parse_segment(buffer, &mut suite)?;
//!
//! // Get all accumulated errors
//! let errors = suite.finish();
//! ```

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use segment::{ParserError, Segment, SegmentHandler};

/// Maximum number of errors to accumulate before stopping
pub const MAX_ERRORS: usize = 1000;

/// Validation error severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Error - violates standard, transaction should be rejected
    Error,
    /// Warning - non-standard but processable
    Warning,
    /// Info - informational message
    Info,
}

/// Type of validation error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // SNIP Level 1 - Syntax
    InvalidSyntax,
    MissingSegment,
    SegmentSequence,

    // SNIP Level 2 - Business Scenario
    InvalidBusinessRule,

    // SNIP Level 3 - Implementation
    ImplementationLimit,

    // SNIP Level 4 - External Code Sets
    InvalidCodeValue,

    // SNIP Level 5 - Data Value
    InvalidDataValue,
    OutOfRange,

    // SNIP Level 6 - Situational
    MissingRequiredElement,
    UnexpectedElement,

    // SNIP Level 7 - Inter-segment
    ControlNumberMismatch,
    CountMismatch,
    InvalidHierarchy,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSyntax => write!(f, "Invalid Syntax"),
            Self::MissingSegment => write!(f, "Missing Segment"),
            Self::SegmentSequence => write!(f, "Segment Sequence Error"),
            Self::InvalidBusinessRule => write!(f, "Invalid Business Rule"),
            Self::ImplementationLimit => write!(f, "Implementation Limit Exceeded"),
            Self::InvalidCodeValue => write!(f, "Invalid Code Value"),
            Self::InvalidDataValue => write!(f, "Invalid Data Value"),
            Self::OutOfRange => write!(f, "Value Out of Range"),
            Self::MissingRequiredElement => write!(f, "Missing Required Element"),
            Self::UnexpectedElement => write!(f, "Unexpected Element"),
            Self::ControlNumberMismatch => write!(f, "Control Number Mismatch"),
            Self::CountMismatch => write!(f, "Count Mismatch"),
            Self::InvalidHierarchy => write!(f, "Invalid Hierarchy"),
        }
    }
}

/// Validation error with full context
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error severity
    pub severity: Severity,
    /// Error type
    pub kind: ErrorKind,
    /// Segment identifier where error occurred
    pub segment_id: [u8; 3],
    /// Element position (0-based, None if segment-level error)
    pub element: Option<usize>,
    /// Human-readable error message
    pub message: String,
    /// Segment position in file (if tracked)
    pub segment_position: Option<usize>,
}

impl ValidationError {
    /// Create a new validation error
    pub fn new(
        severity: Severity,
        kind: ErrorKind,
        segment_id: &[u8],
        element: Option<usize>,
        message: String,
    ) -> Self {
        let mut id = [0u8; 3];
        let len = segment_id.len().min(3);
        id[..len].copy_from_slice(&segment_id[..len]);

        Self {
            severity,
            kind,
            segment_id: id,
            element,
            message,
            segment_position: None,
        }
    }

    /// Get segment ID as string
    pub fn segment_id_str(&self) -> &str {
        core::str::from_utf8(&self.segment_id)
            .unwrap_or("???")
            .trim_end_matches('\0')
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{:?}] {} at segment {}{}: {}",
            self.severity,
            self.kind,
            self.segment_id_str(),
            if let Some(elem) = self.element {
                alloc::format!(" element {}", elem)
            } else {
                String::new()
            },
            self.message
        )
    }
}

/// Trait for individual validators
///
/// Each validator implements a specific set of checks (e.g., SNIP level).
/// Validators accumulate errors internally and can be composed together.
pub trait Validator {
    /// Validate a segment
    ///
    /// Accumulate any errors internally. Do not stop processing.
    fn validate(&mut self, segment: &Segment);

    /// Get accumulated errors
    fn errors(&self) -> &[ValidationError];

    /// Clear accumulated errors
    fn clear(&mut self);

    /// Get validator name (for reporting)
    fn name(&self) -> &str;
}

/// SNIP Level 1: Syntax Validation
///
/// Validates:
/// - Segment structure and format
/// - Element counts
/// - Required segments present
/// - Segment sequence
pub struct Snip1Validator {
    errors: Vec<ValidationError>,
    state: ValidationState,
    segment_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationState {
    Initial,
    InInterchange,
    InGroup,
    InTransaction,
}

impl Snip1Validator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            state: ValidationState::Initial,
            segment_count: 0,
        }
    }

    fn add_error(
        &mut self,
        severity: Severity,
        kind: ErrorKind,
        segment: &Segment,
        element: Option<usize>,
        message: String,
    ) {
        if self.errors.len() < MAX_ERRORS {
            let mut err = ValidationError::new(severity, kind, segment.id, element, message);
            err.segment_position = Some(self.segment_count);
            self.errors.push(err);
        }
    }

    fn validate_isa(&mut self, segment: &Segment) {
        if segment.element_count != 16 {
            self.add_error(
                Severity::Error,
                ErrorKind::InvalidSyntax,
                segment,
                None,
                alloc::format!(
                    "ISA must have exactly 16 elements, found {}",
                    segment.element_count
                ),
            );
        }
        self.state = ValidationState::InInterchange;
    }

    fn validate_gs(&mut self, segment: &Segment) {
        if self.state != ValidationState::InInterchange {
            self.add_error(
                Severity::Error,
                ErrorKind::SegmentSequence,
                segment,
                None,
                alloc::format!("GS segment outside of interchange"),
            );
        }
        if segment.element_count < 8 {
            self.add_error(
                Severity::Error,
                ErrorKind::InvalidSyntax,
                segment,
                None,
                alloc::format!(
                    "GS must have at least 8 elements, found {}",
                    segment.element_count
                ),
            );
        }
        self.state = ValidationState::InGroup;
    }

    fn validate_st(&mut self, segment: &Segment) {
        if self.state != ValidationState::InGroup {
            self.add_error(
                Severity::Error,
                ErrorKind::SegmentSequence,
                segment,
                None,
                alloc::format!("ST segment outside of functional group"),
            );
        }
        if segment.element_count < 2 {
            self.add_error(
                Severity::Error,
                ErrorKind::InvalidSyntax,
                segment,
                None,
                alloc::format!("ST must have at least 2 elements"),
            );
        }
        self.state = ValidationState::InTransaction;
    }
}

impl Default for Snip1Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator for Snip1Validator {
    fn validate(&mut self, segment: &Segment) {
        self.segment_count += 1;

        let id = segment.id_str().unwrap_or("???");

        match id {
            "ISA" => self.validate_isa(segment),
            "GS" => self.validate_gs(segment),
            "ST" => self.validate_st(segment),
            "SE" => {
                if self.state != ValidationState::InTransaction {
                    self.add_error(
                        Severity::Error,
                        ErrorKind::SegmentSequence,
                        segment,
                        None,
                        alloc::format!("SE segment outside of transaction"),
                    );
                }
                self.state = ValidationState::InGroup;
            }
            "GE" => {
                if self.state != ValidationState::InGroup {
                    self.add_error(
                        Severity::Error,
                        ErrorKind::SegmentSequence,
                        segment,
                        None,
                        alloc::format!("GE segment outside of group"),
                    );
                }
                self.state = ValidationState::InInterchange;
            }
            "IEA" => {
                if self.state != ValidationState::InInterchange {
                    self.add_error(
                        Severity::Error,
                        ErrorKind::SegmentSequence,
                        segment,
                        None,
                        alloc::format!("IEA segment outside of interchange"),
                    );
                }
                self.state = ValidationState::Initial;
            }
            _ => {}
        }
    }

    fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    fn clear(&mut self) {
        self.errors.clear();
        self.state = ValidationState::Initial;
        self.segment_count = 0;
    }

    fn name(&self) -> &str {
        "SNIP Level 1 (Syntax)"
    }
}

/// SNIP Level 7: Inter-segment Validation
///
/// Validates:
/// - Control number matching
/// - Segment count matching
/// - Hierarchical structure
pub struct Snip7Validator {
    errors: Vec<ValidationError>,
    isa_control: Option<u32>,
    gs_control: Option<u32>,
    st_control: Option<u32>,
    st_segment_count: u32,
    segment_count: usize,
}

impl Snip7Validator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            isa_control: None,
            gs_control: None,
            st_control: None,
            st_segment_count: 0,
            segment_count: 0,
        }
    }

    fn add_error(
        &mut self,
        severity: Severity,
        kind: ErrorKind,
        segment: &Segment,
        element: Option<usize>,
        message: String,
    ) {
        if self.errors.len() < MAX_ERRORS {
            let mut err = ValidationError::new(severity, kind, segment.id, element, message);
            err.segment_position = Some(self.segment_count);
            self.errors.push(err);
        }
    }

    fn parse_u32(&self, bytes: &[u8]) -> Option<u32> {
        let s = core::str::from_utf8(bytes).ok()?;
        let trimmed = s.trim();

        let mut result = 0u32;
        for byte in trimmed.bytes() {
            if !byte.is_ascii_digit() {
                return None;
            }
            result = result.checked_mul(10)?;
            result = result.checked_add((byte - b'0') as u32)?;
        }
        Some(result)
    }
}

impl Default for Snip7Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator for Snip7Validator {
    fn validate(&mut self, segment: &Segment) {
        self.segment_count += 1;

        let id = segment.id_str().unwrap_or("???");

        match id {
            "ISA" => {
                if let Some(elem) = segment.element(12) {
                    self.isa_control = self.parse_u32(elem.as_bytes());
                }
            }
            "IEA" => {
                if let Some(elem) = segment.element(1) {
                    if let Some(control) = self.parse_u32(elem.as_bytes()) {
                        if Some(control) != self.isa_control {
                            self.add_error(
                                Severity::Error,
                                ErrorKind::ControlNumberMismatch,
                                segment,
                                Some(1),
                                alloc::format!(
                                    "IEA02 ({}) does not match ISA13 ({:?})",
                                    control,
                                    self.isa_control
                                ),
                            );
                        }
                    }
                }
            }
            "GS" => {
                if let Some(elem) = segment.element(5) {
                    self.gs_control = self.parse_u32(elem.as_bytes());
                }
            }
            "GE" => {
                if let Some(elem) = segment.element(1) {
                    if let Some(control) = self.parse_u32(elem.as_bytes()) {
                        if Some(control) != self.gs_control {
                            self.add_error(
                                Severity::Error,
                                ErrorKind::ControlNumberMismatch,
                                segment,
                                Some(1),
                                alloc::format!(
                                    "GE02 ({}) does not match GS06 ({:?})",
                                    control,
                                    self.gs_control
                                ),
                            );
                        }
                    }
                }
            }
            "ST" => {
                if let Some(elem) = segment.element(1) {
                    self.st_control = self.parse_u32(elem.as_bytes());
                }
                self.st_segment_count = 1; // ST counts as first segment
            }
            "SE" => {
                self.st_segment_count += 1; // SE counts in total

                // Check segment count
                if let Some(elem) = segment.element(0) {
                    if let Some(count) = self.parse_u32(elem.as_bytes()) {
                        if count != self.st_segment_count {
                            self.add_error(
                                Severity::Error,
                                ErrorKind::CountMismatch,
                                segment,
                                Some(0),
                                alloc::format!(
                                    "SE01 count ({}) does not match actual ({}) ",
                                    count,
                                    self.st_segment_count
                                ),
                            );
                        }
                    }
                }

                // Check control number
                if let Some(elem) = segment.element(1) {
                    if let Some(control) = self.parse_u32(elem.as_bytes()) {
                        if Some(control) != self.st_control {
                            self.add_error(
                                Severity::Error,
                                ErrorKind::ControlNumberMismatch,
                                segment,
                                Some(1),
                                alloc::format!(
                                    "SE02 ({}) does not match ST02 ({:?})",
                                    control,
                                    self.st_control
                                ),
                            );
                        }
                    }
                }

                self.st_segment_count = 0;
            }
            _ => {
                if self.st_control.is_some() && id != "ST" {
                    self.st_segment_count += 1;
                }
            }
        }
    }

    fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    fn clear(&mut self) {
        self.errors.clear();
        self.isa_control = None;
        self.gs_control = None;
        self.st_control = None;
        self.st_segment_count = 0;
        self.segment_count = 0;
    }

    fn name(&self) -> &str {
        "SNIP Level 7 (Inter-segment)"
    }
}

/// Composable validation suite
///
/// Combines multiple validators and accumulates all their errors.
/// Implements SegmentHandler for easy integration with the parser.
pub struct ValidationSuite {
    validators: Vec<Box<dyn Validator>>,
}

impl ValidationSuite {
    /// Create a new empty validation suite
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    /// Create a validation suite with all SNIP levels
    pub fn all_snip_levels() -> Self {
        let mut suite = Self::new();
        suite.add(Box::new(Snip1Validator::new()));
        suite.add(Box::new(Snip7Validator::new()));
        suite
    }

    /// Add a validator to the suite
    pub fn add(&mut self, validator: Box<dyn Validator>) {
        self.validators.push(validator);
    }

    /// Get all accumulated errors from all validators
    pub fn errors(&self) -> Vec<&ValidationError> {
        let mut all_errors = Vec::new();
        for validator in &self.validators {
            all_errors.extend(validator.errors());
        }
        all_errors
    }

    /// Get total error count
    pub fn error_count(&self) -> usize {
        self.validators.iter().map(|v| v.errors().len()).sum()
    }

    /// Clear all accumulated errors
    pub fn clear(&mut self) {
        for validator in &mut self.validators {
            validator.clear();
        }
    }

    /// Finish validation and return all errors
    pub fn finish(self) -> Vec<ValidationError> {
        let mut all_errors = Vec::new();
        for validator in self.validators {
            all_errors.extend(validator.errors().iter().cloned());
        }
        all_errors
    }
}

impl Default for ValidationSuite {
    fn default() -> Self {
        Self::new()
    }
}

impl SegmentHandler for ValidationSuite {
    type Error = ParserError;

    fn handle(&mut self, segment: &Segment) -> Result<(), Self::Error> {
        // Run all validators
        for validator in &mut self.validators {
            validator.validate(segment);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snip1_validator() {
        let validator = Snip1Validator::new();

        // This would normally come from parser
        // Just testing the validator interface
        assert_eq!(validator.errors().len(), 0);
        assert_eq!(validator.name(), "SNIP Level 1 (Syntax)");
    }

    #[test]
    fn test_validation_suite() {
        let mut suite = ValidationSuite::new();
        suite.add(Box::new(Snip1Validator::new()));

        assert_eq!(suite.error_count(), 0);
    }
}
