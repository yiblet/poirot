//! Evaluator-owned adapters for workbook-independent function algorithms.
//!
//! Each family implements [`crate::eval::adapters::FunctionAdapter`].
//! [`crate::eval::adapters::FunctionAdapter::chain`] composes families through that same
//! interface; [`crate::eval::adapters::builtins`] exposes the complete catalog.
//! Parsing accepts generic function syntax. Binding resolves names through this
//! registry, and actual execution dispatches through these family adapters.
//! A parent collects child registrations once, indexes known names by numeric slot,
//! and stores uncommon names in a hash map. Executors are flattened when composing
//! parents; calls do not search through the child adapters.
//!
//! Arguments remain deferred. [`crate::eval::adapters::ParamVec`] stores up to four arguments
//! inline and spills for larger calls; it never imposes a function arity limit.
//! [`crate::eval::adapters::Input::force`]
//! evaluates one input on demand, retaining its value for subsequent requests in
//! that call. Distinct inputs and later invocations have separate memoization.
//!
//! ```
//! use poirot::eval::{
//!     adapters::{FunctionAdapter, MathAdapter, FinancialAdapter, ParamVec, CommonFunction, FunctionName},
//!     bytecode, EvaluationBudgets, EvaluationInputs, PreparationPolicy,
//!     EvaluationRun, EvaluationTarget, FormulaCompiler, WorkbookView,
//! };
//! use poirot::model::{Workbook, CellAddress, CellValue};
//!
//! let mut workbook = Workbook::new();
//! workbook.add_sheet("Sheet1")?;
//! let policy = PreparationPolicy::default();
//! let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
//! let view = WorkbookView::new(&workbook, &plan, &policy)?;
//! let mut run = EvaluationRun::new(view, EvaluationInputs::default(), EvaluationBudgets::default())?;
//! let adapter = MathAdapter.chain(FinancialAdapter);
//! let argument = bytecode::compile(&poirot_parsing::formula::Expr::Number(5.))?;
//! let arguments: ParamVec<_> = [argument].into_iter().collect();
//! let target = EvaluationTarget { sheet: 0, address: CellAddress::new(0, 0)? };
//! let name = FunctionName::Known(CommonFunction::Fact);
//! assert_eq!(run.call_with_adapter(&adapter, target, &name, arguments)?, CellValue::Number(120.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod aggregate;
/// Date and time execution adapters.
pub mod dates;
/// Engineering and complex number execution adapters.
mod engineering;
/// Cash flow, loan, and monetary formatting adapters.
pub mod financial;
/// Scalar type and error inspection adapters.
pub mod information;
/// Logical tests and lazy branch selection adapters.
pub mod logical;
/// Mathematical and random number execution adapters.
pub mod math;
/// Matrix algebra and array aggregation adapters.
pub mod matrix;
/// Reference construction and selection adapters.
pub mod reference;
/// Lookup and matching execution adapters.
pub mod search;
/// Statistical calculation and aggregation adapters.
pub mod statistics;

/// Criteria-based aggregation adapters.
pub mod criteria;
/// Criteria-table database adapters.
pub mod database;
/// Shared lookup coercion and comparison rules.
pub mod lookup;
/// Text construction, parsing, and formatting adapters.
pub mod text;

/// Typed failures shared by builtin adapters.
pub mod failure;
mod interface;
// Common deferred arguments, values, host context, and adapter interface.
pub use interface::{
    CallContext, FunctionAdapter, FunctionInput, FunctionParameters, FunctionValue,
};
mod elementwise;
mod registry;
/// Indexed adapter composition owning flattened executors.
pub use registry::AdapterRegistry;
/// Compatibility name for the indexed parent produced by chaining adapters.
pub type AdapterChain = AdapterRegistry;
// Deferred input protocol and small argument vector.
pub use poirot_functions::scalar::{Input, ParamVec};
// Parsed builtin names and fallback spellings.
pub use poirot_parsing::formula::{CommonFunction, FunctionName};

use crate::eval::registry::{FUNCTION_DESCRIPTORS, FunctionDescriptor, FunctionId};

macro_rules! family_adapter {
    ($adapter:ident, $variant:ident, $function:ident, $context:ident, $params:ident, $execute:expr) => {
        #[doc = concat!("Registers and executes the ", stringify!($variant), " builtin family. Other handler families remain available to later adapters in a composed registry.")]
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $adapter;
        impl FunctionAdapter for $adapter {
            fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
                FUNCTION_DESCRIPTORS
                    .iter()
                    .filter(|entry| matches!(entry.handler, FunctionId::$variant(_)))
            }
            fn profile_functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
                crate::eval::registry::EXCEL_DESCRIPTORS
                    .iter()
                    .filter(|entry| matches!(entry.handler, FunctionId::$variant(_)))
            }
            fn supports(&self, function: FunctionId) -> bool {
                matches!(function, FunctionId::$variant(_))
            }
            fn execute(
                &self,
                $context: &mut CallContext<'_, '_>,
                handler: FunctionId,
                $params: FunctionParameters<'_>,
            ) -> Option<Result<FunctionValue, crate::eval::EvaluationFailure>> {
                let FunctionId::$variant($function) = handler else {
                    return None;
                };
                Some(($execute).map(FunctionValue))
            }
        }
    };
}
family_adapter!(
    DateAdapter,
    Date,
    function,
    context,
    parameters,
    dates::execution::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    LogicalAdapter,
    Logical,
    function,
    context,
    parameters,
    logical::execution::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    StatisticsAdapter,
    Statistics,
    function,
    context,
    parameters,
    statistics::execution::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    ReferenceAdapter,
    Reference,
    function,
    context,
    parameters,
    reference::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    SearchAdapter,
    Search,
    function,
    context,
    parameters,
    search::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    InformationAdapter,
    Information,
    function,
    context,
    parameters,
    information::execution::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    MathAdapter,
    Math,
    function,
    context,
    parameters,
    math::execution::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    TextAdapter,
    Text,
    function,
    context,
    parameters,
    text::execution::execute_function(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    FinancialAdapter,
    Financial,
    function,
    context,
    parameters,
    financial::execute_function(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    EngineeringAdapter,
    Engineering,
    function,
    context,
    parameters,
    engineering::execute_function(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    MatrixAdapter,
    Matrix,
    function,
    context,
    parameters,
    matrix::execute(
        function,
        context.session,
        context.target.sheet,
        context.target.address,
        parameters.0
    )
);
family_adapter!(
    CriteriaAdapter,
    Criteria,
    function,
    context,
    parameters,
    criteria::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function,
        parameters.0
    )
);
family_adapter!(
    DatabaseAdapter,
    Database,
    function,
    context,
    parameters,
    database::execute(
        context.session,
        context.target.sheet,
        context.target.address,
        function.helper(),
        parameters.0
    )
);

/// Default adapter that exposes and dispatches all builtin function families.
/// Its shared registry retains catalog order and handles ordinary and compatibility-profile lookup.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinAdapters;

impl FunctionAdapter for BuiltinAdapters {
    fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        FUNCTION_DESCRIPTORS.iter()
    }
    fn profile_functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        crate::eval::registry::EXCEL_DESCRIPTORS.iter()
    }
    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        builtin_registry().resolve(name)
    }
    fn resolve_profile(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        builtin_registry().resolve_profile(name)
    }
    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: ParamVec<crate::eval::bytecode::Compiled>,
    ) -> Result<FunctionValue, crate::eval::EvaluationFailure> {
        builtin_registry().call(context, name, parameters)
    }
    fn supports(&self, function: FunctionId) -> bool {
        builtin_registry().supports(function)
    }
    fn into_registry(self) -> AdapterRegistry {
        builtin_registry().clone()
    }
    fn execute(
        &self,
        context: &mut CallContext<'_, '_>,
        function: FunctionId,
        parameters: FunctionParameters<'_>,
    ) -> Option<Result<FunctionValue, crate::eval::EvaluationFailure>> {
        builtin_registry().execute(context, function, parameters)
    }
}

/// Return the adapter exposing every default builtin family.
pub fn builtins() -> BuiltinAdapters {
    BuiltinAdapters
}

fn builtin_registry() -> &'static AdapterRegistry {
    static REGISTRY: std::sync::LazyLock<AdapterRegistry> = std::sync::LazyLock::new(builtin_chain);
    &REGISTRY
}

/// Resolve a profile-only descriptor using documented Excel builtin spellings.
pub(in crate::eval) fn profile_descriptor(
    name: &FunctionName,
) -> Option<&'static FunctionDescriptor> {
    builtin_registry().resolve_profile(name)
}
/// Resolve a typed profile-only handler by function name.
pub(in crate::eval) fn profile_function(name: &FunctionName) -> Option<FunctionId> {
    profile_descriptor(name).map(|entry| entry.handler)
}

fn builtin_chain() -> AdapterRegistry {
    SearchAdapter
        .chain(ReferenceAdapter)
        .chain(MathAdapter)
        .chain(StatisticsAdapter)
        .chain(DatabaseAdapter)
        .chain(DateAdapter)
        .chain(LogicalAdapter)
        .chain(InformationAdapter)
        .chain(FinancialAdapter)
        .chain(EngineeringAdapter)
        .chain(MatrixAdapter)
        .chain(CriteriaAdapter)
        .chain(TextAdapter)
}

/// Dispatch a typed builtin through the complete family adapter set.
pub(in crate::eval) fn execute(
    session: &mut crate::eval::Session<'_>,
    target: crate::eval::EvaluationTarget,
    function: FunctionId,
    parameters: crate::eval::ArgumentsRef<'_>,
) -> Result<crate::eval::Value, crate::eval::EvaluationFailure> {
    builtins()
        .execute(
            &mut CallContext { session, target },
            function,
            FunctionParameters(parameters),
        )
        .ok_or(crate::eval::EvaluationFailure::StalePlan)?
        .map(|value| value.0)
}

#[cfg(test)]
mod tests;
