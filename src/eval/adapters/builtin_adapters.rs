use super::families::{
    CriteriaAdapter, DatabaseAdapter, DateAdapter, EngineeringAdapter, FinancialAdapter,
    InformationAdapter, LogicalAdapter, MathAdapter, MatrixAdapter, ReferenceAdapter,
    SearchAdapter, StatisticsAdapter, TextAdapter,
};
use super::interface::{CallContext, FunctionAdapter, FunctionParameters, FunctionValue};
use super::registry::AdapterRegistry;
use crate::eval::functions::registry::{FUNCTION_DESCRIPTORS, FunctionDescriptor, FunctionId};
use poirot_functions::scalar::ParamVec;
use poirot_parsing::formula::FunctionName;

/// Default adapter that exposes and dispatches all builtin function families.
/// Its shared registry retains catalog order and resolves serialized builtin aliases.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinAdapters;

/// Return the adapter exposing every default builtin family.
pub fn builtins() -> BuiltinAdapters {
    BuiltinAdapters
}

pub(super) fn builtin_registry() -> &'static AdapterRegistry {
    static REGISTRY: std::sync::LazyLock<AdapterRegistry> = std::sync::LazyLock::new(builtin_chain);
    &REGISTRY
}

pub(super) fn builtin_chain() -> AdapterRegistry {
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
    session: &mut crate::eval::session::Session<'_>,
    target: crate::eval::coordinates::EvaluationTarget,
    function: FunctionId,
    parameters: crate::eval::vm::ArgumentsRef<'_>,
) -> Result<crate::eval::values::Value, crate::eval::limits::EvaluationError> {
    builtins()
        .execute(
            &mut CallContext { session, target },
            function,
            FunctionParameters(parameters),
        )
        .ok_or(crate::eval::limits::EvaluationError::StalePlan)?
        .map(|value| value.0)
}

impl FunctionAdapter for BuiltinAdapters {
    fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        FUNCTION_DESCRIPTORS.iter()
    }

    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        builtin_registry().resolve(name)
    }

    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: ParamVec<crate::eval::compile::bytecode::Compiled>,
    ) -> Result<FunctionValue, crate::eval::limits::EvaluationError> {
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
    ) -> Option<Result<FunctionValue, crate::eval::limits::EvaluationError>> {
        builtin_registry().execute(context, function, parameters)
    }
}
