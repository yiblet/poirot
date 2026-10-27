//! Sparse row and typed cell-cache projection into streaming worksheet XML.
use super::xml::{attr, element, events, write_event, xml};
use crate::IoError;
use crate::data::SheetSource;
use crate::{CellAddress, CellValue};
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use std::{collections::BTreeMap, io::Write};

/// Borrowed worksheet XML and canonical cells used to restore typed saved values.
pub(super) struct SheetPatch<'a, S: SheetSource> {
    /// Original backend-generated worksheet XML.
    pub(super) bytes: &'a [u8],
    /// Canonical source sheet supplying formulas, caches, and metadata.
    pub(super) sheet: &'a S,
    /// Canonical source style identities registered by the fresh writer.
    pub(super) styles: &'a super::StyleExport<'a>,
    /// Shared-source equivalence proof reused for sizing and actual serialization.
    pub(super) shared: &'a super::shared::SharedExport,
    /// Storage spellings that differ from source formula text, by cell.
    pub(super) stored_formulas: &'a BTreeMap<CellAddress, String>,
}

/// Resolved OOXML type and payload for one canonical source cell.
struct CellEncoding {
    kind: Option<&'static str>,
    value: Option<String>,
}

impl CellEncoding {
    fn new(value: &CellValue, has_formula: bool) -> Result<Self, IoError> {
        let (kind, value) = match value {
            CellValue::Blank => (None, None),
            CellValue::Number(n) => {
                if !n.is_finite() {
                    return Err(IoError::Unsupported("nonfinite formula cache".into()));
                }
                (None, Some(n.to_string()))
            }
            CellValue::Text(_) | CellValue::TextUtf16(_) => (
                Some(if has_formula { "str" } else { "inlineStr" }),
                Some(super::text::encode(
                    // SAFETY: This match arm explicitly selects a Text or TextUtf16 value before requesting its text view.
                    &value.text_ref().expect("text variant").to_excel_text(),
                )),
            ),
            CellValue::Boolean(b) => (Some("b"), Some(if *b { "1" } else { "0" }.into())),
            CellValue::Error(e) => (Some("e"), Some(e.to_string())),
        };
        Ok(Self { kind, value })
    }
}

fn style_map(bytes: &[u8], key: &str) -> Result<BTreeMap<CellAddress, String>, IoError> {
    let mut result = BTreeMap::new();
    let mut reader = events(bytes);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "c" => {
                if let (Some(r), Some(s)) = (attr(&e, "r")?, attr(&e, key)?) {
                    result.insert(r.parse()?, s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}
fn write_cells<W: Write>(
    writer: &mut Writer<W>,
    sheet: &impl SheetSource,
    styles: &super::StyleExport<'_>,
    metadata: &BTreeMap<CellAddress, String>,
    shared: &super::shared::SharedExport,
    stored_formulas: &BTreeMap<CellAddress, String>,
) -> Result<(), IoError> {
    let spills = super::spill::bounds(sheet)?;
    for index in sheet.row_indices() {
        CellAddress::new(index, 0)?;
        let mut start = BytesStart::new("row");
        let number = index
            .checked_add(1)
            .ok_or_else(|| IoError::Unsupported("row index overflow".into()))?
            .to_string();
        start.push_attribute(("r", number.as_str()));
        // OOXML has one row-hiding flag; the reader maps it back to `manually_hidden`.
        let visibility = sheet.row_visibility(index);
        if visibility.manually_hidden || visibility.filtered_out {
            start.push_attribute(("hidden", "1"));
        }
        write_event(writer, Event::Start(start))?;
        for (address, cell) in sheet.stored_range(
            CellAddress { row: index, col: 0 },
            CellAddress {
                row: index,
                col: crate::MAX_COLUMNS - 1,
            },
        ) {
            let mut start = BytesStart::new("c");
            address.validate()?;
            let reference = address.to_string();
            start.push_attribute(("r", reference.as_str()));
            let style = styles.id(&cell.style)?;
            if style != 0 {
                let style = style.to_string();
                start.push_attribute(("s", style.as_str()));
            }
            if spills.contains_key(address) {
                start.push_attribute(("cm", "1"));
            } else if let Some(index) = metadata.get(address) {
                start.push_attribute(("cm", index.as_str()));
            }
            let CellEncoding { kind, value } =
                CellEncoding::new(&cell.value, cell.formula.is_some())?;
            if let Some(kind) = kind {
                start.push_attribute(("t", kind));
            }
            write_event(writer, Event::Start(start))?;
            if let Some(formula) = &cell.formula {
                let formula = stored_formulas
                    .get(address)
                    .map_or_else(|| formula.trim_start_matches('='), String::as_str);
                if let Some(last) = spills.get(address) {
                    let mut start = BytesStart::new("f");
                    start.push_attribute(("t", "array"));
                    let range = format!("{address}:{last}");
                    start.push_attribute(("ref", range.as_str()));
                    write_event(writer, Event::Start(start))?;
                    write_event(
                        writer,
                        Event::Text(BytesText::new(formula.trim_start_matches('='))),
                    )?;
                    write_event(writer, Event::End(BytesEnd::new("f")))?;
                } else if let Some(role) = shared.get(*address) {
                    let mut start = BytesStart::new("f");
                    start.push_attribute(("t", "shared"));
                    let index = match role {
                        super::shared::SharedCell::Master { index, .. }
                        | super::shared::SharedCell::Follower { index } => index.to_string(),
                    };
                    start.push_attribute(("si", index.as_str()));
                    if let super::shared::SharedCell::Master { first, last, .. } = role {
                        let range = format!("{first}:{last}");
                        start.push_attribute(("ref", range.as_str()));
                        write_event(writer, Event::Start(start))?;
                        write_event(
                            writer,
                            Event::Text(BytesText::new(formula.trim_start_matches('='))),
                        )?;
                        write_event(writer, Event::End(BytesEnd::new("f")))?;
                    } else {
                        write_event(writer, Event::Empty(start))?;
                    }
                } else {
                    element(writer, "f", formula.trim_start_matches('='))?;
                }
            }
            if let Some(value) = value {
                if kind == Some("inlineStr") {
                    write_event(writer, Event::Start(BytesStart::new("is")))?;
                    let mut t = BytesStart::new("t");
                    t.push_attribute(("xml:space", "preserve"));
                    write_event(writer, Event::Start(t))?;
                    write_event(writer, Event::Text(BytesText::new(&value)))?;
                    write_event(writer, Event::End(BytesEnd::new("t")))?;
                    write_event(writer, Event::End(BytesEnd::new("is")))?;
                } else {
                    element(writer, "v", &value)?;
                }
            }
            write_event(writer, Event::End(BytesEnd::new("c")))?;
        }
        write_event(writer, Event::End(BytesEnd::new("row")))?;
    }
    Ok(())
}
impl<S: SheetSource> SheetPatch<'_, S> {
    /// Rewrite worksheet XML using canonical cell types and saved results.
    pub(super) fn execute<W: Write>(&self, output: W) -> Result<W, IoError> {
        let Self {
            bytes,
            sheet,
            shared,
            styles,
            stored_formulas,
        } = *self;
        let metadata = style_map(bytes, "cm")?;
        let mut reader = events(bytes);
        let mut writer = Writer::new(output);
        loop {
            let event = reader.read_event().map_err(xml)?;
            match event {
                Event::Start(e) if e.local_name().as_ref() == "sheetData" => {
                    write_event(&mut writer, Event::Start(e.clone().into_owned()))?;
                    reader.read_to_end(e.name()).map_err(xml)?;
                    write_cells(
                        &mut writer,
                        sheet,
                        styles,
                        &metadata,
                        shared,
                        stored_formulas,
                    )?;
                    write_event(&mut writer, Event::End(BytesEnd::new("sheetData")))?;
                }
                Event::Eof => break,
                _ => write_event(&mut writer, event.into_owned())?,
            }
        }
        Ok(writer.into_inner())
    }
}

#[cfg(test)]
mod custom_row_tests {
    use super::*;
    use crate::test_support::Worksheet;

    #[test]
    fn worksheet_encoder_rejects_invalid_custom_rows_without_a_plan() {
        for row in [crate::MAX_ROWS, u32::MAX] {
            let mut source = Worksheet::new("Data").unwrap();
            source.rows.insert(row);
            let context = crate::StyleContext::default();
            let styles = super::super::StyleExport::new(&context).unwrap();
            let shared = super::super::shared::SharedExport::new(&source).unwrap();
            let mut writer = Writer::new(Vec::new());
            let result = write_cells(
                &mut writer,
                &source,
                &styles,
                &BTreeMap::new(),
                &shared,
                &BTreeMap::new(),
            );
            assert!(matches!(result, Err(IoError::Model(_))), "row={row}");
            assert!(writer.into_inner().is_empty());
        }
    }
}
