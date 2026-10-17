//! Sparse row and typed cell-cache projection into streaming worksheet XML.
use super::xml::{attr, element, events, write_event, xml};
use crate::IoError;
use poirot_model::{CellAddress, CellValue, Worksheet};
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use std::{collections::BTreeMap, io::Write};

/// Borrowed worksheet XML and canonical cells used to restore typed saved values.
pub(super) struct SheetPatch<'a> {
    /// Original backend-generated worksheet XML.
    pub(super) bytes: &'a [u8],
    /// Canonical source sheet supplying formulas, caches, and metadata.
    pub(super) sheet: &'a Worksheet,
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

fn style_map(bytes: &[u8], key: &[u8]) -> Result<BTreeMap<CellAddress, String>, IoError> {
    let mut result = BTreeMap::new();
    let mut reader = events(bytes);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == b"c" => {
                if let (Some(r), Some(s)) = (attr(&reader, &e, b"r")?, attr(&reader, &e, key)?) {
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
    sheet: &Worksheet,
    styles: &BTreeMap<CellAddress, String>,
    metadata: &BTreeMap<CellAddress, String>,
) -> Result<(), IoError> {
    let spills = super::spill::bounds(sheet)?;
    for index in sheet.row_indices() {
        let mut start = BytesStart::new("row");
        let number = (index + 1).to_string();
        start.push_attribute(("r", number.as_str()));
        write_event(writer, Event::Start(start))?;
        for (column, cell) in sheet
            .row(index)
            .ok_or_else(|| crate::IoError::Unsupported("missing source row".into()))?
            .cells()
        {
            let address = &CellAddress {
                row: index,
                col: column,
            };
            let mut start = BytesStart::new("c");
            let reference = address.to_string();
            start.push_attribute(("r", reference.as_str()));
            if let Some(style) = styles.get(address) {
                start.push_attribute(("s", style.as_str()));
            }
            if let Some(index) = metadata.get(address) {
                start.push_attribute(("cm", index.as_str()));
            }
            let CellEncoding { kind, value } =
                CellEncoding::new(&cell.value, cell.formula.is_some())?;
            if let Some(kind) = kind {
                start.push_attribute(("t", kind));
            }
            write_event(writer, Event::Start(start))?;
            if let Some(formula) = &cell.formula {
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
impl SheetPatch<'_> {
    /// Rewrite worksheet XML using canonical cell types and saved results.
    pub(super) fn execute<W: Write>(&self, output: W) -> Result<W, IoError> {
        let Self { bytes, sheet } = *self;
        let styles = style_map(bytes, b"s")?;
        let metadata = style_map(bytes, b"cm")?;
        let mut reader = events(bytes);
        let mut writer = Writer::new(output);
        loop {
            let event = reader.read_event().map_err(xml)?;
            match event {
                Event::Start(e) if e.local_name().as_ref() == b"sheetData" => {
                    write_event(&mut writer, Event::Start(e.clone().into_owned()))?;
                    reader.read_to_end(e.name()).map_err(xml)?;
                    write_cells(&mut writer, sheet, &styles, &metadata)?;
                    write_event(&mut writer, Event::End(BytesEnd::new("sheetData")))?;
                }
                Event::Eof => break,
                _ => write_event(&mut writer, event.into_owned())?,
            }
        }
        Ok(writer.into_inner())
    }
}
