/// Encoding discriminator for a formula's concrete reference binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceKind {
    /// Single-cell reference.
    Cell,
    /// Reference spanning ordered sheet endpoints.
    SheetSpan,
    /// Whole-column reference.
    Column,
    /// Whole-row reference.
    Row,
}
