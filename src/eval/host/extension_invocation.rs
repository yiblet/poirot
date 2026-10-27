//! Deferred provider operands and the invocation transaction.
use super::call_host::CallHost;
use super::missing_policy::MissingPolicy;
use super::operand_kind::OperandKind;
use super::operand_reader::OperandReader;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions::{
    ExtensionBinding, ExtensionCapabilities, ExtensionError, ExtensionEvaluationStrategy,
    ExtensionHost, ExtensionInvocation, ExtensionOperandDescription, ExtensionOperandKind,
    ExtensionOutput, ExtensionResult, ExtensionScalar,
};
use crate::eval::limits::EvaluationError;
use crate::eval::session::ReadRecorder;
use crate::model::{CellAddress, CellError, CellValue};

/// Capability-checked extension bridge over the shared lazy call host.
struct CallbackHost<'a, 'workbook> {
    inner: CallHost<'a, 'workbook>,
    capabilities: ExtensionCapabilities,
    context: EvaluationInputs,
}

impl CallbackHost<'_, '_> {
    fn require(
        &self,
        allowed: bool,
        reason: crate::eval::limits::EvaluationReasonError,
    ) -> ExtensionResult<()> {
        if allowed {
            Ok(())
        } else {
            Err(self
                .inner
                .capability_failure(
                    crate::eval::limits::CapabilityId::UserDefinedFunction,
                    reason,
                )
                .into())
        }
    }

    fn validate_kind(&mut self, index: usize) -> ExtensionResult<()> {
        match self.inner.kind(index)? {
            OperandKind::Reference => self.require(
                self.capabilities.references,
                crate::eval::limits::EvaluationReasonError::ProviderContractDoesNotAllowReferenceArguments,
            ),
            OperandKind::Array => self.require(
                self.capabilities.arrays,
                crate::eval::limits::EvaluationReasonError::ProviderContractDoesNotAllowArrayArguments,
            ),
            _ => Ok(()),
        }
    }
}

impl ExtensionHost for CallbackHost<'_, '_> {
    fn argument_count(&self) -> usize {
        self.inner.len()
    }

    fn target(&mut self) -> ExtensionResult<EvaluationTarget> {
        self.require(
            self.capabilities.context,
            crate::eval::limits::EvaluationReasonError::ProviderContractDoesNotAllowEvaluationPosition,
        )?;
        Ok(self.inner.target())
    }

    fn context(&mut self) -> ExtensionResult<EvaluationInputs> {
        self.require(
            self.capabilities.context,
            crate::eval::limits::EvaluationReasonError::ProviderContractDoesNotAllowEvaluationContext,
        )?;
        Ok(self.context)
    }

    fn describe(&mut self, index: usize) -> ExtensionResult<ExtensionOperandDescription> {
        self.validate_kind(index)?;
        let description = OperandReader::new(&mut self.inner).describe(index)?;
        Ok(ExtensionOperandDescription {
            kind: match description.kind {
                OperandKind::Missing => ExtensionOperandKind::Missing,
                OperandKind::Scalar => ExtensionOperandKind::Scalar,
                OperandKind::Array => ExtensionOperandKind::Array,
                OperandKind::Reference => ExtensionOperandKind::Reference,
            },
            rows: description.rows,
            columns: description.columns,
        })
    }

    fn scalar(&mut self, index: usize) -> ExtensionResult<ExtensionScalar> {
        self.validate_kind(index)?;
        let scalar = self.inner.scalar_data(index, MissingPolicy::Blank)?;
        Ok(ExtensionScalar {
            value: scalar.value,
            missing: scalar.missing,
        })
    }

    fn read_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> ExtensionResult<ExtensionScalar> {
        self.validate_kind(index)?;
        let scalar = OperandReader::new(&mut self.inner).read_cell(index, row, column)?;
        Ok(ExtensionScalar {
            value: scalar.value,
            missing: scalar.missing,
        })
    }

    fn charge_work(&mut self, units: u64) -> ExtensionResult<()> {
        Ok(self.inner.charge_work(units)?)
    }
}

/// Admit arguments and capabilities, record invocation, and execute the bound provider.
pub(in crate::eval) fn execute_extension(
    session: &mut crate::eval::session::Session<'_>,
    sheet: usize,
    address: CellAddress,
    binding: &ExtensionBinding,
    arguments: crate::eval::vm::ArgumentsRef<'_>,
) -> crate::eval::limits::EvalResult<crate::eval::values::Value> {
    let Some(function) = session
        .sources
        .extensions
        .and_then(|r| r.implementation(binding))
    else {
        return Err(EvaluationError::UnsupportedCapability { capability: crate::eval::limits::CapabilityId::UserDefinedFunction, target: Some(EvaluationTarget { sheet, address }), reason: crate::eval::limits::EvaluationReasonError::BoundExtensionImplementationIsAbsentOrChangedRebindAgainstTheCurrentRegistry });
    };
    let requested = session
        .state
        .runtime
        .effects
        .extension_trace
        .invocations
        .len()
        .saturating_add(1);
    session.state.runtime.budget.allocation(requested)?;
    session
        .state
        .runtime
        .effects
        .extension_trace
        .invocations
        .push(ExtensionInvocation {
            target: EvaluationTarget { sheet, address },
            binding: binding.clone(),
        });
    ReadRecorder::new(session).read_lazy(|| {
        crate::eval::ports::DependencyRead::Extension(ExtensionInvocation {
            target: EvaluationTarget { sheet, address },
            binding: binding.clone(),
        })
    });
    let context = session.policies.context;
    let mut host = CallbackHost {
        inner: CallHost::new(session, sheet, address, arguments),
        capabilities: binding.registration.contract.capabilities,
        context,
    };
    let result = (|| -> ExtensionResult<crate::eval::values::Value> {
        if binding.registration.contract.evaluation == ExtensionEvaluationStrategy::Eager {
            for index in 0..arguments.len() {
                host.validate_kind(index)?;
            }
        }
        if !binding.registration.contract.arity.accepts(arguments.len()) {
            return Err(CellError::Value.into());
        }
        match function.evaluate(&mut host)? {
            ExtensionOutput::Scalar(value) => {
                if let CellValue::Text(text) = &value {
                    host.inner.check_text(text.len())?;
                }
                Ok(crate::eval::values::Value::Scalar(match value {
                    CellValue::Number(n) => crate::eval::values::finite(n),
                    other => other,
                }))
            }
            ExtensionOutput::Argument(index) => {
                host.validate_kind(index)?;
                Ok(host.inner.argument(index)?.into_value())
            }
            ExtensionOutput::Array(rows) => {
                host.require(
                    host.capabilities.arrays,
                    crate::eval::limits::EvaluationReasonError::ProviderContractDoesNotAllowArrayOutput,
                )?;
                let width = rows.first().map_or(0, Vec::len);
                if width == 0 || rows.iter().any(|row| row.len() != width) {
                    return Err(CellError::Value.into());
                }
                let count = rows
                    .iter()
                    .try_fold(0usize, |n, row| n.checked_add(row.len()))
                    .ok_or(CellError::Num)?;
                host.inner.check_values(count)?;
                let mut rows = rows;
                for value in rows.iter_mut().flatten() {
                    match value {
                        CellValue::Text(text) => {
                            host.inner.check_text(text.len())?;
                        }
                        CellValue::Number(n) if !n.is_finite() => {
                            *value = CellValue::Error(CellError::Num)
                        }
                        _ => {}
                    }
                }
                Ok(crate::eval::values::Value::Array(
                    crate::eval::values::ArrayView::unknown(rows),
                ))
            }
        }
    })();
    match result {
        Ok(value) => Ok(value),
        Err(ExtensionError::Spreadsheet(error)) => Ok(crate::eval::values::err(error)),
        Err(ExtensionError::Operational(mut failure)) => {
            failure.set_target(EvaluationTarget { sheet, address });
            Err(failure)
        }
    }
}
