/// Category of deterministic evaluator work, used for accounting rather than timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkKind {
    /// Expression admission.
    Expression,
    /// Workbook cell read.
    CellRead,
    /// Sparse range entry inspection.
    SparseScan,
    /// Name or sheet namespace entry inspection.
    NamespaceScan,
    /// Collection element materialization.
    Materialization,
    /// Value comparison.
    Comparison,
    /// Scalar arithmetic or logical operation.
    ScalarOperator,
    /// Work performed by a builtin helper.
    HelperOperation,
    /// Wildcard matching state.
    WildcardState,
    /// Text processing byte.
    TextByte,
    /// Calendar iteration step.
    CalendarStep,
    /// Pair of references inspected by a reference operator.
    ReferencePair,
}
