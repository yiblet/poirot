use crate::lookup::{BudgetRequest, Failure, Reference, Scalar, Value};
/// Zero-based caller sheet, row, and column for reference resolution and scalar selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// Current sheet used for unqualified references.
    pub sheet: u32,
    /// Caller row used by implicit intersection and relative references.
    pub row: u32,
    /// Caller column used by implicit intersection and relative references.
    pub col: u32,
}
/// Parsed INDIRECT-style reference text awaiting workbook resolution.
/// `a1` selects A1 versus R1C1 interpretation; names/endpoints are not resolved here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicReference {
    /// Optional workbook qualifier decoded from the reference spelling.
    pub workbook: Option<String>,
    /// Optional sheet qualifier decoded from the reference spelling.
    pub sheet: Option<String>,
    /// First parsed reference endpoint.
    pub first: String,
    /// Optional opposite endpoint of a rectangular reference.
    pub last: Option<String>,
    /// Whether endpoints use A1 rather than R1C1 notation.
    pub a1: bool,
}
/// The implementation must record resolved reference dependencies and retain workbook identity.
/// Missing workbook/metadata must return an error, never a fabricated index or value.
pub trait Workbook {
    /// Required preflight boundary. Operational failures must never be converted to cell errors.
    fn preflight(&mut self, request: BudgetRequest) -> Result<(), Failure>;
    /// Resolve one deferred operand only when requested; spreadsheet errors are returned as scalar data.
    fn argument(&mut self, index: usize, context: Context) -> Result<Value, Failure>;

    /// Shared POI numeric text coercion, including its date/time fallback.
    fn coerce_numeric_text(&mut self, text: &str) -> Result<f64, Failure>;
    /// Excel/POI NumberToTextConverter rendering; never a locale formatter.
    fn coerce_number_to_text(&mut self, number: f64) -> Result<String, Failure>;
    /// Read a source coordinate while recording its consumed dependency.
    fn cell(
        &mut self,
        reference: &Reference,
        sheet: u32,
        row: u32,
        col: u32,
    ) -> Result<Scalar, Failure>;
    /// Signed logical OFFSET areas require an adapter that preserves blank grid padding.
    /// Existing unsigned ports continue unchanged; unsupported adapters must not fabricate reads.
    fn signed_cell(
        &mut self,
        reference: &crate::lookup::SignedReference,
        sheet: u32,
        row: i32,
        col: i32,
    ) -> Result<Scalar, Failure> {
        let _ = (reference, sheet, row, col);
        Err(Failure::Unsupported(
            crate::lookup::Unsupported::WorkbookMetadata,
        ))
    }
    /// Resolve a computed reference while retaining namespace dependencies.
    fn dynamic_reference(
        &mut self,
        query: &DynamicReference,
        context: Context,
    ) -> Result<Value, Failure>;
    /// Resolve a table-style reference when the host supports it.
    fn structured_reference(&mut self, text: &str, context: Context) -> Result<Value, Failure>;
}
