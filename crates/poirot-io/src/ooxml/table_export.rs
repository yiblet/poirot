//! Install modeled table definitions and their OPC linkage in fresh XLSX output.

use super::budget::next_total;
use super::package::Package;
use super::xml::{events, part, write_event, xml};
use crate::{IoError, WorksheetTable, WriteOptions};
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, Event},
};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const TABLE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const PACKAGE_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const TABLE_CONTENT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";

/// Attach table parts to an already projected worksheet without changing its cells.
pub(super) fn install_tables(
    package: &mut Package,
    owner: &str,
    tables: &[WorksheetTable],
    options: WriteOptions,
    total: &mut u64,
) -> Result<(), IoError> {
    if tables.is_empty() {
        return Ok(());
    }
    let relationships_path = crate::opc::relationship_part(owner);
    let relationships = if package.contains_key(&relationships_path) {
        part(package, &relationships_path)?.to_vec()
    } else {
        format!(r#"<Relationships xmlns="{PACKAGE_RELATIONSHIPS}"></Relationships>"#).into_bytes()
    };
    let mut worksheet = Writer::new(Vec::new());
    let mut relationships_xml = Writer::new(Vec::new());
    let mut declarations = Writer::new(Vec::new());
    let count = tables.len().to_string();
    let mut start = BytesStart::new("tableParts");
    start.push_attribute(("count", count.as_str()));
    start.push_attribute(("xmlns:r", RELATIONSHIPS));
    write_event(&mut worksheet, Event::Start(start))?;
    for table in tables {
        let id = format!("poirotTable{}", table.id);
        let target = format!("../tables/table{}.xml", table.id);
        let table_path = crate::opc::resolve_target(owner, &target).map_err(IoError::Opc)?;
        let override_path = format!("/{table_path}");
        let mut tag = BytesStart::new("tablePart");
        tag.push_attribute(("r:id", id.as_str()));
        write_event(&mut worksheet, Event::Empty(tag))?;

        let mut relationship = BytesStart::new("Relationship");
        relationship.push_attribute(("Id", id.as_str()));
        relationship.push_attribute(("Type", TABLE_RELATIONSHIP));
        relationship.push_attribute(("Target", target.as_str()));
        write_event(&mut relationships_xml, Event::Empty(relationship))?;
        replace(package, table_path, encode(table)?, options, total)?;

        let mut declaration = BytesStart::new("Override");
        declaration.push_attribute(("PartName", override_path.as_str()));
        declaration.push_attribute(("ContentType", TABLE_CONTENT));
        write_event(&mut declarations, Event::Empty(declaration))?;
    }
    write_event(&mut worksheet, Event::End(BytesEnd::new("tableParts")))?;
    let bytes = append(part(package, owner)?, "worksheet", &worksheet.into_inner())?;
    replace(package, owner.to_owned(), bytes, options, total)?;
    let relationships = append(
        &relationships,
        "Relationships",
        &relationships_xml.into_inner(),
    )?;
    replace(package, relationships_path, relationships, options, total)?;
    let declaration_path = "[Content_Types].xml";
    let content = append(
        part(package, declaration_path)?,
        "Types",
        &declarations.into_inner(),
    )?;
    replace(package, declaration_path.into(), content, options, total)
}

/// Encode the persisted table and column identities without changing saved cell data.
fn encode(table: &WorksheetTable) -> Result<Vec<u8>, IoError> {
    let mut writer = Writer::new(Vec::new());
    let id = table.id.to_string();
    let address = format!("{}:{}", table.first, table.last);
    let mut root = BytesStart::new("table");
    if let Some(name) = &table.name {
        root.push_attribute(("name", name.as_str()));
    }
    for attribute in [
        ("xmlns", MAIN),
        ("id", id.as_str()),
        ("displayName", table.display_name.as_str()),
        ("ref", address.as_str()),
        ("headerRowCount", if table.header_row { "1" } else { "0" }),
        ("totalsRowCount", if table.totals_row { "1" } else { "0" }),
    ] {
        root.push_attribute(attribute);
    }
    write_event(&mut writer, Event::Start(root))?;
    let mut columns = BytesStart::new("tableColumns");
    let count = table.columns.len().to_string();
    columns.push_attribute(("count", count.as_str()));
    write_event(&mut writer, Event::Start(columns))?;
    for column in &table.columns {
        let mut node = BytesStart::new("tableColumn");
        let id = column.id.to_string();
        node.push_attribute(("id", id.as_str()));
        node.push_attribute(("name", column.name.as_str()));
        write_event(&mut writer, Event::Empty(node))?;
    }
    write_event(&mut writer, Event::End(BytesEnd::new("tableColumns")))?;
    write_event(&mut writer, Event::End(BytesEnd::new("table")))?;
    Ok(writer.into_inner())
}

/// Append one generated payload before the known root's ending tag.
fn append(bytes: &[u8], root: &str, payload: &[u8]) -> Result<Vec<u8>, IoError> {
    let mut reader = events(bytes);
    let mut writer = Writer::new(Vec::new());
    use std::io::Write;
    loop {
        let event = reader.read_event().map_err(xml)?;
        match &event {
            Event::End(end) if end.local_name().as_ref() == root => {
                writer.get_mut().write_all(payload).map_err(xml)?;
            }
            Event::Eof => break,
            _ => {}
        }
        write_event(&mut writer, event.into_owned())?;
    }
    Ok(writer.into_inner())
}

/// Account for generated XLSX table bytes before replacing the package entry.
fn replace(
    package: &mut Package,
    path: String,
    bytes: Vec<u8>,
    options: WriteOptions,
    total: &mut u64,
) -> Result<(), IoError> {
    let previous = package.get(&path).map_or(0, |part| part.len() as u64);
    *total = next_total(bytes.len() as u64, previous, *total, options)?;
    package.insert(path, bytes);
    Ok(())
}
