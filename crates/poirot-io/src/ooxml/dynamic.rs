//! Dynamic-array metadata absent from rxls fresh authoring.
use super::{
    package::Package,
    xml::{events, write_event, xml},
};
use crate::IoError;
use quick_xml::{
    Writer,
    events::{BytesStart, Event},
};

/// Add the dynamic-array cell metadata and its package declarations.
/// Structure follows MS-XLSX CT_DynamicArrayProperties and SpreadsheetML
/// CT_Metadata; <https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/>
/// and <https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.metadata>.
/// Original implementation; no upstream code or fixtures reused. All spill anchors
/// refer to the same reusable metadata record.
pub(super) fn add_metadata(package: &mut Package) -> Result<(), IoError> {
    package.insert("xl/metadata.xml".into(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray" fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#.to_vec());
    for (path, root, tag, attributes) in [
        (
            "[Content_Types].xml",
            "Types",
            "Override",
            vec![
                ("PartName", "/xl/metadata.xml"),
                (
                    "ContentType",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml",
                ),
            ],
        ),
        (
            "xl/_rels/workbook.xml.rels",
            "Relationships",
            "Relationship",
            vec![
                ("Id", "poirotDynamicMetadata"),
                (
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata",
                ),
                ("Target", "metadata.xml"),
            ],
        ),
    ] {
        let source = package
            .get(path)
            .ok_or_else(|| IoError::Xml(format!("missing generated part {path}")))?;
        let mut reader = events(source);
        let mut writer = Writer::new(Vec::new());
        loop {
            let event = reader.read_event().map_err(xml)?;
            if let Event::End(end) = &event
                && end.local_name().as_ref() == root
            {
                let mut start = BytesStart::new(tag);
                for attribute in &attributes {
                    start.push_attribute(*attribute);
                }
                write_event(&mut writer, Event::Empty(start))?;
            }
            if matches!(event, Event::Eof) {
                break;
            }
            write_event(&mut writer, event.into_owned())?;
        }
        package.insert(path.into(), writer.into_inner());
    }
    Ok(())
}
