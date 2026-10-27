use crate::model::snapshot_collection::SnapshotCollection;
use crate::model::{CellAddress, FormulaSource};

/// Failure to validate or mutate workbook source data.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Serialized scalar-kind flags contain unknown alternatives.
    #[error("unknown scalar kind bits {bits:#x}")]
    InvalidScalarKindBits {
        /// Rejected scalar-kind flags.
        bits: u8,
    },
    /// Workbook data validation failed.
    #[error("workbook data validation failed")]
    Io(#[from] poirot_io::ModelError),
    /// Identifier comparison failed.
    #[error("identifier comparison failed")]
    Identifier(#[from] poirot_io::identifiers::IdentifierError),
    /// A worksheet operation selected an absent index.
    #[error("worksheet index {index} is out of bounds")]
    InvalidWorksheetIndex {
        /// Zero-based index rejected by the operation.
        index: usize,
    },
    /// The requested worksheet identity already exists.
    #[error("duplicate worksheet name {name:?}")]
    DuplicateWorksheetName {
        /// Rejected identifier spelling.
        name: String,
    },
    /// Inserted worksheet is absent.
    #[error("inserted worksheet is absent")]
    WorksheetInsertionMissing,
    /// A defined-name spelling violates authoring rules.
    #[error("invalid defined name {name:?}")]
    InvalidDefinedName {
        /// Rejected identifier spelling.
        name: String,
    },
    /// The requested defined-name identity already exists in its scope.
    #[error("duplicate defined name {name:?} in scope {sheet_scope:?}")]
    DuplicateDefinedName {
        /// Rejected identifier spelling.
        name: String,
        /// Worksheet scope, or the workbook scope when absent.
        sheet_scope: Option<usize>,
    },
    /// A cache update targets an unstored cell.
    #[error("cached value target {address} on worksheet {sheet} is absent")]
    MissingCachedValueTarget {
        /// Zero-based worksheet containing the target.
        sheet: usize,
        /// Coordinate rejected by the operation.
        address: CellAddress,
    },
    /// A cache update no longer matches the authored formula.
    #[error("formula at {address} on worksheet {sheet} changed")]
    CachedValueFormulaChanged {
        /// Zero-based worksheet containing the target.
        sheet: usize,
        /// Coordinate rejected by the operation.
        address: CellAddress,
    },
    /// A spill update targets an unstored anchor.
    #[error("spill anchor {anchor} on worksheet {sheet} is absent")]
    MissingSpillAnchor {
        /// Zero-based worksheet containing the target.
        sheet: usize,
        /// Formula anchor coordinate.
        anchor: CellAddress,
    },
    /// Workbook source changed after rename planning.
    #[error("workbook source changed after rename planning")]
    StaleRenamePlan,
    /// A rename replacement targets an unstored cell.
    #[error("rename target {address} on worksheet {sheet} is absent")]
    MissingRenameCell {
        /// Zero-based worksheet containing the target.
        sheet: usize,
        /// Coordinate rejected by the operation.
        address: CellAddress,
    },
    /// A rename replacement targets an absent defined name.
    #[error("defined name at index {index} is absent")]
    MissingRenameDefinedName {
        /// Zero-based index rejected by the operation.
        index: usize,
    },
    /// A replacement repeats a target or mismatches its expected source.
    #[error("invalid or duplicate replacement for {target:?}")]
    InvalidFormulaReplacement {
        /// Formula source rejected by the operation.
        target: FormulaSource,
    },
    /// A replacement contains unnormalized formula text.
    #[error("replacement for {target:?} is not normalized")]
    UnnormalizedFormulaReplacement {
        /// Formula source rejected by the operation.
        target: FormulaSource,
    },
    /// A query rectangle has reversed endpoints.
    #[error("query range {first}:{last} is not ordered")]
    UnorderedQueryRange {
        /// Requested top-left coordinate.
        first: CellAddress,
        /// Requested bottom-right coordinate.
        last: CellAddress,
    },
    /// Dense query size exceeds u64 representation.
    #[error("dense query size exceeds u64 representation")]
    DenseQuerySizeOverflow,
    /// Dense expansion would exceed the configured cell budget.
    #[error("dense query requests {requested} cells; limit is {limit}")]
    DenseQueryBudgetExceeded {
        /// Cell count requested by dense expansion.
        requested: u64,
        /// Maximum admitted record or cell count.
        limit: u64,
    },
    /// Dense expansion cannot fit the platform address space.
    #[error("dense query of {requested} cells exceeds address space")]
    DenseQueryAddressSpaceExceeded {
        /// Cell count requested by dense expansion.
        requested: u64,
    },
    /// A snapshot record collection exceeds its restoration limit.
    #[error("snapshot {kind:?} count {count} exceeds limit {limit}")]
    SnapshotLimitExceeded {
        /// Collection whose count exceeds the restoration limit.
        kind: SnapshotCollection,
        /// Actual record count in the snapshot.
        count: usize,
        /// Maximum admitted record or cell count.
        limit: usize,
    },
    /// A snapshot cell refers to an absent style record.
    #[error("snapshot style index {index} is out of bounds")]
    InvalidSnapshotStyle {
        /// Zero-based index rejected by the operation.
        index: usize,
    },
    /// A snapshot stores the same cell more than once.
    #[error("snapshot repeats cell {address}")]
    DuplicateSnapshotCell {
        /// Coordinate rejected by the operation.
        address: CellAddress,
    },
    /// A snapshot spill rectangle has reversed endpoints.
    #[error("snapshot spill extent {anchor}:{last} is not ordered")]
    InvalidSnapshotSpillExtent {
        /// Formula anchor coordinate.
        anchor: CellAddress,
        /// Requested bottom-right coordinate.
        last: CellAddress,
    },
    /// A snapshot stores the same spill extent more than once.
    #[error("snapshot repeats spill anchor {anchor}")]
    DuplicateSnapshotSpillExtent {
        /// Formula anchor coordinate.
        anchor: CellAddress,
    },
    /// A snapshot spill child violates saved ownership or source constraints.
    #[error("invalid snapshot spill child {address} owned by {owner}")]
    InvalidSnapshotSpillCache {
        /// Coordinate rejected by the operation.
        address: CellAddress,
        /// Formula anchor claiming the spill child.
        owner: CellAddress,
    },
    /// A snapshot stores the same spill child more than once.
    #[error("snapshot repeats spill child {address}")]
    DuplicateSnapshotSpillCache {
        /// Coordinate rejected by the operation.
        address: CellAddress,
    },
}

impl From<std::convert::Infallible> for ModelError {
    fn from(error: std::convert::Infallible) -> Self {
        match error {}
    }
}
