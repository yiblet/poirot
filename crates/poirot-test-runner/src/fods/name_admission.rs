//! Conservative original-source proof for quarantining rejected fixture declarations.
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
pub(super) fn prove_unused(data: &FodsWorkbook, name: &str) -> Result<(), String> {
    if data.names.len() > MAX_NAMES {
        return Err("defined-name inspection limit exceeded".into());
    }
    if name.len() > 255
        || name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("rejected spelling is outside the ASCII identifier proof".into());
    }
    let initial = data
        .sheets
        .len()
        .checked_add(data.cases.len())
        .and_then(|n| n.checked_add(data.names.len()))
        .and_then(|n| n.checked_add(data.omissions.len()))
        .and_then(|n| n.checked_add(data.original_formula_sources.as_ref().map_or(0, Vec::len)))
        .filter(|n| *n <= MAX_RECORDS)
        .ok_or("source record inspection limit exceeded")?;
    data.sheets.iter().try_fold(initial, |count, sheet| {
        count
            .checked_add(sheet.cells.len())
            .filter(|n| *n <= MAX_RECORDS)
            .ok_or("source record inspection limit exceeded")
    })?;
    let sources = data
        .original_formula_sources
        .as_ref()
        .ok_or("complete original XML formula inspection record is unavailable")?;
    if data.sheets.iter().any(|sheet| sheet.name.len() > 1024) {
        return Err("worksheet spelling exceeds inspection limit".into());
    }
    let candidate = name.to_ascii_lowercase();
    let mut bytes = 0usize;
    let mut inspect = |source: &str, location: &str| -> Result<(), String> {
        bytes = bytes
            .checked_add(source.len())
            .ok_or("source inspection limit exceeded")?;
        if bytes > MAX_SOURCE_BYTES {
            return Err("whole-fixture source inspection limit exceeded".into());
        }
        scan(source, &candidate).map_err(|reason| format!("{location}: {reason}"))
    };
    for (index, source) in sources.iter().enumerate() {
        inspect(
            &source.source,
            &format!("native XML {:?} attribute {index}", source.attribute),
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
            let location = format!(
                "sheet {:?} row {} column {}",
                sheet.name, cell.row, cell.col
            );
            if cell.formula.is_some() && cell.original_formula.is_none() {
                return Err(format!(
                    "{location}: original formula source is unavailable"
                ));
            }
            if let Some(source) = &cell.original_formula {
                inspect(source, &location)?;
            }
            if let Some(matrix) = &cell.matrix
                && !anchors.contains(&(matrix.origin_row, matrix.origin_col))
            {
                return Err(format!(
                    "{location}: original matrix-anchor source is unavailable"
                ));
            }
        }
    }
    for definition in &data.names {
        let original = definition.original_definition.as_deref().ok_or_else(|| {
            format!(
                "name {:?}: original definition is unavailable",
                definition.name
            )
        })?;
        inspect(original, &format!("definition {:?}", definition.name))?;
        if let Some(base) = &definition.base_cell_address {
            inspect(base, &format!("definition {:?} base", definition.name))?;
        }
    }
    for case in &data.cases {
        inspect(&case.original_formula, &format!("case {} actual", case.id))?;
        inspect(
            &case.original_correct_formula,
            &format!("case {} Correct", case.id),
        )?;
    }
    Ok(())
}

fn scan(source: &str, candidate: &str) -> Result<(), String> {
    if source.len() > MAX_FORMULA_BYTES {
        return Err("formula inspection limit exceeded".into());
    }
    if !source.is_ascii() {
        return Err("non-ASCII source cannot establish non-use".into());
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
        return Err(
            "potential rejected-name spelling, including address or string ambiguity".into(),
        );
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
            return Err("runtime name/formula resolver prevents a static non-use proof".into());
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
                    return Err("source nesting inspection limit exceeded".into());
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
                    return Err("unbalanced source delimiters prevent non-use proof".into());
                }
            }
            c if c.is_ascii_alphanumeric()
                || c.is_ascii_whitespace()
                || "_.$!:;,#%^+-*/&=<>|".contains(c) => {}
            _ => return Err("unrecognized lexical syntax prevents non-use proof".into()),
        }
    }
    if quoted.is_some() || !delimiters.is_empty() {
        return Err("unterminated source quotation or delimiter prevents non-use proof".into());
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
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("complete original")
        );
        data.original_formula_sources = Some(vec![]);
        data.names[0].original_definition = None;
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("original definition")
        );
        data.names[0].original_definition = Some("of:=123".into());
        let mut cell = CellInput::blank(0, 0);
        cell.formula = Some("42".into());
        data.sheets.push(Sheet {
            name: "Test".into(),
            cells: vec![cell],
        });
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("original formula")
        );
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
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("matrix-anchor")
        );
    }

    #[test]
    fn bounded_proof_never_converts_limits_or_unrecognized_names_to_unused() {
        let mut data = data();
        data.names[0].original_definition = Some("1".repeat(MAX_FORMULA_BYTES + 1));
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("inspection limit")
        );
        data.names[0].original_definition = Some(format!(
            "{}1{}",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        ));
        assert!(prove_unused(&data, "GQ17").unwrap_err().contains("nesting"));
        data.names[0].original_definition = Some("of:=1".into());
        data.names = vec![data.names[0].clone(); MAX_NAMES + 1];
        assert!(
            prove_unused(&data, "GQ17")
                .unwrap_err()
                .contains("defined-name")
        );
        let mut many = self::data();
        many.sheets = vec![Sheet {
            name: "Test".into(),
            cells: vec![CellInput::blank(0, 0); MAX_RECORDS],
        }];
        assert!(
            prove_unused(&many, "GQ17")
                .unwrap_err()
                .contains("source record")
        );
        assert!(prove_unused(&self::data(), "é17").is_err());
        assert!(prove_unused(&self::data(), "bad name").is_err());
    }
}
