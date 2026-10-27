//! Pure decoded-value conversion and import fidelity diagnostics.
use crate::IoError;
use crate::data::{SheetSource, WorkbookSource};
use crate::{CellError, CellValue};
use crate::{ImportDiagnostic, ImportDiagnosticKind};

/// Report identifier spellings outside supported comparison fidelity.
pub(super) fn identifier_warnings(
    workbook: &impl WorkbookSource,
    warnings: &mut Vec<ImportDiagnostic>,
) {
    use crate::identifiers::{IdentifierComparison, compare};
    for (index, sheet) in workbook.sheets().iter().enumerate() {
        // SAFETY: index comes from enumerating the same unchanged collection, so its prefix is in bounds.
        for previous in &workbook.sheets()[..index] {
            if compare(previous.name(), sheet.name()) == IdentifierComparison::UnsupportedMapping {
                warnings.push(ImportDiagnostic { kind: ImportDiagnosticKind::IdentifierIdentity, detail: format!("unsupported identifier comparison: sheets {:?} and {:?}; raw spelling preserved", previous.name(), sheet.name()) });
            }
        }
    }
    for (index, name) in workbook.named_ranges().iter().enumerate() {
        // SAFETY: index comes from enumerating the same unchanged collection, so its prefix is in bounds.
        for previous in &workbook.named_ranges()[..index] {
            if previous.sheet_scope == name.sheet_scope {
                match compare(&previous.name, &name.name) {
                    IdentifierComparison::Different => {},
                    comparison => warnings.push(ImportDiagnostic { kind: ImportDiagnosticKind::IdentifierIdentity, detail: format!("imported defined-name identity {comparison:?}: {:?} and {:?}, scope {:?}; raw spelling preserved", previous.name, name.name, name.sheet_scope) }),
                }
            }
        }
    }
}
/// Collect source features whose decoding cannot preserve full fidelity.
pub(super) fn fidelity_warnings(format: crate::FileFormat) -> Vec<ImportDiagnostic> {
    let detail = match format {
        crate::FileFormat::Xlsx => {
            "XLSX charts, macros, drawings, conditional formats, validations, links, merge layout and opaque records are not preserved; ordinary cell styles retain the supported typed subset"
        }
        crate::FileFormat::Xls => {
            "XLS charts, macros, drawings, rich styles, validations, links, merge layout and opaque records are not preserved"
        }
    };
    vec![ImportDiagnostic {
        kind: ImportDiagnosticKind::OmittedFeatures,
        detail: detail.into(),
    }]
}

/// Project a backend cell into canonical values with explicit fidelity handling.
pub(super) fn xlsx_value(data: calamine::Data) -> Result<CellValue, IoError> {
    use calamine::Data;
    Ok(match data {
        Data::Empty => CellValue::Blank,
        Data::Int(n) => CellValue::Number(n as f64),
        Data::Float(n) => CellValue::Number(n),
        Data::String(s) => CellValue::Text(s),
        Data::Bool(b) => CellValue::Boolean(b),
        Data::DateTime(d) => CellValue::Number(d.as_f64()),
        Data::Error(e) => CellValue::Error(
            e.to_string()
                .parse::<CellError>()
                .map_err(crate::ModelError::from)?,
        ),
        Data::DateTimeIso(s) | Data::DurationIso(s) => {
            return Err(IoError::Unsupported(
                crate::WorkbookFeatureError::IsoDateOrDuration { input: s },
            ));
        }
    })
}
