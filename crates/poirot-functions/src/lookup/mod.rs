//! Look up spreadsheet values and select references.
//!
//! Use [`evaluate`](crate::lookup::evaluate) with a selected function and its arguments. Implement the
//! [`Workbook`](crate::lookup::Workbook) trait to supply cell reads and workbook metadata. Results retain
//! references when the spreadsheet function returns one.
//!
//! Lower-level helpers cover `MATCH`, `XMATCH`, approximate search, `INDEX`, ADDRESS text, and
//! parsing dynamic references. The caller evaluates arguments and controls limits.
mod classic_match;
mod comparison;
mod domain;
mod geometry;
mod lookup_error;
mod lookup_operational_error;
mod lookup_unsupported_error;
mod match_selection_error;
/// Pure reference-axis translation and caller-coordinate intersection.
pub use geometry::{intersection_offsets, offset_extent};
mod index;
mod operands;
mod policy;
mod ports;
mod reference;
mod search;
mod syntax;
mod table;
mod wildcard;
// Typed classic MATCH selection with fallible, lazy candidate access.
pub use classic_match::{ClassicMatchMode, MatchCandidateReader, classic_match_index};
// Workbook-independent values, reference geometry, failures, and work requests.
pub use domain::{
    Array, BudgetRequest, LookupValueError, Reference, ReferenceKind, Scalar, SignedReference,
    Value, Work,
};
// Approximate and XMATCH position selection over admitted search vectors.
pub use index::{approximate_index, xmatch_index};
use operands::Arguments;
// Typed call preparation, dependency contracts, and match/search semantics.
pub use policy::{
    CallPlan, DependencyPolicy, Function, FunctionPolicy, MatchMode, ReferenceFunction, Search,
    SearchMode,
};
// Host ports for consumed cell reads, workbook metadata, and computed references.
pub use ports::{Context, DynamicReference, Workbook};
/// Pure coordinate selection and typed ADDRESS spelling without workbook access.
pub use reference::{AddressAxes, AddressStyle, address_text, index_selection};
/// Parsing of dynamic A1/R1C1 reference spellings before host resolution.
pub use syntax::parse_dynamic_reference;
// Algorithms adapted from Apache POI (Apache-2.0), revision
// 732120980140d5ed64b482c470e0b625cdb1ab15; source URLs and reused portions
// are recorded beside each lookup implementation.
/// Evaluate a lookup function with supplied arguments and workbook access.
/// Preserves deferred reference construction and eager XLOOKUP fallback.
pub fn evaluate(
    function: Function,
    args: &[Value],
    context: Context,
    workbook: &mut impl Workbook,
) -> Result<Value, LookupError> {
    evaluate_arguments(
        function,
        &Arguments {
            ready: Some(args),
            count: args.len(),
        },
        context,
        workbook,
    )
}
fn evaluate_arguments(
    function: Function,
    args: &Arguments<'_>,
    context: Context,
    workbook: &mut impl Workbook,
) -> Result<Value, LookupError> {
    match function {
        Function::Search(f) => crate::lookup::search::search(f, args, context, workbook),
        Function::Reference(f) => match f {
            ReferenceFunction::Indirect => {
                crate::lookup::reference::indirect(args, context, workbook)
            }
        },
    }
}

pub use lookup_error::LookupError;

pub use lookup_operational_error::LookupOperationalError;

pub use lookup_unsupported_error::LookupUnsupportedError;

pub use match_selection_error::MatchSelectionError;
