//! Transient checked-part adaptation for the pinned reader backend.
use super::{
    package::Package,
    xml::{attr, element, events, part, text, write_event, xml},
};
use crate::IoError;
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, Event},
};
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

/// Adapt checked part identities to the pinned backend's conventional metadata paths.
/// Original parts are retained only inside this transient decoding package, not as preservation.
pub(crate) fn backend_package(
    package: &Package,
    plan: &crate::opc::OpcReadPlan,
) -> Result<Vec<u8>, IoError> {
    let mut adapted = package.clone();
    adapted.insert(
        "xl/workbook.xml".into(),
        part(package, &plan.workbook_part)?.to_vec(),
    );
    for (source, target) in [
        (&plan.styles_part, "xl/styles.xml"),
        (&plan.shared_strings_part, "xl/sharedStrings.xml"),
    ] {
        adapted.remove(target);
        if let Some(source) = source {
            adapted.insert(target.into(), part(package, source)?.to_vec());
        }
    }
    // Calamine's legacy error enum cannot decode modern #SPILL!/#CALC!.
    // Substitute only in this transient backend package; authoritative original
    // error spellings are restored by apply_spills after decoding.
    for sheet in plan.sheets.iter().filter(|s| s.supported) {
        let bytes = part(package, &sheet.part)?;
        let mut reader = events(bytes);
        let mut writer = Writer::new(Vec::new());
        let mut error_cell = false;
        loop {
            let event = reader.read_event().map_err(xml)?;
            match event {
                Event::Start(ref e) if e.local_name().as_ref() == b"c" => {
                    error_cell = attr(&reader, e, b"t")?.as_deref() == Some("e");
                    write_event(&mut writer, event.into_owned())?;
                }
                Event::Start(ref e) if error_cell && e.local_name().as_ref() == b"v" => {
                    let raw = text(&mut reader, e.name())?;
                    element(
                        &mut writer,
                        "v",
                        if matches!(raw.as_str(), "#SPILL!" | "#CALC!") {
                            "#VALUE!"
                        } else {
                            &raw
                        },
                    )?;
                }
                Event::Eof => break,
                _ => write_event(&mut writer, event.into_owned())?,
            }
        }
        adapted.insert(sheet.part.clone(), writer.into_inner());
    }
    let mut writer = Writer::new(Vec::new());
    let mut root = BytesStart::new("Relationships");
    root.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    write_event(&mut writer, Event::Start(root))?;
    for sheet in &plan.sheets {
        let mut rel = BytesStart::new("Relationship");
        rel.push_attribute(("Id", sheet.relationship_id.as_str()));
        rel.push_attribute((
            "Type",
            if sheet.supported {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"
            } else {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet"
            },
        ));
        let target = format!("/{}", sheet.part);
        rel.push_attribute(("Target", target.as_str()));
        write_event(&mut writer, Event::Empty(rel))?;
    }
    write_event(&mut writer, Event::End(BytesEnd::new("Relationships")))?;
    adapted.insert("xl/_rels/workbook.xml.rels".into(), writer.into_inner());
    adapted.insert("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="poirotDocument" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in adapted {
        zip.start_file(name, SimpleFileOptions::default())
            .map_err(xml)?;
        zip.write_all(&bytes)?;
    }
    Ok(zip.finish().map_err(xml)?.into_inner())
}
