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
    /// Required Unicode search behavior is outside the supported mapping.
    UnicodeSearch,
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
/// Spreadsheet error vocabulary shared by lookup kernels and host adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid operand or argument shape (`#VALUE!`).
    Value,
    /// Invalid reference (`#REF!`).
    Ref,
    /// No matching value (`#N/A`).
    Na,
    /// Invalid numeric argument or result (`#NUM!`).
    Num,
    /// Zero denominator (`#DIV/0!`).
    Div0,
    /// Unresolved name (`#NAME?`).
    Name,
    /// Empty intersection (`#NULL!`).
    Null,
}
/// Workbook-independent scalar channel used by lookup and reference kernels.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    /// Numeric operand.
    Number(f64),
    /// Owned text operand.
    Text(String),
    /// Boolean operand.
    Bool(bool),
    /// Blank operand, retaining empty-cell semantics.
    Blank,
    /// Omitted argument slot.
    Missing,
    /// Catchable spreadsheet error operand.
    Error(Error),
}
/// Inclusive workbook/sheet/row/column bounds for a logical lookup reference.
/// Coordinates and sheet indices are zero-based; cell versus area identity is retained
/// even for a one-cell rectangle. Validate before using its dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// Cell or area provenance used by source-specific selection rules.
    pub kind: ReferenceKind,
    /// Optional external workbook qualifier retained for host resolution.
    pub workbook: Option<String>,
    /// Zero-based first sheet index, inclusive.
    pub first_sheet: u32,
    /// Zero-based last sheet index, inclusive.
    pub last_sheet: u32,
    /// First row endpoint, inclusive; signed references preserve off-grid coordinates.
    pub first_row: u32,
    /// Last row endpoint, inclusive.
    pub last_row: u32,
    /// First column endpoint, inclusive; signed references preserve off-grid coordinates.
    pub first_col: u32,
    /// Last column endpoint, inclusive.
    pub last_col: u32,
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
    /// Return inclusive row extent; call only after validating the reference.
    pub fn height(&self) -> usize {
        (self.last_row - self.first_row + 1) as usize
    }
    /// Return inclusive column extent; call only after validating the reference.
    pub fn width(&self) -> usize {
        (self.last_col - self.first_col + 1) as usize
    }
    /// Check ordered sheet/coordinate bounds against worksheet limits.
    pub fn valid(&self) -> bool {
        self.first_sheet <= self.last_sheet
            && self.first_row <= self.last_row
            && self.first_col <= self.last_col
            && self.last_row < 1_048_576
            && self.last_col < 16_384
    }
}
/// Owned, nonempty row-major lookup operands with validated rectangular dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    /// Nonzero logical row count.
    pub rows: usize,
    /// Nonzero logical column count.
    pub cols: usize,
    /// Flattened row-major scalar storage.
    pub values: Vec<Scalar>,
}
impl Array {
    /// Reject empty, overflowing, or inconsistent rectangular storage.
    pub fn new(rows: usize, cols: usize, values: Vec<Scalar>) -> Result<Self, Failure> {
        if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(values.len()) {
            return Err(Error::Value.into());
        }
        Ok(Self { rows, cols, values })
    }
    fn get(&self, row: usize, col: usize) -> Result<Scalar, Failure> {
        if row >= self.rows || col >= self.cols {
            return Err(Error::Ref.into());
        }
        self.values
            .get(row * self.cols + col)
            .cloned()
            .ok_or(Failure::Spreadsheet(Error::Value))
    }
}
/// Lookup argument channels; references retain geometry until a consumer selects values.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Already materialized scalar.
    Scalar(Scalar),
    /// Validated-on-use unsigned worksheet reference.
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

/// A reference admitted at an existing reference-validation boundary.
pub(super) struct CheckedReference<'a>(&'a Reference);
impl<'a> CheckedReference<'a> {
    /// Admit an ordinary reference only when its public DTO is valid.
    pub(super) fn new(reference: &'a Reference) -> Option<Self> {
        reference.valid().then_some(Self(reference))
    }
    /// Borrow the reference whose geometry has been admitted.
    pub(super) fn reference(&self) -> &'a Reference {
        self.0
    }
    /// Return the admitted row extent.
    pub(super) fn height(&self) -> usize {
        self.0.height()
    }
    /// Return the admitted column extent.
    pub(super) fn width(&self) -> usize {
        self.0.width()
    }
}
/// A borrowed array admitted without changing the raw public DTO.
pub(super) struct CheckedArray<'a>(&'a Array);
impl<'a> CheckedArray<'a> {
    /// Validate public array geometry before indexing its storage.
    pub(super) fn new(array: &'a Array) -> Option<Self> {
        (array.rows > 0
            && array.cols > 0
            && array.rows.checked_mul(array.cols) == Some(array.values.len()))
        .then_some(Self(array))
    }
    /// Borrow the admitted array DTO.
    pub(super) fn array(&self) -> &'a Array {
        self.0
    }
    /// Read a checked array coordinate with spreadsheet error reporting.
    pub(super) fn get(&self, row: usize, col: usize) -> Result<Scalar, Failure> {
        self.0.get(row, col)
    }
}

/// A logical POI area whose original signed endpoints may extend below the grid.
/// Identity metadata has the same layout as an ordinary unsigned Reference.
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
