use crate::model::CellAddress;

/// A table could not append data rows without changing unrelated worksheet content.
#[derive(Debug, thiserror::Error)]
pub enum TableGrowthError {
    /// The selected worksheet has no table with this display name.
    #[error("unknown table {name:?}")]
    UnknownTable {
        /// Requested table display name.
        name: String,
    },
    /// Table identifier comparison failed.
    #[error("table name comparison failed")]
    Identifier(#[from] poirot_io::identifiers::IdentifierError),
    /// Appending rows would require moving an existing totals row.
    #[error("a table with a totals row cannot append data rows")]
    TotalsRow,
    /// The appended extent would leave the worksheet grid.
    #[error("table growth exceeds worksheet row bounds")]
    RowBounds,
    /// The resulting geometry or metadata conflicts with the worksheet.
    #[error("invalid table growth")]
    InvalidTable(#[from] poirot_io::ModelError),
    /// A stored source value, formula, or shared formula record blocks growth.
    #[error("table growth would overwrite the cell at {address}")]
    Occupied {
        /// Blocking coordinate.
        address: CellAddress,
    },
    /// A saved spill would intersect the appended extent.
    #[error("table growth would intersect the saved spill at {anchor}")]
    Spill {
        /// Anchor of the blocking spill.
        anchor: CellAddress,
    },
    /// The stored filter selects a partial row range without the current table bottom.
    #[error("the table filter does not end at the bottom of the table")]
    FilterRange,
    /// Array-style calculated-column definitions require spill ownership.
    #[error("table growth cannot fill an array-style definition in column {column}")]
    ArrayFormula {
        /// Zero-based worksheet column.
        column: u32,
    },
    /// The calculated-column definition could not be copied to the new row.
    #[error("could not translate the calculated column formula at {address}")]
    Formula {
        /// Destination cell of the copied formula.
        address: CellAddress,
        /// Parser or translation diagnostic.
        #[source]
        error: poirot_parsing::formula::ParseError,
    },
    /// Storage for the staged formula cells could not be reserved.
    #[error("could not reserve table growth formulas")]
    Reservation(#[from] std::collections::TryReserveError),
}
