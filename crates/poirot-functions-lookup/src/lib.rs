//! Reference-preserving lookup kernels. Workbook services are explicit query ports.
mod comparison;
mod domain;
mod geometry;
mod index;
mod operands;
mod policy;
mod ports;
mod reference;
mod search;
mod syntax;
mod table;
mod wildcard;
/// Workbook-independent values, reference geometry, failures, and work requests.
pub use domain::{
    Array, BudgetRequest, Error, Failure, OperationalFailure, Reference, ReferenceKind, Scalar,
    SignedReference, Unsupported, Value, Work,
};
/// Approximate and XMATCH position selection over admitted search vectors.
pub use index::{approximate_index, xmatch_index};
use operands::{num, Arguments};
/// Typed call preparation, dependency contracts, and match/search semantics.
pub use policy::{
    CallPlan, DependencyPolicy, Function, FunctionPolicy, MatchMode, ReferenceFunction, Search,
    SearchMode, SemanticsPolicy,
};
/// Host ports for consumed cell reads, workbook metadata, and computed references.
pub use ports::{Context, DynamicReference, Workbook};
/// Parsing of dynamic A1/R1C1 reference spellings before host resolution.
pub use syntax::parse_dynamic_reference;
/// Pinned POI source revision defining compatibility behavior.
pub const POI_REVISION: &str = "732120980140d5ed64b482c470e0b625cdb1ab15";
/// POI argument and evaluation order, including pass-through HYPERLINK and eager XLOOKUP fallback.
pub fn evaluate(
    function: Function,
    args: &[Value],
    context: Context,
    workbook: &mut impl Workbook,
) -> Result<Value, Failure> {
    evaluate_with_policy(
        function,
        args,
        context,
        workbook,
        SemanticsPolicy::PinnedPoi,
    )
}
/// Prepare and execute a family call under an explicit semantics policy.
pub fn evaluate_with_policy(
    function: Function,
    args: &[Value],
    context: Context,
    workbook: &mut impl Workbook,
    policy: SemanticsPolicy,
) -> Result<Value, Failure> {
    evaluate_arguments(
        function,
        &Arguments {
            ready: Some(args),
            count: args.len(),
        },
        context,
        workbook,
        policy,
    )
}
fn evaluate_arguments(
    function: Function,
    args: &Arguments<'_>,
    context: Context,
    workbook: &mut impl Workbook,
    policy: SemanticsPolicy,
) -> Result<Value, Failure> {
    match function {
        Function::Search(f) => crate::search::search(f, args, context, workbook),
        Function::Reference(f) => match f {
            ReferenceFunction::Address => {
                crate::reference::address(args, context, workbook, policy)
            }
            ReferenceFunction::Areas => {
                if args.is_empty() {
                    return Err(Error::Value.into());
                }
                let first = args.get(0, context, workbook)?;
                Ok(num(match first {
                    Value::Union(v) => v.len() as f64,
                    Value::SignedUnion(v) => v.len() as f64,
                    _ => 1.0,
                }))
            }
            ReferenceFunction::Hyperlink => {
                if !(1..=2).contains(&args.len()) {
                    return Err(Error::Value.into());
                }
                args.get(args.len() - 1, context, workbook)
            }
            ReferenceFunction::Offset => crate::reference::offset(args, context, workbook),
            ReferenceFunction::Indirect => crate::reference::indirect(args, context, workbook),
            ReferenceFunction::Sheet => crate::reference::sheet(args, context, workbook),
            ReferenceFunction::Single => crate::reference::single(args, context, workbook),
        },
    }
}
