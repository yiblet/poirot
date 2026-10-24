/// Lookup failure preserving spreadsheet errors, operational aborts, and unsupported capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Catchable error returned as a spreadsheet value.
    Spreadsheet(Error),
    /// Host execution failure that formula error handlers cannot catch.
    Operational(OperationalFailure),
    /// Required lookup/reference capability is unavailable.
    Unsupported(Unsupported),
}
/// Host execution failures kept outside spreadsheet error handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationalFailure {
    /// Host refused the requested work reservation.
    BudgetExceeded,
    /// Resource-size arithmetic overflowed before allocation.
    ResourceOverflow,
    /// Host cancelled evaluation.
    Cancelled,
    /// Required host service is unavailable.
    HostUnavailable,
}
/// Explicit capability gaps reported without silently changing reference semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// Operation needs workbook or sheet metadata absent from the host.
    WorkbookMetadata,
    /// Host cannot resolve the computed reference.
    DynamicReference,
    /// Host cannot supply arguments lazily.
    DeferredArguments,
    /// Lookup geometry is outside the supported vector contract.
    LookupNonVector,
    /// Result vector is too short for the selected lookup behavior.
    LookupShortResult,
    /// OFFSET crosses a pinned Java-short coordinate boundary.
    OffsetJavaShortBoundary,
    /// Reference dimensions cannot represent the pinned source behavior.
    SourceDimension,
    /// Missing OFFSET operands require unsupported source handling.
    OffsetMissingOperands,
    /// Scalar selection from this array has no supported coordinate contract.
    ScalarArrayIntersection,
    /// Sheet-name control characters cannot be rendered with supported ADDRESS fidelity.
    AddressSheetNameControlCharacter,
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Spreadsheet(error)
    }
}
/// Lookup-specific work reservations mapped onto the evaluator's resource meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// Materialize admitted arguments or operand values.
    InputMaterialization,
    /// Copy an admitted search or result vector.
    VectorCopy,
    /// Inspect text during lookup comparison.
    SearchText,
    /// Advance wildcard matching state.
    WildcardStates,
    /// Materialize lookup output values.
    QueryReturn,
    /// Inspect lookup candidates.
    SearchSteps,
}
/// A lookup operation's work category and units for host admission before execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetRequest {
    /// Category of work the host must admit.
    pub work: Work,
    /// Number of units requested before execution.
    pub units: usize,
}
pub use poirot_coercionv2::lookup::{Error, Scalar};
/// Inclusive workbook/sheet/row/column bounds for a logical lookup reference.
/// Coordinates and sheet indices are zero-based; cell versus area identity is retained
/// even for a one-cell rectangle. Geometry is validated at construction.
///
/// ```compile_fail
/// use poirot_functions_lookup::{Reference, ReferenceKind};
/// let mut reference = Reference::new(ReferenceKind::Area, None, 0..=0, 0..=0, 0..=0).unwrap();
/// reference.last_row = u32::MAX;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// Cell or area provenance used by source-specific selection rules.
    kind: ReferenceKind,
    /// Optional external workbook qualifier retained for host resolution.
    workbook: Option<String>,
    /// Zero-based first sheet index, inclusive.
    first_sheet: u32,
    /// Zero-based last sheet index, inclusive.
    last_sheet: u32,
    /// First row endpoint, inclusive.
    first_row: u32,
    /// Last row endpoint, inclusive.
    last_row: u32,
    /// First column endpoint, inclusive.
    first_col: u32,
    /// Last column endpoint, inclusive.
    last_col: u32,
    /// Validated nonzero row extent.
    rows: std::num::NonZeroU32,
    /// Validated nonzero column extent.
    cols: std::num::NonZeroU32,
}
/// Preserve cell-versus-area provenance even for a one-cell rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    /// Single-cell reference with cell-origin coercion.
    Cell,
    /// Area reference with range-origin coercion.
    Area,
}
impl Reference {
    /// Construct ordered sheet and worksheet bounds, rejecting reversed or off-grid geometry.
    pub fn new(
        kind: ReferenceKind,
        workbook: Option<String>,
        sheets: std::ops::RangeInclusive<u32>,
        rows: std::ops::RangeInclusive<u32>,
        cols: std::ops::RangeInclusive<u32>,
    ) -> Result<Self, Failure> {
        if sheets.is_empty()
            || rows.is_empty()
            || cols.is_empty()
            || *rows.end() >= 1_048_576
            || *cols.end() >= 16_384
        {
            return Err(Error::Ref.into());
        }
        let height = rows
            .end()
            .checked_sub(*rows.start())
            .and_then(|n| n.checked_add(1))
            .and_then(std::num::NonZeroU32::new)
            .ok_or(Error::Ref)?;
        let width = cols
            .end()
            .checked_sub(*cols.start())
            .and_then(|n| n.checked_add(1))
            .and_then(std::num::NonZeroU32::new)
            .ok_or(Error::Ref)?;
        Ok(Self {
            kind,
            workbook,
            rows: height,
            cols: width,
            first_sheet: *sheets.start(),
            last_sheet: *sheets.end(),
            first_row: *rows.start(),
            last_row: *rows.end(),
            first_col: *cols.start(),
            last_col: *cols.end(),
        })
    }
    /// Return the nonzero inclusive row extent.
    pub fn height(&self) -> usize {
        self.rows.get() as usize
    }
    /// Return the nonzero inclusive column extent.
    pub fn width(&self) -> usize {
        self.cols.get() as usize
    }
    /// Return the cell or area provenance.
    pub fn kind(&self) -> ReferenceKind {
        self.kind
    }
    /// Return the optional external workbook qualifier.
    pub fn workbook(&self) -> Option<&str> {
        self.workbook.as_deref()
    }
    /// Return first sheet.
    pub fn first_sheet(&self) -> u32 {
        self.first_sheet
    }
    /// Return last sheet.
    pub fn last_sheet(&self) -> u32 {
        self.last_sheet
    }
    /// Return first row.
    pub fn first_row(&self) -> u32 {
        self.first_row
    }
    /// Return last row.
    pub fn last_row(&self) -> u32 {
        self.last_row
    }
    /// Return first col.
    pub fn first_col(&self) -> u32 {
        self.first_col
    }
    /// Return last col.
    pub fn last_col(&self) -> u32 {
        self.last_col
    }
}
/// Owned, nonempty row-major lookup operands with validated rectangular dimensions.
///
/// ```compile_fail
/// use poirot_functions_lookup::Array;
/// let array = Array { rows: 2, cols: 2, values: vec![] };
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    /// Nonzero logical row count.
    rows: usize,
    /// Nonzero logical column count.
    cols: usize,
    /// Flattened row-major scalar storage.
    values: Vec<Scalar>,
}
impl Array {
    /// Reject empty, overflowing, or inconsistent rectangular storage.
    pub fn new(rows: usize, cols: usize, values: Vec<Scalar>) -> Result<Self, Failure> {
        if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(values.len()) {
            return Err(Error::Value.into());
        }
        Ok(Self { rows, cols, values })
    }
    /// Return the validated row count.
    pub fn rows(&self) -> usize {
        self.rows
    }
    /// Return the validated column count.
    pub fn cols(&self) -> usize {
        self.cols
    }
    /// Borrow row-major values without permitting shape changes.
    pub fn values(&self) -> &[Scalar] {
        &self.values
    }
    /// Mutate members while retaining the rectangular storage length.
    pub fn values_mut(&mut self) -> &mut [Scalar] {
        &mut self.values
    }
    /// Consume the array into its row-major values.
    pub fn into_values(self) -> Vec<Scalar> {
        self.values
    }
    /// Read one member, rejecting coordinates outside the rectangle.
    pub(super) fn get(&self, row: usize, col: usize) -> Result<&Scalar, Failure> {
        if row >= self.rows || col >= self.cols {
            return Err(Error::Ref.into());
        }
        let index = row
            .checked_mul(self.cols)
            .and_then(|n| n.checked_add(col))
            .ok_or(Failure::Spreadsheet(Error::Value))?;
        self.values
            .get(index)
            .ok_or(Failure::Spreadsheet(Error::Value))
    }
}
/// Lookup argument channels; references retain geometry until a consumer selects values.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Already materialized scalar.
    Scalar(Scalar),
    /// Materialized member whose original source still controls argument admission.
    SelectedScalar(Scalar, poirot_coercionv2::Origin),
    /// Validated unsigned worksheet reference.
    Reference(Reference),
    /// Pinned-source reference retaining signed endpoints.
    SignedReference(SignedReference),
    /// Multiple signed reference areas.
    SignedUnion(Vec<SignedReference>),
    /// Multiple unsigned reference areas.
    Union(Vec<Reference>),
    /// Materialized rectangular row-major values.
    Array(Array),
}

/// A logical POI area whose original signed endpoints may extend below the grid.
/// Identity metadata follows ordinary references; signed geometry is validated separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedReference {
    /// Cell or area provenance used by source-specific selection rules.
    pub kind: ReferenceKind,
    /// Optional external workbook qualifier retained for host resolution.
    pub workbook: Option<String>,
    /// Zero-based first sheet index, inclusive.
    pub first_sheet: u32,
    /// Zero-based last sheet index, inclusive.
    pub last_sheet: u32,
    /// First row endpoint, inclusive; signed references preserve off-grid coordinates.
    pub first_row: i32,
    /// Last row endpoint, inclusive.
    pub last_row: i32,
    /// First column endpoint, inclusive; signed references preserve off-grid coordinates.
    pub first_col: i32,
    /// Last column endpoint, inclusive.
    pub last_col: i32,
}
impl SignedReference {
    /// Validate sheet ordering while retaining signed endpoint behavior.
    pub fn valid(&self) -> bool {
        self.first_sheet <= self.last_sheet
    }
    /// Compute the pinned signed row extent with wrapping arithmetic.
    pub fn source_height(&self) -> i32 {
        self.last_row.wrapping_sub(self.first_row).wrapping_add(1)
    }
    /// Compute the pinned signed column extent with wrapping arithmetic.
    pub fn source_width(&self) -> i32 {
        self.last_col.wrapping_sub(self.first_col).wrapping_add(1)
    }
    /// Clamp the source row extent to a nonnegative logical size.
    pub fn height(&self) -> usize {
        self.source_height().max(0) as usize
    }
    /// Clamp the source column extent to a nonnegative logical size.
    pub fn width(&self) -> usize {
        self.source_width().max(0) as usize
    }
    /// Read an in-grid coordinate through the host; off-grid coordinates are blank.
    pub(crate) fn read(
        &self,
        row: i32,
        col: i32,
        w: &mut impl crate::Workbook,
    ) -> Result<Scalar, Failure> {
        if !(0..1_048_576).contains(&row) || !(0..16_384).contains(&col) {
            return Ok(Scalar::Blank);
        }
        w.signed_cell(self, self.first_sheet, row, col)
    }
    /// Select caller-intersected coordinates using signed source geometry.
    pub(crate) fn scalar_coordinates(&self, c: crate::Context) -> Result<(i32, i32), Failure> {
        let (row, col) = (c.row as i32, c.col as i32);
        let contains_row = (self.first_row..=self.last_row).contains(&row);
        let contains_col = (self.first_col..=self.last_col).contains(&col);
        if self.height() == 1 && self.width() == 1 {
            Ok((self.first_row, self.first_col))
        } else if self.width() == 1 && contains_row {
            Ok((row, self.first_col))
        } else if self.height() == 1 && contains_col {
            Ok((self.first_row, col))
        } else if contains_row && contains_col {
            Ok((row, col))
        } else {
            Err(Error::Value.into())
        }
    }
}
