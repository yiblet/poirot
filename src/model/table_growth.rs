use crate::model::{Cell, CellAddress, CellValue, TableGrowthError, Worksheet, WorksheetTable};
use poirot_parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy};

/// One validated row extension and its staged worksheet formula cells.
pub(super) struct TableGrowth {
    pub(super) index: usize,
    pub(super) table: WorksheetTable,
    pub(super) cells: Vec<(CellAddress, Cell)>,
}

impl TableGrowth {
    /// Validate all source changes before publishing any updated cells or table bounds.
    pub(super) fn prepare(
        sheet: &Worksheet,
        name: &str,
        additional_rows: u32,
    ) -> Result<Self, TableGrowthError> {
        let index = table_index(sheet, name)?;
        let existing = sheet
            .tables()
            .get(index)
            .ok_or_else(|| TableGrowthError::UnknownTable { name: name.into() })?;
        if existing.totals_row {
            return Err(TableGrowthError::TotalsRow);
        }
        let mut table = existing.clone();
        let first_row = table
            .last
            .row
            .checked_add(u32::from(!table.insert_row))
            .ok_or(TableGrowthError::RowBounds)?;
        let remaining_rows = additional_rows
            .checked_sub(1)
            .ok_or(TableGrowthError::RowBounds)?;
        let last_row = first_row
            .checked_add(remaining_rows)
            .filter(|last| *last < crate::model::MAX_ROWS)
            .ok_or(TableGrowthError::RowBounds)?;
        if let Some(filter) = &mut table.auto_filter
            && let Some((_, last)) = &mut filter.range
        {
            let ends_before_placeholder =
                table.insert_row && last.row.checked_add(1) == Some(first_row);
            if last.row != table.last.row && !ends_before_placeholder {
                return Err(TableGrowthError::FilterRange);
            }
            last.row = last_row;
        }
        table.last.row = last_row;
        table.insert_row = false;
        let peers = || {
            sheet
                .tables()
                .iter()
                .enumerate()
                .filter_map(|(peer, table)| (peer != index).then_some(table))
        };
        table.validate(peers(), peers())?;
        check_space(sheet, &table, first_row)?;
        let cells = fill(sheet, &table, first_row, additional_rows)?;
        Ok(Self {
            index,
            table,
            cells,
        })
    }
}

/// Resolve a local table name using the same identifier comparison as formulas.
fn table_index(sheet: &Worksheet, name: &str) -> Result<usize, TableGrowthError> {
    let mut diagnostic = None;
    for (index, table) in sheet.tables().iter().enumerate() {
        match crate::model::identifiers::checked_matches(&table.display_name, name) {
            Ok(true) => return Ok(index),
            Ok(false) => {}
            Err(error) => diagnostic = Some(error),
        }
    }
    Err(diagnostic.map_or_else(
        || TableGrowthError::UnknownTable { name: name.into() },
        Into::into,
    ))
}

/// Refuse source data and saved spill ownership in the appended rectangle.
fn check_space(
    sheet: &Worksheet,
    table: &WorksheetTable,
    first_row: u32,
) -> Result<(), TableGrowthError> {
    for (anchor, last) in sheet.spill_extents() {
        if anchor.row <= table.last.row
            && last.row >= first_row
            && anchor.col <= table.last.col
            && last.col >= table.first.col
        {
            return Err(TableGrowthError::Spill { anchor });
        }
    }
    for (child, anchor) in sheet.spill_children() {
        if (first_row..=table.last.row).contains(&child.row)
            && (table.first.col..=table.last.col).contains(&child.col)
        {
            return Err(TableGrowthError::Spill { anchor });
        }
    }
    for column in table.first.col..=table.last.col {
        for (&address, cell) in sheet.cells().column(column, first_row, table.last.row) {
            if cell.formula.is_some()
                || cell.shared_formula.is_some()
                || cell.value != CellValue::Blank
            {
                return Err(TableGrowthError::Occupied { address });
            }
        }
    }
    Ok(())
}

/// Generate formulas relative to the table's first data row while retaining target styles.
fn fill(
    sheet: &Worksheet,
    table: &WorksheetTable,
    first_row: u32,
    additional_rows: u32,
) -> Result<Vec<(CellAddress, Cell)>, TableGrowthError> {
    let mut formulas = 0usize;
    for (column, definition) in (table.first.col..=table.last.col).zip(&table.columns) {
        if let Some(formula) = &definition.calculated_formula {
            if formula.array == Some(true) {
                return Err(TableGrowthError::ArrayFormula { column });
            }
            formulas = formulas.checked_add(1).ok_or(TableGrowthError::RowBounds)?;
        }
    }
    let mut cells = Vec::new();
    let count = formulas
        .checked_mul(additional_rows as usize)
        .ok_or(TableGrowthError::RowBounds)?;
    cells.try_reserve_exact(count)?;
    let origin = table
        .first
        .row
        .checked_add(u32::from(table.header_row))
        .ok_or(TableGrowthError::RowBounds)?;
    for (column, definition) in (table.first.col..=table.last.col).zip(&table.columns) {
        let Some(definition) = &definition.calculated_formula else {
            continue;
        };
        for row in first_row..=table.last.row {
            let address = CellAddress { row, col: column };
            let translated = TranslationPlan::new(
                &definition.text,
                FormulaLocation {
                    row: origin,
                    column,
                },
                FormulaLocation { row, column },
                TranslationPolicy::default(),
            )
            .map_err(|error| TableGrowthError::Formula { address, error })?;
            let mut cell = Cell::with_formula(translated.replacement(), CellValue::Blank)?;
            if let Some(existing) = sheet.get_validated(address) {
                cell.style = existing.style.clone();
            }
            // SAFETY: The successful reservation covers one staged cell per formula column and new row.
            cells.push((address, cell));
        }
    }
    Ok(cells)
}
