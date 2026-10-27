//! BIFF8 formula token streams paired with their trailing extra data.

/// One [MS-XLS] CellParsedFormula, SharedParsedFormula or NameParsedFormula:
/// the Ptg tokens and the RgbExtra bytes that hold PtgArray and PtgMemArea data.
#[derive(Clone, Copy)]
pub(in crate::xls) struct ParsedFormula<'a> {
    /// Ptg token stream (`rgce`, exactly `cce` bytes).
    pub(in crate::xls) rgce: &'a [u8],
    /// Record bytes after `rgce`. Extra data is read from the start in token
    /// order; unrelated record fields may follow it.
    pub(in crate::xls) rgcb: &'a [u8],
}
