use crate::model::CellValue;

/// One resolved argument value with omitted-argument identity preserved in `missing`.
/// Spreadsheet errors remain in `value` for the provider to inspect.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionScalar {
    /// Resolved scalar, preserving spreadsheet errors as values.
    pub value: CellValue,
    /// Whether this value originated from an omitted argument.
    pub missing: bool,
}
