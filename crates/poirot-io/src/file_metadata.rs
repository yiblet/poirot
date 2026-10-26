//! File provenance and diagnostics retained with workbook source data.

/// File representation used when importing workbook source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileFormat {
    /// OOXML spreadsheet package.
    Xlsx,
    /// BIFF binary spreadsheet.
    Xls,
}

/// The original file format and warnings from importing a workbook.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    /// Original spreadsheet representation.
    pub format: FileFormat,
    /// Features omitted or altered by the importer.
    pub import_warnings: Vec<ImportDiagnostic>,
}

/// Category of information lost or altered during spreadsheet import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImportDiagnosticKind {
    /// Source features outside the workbook model are omitted.
    OmittedFeatures,
    /// Name identity could not be compared or normalized completely.
    IdentifierIdentity,
    /// External relationships are neither followed nor preserved.
    ExternalLink,
    /// A source representation was reduced or recovered through backend limitations.
    RepresentationLoss,
}
/// A feature changed or omitted while importing a workbook.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportDiagnostic {
    /// Category for matching warnings in application code.
    pub kind: ImportDiagnosticKind,
    /// Source-specific details explaining the loss.
    pub detail: String,
}
impl std::fmt::Display for ImportDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}
