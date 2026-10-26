//! Actual reads observed during execution; retention belongs to the caller.
use crate::eval::EvaluationTarget;
use crate::model::CellAddress;

/// Individual context fields consumed by executed evaluator operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextRead {
    /// Captured clock value.
    AsOf,
    /// Fixed local UTC offset.
    UtcOffset,
    /// Numeric slash-date component order.
    DateOrder,
    /// Workbook's 1900 or 1904 date system.
    DateSystem,
}

/// A dependency or setting observed during execution, including absent-cell membership.
/// A skipped branch emits no reads; some selected-handler context guards remain conservative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyRead {
    /// Value read from a cell, including an implicit blank.
    Cell(EvaluationTarget),
    /// Source formula and row visibility consumed without evaluating excluded cells.
    SubtotalMetadata(EvaluationTarget),
    /// Defined-name lookup and its selected workbook entry, if any.
    Name {
        /// Sheet scope from which lookup began.
        sheet: usize,
        /// Requested name spelling.
        name: String,
        /// Selected defined-name index, or `None` for a miss.
        selected: Option<usize>,
    },
    /// Sheet-name lookup and its selected workbook sheet, if any.
    Sheet {
        /// Current formula sheet.
        current: usize,
        /// Requested sheet spelling.
        name: String,
        /// Selected sheet index, or `None` for a miss.
        selected: Option<usize>,
    },
    /// Executed reference selection consumes ordered workbook sheet names.
    SheetLayout,
    /// Sparse scan whose result also depends on cells absent from the rectangle.
    SparseMembership {
        /// Scanned sheet index.
        sheet: usize,
        /// Inclusive top-left boundary.
        first: CellAddress,
        /// Inclusive bottom-right boundary.
        last: CellAddress,
    },
    /// Explicit coarse context read; normal builtin execution reports consumed fields.
    Context(crate::eval::registry::DependencyPolicy),
    /// A specific context field consumed by the executed operation.
    ContextField(ContextRead),
    /// Executed builtin scheduling volatility, independent of context guards.
    Volatile(crate::eval::registry::FunctionId),
    /// Provider execution whose dependency surface is not fully observable.
    OpaqueProvider(String),
    /// Formatting settings consumed by execution.
    Formatting,
    /// Numeric input tokens consumed while parsing a text argument.
    NumberLocale,
    /// Text semantics consumed by execution.
    TextPolicy,
    /// Execution-time builtin contracts; guarded globally by computation identity.
    BuiltinPolicy,
    /// Executed extension binding and declared volatility.
    Extension(crate::eval::extensions::ExtensionInvocation),
    /// Access to a cell-local random stream.
    Random(crate::eval::random::RandomAccess),
}
/// An observed read and its consuming cell, if the read has a cell attribution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadEvent {
    /// Formula cell consuming this read, when execution has a cell attribution.
    pub consumer: Option<EvaluationTarget>,
    /// Dependency or setting actually observed.
    pub read: DependencyRead,
}
/// Caller-owned consumption port. Implementations choose retention and persistence;
/// the evaluator does not allocate a second dependency/result cache.
pub trait DependencyReadSink {
    /// Accept a consumed read, retaining it according to the caller's policy.
    fn read(&mut self, event: ReadEvent);
}
