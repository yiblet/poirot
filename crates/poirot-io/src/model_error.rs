/// Invalid file-model values that cannot be represented faithfully.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Invalid scalar input.
    #[error("invalid scalar value")]
    Value(#[from] poirot_functions::scalar::ValueError),
    /// Shared formula has no source spelling.
    #[error("shared formula requires formula source")]
    SharedFormulaRequiresSource,
    /// Formula spelling is empty.
    #[error("formula must not be empty")]
    EmptyFormula,
    /// Font point size is invalid.
    #[error("invalid font size {value}")]
    InvalidFontSize {
        /// Rejected point size.
        value: f64,
    },
    /// Color tint is invalid.
    #[error("invalid tint {value}")]
    InvalidTint {
        /// Rejected tint.
        value: f64,
    },
    /// Indexed palette reference is invalid.
    #[error("invalid indexed color {value}")]
    InvalidIndexedColor {
        /// Rejected palette index.
        value: u8,
    },
    /// Text rotation angle is invalid.
    #[error("invalid rotation {value}")]
    InvalidRotation {
        /// Rejected angle.
        value: i16,
    },
    /// Palette contains more than the supported entries.
    #[error("style context palette exceeds 64 entries")]
    StyleContextLimit,
    /// Coordinates exceed worksheet dimensions.
    #[error("cell coordinates ({row}, {column}) exceed worksheet dimensions")]
    InvalidCellCoordinates {
        /// Zero-based row.
        row: u32,
        /// Zero-based column.
        column: u32,
    },
    /// A1 spelling cannot denote a worksheet cell.
    #[error("invalid A1 cell address {input:?}")]
    InvalidCellAddress {
        /// Rejected source spelling.
        input: String,
    },
    /// Generic integer row cannot fit the coordinate representation.
    #[error("invalid zero-based row coordinate")]
    InvalidZeroBasedRow,
    /// Generic integer column cannot fit the coordinate representation.
    #[error("invalid zero-based column coordinate")]
    InvalidZeroBasedColumn,
    /// Test fixture lacks the requested worksheet.
    #[error("missing fixture worksheet")]
    MissingFixtureSheet,
    /// Explicit row exceeds worksheet dimensions.
    #[error("row {row} exceeds worksheet dimensions")]
    InvalidRow {
        /// Rejected zero-based row.
        row: u32,
    },
    /// Cell inserted into storage cannot be recovered.
    #[error("inserted cell {address} is missing")]
    CellInsertionMissing {
        /// Requested cell coordinate.
        address: crate::CellAddress,
    },
    /// Saved spill rectangle is inverted or lacks its anchor.
    #[error("invalid spill extent {anchor}:{last}")]
    InvalidSpillExtent {
        /// Saved formula anchor.
        anchor: crate::CellAddress,
        /// Inclusive saved rectangle end.
        last: crate::CellAddress,
    },
    /// Saved spill child cannot belong to the specified anchor.
    #[error("invalid spill cache anchor {owner} for {address}")]
    InvalidSpillCacheAnchor {
        /// Saved child coordinate.
        address: crate::CellAddress,
        /// Claimed formula anchor.
        owner: crate::CellAddress,
    },
    /// Saved spill child overlaps authored content.
    #[error("spill cache overlaps authored cell {address}")]
    SpillCacheAuthoredCell {
        /// Conflicting authored coordinate.
        address: crate::CellAddress,
    },
    /// Saved spill child is already owned by another anchor.
    #[error("spill cache ownership conflict at {address}")]
    SpillCacheOwnershipConflict {
        /// Conflicting child coordinate.
        address: crate::CellAddress,
    },
    /// Worksheet name violates the creation contract.
    #[error("invalid worksheet name {name:?}")]
    InvalidWorksheetName {
        /// Rejected source name.
        name: String,
    },
    /// Worksheet name already exists under the identifier policy.
    #[error("duplicate worksheet name {name:?}")]
    DuplicateWorksheetName {
        /// Conflicting source name.
        name: String,
    },
    /// Worksheet position does not exist.
    #[error("worksheet index {index} is out of bounds")]
    InvalidWorksheetIndex {
        /// Rejected zero-based sheet position.
        index: usize,
    },
}

impl From<std::convert::Infallible> for ModelError {
    fn from(error: std::convert::Infallible) -> Self {
        match error {}
    }
}

#[cfg(test)]
mod tests {
    use super::ModelError;
    use crate::CellAddress;

    #[test]
    fn typed_addresses_support_fallible_generic_model_apis() {
        fn checked<T: TryInto<CellAddress>>(input: T) -> Result<CellAddress, ModelError>
        where
            ModelError: From<T::Error>,
        {
            Ok(input.try_into()?)
        }
        let address = CellAddress::new(1, 2).unwrap();
        assert_eq!(checked(address).unwrap(), address);
        assert_eq!(checked::<&CellAddress>(&address).unwrap(), address);
        assert_eq!(checked("C2").unwrap(), address);
    }
}
