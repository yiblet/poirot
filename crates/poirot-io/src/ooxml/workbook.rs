//! Borrowed workbook metadata projection and XML execution.
use super::xml::{events, write_event, xml};
use crate::IoError;
use poirot_model::{DateSystem, Workbook};
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use std::io::Write;

/// Borrowed backend workbook XML plus canonical source metadata to restore.
pub(super) struct WorkbookPatch<'a> {
    /// Original backend-generated workbook XML.
    pub(super) bytes: &'a [u8],
    /// Canonical source supplying names and workbook metadata.
    pub(super) workbook: &'a Workbook,
}

fn write_names<W: Write>(writer: &mut Writer<W>, workbook: &Workbook) -> Result<(), IoError> {
    if workbook.named_ranges.is_empty() {
        return Ok(());
    }
    write_event(writer, Event::Start(BytesStart::new("definedNames")))?;
    for name in &workbook.named_ranges {
        let mut start = BytesStart::new("definedName");
        start.push_attribute(("name", name.name.as_str()));
        let scope = name.sheet_scope.map(|i| i.to_string());
        if let Some(scope) = &scope {
            start.push_attribute(("localSheetId", scope.as_str()));
        }
        write_event(writer, Event::Start(start))?;
        write_event(writer, Event::Text(BytesText::new(&name.formula)))?;
        write_event(writer, Event::End(BytesEnd::new("definedName")))?;
    }
    write_event(writer, Event::End(BytesEnd::new("definedNames")))
}
impl WorkbookPatch<'_> {
    /// Rewrite workbook XML with the canonical metadata selected for export.
    pub(super) fn execute<W: Write>(&self, output: W) -> Result<W, IoError> {
        let Self { bytes, workbook } = *self;
        let mut reader = events(bytes);
        let mut writer = Writer::new(output);
        let mut names = false;
        loop {
            let event = reader.read_event().map_err(xml)?;
            match &event {
                Event::Start(e) if e.local_name().as_ref() == b"workbookPr" => {
                    let mut start =
                        BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
                    for a in e.attributes() {
                        let a = a.map_err(xml)?;
                        if a.key.as_ref() != b"date1904" {
                            start.push_attribute(a);
                        }
                    }
                    start.push_attribute((
                        "date1904",
                        if workbook.date_system == DateSystem::Excel1904 {
                            "1"
                        } else {
                            "0"
                        },
                    ));
                    write_event(&mut writer, Event::Start(start))?;
                    continue;
                }
                Event::Start(e) if e.local_name().as_ref() == b"calcPr" && !names => {
                    write_names(&mut writer, workbook)?;
                    names = true;
                }
                Event::End(e) if e.local_name().as_ref() == b"workbook" && !names => {
                    write_names(&mut writer, workbook)?;
                    names = true;
                }
                Event::Eof => break,
                _ => {}
            }
            write_event(&mut writer, event.into_owned())?;
        }
        Ok(writer.into_inner())
    }
}
