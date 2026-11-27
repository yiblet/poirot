use super::read_recorder::ReadRecorder;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvalResult, EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::composition::Session;
use crate::eval::values::{Area, AreaKind, ReferenceAreas, Value, err, identifier_failure};
use crate::model::{CellAddress, CellError, WorksheetTable};
use poirot_parsing::formula::{TableColumns, TableReference, TableSection};

/// Resolves workbook table selectors to ordinary worksheet areas.
pub(in crate::eval) struct TableResolver<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> TableResolver<'s, 'a> {
    /// Borrow the session whose workbook contains the imported table definitions.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Resolve a structured reference into the ordinary worksheet area channel.
    pub(in crate::eval) fn resolve(
        &mut self,
        target: EvaluationTarget,
        reference: TableReference<&str>,
    ) -> EvalResult<Option<Value>> {
        let matched = match reference.table {
            Some(name) => self.named_table(name)?,
            None => self
                .enclosing_table(target.sheet, target.address)?
                .map(|table| (target.sheet, table)),
        };
        let Some((sheet, table)) = matched else {
            return Ok(reference.table.is_none().then_some(err(CellError::Value)));
        };
        let (r0, r1) = match table_rows(table, reference.section, target.address.row) {
            Ok(rows) => rows,
            Err(error) => return Ok(Some(err(error))),
        };
        let (c0, c1) = match reference.columns {
            TableColumns::All => (table.first.col, table.last.col),
            TableColumns::One(name) => {
                let Some(column) = self.table_column(table, name)? else {
                    return Ok(Some(err(CellError::Ref)));
                };
                (column, column)
            }
            TableColumns::Range(first, last) => {
                let Some(first) = self.table_column(table, first)? else {
                    return Ok(Some(err(CellError::Ref)));
                };
                let Some(last) = self.table_column(table, last)? else {
                    return Ok(Some(err(CellError::Ref)));
                };
                if last < first {
                    return Ok(Some(err(CellError::Ref)));
                }
                (first, last)
            }
        };
        self.session.state.runtime.budget.allocation(1)?;
        Ok(Some(Value::Refs(ReferenceAreas::One(Area {
            kind: AreaKind::Area,
            sheet,
            r0,
            r1,
            c0,
            c1,
        }))))
    }

    /// Select a workbook-wide table by display name using the checked identifier policy.
    fn named_table(&mut self, name: &str) -> EvalResult<Option<(usize, &'a WorksheetTable)>> {
        let budget = &mut self.session.state.runtime.budget;
        let mut requested_text = Some(name);
        let mut matched = None;
        let mut unresolved = None;
        'sheets: for (index, sheet) in self.session.sources.workbook.sheets().iter().enumerate() {
            for table in sheet.tables() {
                if let Some(text) = requested_text.take() {
                    budget.text(text.len())?;
                }
                budget.charge(WorkKind::NamespaceScan, 1)?;
                match crate::model::identifiers::checked_matches(&table.display_name, name) {
                    Ok(true) => {
                        matched = Some((index, table));
                        break 'sheets;
                    }
                    Ok(false) => {}
                    Err(error) => unresolved = Some(error),
                }
            }
        }
        if requested_text.is_none() {
            ReadRecorder::new(self.session).read_lazy(|| DependencyRead::SheetLayout);
        }
        if matched.is_some() {
            return Ok(matched);
        }
        unresolved.map_or(Ok(None), |error| Err(identifier_failure(error)))
    }

    /// Select the table that contains the actual formula address on its worksheet.
    fn enclosing_table(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<Option<&'a WorksheetTable>> {
        let workbook = self.session.sources.workbook;
        let Some(worksheet) = workbook.sheet(sheet) else {
            return Ok(None);
        };
        let tables = worksheet.tables();
        if !tables.is_empty() {
            ReadRecorder::new(self.session).read_lazy(|| DependencyRead::SheetLayout);
        }
        for table in tables {
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::NamespaceScan, 1)?;
            if (table.first.row..=table.last.row).contains(&address.row)
                && (table.first.col..=table.last.col).contains(&address.col)
            {
                return Ok(Some(table));
            }
        }
        Ok(None)
    }

    /// Translate a column caption to its sheet column with metered identifier lookup.
    fn table_column(&mut self, table: &WorksheetTable, caption: &str) -> EvalResult<Option<u32>> {
        let budget = &mut self.session.state.runtime.budget;
        budget.text(caption.len())?;
        let mut unresolved = None;
        for (index, column) in table.columns.iter().enumerate() {
            budget.charge(WorkKind::NamespaceScan, 1)?;
            match crate::model::identifiers::checked_matches(&column.name, caption) {
                Ok(true) => {
                    let index = u32::try_from(index).map_err(|_| EvaluationError::StalePlan)?;
                    return Ok(Some(
                        table
                            .first
                            .col
                            .checked_add(index)
                            .ok_or(EvaluationError::StalePlan)?,
                    ));
                }
                Ok(false) => {}
                Err(error) => unresolved = Some(error),
            }
        }
        unresolved.map_or(Ok(None), |error| Err(identifier_failure(error)))
    }
}

/// Return the selected table rows and distinguish an invalid caller row.
fn table_rows(
    table: &WorksheetTable,
    section: TableSection,
    row: u32,
) -> Result<(u32, u32), CellError> {
    match section {
        TableSection::All => Ok((table.first.row, table.last.row)),
        TableSection::Headers => table
            .header_row
            .then_some((table.first.row, table.first.row))
            .ok_or(CellError::Ref),
        TableSection::Totals => table
            .totals_row
            .then_some((table.last.row, table.last.row))
            .ok_or(CellError::Ref),
        TableSection::Data => data_rows(table).ok_or(CellError::Ref),
        TableSection::CurrentRow => data_rows(table)
            .filter(|(first, last)| (*first..=*last).contains(&row))
            .map(|_| (row, row))
            .ok_or(CellError::Value),
    }
}

/// Return the exclusive-of-header-and-totals span when it contains a data row.
fn data_rows(table: &WorksheetTable) -> Option<(u32, u32)> {
    let first = table.first.row.checked_add(u32::from(table.header_row))?;
    let last = table.last.row.checked_sub(u32::from(table.totals_row))?;
    (first <= last).then_some((first, last))
}
