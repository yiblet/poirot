//! Stored spreadsheet file metadata.
/// Persisted worksheet visibility; imports may retain very-hidden sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SheetVisibility {
    /// Ordinary visible worksheet.
    #[default]
    Visible,
    /// Hidden worksheet, available to an unhide control.
    Hidden,
    /// Workbook author restricted visibility, preserved on import/export.
    VeryHidden,
}
/// Workbook-owned presentation independent of row visibility.
/// The worksheet example sets sheet visibility and a tab color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SheetPresentation {
    /// Saved sheet visibility.
    pub visibility: SheetVisibility,
    /// Explicit sRGB channels; no palette/theme approximation is performed.
    pub tab_color: Option<[u8; 3]>,
}
