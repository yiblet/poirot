use super::interface::{CallContext, FunctionAdapter, FunctionParameters, FunctionValue};
use super::{
    criteria, database, dates, engineering, financial, information, logical, math, matrix,
    reference, search, statistics, text,
};
use crate::eval::functions::registry::{FUNCTION_DESCRIPTORS, FunctionDescriptor, FunctionId};

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
                crate::eval::functions::registry::EXCEL_DESCRIPTORS
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
            ) -> Option<Result<FunctionValue, crate::eval::limits::EvaluationFailure>> {
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
