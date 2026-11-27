//! Resolve worksheet-owned table parts and restore their modeled metadata.

use super::package::ImportPackage;
use super::xml::{attr, xml};
use crate::opc::{OpcError, OpcImportPolicy};
use crate::{
    CellAddress, ImportDiagnostic, ImportDiagnosticKind, IoError, ModelError, TableColumn,
    WorksheetTable,
};
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use std::collections::HashSet;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Read modeled tables from the worksheet's declared relationships.
pub(crate) fn read_sheet_tables(
    package: &mut ImportPackage<'_>,
    owner: &str,
    worksheet: &[u8],
    policy: OpcImportPolicy,
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<Vec<WorksheetTable>, IoError> {
    let mut reader = NsReader::from_reader(worksheet);
    reader.config_mut().expand_empty_elements = true;
    let mut ids = Vec::new();
    let mut unique = HashSet::new();
    let mut depth = 0usize;
    let mut seen_parts = false;
    let mut in_parts = false;
    let mut count = None;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(element) => {
                let (namespace, _) = reader.resolver().resolve_element(element.name());
                let in_main = matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == MAIN);
                if in_main && depth == 1 && element.local_name().as_ref() == "tableParts" {
                    if seen_parts {
                        return Err(invalid(owner).into());
                    }
                    seen_parts = true;
                    in_parts = true;
                    count = attr(&element, "count")?
                        .map(|value| value.parse::<usize>().map_err(|_| invalid(owner)))
                        .transpose()?;
                }
                if in_main && in_parts && depth == 2 && element.local_name().as_ref() == "tablePart"
                {
                    let mut id = None;
                    for entry in element.attributes() {
                        let entry = entry.map_err(xml)?;
                        let (namespace, local) = reader.resolver().resolve_attribute(entry.key);
                        if local.as_ref() == "id"
                            && matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == RELATIONSHIPS)
                        {
                            id = Some(
                                entry
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(xml)?
                                    .into_owned(),
                            );
                        }
                    }
                    let id = id
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| invalid(owner))?;
                    if !unique.insert(id.clone()) {
                        return Err(IoError::Opc(OpcError::DuplicateIdentity { identity: id }));
                    }
                    ids.push(id);
                }
                depth = depth.saturating_add(1);
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    in_parts = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if seen_parts && (depth != 0 || count.is_some_and(|count| count != ids.len())) {
        return Err(invalid(owner).into());
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let relationships = package.relationships(owner, warnings, policy, |id| unique.contains(id))?;
    ids.into_iter()
        .map(|id| {
            let selected = relationships
                .get(&id)
                .ok_or_else(|| IoError::Opc(OpcError::MissingRelationship { id: id.clone() }))?;
            if selected.kind.strip_prefix(RELATIONSHIPS) != Some("/table") {
                return Err(invalid(&id).into());
            }
            let path = selected.target.as_deref().ok_or_else(|| {
                IoError::Opc(OpcError::ExternalRelationship {
                    owner: Some(owner.into()),
                    id,
                })
            })?;
            decode_table(&package.read(path)?, path, warnings)
        })
        .collect()
}

/// Decode a table rectangle and its ordered columns, reporting omitted features.
fn decode_table(
    bytes: &[u8],
    path: &str,
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<WorksheetTable, IoError> {
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().expand_empty_elements = true;
    let mut table = None;
    let mut depth = 0usize;
    let mut columns_depth = false;
    let mut seen_columns = false;
    let mut count = None;
    let mut omitted = false;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(element) => {
                let (namespace, _) = reader.resolver().resolve_element(element.name());
                let in_main = matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == MAIN);
                match (depth, element.local_name().as_ref(), in_main) {
                    (0, "table", true) if table.is_none() => {
                        let display_name = required(&element, "displayName", path)?;
                        let name = attr(&element, "name")?;
                        let range = required(&element, "ref", path)?;
                        let (first, last) = range.split_once(':').unwrap_or((&range, &range));
                        table = Some(WorksheetTable {
                            id: number(&element, "id", path)?,
                            name,
                            display_name,
                            first: first.parse::<CellAddress>()?,
                            last: last.parse::<CellAddress>()?,
                            header_row: row_flag(&element, "headerRowCount", true, path)?,
                            totals_row: row_flag(&element, "totalsRowCount", false, path)?,
                            columns: Vec::new(),
                        });
                        omitted |= unmodeled_attributes(
                            &element,
                            &[
                                "id",
                                "name",
                                "displayName",
                                "ref",
                                "headerRowCount",
                                "totalsRowCount",
                            ],
                        )?;
                    }
                    (1, "tableColumns", true) => {
                        if seen_columns {
                            return Err(invalid(path).into());
                        }
                        seen_columns = true;
                        count = attr(&element, "count")?
                            .map(|value| value.parse::<usize>().map_err(|_| invalid(path)))
                            .transpose()?;
                        columns_depth = true;
                    }
                    (2, "tableColumn", true) if columns_depth => {
                        table
                            .as_mut()
                            .ok_or_else(|| invalid(path))?
                            .columns
                            .push(TableColumn {
                                id: number(&element, "id", path)?,
                                name: required(&element, "name", path)?,
                            });
                        omitted |= unmodeled_attributes(&element, &["id", "name"])?;
                    }
                    _ => omitted = true,
                }
                depth = depth.saturating_add(1);
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    columns_depth = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let table = table.ok_or_else(|| invalid(path))?;
    if depth != 0 || !seen_columns || count.is_some_and(|count| count != table.columns.len()) {
        return Err(invalid(path).into());
    }
    if omitted {
        warnings.push(ImportDiagnostic {
            kind: ImportDiagnosticKind::RepresentationLoss,
            detail: format!(
                "unmodeled metadata for Excel table {:?} was omitted",
                table.display_name
            ),
        });
    }
    Ok(table)
}

/// Identify optional table features that cannot be written from the modeled record.
fn unmodeled_attributes(element: &BytesStart<'_>, allowed: &[&str]) -> Result<bool, IoError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(xml)?;
        let key = attribute.key.as_ref();
        if key != "xmlns" && !key.starts_with("xmlns:") && !allowed.contains(&key) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Read a mandatory nonempty attribute.
fn required(element: &BytesStart<'_>, key: &str, path: &str) -> Result<String, IoError> {
    attr(element, key)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(path).into())
}

/// Read a numeric table attribute without a default.
fn number(element: &BytesStart<'_>, key: &str, path: &str) -> Result<u32, IoError> {
    required(element, key, path)?
        .parse()
        .map_err(|_| invalid(path).into())
}

/// Read an optional row count, which must be zero or one.
fn row_flag(
    element: &BytesStart<'_>,
    key: &str,
    default: bool,
    path: &str,
) -> Result<bool, IoError> {
    match attr(element, key)?.as_deref() {
        None => Ok(default),
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(invalid(path).into()),
    }
}

/// Describe malformed table records and relationship references.
fn invalid(name: &str) -> ModelError {
    ModelError::InvalidWorksheetTable { name: name.into() }
}
