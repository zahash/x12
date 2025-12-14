use std::fmt::Display;

use parser::{Delimiters, Halt, Segment, SegmentHandler};

/// Collects parsed segments for reconstruction and validation
pub struct SegmentCollector {
    segments: Vec<SegmentData>,
}

#[derive(Debug, Clone)]
pub struct SegmentData {
    pub id: Vec<u8>,
    pub elements: Vec<Vec<u8>>,
    pub delimiters: Delimiters,
}

impl SegmentCollector {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Reconstruct the X12 document from collected segments
    pub fn reconstruct(&self) -> String {
        self.segments.iter().fold(String::new(), |mut acc, seg| {
            acc.push_str(&seg.to_string());
            acc
        })
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    pub fn get_segment(&self, idx: usize) -> Option<&SegmentData> {
        self.segments.get(idx)
    }
}

impl SegmentHandler for SegmentCollector {
    fn handle(&mut self, segment: &Segment) -> Result<(), Halt> {
        let id = segment.id.to_vec();
        let elements: Vec<Vec<u8>> = segment.elements().map(|e| e.as_bytes().to_vec()).collect();

        self.segments.push(SegmentData {
            id,
            elements,
            delimiters: segment.delimiters,
        });

        Ok(())
    }
}

impl Display for SegmentData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", str::from_utf8(&self.id).unwrap())?;
        for element in &self.elements {
            write!(
                f,
                "{}{}",
                self.delimiters.element as char,
                str::from_utf8(element).unwrap()
            )?;
        }
        write!(f, "{}", self.delimiters.segment as char)
    }
}
