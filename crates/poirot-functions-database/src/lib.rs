//! Typed database aggregation, following POI DStarRunner and its twelve algorithms.
//! All grid cells have reference origin: blanks remain blanks, and numeric text
//! is ignored by aggregators. Boolean cells are coercev2_database_number (POI NumericValueEval).
#[cfg(test)]
use poirot_coercionv2::compatibility::coercev2_java_number_text;
use poirot_functions_core::{CellError, CellValue};
mod aggregation;
mod criteria;
mod estimate;
/// Numeric comparison and text conversion used by database criteria.
pub mod number;
mod selection;

/// Database criteria and field-selection helpers shared with host adapters.
pub use criteria::condition_matches;
/// Database criteria and field-selection helpers shared with host adapters.
pub use selection::{
    coercev2_database_criteria_header_index, coercev2_database_field_index,
    coercev2_database_number, coercev2_database_text, equal_ignore_case,
};

/// D-function aggregate applied after header and row selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseFunction {
    /// DAVERAGE: arithmetic mean of selected numeric records.
    Average,
    /// DCOUNT: count selected numeric records.
    Count,
    /// DCOUNTA: count selected nonblank records.
    CountA,
    /// DGET: require exactly one matching record.
    Get,
    /// DMAX: largest selected numeric value.
    Max,
    /// DMIN: smallest selected numeric value.
    Min,
    /// DPRODUCT: multiply selected numeric values.
    Product,
    /// DSTDEV: sample standard deviation.
    Stdev,
    /// DSTDEVP: population standard deviation.
    StdevP,
    /// DSUM: sum selected numeric values.
    Sum,
    /// DVAR: sample variance.
    Var,
    /// DVARP: population variance.
    VarP,
}
impl DatabaseFunction {
    fn empty_field(self) -> bool {
        matches!(self, Self::Count | Self::CountA)
    }
    fn variance(self) -> bool {
        matches!(self, Self::Stdev | Self::StdevP | Self::Var | Self::VarP)
    }
}

/// Finalization contract, independent of record selection and numeric admission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FinalizationPolicy {
    /// Preserve POI's zero-valued empty DAVERAGE result.
    #[default]
    PinnedPoi,
    /// Return division by zero when DAVERAGE has no admitted numeric values.
    ExcelDocumentation,
}

/// A rectangular, row-major area, including its header row. The adapter must
/// reject scalar arguments before constructing this value.
#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    rows: usize,
    columns: usize,
    cells: &'a [CellValue],
}
impl<'a> Grid<'a> {
    /// Validate rectangular row-major storage before database planning.
    pub fn new(rows: usize, columns: usize, cells: &'a [CellValue]) -> Result<Self, CellError> {
        if rows == 0 || columns == 0 || rows.checked_mul(columns) != Some(cells.len()) {
            return Err(CellError::Value);
        }
        Ok(Self {
            rows,
            columns,
            cells,
        })
    }
    fn get(self, row: usize, col: usize) -> &'a CellValue {
        if row >= self.rows || col >= self.columns {
            return &CellValue::Error(CellError::Value);
        }
        row.checked_mul(self.columns)
            .and_then(|start| start.checked_add(col))
            .and_then(|index| self.cells.get(index))
            .unwrap_or(&CellValue::Error(CellError::Value))
    }
}
/// Conservative database traversal and numeric/text work before executing a plan.
/// The adapter must reserve these costs without eagerly evaluating unselected criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkEstimate {
    /// Conservative cell-inspection work reserved before execution.
    pub cell_visits: u64,
    /// Conservative character comparison and numeric normalization operations.
    pub work_units: u64,
    /// Estimated temporary storage needed by the database operation.
    pub scratch_bytes: u64,
}
/// Separate spreadsheet failures from unsupported criteria/projection behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseError {
    /// Catchable spreadsheet error produced by selection or aggregation.
    Cell(CellError),
    /// Formula-based criteria require an unsupported evaluation port.
    FormulaCriteria,
    /// Host cannot provide the required text interpretation.
    UnsupportedTextProjection,
}
impl From<CellError> for DatabaseError {
    fn from(value: CellError) -> Self {
        Self::Cell(value)
    }
}

/// Planning borrows areas without copying cells. Budget the estimate before
/// executing; criteria evaluation deliberately stays lazy to retain POI's
/// row/column short circuit and errors that occur only on visited criteria.
#[derive(Debug, Clone, Copy)]
pub struct DatabasePlan<'a> {
    function: DatabaseFunction,
    database: Grid<'a>,
    criteria: Grid<'a>,
    field: Option<usize>,
}
impl<'a> DatabasePlan<'a> {
    /// Validate field/header/criteria geometry without executing the aggregate.
    pub fn new(
        function: DatabaseFunction,
        database: Grid<'a>,
        field: &CellValue,
        criteria: Grid<'a>,
    ) -> Result<Self, DatabaseError> {
        let selected = selection::select_field(function, database, field)?;
        Ok(Self {
            function,
            database,
            criteria,
            field: selected,
        })
    }
    /// Return the work and scratch reservation required before execution.
    pub fn work_estimate(&self) -> WorkEstimate {
        estimate::work_estimate(self)
    }
    /// Apply the prepared criteria and aggregate selected records.
    pub fn execute(&self) -> Result<CellValue, DatabaseError> {
        self.execute_with_policy(FinalizationPolicy::PinnedPoi)
    }
    /// Aggregate with an explicit finalization contract; selection and admission stay unchanged.
    pub fn execute_with_policy(
        &self,
        policy: FinalizationPolicy,
    ) -> Result<CellValue, DatabaseError> {
        aggregation::execute(self, policy)
    }
}

#[cfg(test)]
mod tests;
