use super::function_input::FunctionInput;
use super::function_parameters::FunctionParameters;
use super::function_value::FunctionValue;
use crate::eval::compile::bytecode::Compiled;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::{FunctionDescriptor, FunctionId};
use crate::eval::limits::EvaluationError;
use crate::eval::session::{ReadRecorder, ResultProjection, Session};
use crate::eval::vm::ExecutableRef;
use crate::model::CellValue;
use poirot_functions::scalar::ParamVec;
use poirot_parsing::formula::FunctionName;

/// Evaluator services and caller coordinates supplied to a function adapter.
/// Adapters use it to force deferred arguments and consume results while independent function libraries remain unaware of evaluator state.
pub struct CallContext<'call, 'workbook> {
    /// Shared bounded execution state for the current call.
    pub(in crate::eval) session: &'call mut Session<'workbook>,
    /// Caller coordinates used for implicit intersection and attribution.
    pub target: EvaluationTarget,
}

impl<'call, 'workbook> CallContext<'call, 'workbook> {
    /// Wrap one compiled expression as a memoized deferred input.
    pub fn input<'input>(
        &'input mut self,
        expression: &'input Compiled,
    ) -> FunctionInput<'input, 'call, 'workbook> {
        FunctionInput {
            context: self,
            expression: ExecutableRef::Bytecode {
                program: expression,
                start: 0,
            },
            value: None,
        }
    }

    /// Request one lazy argument without inspecting or forcing any other argument.
    pub fn argument<'input>(
        &'input mut self,
        parameters: FunctionParameters<'input>,
        index: usize,
    ) -> Option<FunctionInput<'input, 'call, 'workbook>> {
        Some(FunctionInput {
            context: self,
            expression: parameters.0.get(index)?,
            value: None,
        })
    }

    /// Admit a resolved call, observe it, and dispatch without forcing its operands.
    pub(in crate::eval::adapters) fn invoke_registration(
        &mut self,
        name: &FunctionName,
        descriptor: Option<&'static FunctionDescriptor>,
        parameters: ParamVec<Compiled>,
        execute: impl FnOnce(
            &mut Self,
            FunctionId,
            FunctionParameters<'_>,
        ) -> Option<Result<FunctionValue, EvaluationError>>,
    ) -> Result<FunctionValue, EvaluationError> {
        self.session
            .ports
            .observe_resolved_call(descriptor.map(|entry| {
                crate::eval::observe::ConsumedBuiltinInvocation {
                    target: self.target,
                    handler: entry.handler,
                    parsed_spelling: name.as_str(),
                    resolved_spelling: name.as_str(),
                    argument_count: parameters.len(),
                }
            }))?;
        let Some(descriptor) = descriptor else {
            ReadRecorder::new(self.session).read_lazy(|| {
                crate::eval::ports::DependencyRead::OpaqueProvider(name.as_str().to_owned())
            });
            return Ok(FunctionValue::scalar(CellValue::Error(
                crate::model::CellError::Name,
            )));
        };
        execute(self, descriptor.handler, parameters.as_slice().into()).unwrap_or_else(|| {
            Err(EvaluationError::UnsupportedCapability {
                capability: crate::eval::limits::CapabilityId::FunctionAdapter,
                target: Some(self.target),
                reason: crate::eval::limits::EvaluationReasonError::MissingFunctionAdapter {
                    function: name.as_str().to_owned(),
                },
            })
        })
    }

    /// Select a scalar at the current caller coordinates, resolving reference or
    /// array values only when this projection is requested.
    pub fn scalar(&mut self, value: FunctionValue) -> Result<CellValue, EvaluationError> {
        ResultProjection::new(self.session).select_legacy_scalar(self.target.address, value.0)
    }
}
