pub mod fixed;
pub mod hierarchical;
pub mod propositional;
pub mod semantic;
pub mod structure;
pub use fixed::FixedSizeChunker;
pub use hierarchical::HierarchicalChunker;
pub use propositional::PropositionalChunker;
pub use semantic::SemanticChunker;
pub use structure::StructureAwareChunker;

/// Narrows the byte range `[start, end)` of `text` to its trimmed span.
/// Returns `None` when the range holds only whitespace.
pub(crate) fn trimmed_span(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let slice = &text[start..end];
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lead = slice.len() - slice.trim_start().len();
    Some((start + lead, start + lead + trimmed.len()))
}
