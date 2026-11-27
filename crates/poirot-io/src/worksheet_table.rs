use crate::{CellAddress, ModelError, TableColumn};

/// File metadata describing one fixed rectangular Excel worksheet table.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorksheetTable {
    /// Positive ID unique within the workbook.
    pub id: u32,
    /// Optional internal table name.
    pub name: Option<String>,
    /// Workbook-wide name used in formulas.
    pub display_name: String,
    /// Upper-left coordinate of the complete table.
    pub first: CellAddress,
    /// Lower-right coordinate of the complete table.
    pub last: CellAddress,
    /// Whether the first row is a header.
    pub header_row: bool,
    /// Whether the last row contains totals.
    pub totals_row: bool,
    /// Columns in worksheet order.
    pub columns: Vec<TableColumn>,
}

impl WorksheetTable {
    /// Check geometry, identity and uniqueness against the supplied tables.
    pub fn validate<'a>(
        &self,
        workbook_tables: impl IntoIterator<Item = &'a Self>,
        sheet_tables: impl IntoIterator<Item = &'a Self>,
    ) -> Result<(), ModelError> {
        self.first.validate()?;
        self.last.validate()?;
        let invalid = || ModelError::InvalidWorksheetTable {
            name: self.display_name.clone(),
        };
        let width = self
            .last
            .col
            .checked_sub(self.first.col)
            .and_then(|delta| delta.checked_add(1))
            .ok_or_else(invalid)?;
        let height = self
            .last
            .row
            .checked_sub(self.first.row)
            .and_then(|delta| delta.checked_add(1))
            .ok_or_else(invalid)?;
        if self.id == 0
            || self.name.as_deref().is_some_and(str::is_empty)
            || self.display_name.is_empty()
            || (width as usize) != self.columns.len()
            || height < u32::from(self.header_row) + u32::from(self.totals_row)
            || sheet_tables.into_iter().any(|other| {
                self.first.row <= other.last.row
                    && other.first.row <= self.last.row
                    && self.first.col <= other.last.col
                    && other.first.col <= self.last.col
            })
            || workbook_tables.into_iter().any(|other| {
                other.id == self.id
                    || crate::identifiers::matches(&other.display_name, &self.display_name)
                    || other
                        .name
                        .as_deref()
                        .zip(self.name.as_deref())
                        .is_some_and(|(other, name)| crate::identifiers::matches(other, name))
            })
        {
            return Err(invalid());
        }
        let mut ids = std::collections::HashSet::new();
        let mut captions = std::collections::HashSet::new();
        for column in &self.columns {
            if column.id == 0
                || column.name.is_empty()
                || !ids.insert(column.id)
                || !captions.insert(crate::identifiers::known_case_key(&column.name))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
