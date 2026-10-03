//! Pure decoded-value conversion and import fidelity diagnostics.
use crate::IoError;
use calamine::Data;
use poirot_model::{CellError, CellValue, Workbook};

/// Project a backend cell into canonical values with explicit fidelity handling.
pub(super) fn value(data: Data) -> Result<CellValue, IoError> {
    Ok(match data {
        Data::Empty => CellValue::Blank,
        Data::Int(n) => CellValue::Number(n as f64),
        Data::Float(n) => CellValue::Number(n),
        Data::String(s) => CellValue::Text(s),
        Data::Bool(b) => CellValue::Boolean(b),
        Data::DateTime(d) => CellValue::Number(d.as_f64()),
        Data::Error(e) => CellValue::Error(e.to_string().parse::<CellError>()?),
        Data::DateTimeIso(s) | Data::DurationIso(s) => {
            return Err(IoError::Unsupported(format!(
                "ISO date/duration has no typed model representation: {s}"
            )));
        }
    })
}

/// Report identifier spellings outside supported comparison fidelity.
pub(super) fn identifier_warnings(workbook: &Workbook, warnings: &mut Vec<String>) {
    use poirot_model::identifiers::{IdentifierComparison, compare};
    for (index, sheet) in workbook.sheets().iter().enumerate() {
        // SAFETY: index comes from enumerating the same unchanged collection, so its prefix is in bounds.
        for previous in &workbook.sheets()[..index] {
            if compare(previous.name(), sheet.name()) == IdentifierComparison::UnsupportedMapping {
                warnings.push(format!("unsupported identifier comparison: sheets {:?} and {:?}; raw spelling preserved", previous.name(), sheet.name()));
            }
        }
    }
    for (index, name) in workbook.named_ranges.iter().enumerate() {
        // SAFETY: index comes from enumerating the same unchanged collection, so its prefix is in bounds.
        for previous in &workbook.named_ranges[..index] {
            if previous.sheet_scope == name.sheet_scope {
                match compare(&previous.name, &name.name) {
                    IdentifierComparison::Different => {},
                    comparison => warnings.push(format!("imported defined-name identity {comparison:?}: {:?} and {:?}, scope {:?}; raw spelling preserved", previous.name, name.name, name.sheet_scope)),
                }
            }
        }
    }
}
/// Collect source features whose decoding cannot preserve full fidelity.
pub(super) fn fidelity_warnings() -> Vec<String> {
    vec!["Only sparse rows/cells/formulas/cached values/date system/names/number-format strings are represented; charts, macros, drawings, rich styles, validations, links, merge layout and opaque records are not preserved".into()]
}
