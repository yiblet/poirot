//! Conservative original-source proof for quarantining rejected fixture declarations.
use super::inspection_location::InspectionLocation;
use super::name_admission_error::NameAdmissionError;
use super::xml::model::FodsWorkbook;
use std::collections::BTreeSet;

// These limits bound even an adversarial synthetic record: at most 128 scans of
// four MiB each. Exceeding a bound retains the workbook-context blocker.
const MAX_NAMES: usize = 128;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_FORMULA_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 256;
const MAX_RECORDS: usize = 65_536;

/// Prove only absence of potential use; never bind or reinterpret a Calc name.
pub(super) fn prove_unused(data: &FodsWorkbook, name: &str) -> Result<(), NameAdmissionError> {
    if data.names.len() > MAX_NAMES {
        return Err(NameAdmissionError::NameLimit);
    }
    if name.len() > 255
        || name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(NameAdmissionError::InvalidNameSpelling);
    }
    let initial = data
        .sheets
        .len()
        .checked_add(data.cases.len())
        .and_then(|n| n.checked_add(data.names.len()))
        .and_then(|n| n.checked_add(data.omissions.len()))
        .and_then(|n| n.checked_add(data.original_formula_sources.as_ref().map_or(0, Vec::len)))
        .filter(|n| *n <= MAX_RECORDS)
        .ok_or(NameAdmissionError::RecordLimit)?;
    data.sheets.iter().try_fold(initial, |count, sheet| {
        count
            .checked_add(sheet.cells.len())
            .filter(|n| *n <= MAX_RECORDS)
            .ok_or(NameAdmissionError::RecordLimit)
    })?;
    let sources = data
        .original_formula_sources
        .as_ref()
        .ok_or(NameAdmissionError::MissingOriginalSources)?;
    if data.sheets.iter().any(|sheet| sheet.name.len() > 1024) {
        return Err(NameAdmissionError::WorksheetNameLimit);
    }
    let candidate = name.to_ascii_lowercase();
    let mut bytes = 0usize;
    let mut inspect =
        |source: &str, location: InspectionLocation| -> Result<(), NameAdmissionError> {
            bytes = bytes
                .checked_add(source.len())
                .ok_or(NameAdmissionError::SourceSizeOverflow)?;
            if bytes > MAX_SOURCE_BYTES {
                return Err(NameAdmissionError::SourceSizeLimit);
            }
            scan(source, &candidate).map_err(|source| NameAdmissionError::Inspection {
                location,
                source: Box::new(source),
            })
        };
    for (index, source) in sources.iter().enumerate() {
        inspect(
            &source.source,
            InspectionLocation::Attribute {
                attribute: source.attribute,
                index,
            },
        )?;
    }
    for sheet in &data.sheets {
        let anchors: BTreeSet<_> = sheet
            .cells
            .iter()
            .filter(|cell| cell.formula.is_some() && cell.original_formula.is_some())
            .map(|cell| (cell.row, cell.col))
            .collect();
        for cell in &sheet.cells {
            let location = InspectionLocation::Cell {
                sheet: sheet.name.clone(),
                row: cell.row,
                col: cell.col,
            };
            if cell.formula.is_some() && cell.original_formula.is_none() {
                return Err(NameAdmissionError::MissingFormula { location });
            }
            if let Some(source) = &cell.original_formula {
                inspect(source, location.clone())?;
            }
            if let Some(matrix) = &cell.matrix
                && !anchors.contains(&(matrix.origin_row, matrix.origin_col))
            {
                return Err(NameAdmissionError::MissingMatrixAnchor { location });
            }
        }
    }
    for definition in &data.names {
        let original = definition.original_definition.as_deref().ok_or_else(|| {
            NameAdmissionError::MissingDefinition {
                name: definition.name.clone(),
            }
        })?;
        inspect(
            original,
            InspectionLocation::Definition {
                name: definition.name.clone(),
            },
        )?;
        if let Some(base) = &definition.base_cell_address {
            inspect(
                base,
                InspectionLocation::DefinitionBase {
                    name: definition.name.clone(),
                },
            )?;
        }
    }
    for case in &data.cases {
        inspect(
            &case.original_formula,
            InspectionLocation::Actual {
                case: case.id.clone(),
            },
        )?;
        inspect(
            &case.original_correct_formula,
            InspectionLocation::Correct {
                case: case.id.clone(),
            },
        )?;
    }
    Ok(())
}

fn scan(source: &str, candidate: &str) -> Result<(), NameAdmissionError> {
    if source.len() > MAX_FORMULA_BYTES {
        return Err(NameAdmissionError::FormulaSizeLimit);
    }
    if !source.is_ascii() {
        return Err(NameAdmissionError::NonAsciiSource);
    }
    // Substring comparison deliberately over-blocks longer identifiers and literal
    // strings. Removing dollar signs also catches absolute DAY$1 / $DAY$1 uses.
    // It never rewrites the source passed to the workbook or evaluator.
    let folded: String = source
        .bytes()
        .filter(|b| *b != b'$')
        .map(|b| char::from(b.to_ascii_lowercase()))
        .collect();
    if folded.contains(candidate) {
        return Err(NameAdmissionError::PotentialUse);
    }
    // Inspect tokens even inside strings: false positives are safe here. Namespace
    // components cover OpenFormula and _xlfn spellings without using translation.
    for token in source.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.') {
        if token.split('.').any(|part| {
            [
                "INDIRECT", "EVALUATE", "CALL", "RUN", "EXEC", "REGISTER", "DDE",
            ]
            .iter()
            .any(|resolver| part.eq_ignore_ascii_case(resolver))
        }) {
            return Err(NameAdmissionError::RuntimeResolver);
        }
    }
    let mut delimiters = Vec::new();
    let mut quoted = None;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(quote) = quoted {
            if c == quote {
                if chars.peek() == Some(&quote) {
                    chars.next();
                } else {
                    quoted = None;
                }
            }
            continue;
        }
        match c {
            '\'' | '"' => quoted = Some(c),
            '(' | '[' | '{' => {
                if delimiters.len() >= MAX_DEPTH {
                    return Err(NameAdmissionError::NestingLimit);
                }
                delimiters.push(c);
            }
            ')' | ']' | '}' => {
                let expected = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if delimiters.pop() != Some(expected) {
                    return Err(NameAdmissionError::UnbalancedDelimiters);
                }
            }
            c if c.is_ascii_alphanumeric()
                || c.is_ascii_whitespace()
                || "_.$!:;,#%^+-*/&=<>|".contains(c) => {}
            _ => return Err(NameAdmissionError::UnrecognizedSyntax),
        }
    }
    if quoted.is_some() || !delimiters.is_empty() {
        return Err(NameAdmissionError::UnterminatedSource);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fods::xml::model::{CellInput, MatrixSpan, NamedDefinition, Sheet};

    fn data() -> FodsWorkbook {
        FodsWorkbook {
            source: "independent".into(),
            source_commit: "test".into(),
            sha256: "test".into(),
            sheets: vec![],
            names: vec![NamedDefinition {
                name: "GQ17".into(),
                formula: "123".into(),
                original_definition: Some("of:=123".into()),
                base_cell_address: None,
            }],
            original_formula_sources: Some(vec![]),
            calculation_settings: vec![],
            cases: vec![],
            omissions: vec![],
        }
    }

    #[test]
    fn incomplete_source_records_cannot_establish_non_use() {
        let mut data = data();
        assert!(prove_unused(&data, "GQ17").is_ok());
        data.original_formula_sources = None;
        assert!(matches!(
            prove_unused(&data, "GQ17"),
            Err(NameAdmissionError::MissingOriginalSources)
        ));
        data.original_formula_sources = Some(vec![]);
        data.names[0].original_definition = None;
        assert!(matches!(
            prove_unused(&data, "GQ17"),
            Err(NameAdmissionError::MissingDefinition { .. })
        ));
        data.names[0].original_definition = Some("of:=123".into());
        let mut cell = CellInput::blank(0, 0);
        cell.formula = Some("42".into());
        data.sheets.push(Sheet {
            name: "Test".into(),
            cells: vec![cell],
        });
        assert!(matches!(
            prove_unused(&data, "GQ17"),
            Err(NameAdmissionError::MissingFormula { .. })
        ));
        data.sheets[0].cells[0].original_formula = Some("of:=42".into());
        assert!(prove_unused(&data, "GQ17").is_ok());
        data.sheets[0].cells[0].matrix = Some(MatrixSpan {
            rows: 2,
            cols: 1,
            origin_row: 1,
            origin_col: 0,
            formula: "GQ17".into(),
            offset_row: 0,
            offset_col: 0,
        });
        assert!(matches!(
            prove_unused(&data, "GQ17"),
            Err(NameAdmissionError::MissingMatrixAnchor { .. })
        ));
    }

    #[test]
    fn bounded_proof_never_converts_limits_or_unrecognized_names_to_unused() {
        let mut data = data();
        data.names[0].original_definition = Some("1".repeat(MAX_FORMULA_BYTES + 1));
        let NameAdmissionError::Inspection { source, .. } =
            prove_unused(&data, "GQ17").unwrap_err()
        else {
            panic!("wrong proof error category")
        };
        assert!(matches!(*source, NameAdmissionError::FormulaSizeLimit));
        data.names[0].original_definition = Some(format!(
            "{}1{}",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        ));
        let NameAdmissionError::Inspection { source, .. } =
            prove_unused(&data, "GQ17").unwrap_err()
        else {
            panic!("wrong proof error category")
        };
        assert!(matches!(*source, NameAdmissionError::NestingLimit));
        data.names[0].original_definition = Some("of:=1".into());
        data.names = vec![data.names[0].clone(); MAX_NAMES + 1];
        assert!(matches!(
            prove_unused(&data, "GQ17"),
            Err(NameAdmissionError::NameLimit)
        ));
        let mut many = self::data();
        many.sheets = vec![Sheet {
            name: "Test".into(),
            cells: vec![CellInput::blank(0, 0); MAX_RECORDS],
        }];
        assert!(matches!(
            prove_unused(&many, "GQ17"),
            Err(NameAdmissionError::RecordLimit)
        ));
        assert!(prove_unused(&self::data(), "é17").is_err());
        assert!(prove_unused(&self::data(), "bad name").is_err());
    }
}
