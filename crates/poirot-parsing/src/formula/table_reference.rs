use super::table_columns::TableColumns;
use super::table_section::TableSection;

/// Selects a worksheet rectangle by Excel table and column names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReference<T = String> {
    /// Workbook-wide table name, or the table containing the formula.
    pub table: Option<T>,
    /// Selected rows.
    pub section: TableSection,
    /// Selected columns.
    pub columns: TableColumns<T>,
}

impl TableReference {
    /// Decode a bracketed selector with an optional workbook-wide table name.
    pub(super) fn parse(table: Option<String>, input: &str) -> Option<(&str, Self)> {
        let inner = input.strip_prefix('[')?;
        let (rest, section, columns) = if let Some(inner) = inner.strip_prefix('@') {
            let (rest, columns) = current_row(inner)?;
            (rest, TableSection::CurrentRow, columns)
        } else if inner.trim_start().starts_with('[') {
            nested(inner.trim_start())?
        } else {
            let (rest, text, section) = item(input)?;
            let (section, columns) = if section {
                (TableSection::parse(&text)?, TableColumns::All)
            } else {
                (TableSection::Data, TableColumns::One(text))
            };
            (rest, section, columns)
        };
        Some((
            rest,
            Self {
                table,
                section,
                columns,
            },
        ))
    }

    /// Append canonical bracket syntax, escaping special characters in captions.
    pub(super) fn write(&self, out: &mut String) {
        if let Some(table) = &self.table {
            out.push_str(table);
        }
        match (self.section, &self.columns) {
            (section, TableColumns::All) => bracket(section.spelling(), out),
            (TableSection::Data, TableColumns::One(column)) => caption(column, out),
            (section, columns) => {
                out.push('[');
                if section != TableSection::Data {
                    bracket(section.spelling(), out);
                    out.push(',');
                }
                match columns {
                    TableColumns::All => {}
                    TableColumns::One(column) => caption(column, out),
                    TableColumns::Range(first, last) => {
                        caption(first, out);
                        out.push(':');
                        caption(last, out);
                    }
                }
                out.push(']');
            }
        }
    }

    /// Borrow decoded spellings for execution.
    pub fn as_ref(&self) -> TableReference<&str> {
        TableReference {
            table: self.table.as_deref(),
            section: self.section,
            columns: match &self.columns {
                TableColumns::All => TableColumns::All,
                TableColumns::One(column) => TableColumns::One(column),
                TableColumns::Range(first, last) => TableColumns::Range(first, last),
            },
        }
    }
}

/// Read the short current-row selector after its `@` marker.
fn current_row(input: &str) -> Option<(&str, TableColumns)> {
    if let Some(rest) = input.strip_prefix(']') {
        return Some((rest, TableColumns::All));
    }
    if input.starts_with('[') {
        let (rest, first, is_section) = item(input)?;
        return if is_section {
            None
        } else {
            columns(rest, first)
        };
    }
    let (rest, column, is_section) = item_text(input)?;
    (!is_section).then_some((rest, TableColumns::One(column)))
}

/// Read a section or first caption, followed by an optional column range.
fn nested(input: &str) -> Option<(&str, TableSection, TableColumns)> {
    let (rest, first, section) = item(input)?;
    let (rest, section, first) = if section {
        let section = TableSection::parse(&first)?;
        if let Some(rest) = rest.trim_start().strip_prefix(']') {
            return Some((rest, section, TableColumns::All));
        }
        let (rest, column, is_section) = item(rest.trim_start().strip_prefix(',')?.trim_start())?;
        if is_section {
            return None;
        }
        (rest, section, column)
    } else {
        (rest, TableSection::Data, first)
    };
    let (rest, columns) = columns(rest, first)?;
    Some((rest, section, columns))
}

/// Read the remaining column selection and the closing outer bracket.
fn columns(input: &str, first: String) -> Option<(&str, TableColumns)> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix(':') {
        let (rest, last, is_section) = item(rest.trim_start())?;
        if is_section {
            return None;
        }
        Some((
            rest.trim_start().strip_prefix(']')?,
            TableColumns::Range(first, last),
        ))
    } else {
        Some((input.strip_prefix(']')?, TableColumns::One(first)))
    }
}

/// Read one bracketed item with apostrophe escapes and report a section marker.
fn item(input: &str) -> Option<(&str, String, bool)> {
    item_text(input.strip_prefix('[')?)
}

/// Read a selector up to its closing bracket.
fn item_text(input: &str) -> Option<(&str, String, bool)> {
    let mut chars = input.chars();
    let section = input.starts_with('#');
    let mut text = String::new();
    while let Some(character) = chars.next() {
        match character {
            ']' => return (!text.is_empty()).then_some((chars.as_str(), text, section)),
            '\'' => {
                let escaped = chars.next()?;
                if !matches!(escaped, '[' | ']' | '#' | '\'' | '@') {
                    return None;
                }
                text.push(escaped);
            }
            '[' | '@' => return None,
            '#' if !section || !text.is_empty() => return None,
            other => text.push(other),
        }
    }
    None
}

/// Emit one fixed selector.
fn bracket(text: &str, out: &mut String) {
    out.push('[');
    out.push_str(text);
    out.push(']');
}

/// Emit one column selector, keeping its text opaque to formula translation.
fn caption(text: &str, out: &mut String) {
    out.push('[');
    for character in text.chars() {
        if matches!(character, '[' | ']' | '#' | '\'' | '@') {
            out.push('\'');
        }
        out.push(character);
    }
    out.push(']');
}
