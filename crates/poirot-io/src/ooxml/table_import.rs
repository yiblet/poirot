//! Resolve worksheet-owned table parts and restore their modeled metadata.

use super::package::ImportPackage;
use super::text;
use super::xml::{attr, xml};
use crate::opc::{OpcError, OpcImportPolicy};
use crate::{
    CalculatedColumnFormula, CellAddress, ImportDiagnostic, ImportDiagnosticKind, IoError,
    ModelError, TableAutoFilter, TableColumn, TableFilterColumn, TableFilterValues, TableStyleInfo,
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
    let mut in_column = false;
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
                        let display_name = escaped_attr(&element, "displayName", path)?
                            .ok_or_else(|| invalid(path))?;
                        let name = escaped_attr(&element, "name", path)?;
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
                            insert_row: flag(&element, "insertRow", path)?.unwrap_or(false),
                            columns: Vec::new(),
                            style: None,
                            auto_filter: None,
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
                                "insertRow",
                            ],
                        )?;
                    }
                    (1, "tableStyleInfo", true) => {
                        let record = table.as_mut().ok_or_else(|| invalid(path))?;
                        if record.style.is_some() {
                            return Err(invalid(path).into());
                        }
                        record.style = Some(TableStyleInfo {
                            name: escaped_attr(&element, "name", path)?,
                            show_first_column: flag(&element, "showFirstColumn", path)?,
                            show_last_column: flag(&element, "showLastColumn", path)?,
                            show_row_stripes: flag(&element, "showRowStripes", path)?,
                            show_column_stripes: flag(&element, "showColumnStripes", path)?,
                        });
                        omitted |= unmodeled_attributes(
                            &element,
                            &[
                                "name",
                                "showFirstColumn",
                                "showLastColumn",
                                "showRowStripes",
                                "showColumnStripes",
                            ],
                        )?;
                    }
                    (1, "autoFilter", true) => {
                        let record = table.as_mut().ok_or_else(|| invalid(path))?;
                        if record.auto_filter.is_some() {
                            return Err(invalid(path).into());
                        }
                        record.auto_filter =
                            Some(read_filter(&mut reader, &element, path, &mut omitted)?);
                        continue;
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
                                name: escaped_attr(&element, "name", path)?
                                    .ok_or_else(|| invalid(path))?,
                                calculated_formula: None,
                            });
                        in_column = true;
                        omitted |= unmodeled_attributes(&element, &["id", "name"])?;
                    }
                    (3, "calculatedColumnFormula", true) if in_column => {
                        let column = table
                            .as_mut()
                            .and_then(|table| table.columns.last_mut())
                            .ok_or_else(|| invalid(path))?;
                        if column.calculated_formula.is_some() {
                            return Err(invalid(path).into());
                        }
                        column.calculated_formula = Some(CalculatedColumnFormula {
                            array: flag(&element, "array", path)?,
                            text: formula_text(&mut reader, path)?,
                        });
                        omitted |= unmodeled_attributes(&element, &["array", "xml:space"])?;
                        continue;
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
                if depth == 2 {
                    in_column = false;
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

/// Decode a modeled ST_Xstring attribute into Unicode model text.
fn escaped_attr(
    element: &BytesStart<'_>,
    key: &str,
    path: &str,
) -> Result<Option<String>, IoError> {
    let mut value = attr(element, key)?;
    if let Some(value) = &mut value
        && value.contains("_x")
    {
        *value = text::decode(value).to_utf8().map_err(|_| invalid(path))?;
    }
    Ok(value)
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

/// Decode the defined bool spellings while retaining absence of the attribute.
fn flag(element: &BytesStart<'_>, key: &str, path: &str) -> Result<Option<bool>, IoError> {
    match attr(element, key)?.as_deref() {
        Some("1" | "true") => Ok(Some(true)),
        Some("0" | "false") => Ok(Some(false)),
        None => Ok(None),
        Some(_) => Err(invalid(path).into()),
    }
}

/// Read text-only formula definitions while decoding predefined XML entities.
fn formula_text(reader: &mut NsReader<&[u8]>, path: &str) -> Result<String, IoError> {
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Text(value) => text.push_str(&value.xml10_content()),
            Event::CData(value) => text.push_str(&value.xml10_content()),
            Event::GeneralRef(reference) => {
                if let Some(value) = reference.resolve_char_ref().map_err(xml)? {
                    text.push(value);
                } else {
                    let value = quick_xml::escape::resolve_predefined_entity(reference.as_ref())
                        .ok_or_else(|| invalid(path))?;
                    text.push_str(value);
                }
            }
            Event::End(_) => return Ok(text),
            Event::Comment(_) | Event::PI(_) => {}
            _ => return Err(invalid(path).into()),
        }
    }
}

/// Return whether a direct filter child has the SpreadsheetML namespace and name.
fn is_main(reader: &NsReader<&[u8]>, element: &BytesStart<'_>, name: &str) -> bool {
    element.local_name().as_ref() == name
        && matches!(reader.resolver().resolve_element(element.name()).0,
            ResolveResult::Bound(namespace) if namespace.as_ref() == MAIN)
}

/// Consume the AutoFilter part, retaining even an empty filter-control record.
fn read_filter(
    reader: &mut NsReader<&[u8]>,
    root: &BytesStart<'_>,
    path: &str,
    omitted: &mut bool,
) -> Result<TableAutoFilter, IoError> {
    let range = attr(root, "ref")?
        .map(|range| {
            let (first, last) = range.split_once(':').unwrap_or((&range, &range));
            Ok::<_, IoError>((first.parse()?, last.parse()?))
        })
        .transpose()?;
    let mut filter = TableAutoFilter {
        range,
        columns: Vec::new(),
    };
    *omitted |= unmodeled_attributes(root, &["ref"])?;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(element) if is_main(reader, &element, "filterColumn") => {
                filter
                    .columns
                    .push(read_filter_column(reader, &element, path, omitted)?);
            }
            Event::Start(element) => {
                *omitted = true;
                reader.read_to_end(element.name()).map_err(xml)?;
            }
            Event::End(_) => return Ok(filter),
            Event::Eof => return Err(invalid(path).into()),
            _ => {}
        }
    }
}

/// Read filter-button settings and the supported value selection.
fn read_filter_column(
    reader: &mut NsReader<&[u8]>,
    root: &BytesStart<'_>,
    path: &str,
    omitted: &mut bool,
) -> Result<TableFilterColumn, IoError> {
    let mut column = TableFilterColumn {
        index: number(root, "colId", path)?,
        hidden_button: flag(root, "hiddenButton", path)?,
        show_button: flag(root, "showButton", path)?,
        filters: None,
    };
    let mut seen_filters = false;
    let mut lost = false;
    *omitted |= unmodeled_attributes(root, &["colId", "hiddenButton", "showButton"])?;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(element) if is_main(reader, &element, "filters") => {
                if seen_filters {
                    return Err(invalid(path).into());
                }
                seen_filters = true;
                column.filters = read_filter_values(reader, &element, path)?;
                lost |= column.filters.is_none();
            }
            Event::Start(element) => {
                lost = true;
                reader.read_to_end(element.name()).map_err(xml)?;
            }
            Event::End(_) => {
                if lost {
                    *omitted = true;
                    column.filters = None;
                }
                return Ok(column);
            }
            Event::Eof => return Err(invalid(path).into()),
            _ => {}
        }
    }
}

/// Avoid changing a filter's meaning by preserving only its literal-value portion.
fn read_filter_values(
    reader: &mut NsReader<&[u8]>,
    root: &BytesStart<'_>,
    path: &str,
) -> Result<Option<TableFilterValues>, IoError> {
    let mut values = TableFilterValues {
        blank: flag(root, "blank", path)?,
        values: Vec::new(),
    };
    let mut lost = unmodeled_attributes(root, &["blank"])?;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(element) if is_main(reader, &element, "filter") => {
                values
                    .values
                    .push(escaped_attr(&element, "val", path)?.ok_or_else(|| invalid(path))?);
                lost |= unmodeled_attributes(&element, &["val"])?;
                lost |= !reader.read_to_end(element.name()).map_err(xml)?.is_empty();
            }
            Event::Start(element) => {
                lost = true;
                reader.read_to_end(element.name()).map_err(xml)?;
            }
            Event::End(_) => return Ok((!lost).then_some(values)),
            Event::Eof => return Err(invalid(path).into()),
            _ => {}
        }
    }
}
