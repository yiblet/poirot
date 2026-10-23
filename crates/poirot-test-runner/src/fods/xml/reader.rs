//! Namespace-aware XML input reader. Cached formula results are discarded.
use super::{
    SOURCE_COMMIT,
    model::{
        CellInput, FodsWorkbook, FormulaAttribute, InputValue, MatrixSpan, NamedDefinition,
        Omission, OriginalFormulaSource, Sheet,
    },
};
use crate::fods::{cases, formula::translate};
use regex::{Captures, Regex};
use roxmltree::{Document, Node};
use std::{collections::BTreeMap, path::Path, sync::OnceLock};

const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const MAX_ROWS: usize = 1_048_576;
const MAX_COLS: usize = 16_384;
const MAX_CELLS: usize = 1_000_000;
const MAX_TEXT: usize = 16 * 1024 * 1024;
const MAX_SOURCE: usize = 64 * 1024 * 1024;

/// Worksheet row/column key used while extracting sparse native cells.
type Coordinate = (u32, u32);
fn tag(node: Node<'_, '_>, namespace: &str, name: &str) -> bool {
    node.is_element()
        && node.tag_name().namespace() == Some(namespace)
        && node.tag_name().name() == name
}
fn attr<'a, 'input>(node: Node<'a, 'input>, namespace: &str, key: &str) -> Option<&'a str> {
    node.attribute((namespace, key))
}
fn required<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    key: &str,
) -> Result<&'a str, String> {
    attr(node, namespace, key).ok_or_else(|| format!("missing {key} on {}", node.tag_name().name()))
}
fn positive_attr(node: Node<'_, '_>, key: &str, default: usize) -> Result<usize, String> {
    match attr(node, TABLE, key) {
        None => Ok(default),
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("invalid positive {key}: {value}")),
    }
}
fn append_text(output: &mut String, text: &str) -> Result<(), String> {
    if output
        .len()
        .checked_add(text.len())
        .is_none_or(|n| n > MAX_TEXT)
    {
        return Err("ODF text exceeds bounded input size".into());
    }
    output.push_str(text);
    Ok(())
}
fn odf_text(node: Node<'_, '_>, output: &mut String) -> Result<(), String> {
    for child in node.children() {
        if child.is_text() {
            append_text(output, child.text().unwrap_or(""))?;
        } else if tag(child, TEXT, "s") {
            let n = attr(child, TEXT, "c")
                .unwrap_or("1")
                .parse::<usize>()
                .map_err(|_| "invalid ODF space count")?;
            if n > MAX_TEXT || output.len().checked_add(n).is_none_or(|n| n > MAX_TEXT) {
                return Err("ODF spaces exceed bounded input size".into());
            }
            output.extend(std::iter::repeat_n(' ', n));
        } else if tag(child, TEXT, "tab") {
            append_text(output, "\t")?;
        } else if tag(child, TEXT, "line-break") {
            append_text(output, "\n")?;
        } else if child.is_element() {
            odf_text(child, output)?;
        }
    }
    Ok(())
}
fn finite_number(raw: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("invalid finite ODF number: {raw}"))
}
fn capture_number(m: &Captures<'_>, index: usize) -> Result<f64, String> {
    m.get(index).map_or(Ok(0.0), |s| finite_number(s.as_str()))
}
fn date_regex() -> &'static Regex {
    static DATE: OnceLock<Regex> = OnceLock::new();
    DATE.get_or_init(|| {
        // SAFETY: This literal regex has balanced groups/classes and supported syntax; no caller input is compiled.
        Regex::new(r"^(-?\d+)-(\d+)-(\d+)(?:T(\d+):(\d+):(\d+(?:\.\d+)?))?").unwrap()
    })
}
fn time_regex() -> &'static Regex {
    static TIME: OnceLock<Regex> = OnceLock::new();
    // SAFETY: This literal regex has balanced groups/classes and supported syntax; no caller input is compiled.
    TIME.get_or_init(|| Regex::new(r"^(-)?PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?$").unwrap())
}
fn cell_value(node: Node<'_, '_>) -> Result<InputValue, String> {
    if attr(node, TABLE, "formula").is_some_and(|s| !s.is_empty()) {
        return Ok(InputValue::Blank);
    }
    let mut paragraphs = Vec::new();
    for p in node.children().filter(|n| tag(*n, TEXT, "p")) {
        let mut text = String::new();
        odf_text(p, &mut text)?;
        paragraphs.push(text);
    }
    let text = paragraphs.join("\n");
    let text_value = || InputValue::Text {
        value: attr(node, OFFICE, "string-value").unwrap_or(&text).into(),
    };
    match attr(node, OFFICE, "value-type") {
        Some("float" | "percentage" | "currency") => Ok(InputValue::number(finite_number(
            required(node, OFFICE, "value")?,
        )?)),
        Some("boolean") => Ok(InputValue::Boolean {
            value: attr(node, OFFICE, "boolean-value") == Some("true"),
        }),
        Some("date") => {
            let raw = required(node, OFFICE, "date-value")?;
            let m = date_regex()
                .captures(raw)
                .ok_or_else(|| format!("invalid ODF date: {raw}"))?;
            let mut year = m[1].parse::<i64>().map_err(|_| "ODF year out of bounds")?;
            if !(-1_000_000..=1_000_000).contains(&year) {
                return Err("ODF year out of bounds".into());
            }
            let month = m[2].parse::<i64>().map_err(|_| "invalid ODF month")?;
            let day = m[3].parse::<i64>().map_err(|_| "invalid ODF day")?;
            if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                return Err(format!("invalid ODF date fields: {raw}"));
            }
            year -= i64::from(month <= 2);
            let era = year.div_euclid(400);
            let yo = year - era * 400;
            // Gregorian conversion adapts Howard Hinnant's days_from_civil/civil_from_days.
            // MIT reference: HowardHinnant/date 5bdb7e6f31fac909c090a46dbd9fea27b6e609a4,
            // include/date/date.h; see root LICENSES/ACKNOWLEDGEMENTS.
            let days = era * 146097 + yo * 365 + yo / 4 - yo / 100
                + (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5
                + day
                - 1
                - 719468;
            let fraction = (capture_number(&m, 4)? * 3600.0
                + capture_number(&m, 5)? * 60.0
                + capture_number(&m, 6)?)
                / 86400.0;
            Ok(InputValue::Number {
                value: days as f64 + 25569.0 + fraction,
                odf_type: Some("date".into()),
                odf_value: Some(raw.into()),
            })
        }
        Some("time") => {
            let raw = attr(node, OFFICE, "time-value").unwrap_or("");
            if let Some(m) = time_regex().captures(raw) {
                let sign = if m.get(1).is_some() { -1.0 } else { 1.0 };
                Ok(InputValue::Number {
                    value: sign
                        * (capture_number(&m, 2)? * 3600.0
                            + capture_number(&m, 3)? * 60.0
                            + capture_number(&m, 4)?)
                        / 86400.0,
                    odf_type: Some("time".into()),
                    odf_value: None,
                })
            } else if !text.is_empty() {
                Ok(text_value())
            } else {
                Ok(InputValue::Blank)
            }
        }
        Some("string") => Ok(text_value()),
        _ if !text.is_empty() => Ok(text_value()),
        _ => Ok(InputValue::Blank),
    }
}
fn insert_cell(cells: &mut BTreeMap<Coordinate, CellInput>, item: CellInput) -> Result<(), String> {
    if cells.len() >= MAX_CELLS {
        return Err("materialized FODS cells exceed bounded input size".into());
    }
    cells.insert((item.row, item.col), item);
    Ok(())
}
fn read_sheet(table: Node<'_, '_>) -> Result<Sheet, String> {
    let name = required(table, TABLE, "name")?.into();
    let mut cells: BTreeMap<Coordinate, CellInput> = BTreeMap::new();
    let mut r = 0usize;
    for row in table.children().filter(|n| tag(*n, TABLE, "table-row")) {
        let repeat = positive_attr(row, "number-rows-repeated", 1)?;
        let end_row = r
            .checked_add(repeat)
            .ok_or("FODS row padding overflows coordinates")?;
        let mut c = 0usize;
        let mut row_cells = Vec::new();
        for node in row
            .children()
            .filter(|n| tag(*n, TABLE, "table-cell") || tag(*n, TABLE, "covered-table-cell"))
        {
            let n = positive_attr(node, "number-columns-repeated", 1)?;
            let end_col = c
                .checked_add(n)
                .ok_or("FODS column padding overflows coordinates")?;
            let formula = attr(node, TABLE, "formula").filter(|s| !s.is_empty());
            let value = cell_value(node)?;
            if formula.is_some() || !value.is_blank() {
                if end_col > MAX_COLS || r >= MAX_ROWS {
                    return Err("nonempty FODS cell exceeds worksheet bounds".into());
                }
                for offset in 0..n {
                    let mut cell = CellInput::blank(r as u32, (c + offset) as u32);
                    cell.value = value.clone();
                    if let Some(raw) = formula {
                        let translated = translate(raw);
                        cell.formula = Some(translated.clone());
                        cell.original_formula = Some(raw.into());
                        if attr(node, TABLE, "number-matrix-columns-spanned").is_some()
                            || attr(node, TABLE, "number-matrix-rows-spanned").is_some()
                        {
                            let rows = positive_attr(node, "number-matrix-rows-spanned", 1)?;
                            let cols = positive_attr(node, "number-matrix-columns-spanned", 1)?;
                            if r.checked_add(rows).is_none_or(|n| n > MAX_ROWS)
                                || (c + offset).checked_add(cols).is_none_or(|n| n > MAX_COLS)
                                || rows.checked_mul(cols).is_none_or(|n| n > MAX_CELLS)
                            {
                                return Err(
                                    "matrix span exceeds worksheet/materialization bounds".into()
                                );
                            }
                            cell.matrix = Some(MatrixSpan {
                                rows: rows as u32,
                                cols: cols as u32,
                                origin_row: r as u32,
                                origin_col: (c + offset) as u32,
                                formula: translated,
                                offset_row: 0,
                                offset_col: 0,
                            });
                        }
                    }
                    row_cells.push(cell);
                }
            }
            c = end_col;
        }
        if !row_cells.is_empty() {
            if end_row > MAX_ROWS {
                return Err("nonempty FODS row exceeds worksheet bounds".into());
            }
            if repeat > 10000
                || repeat
                    .checked_mul(row_cells.len())
                    .is_none_or(|n| n > MAX_CELLS.saturating_sub(cells.len()))
            {
                return Err("enormous nonempty repeated row".into());
            }
            for dr in 0..repeat {
                for cell in &row_cells {
                    let mut cell = cell.clone();
                    cell.row = (r + dr) as u32;
                    insert_cell(&mut cells, cell)?;
                }
            }
        }
        r = end_row;
    }
    let matrices: Vec<CellInput> = cells
        .values()
        .filter(|c| c.matrix.is_some())
        .cloned()
        .collect();
    for origin in matrices {
        let matrix = origin.matrix.as_ref().ok_or("missing matrix origin")?;
        for dr in 0..matrix.rows {
            for dc in 0..matrix.cols {
                let at = (origin.row + dr, origin.col + dc);
                if !cells.contains_key(&at) {
                    insert_cell(&mut cells, CellInput::blank(at.0, at.1))?;
                }
                let cell = cells.get_mut(&at).ok_or("missing matrix cell")?;
                cell.value = InputValue::Blank;
                let mut span = matrix.clone();
                span.offset_row = dr;
                span.offset_col = dc;
                cell.matrix = Some(span);
                if dr != 0 || dc != 0 {
                    cell.original_formula = origin.original_formula.clone();
                    cell.matrix_continuation = Some(true);
                }
            }
        }
    }
    Ok(Sheet {
        name,
        cells: cells.into_values().collect(),
    })
}
/// Identify linked table caches rather than local worksheet source.
fn linked_table_source<'a, 'input>(table: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    table.children().find(|node| {
        tag(*node, TABLE, "table-source")
            && attr(*node, XLINK, "href").is_some_and(|href| !href.is_empty())
    })
}
/// Extract original inputs and checks into typed records without cached formula values.
pub fn extract_bytes(bytes: &[u8], source: &str) -> Result<FodsWorkbook, String> {
    if bytes.len() > MAX_SOURCE {
        return Err("FODS fixture exceeds bounded source size".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|e| format!("FODS is not UTF-8: {e}"))?;
    let document = Document::parse(text).map_err(|e| format!("invalid FODS XML: {e}"))?;
    let mut sheets = Vec::new();
    let mut total_cells = 0usize;
    let mut omissions = Vec::new();
    for table in document.descendants().filter(|n| tag(*n, TABLE, "table")) {
        // ODF 1.1 section 8.3.2 defines table:table-source as linked table data:
        // https://docs.oasis-open.org/office/v1.1/OS/OpenDocument-v1.1-html/OpenDocument-v1.1.html
        // Its stored results are external caches, not authored local inputs.
        if let Some(link) = linked_table_source(table) {
            omissions.push(Omission {
                kind: super::model::OmissionKind::ExternalLinkedTable,
                source: Some(source.into()),
                sheet: attr(table, TABLE, "name").map(str::to_owned),
                cell: None,
                reason: format!(
                    "external linked table cache omitted: {}",
                    attr(link, XLINK, "href").unwrap_or("")
                ),
            });
            continue;
        }
        let sheet = read_sheet(table)?;
        total_cells = total_cells
            .checked_add(sheet.cells.len())
            .filter(|n| *n <= MAX_CELLS)
            .ok_or("FODS workbook materialization exceeds bounded size")?;
        sheets.push(sheet);
    }
    let (cases, case_omissions) = cases::extract(&sheets, source)?;
    omissions.extend(case_omissions);
    let mut names = Vec::new();
    for kind in ["named-range", "named-expression"] {
        for node in document.descendants().filter(|n| tag(*n, TABLE, kind)) {
            if node.ancestors().any(|ancestor| {
                tag(ancestor, TABLE, "table") && linked_table_source(ancestor).is_some()
            }) {
                continue;
            }
            let (original_definition, formula) = if let Some(raw) =
                attr(node, TABLE, "cell-range-address").filter(|s| !s.is_empty())
            {
                (raw.to_owned(), translate(&format!("of:=[{raw}]")))
            } else {
                let raw = required(node, TABLE, "expression")?;
                (raw.to_owned(), translate(raw))
            };
            names.push(NamedDefinition {
                name: required(node, TABLE, "name")?.into(),
                formula,
                original_definition: Some(original_definition),
                base_cell_address: attr(node, TABLE, "base-cell-address").map(str::to_owned),
            });
        }
    }
    let calculation_settings = document
        .descendants()
        .filter(|n| tag(*n, TABLE, "calculation-settings"))
        .map(|n| {
            n.attributes()
                .map(|a| {
                    let key = a
                        .namespace()
                        .map_or_else(|| a.name().into(), |ns| format!("{{{ns}}}{}", a.name()));
                    (key, a.value().into())
                })
                .collect()
        })
        .collect();
    // Preserve these even in excluded linked tables. They are inspection inputs,
    // never workbook cells or substitutes for executing the native Correct check.
    let original_formula_sources = document
        .descendants()
        .flat_map(|node| {
            [
                ("formula", FormulaAttribute::CellFormula),
                ("expression", FormulaAttribute::NameExpression),
                ("cell-range-address", FormulaAttribute::NameRange),
                ("base-cell-address", FormulaAttribute::NameBase),
            ]
            .into_iter()
            .filter_map(move |(name, attribute)| {
                attr(node, TABLE, name).map(|source| OriginalFormulaSource {
                    attribute,
                    source: source.to_owned(),
                })
            })
        })
        .collect();
    Ok(FodsWorkbook {
        source: source.into(),
        source_commit: SOURCE_COMMIT.into(),
        sha256: crate::model::digest(bytes),
        sheets,
        names,
        calculation_settings,
        original_formula_sources: Some(original_formula_sources),
        cases,
        omissions,
    })
}
/// Return structured extracted source records for one native fixture.
pub fn fixture_data(path: &Path, source: &str) -> Result<FodsWorkbook, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("cannot read FODS {}: {e}", path.display()))?;
    extract_bytes(&bytes, source).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(inner: &str) -> String {
        format!(
            r#"<office:document xmlns:office="{OFFICE}" xmlns:table="{TABLE}" xmlns:text="{TEXT}">{inner}</office:document>"#
        )
    }
    #[test]
    fn preserves_odf_spaces_and_date_epoch() {
        let xml = document(
            r#"<table:table table:name="Inputs"><table:table-row><table:table-cell office:value-type="string"><text:p><text:s text:c="3"/>a<text:tab/>b<text:line-break/>c<text:s/></text:p></table:table-cell><table:table-cell office:value-type="date" office:date-value="1900-02-28"/></table:table-row></table:table>"#,
        );
        let data = extract_bytes(xml.as_bytes(), "text.fods").unwrap();
        assert_eq!(
            data.sheets[0].cells[0].value,
            InputValue::Text {
                value: "   a\tb\nc ".into()
            }
        );
        assert!(matches!(
            data.sheets[0].cells[1].value,
            InputValue::Number { value: 60.0, .. }
        ));
    }
    #[test]
    fn matrix_caches_are_blank_and_oracles_are_independent() {
        let xml = document(
            r#"<table:table table:name="Cases"><table:table-row><table:table-cell office:value-type="string"><text:p>Function</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Expected</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Correct</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell table:formula="of:=TRANSPOSE([.D2:.E2])" table:number-matrix-rows-spanned="2" table:number-matrix-columns-spanned="1" office:value-type="float" office:value="999"/><table:table-cell office:value-type="float" office:value="3"/><table:table-cell table:formula="of:=[.A2]=[.B2]" office:value-type="boolean" office:boolean-value="false"/><table:table-cell office:value-type="float" office:value="3"/><table:table-cell office:value-type="float" office:value="4"/></table:table-row><table:table-row><table:table-cell office:value-type="float" office:value="888"/><table:table-cell office:value-type="float" office:value="4"/><table:table-cell table:formula="of:=[.A3]=[.B3]" office:value-type="boolean" office:boolean-value="false"/></table:table-row></table:table>"#,
        );
        let data = extract_bytes(
            xml.as_bytes(),
            "sc/qa/unit/data/functions/array/fods/transpose.fods",
        )
        .unwrap();
        assert_eq!(data.cases.len(), 2);
        assert!(matches!(
            data.cases[0].expected.value,
            InputValue::Number { value: 3.0, .. }
        ));
        assert_eq!(data.cases[1].matrix.as_ref().unwrap().offset_row, 1);
        assert_eq!(data.cases[1].id, "array__fods__transpose__00001");
        for cell in data.sheets[0]
            .cells
            .iter()
            .filter(|c| c.matrix.is_some() || c.formula.is_some())
        {
            assert!(cell.value.is_blank());
        }
    }
    #[test]
    fn malformed_or_excessive_repetition_fails_without_allocation() {
        for repeat in ["0", "-1", "10001", "999999999999999999999999"] {
            let xml = document(&format!(
                r#"<table:table table:name="Bad"><table:table-row table:number-rows-repeated="{repeat}"><table:table-cell office:value-type="float" office:value="1"/></table:table-row></table:table>"#
            ));
            assert!(extract_bytes(xml.as_bytes(), "bad.fods").is_err());
        }
    }
}
