use super::context_read::ContextRead;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions::ExtensionInvocation;
use crate::eval::functions::registry::{DependencyPolicy, FunctionId};
use crate::eval::limits::RandomAccess;
use crate::model::CellAddress;

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
    Context(DependencyPolicy),
    /// A specific context field consumed by the executed operation.
    ContextField(ContextRead),
    /// Executed builtin scheduling volatility, independent of context guards.
    Volatile(FunctionId),
    /// Provider execution whose dependency surface is not fully observable.
    OpaqueProvider(String),
    /// Formatting settings consumed by execution.
    Formatting,
    /// Numeric input tokens consumed while parsing a text argument.
    NumberLocale,
    /// Executed extension binding and declared volatility.
    Extension(ExtensionInvocation),
    /// Access to a cell-local random stream.
    Random(RandomAccess),
}
