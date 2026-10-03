//! Deferred provider operands and the invocation transaction.
use super::{
    ExtensionBinding, ExtensionCapabilities, ExtensionEvaluationStrategy, ExtensionFailure,
    ExtensionHost, ExtensionInvocation, ExtensionOperandDescription, ExtensionOperandKind,
    ExtensionOutput, ExtensionResult, ExtensionScalar,
};
use crate::{
    CellAddress, CellError, CellValue, EvaluationInputs, EvaluationTarget,
    budget::EvaluationFailure,
};

/// Capability-checked extension bridge over the shared lazy call host.
struct CallbackHost<'a, 'workbook> {
    inner: crate::host::CallHost<'a, 'workbook>,
    capabilities: ExtensionCapabilities,
    context: EvaluationInputs,
}
impl From<crate::FunctionFailure> for ExtensionFailure {
    fn from(value: crate::FunctionFailure) -> Self {
        match value {
            crate::FunctionFailure::Spreadsheet(e) => Self::Spreadsheet(e),
            crate::FunctionFailure::Operational(e) => Self::Operational(e),
        }
    }
}
impl CallbackHost<'_, '_> {
    fn require(&self, allowed: bool, detail: &str) -> ExtensionResult<()> {
        if allowed {
            Ok(())
        } else {
            Err(self
                .inner
                .capability_failure(crate::CapabilityId::UserDefinedFunction, detail)
                .into())
        }
    }
    fn validate_kind(&mut self, index: usize) -> ExtensionResult<()> {
        match self.inner.kind(index)? {
            crate::host::OperandKind::Reference => self.require(
                self.capabilities.references,
                "provider contract does not allow reference arguments",
            ),
            crate::host::OperandKind::Array => self.require(
                self.capabilities.arrays,
                "provider contract does not allow array arguments",
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
            "provider contract does not allow evaluation position",
        )?;
        Ok(self.inner.target())
    }
    fn context(&mut self) -> ExtensionResult<EvaluationInputs> {
        self.require(
            self.capabilities.context,
            "provider contract does not allow evaluation context",
        )?;
        Ok(self.context)
    }
    fn describe(&mut self, index: usize) -> ExtensionResult<ExtensionOperandDescription> {
        self.validate_kind(index)?;
        let description = self.inner.describe(index)?;
        Ok(ExtensionOperandDescription {
            kind: match description.kind {
                crate::host::OperandKind::Missing => ExtensionOperandKind::Missing,
                crate::host::OperandKind::Scalar => ExtensionOperandKind::Scalar,
                crate::host::OperandKind::Array => ExtensionOperandKind::Array,
                crate::host::OperandKind::Reference => ExtensionOperandKind::Reference,
            },
            rows: description.rows,
            columns: description.columns,
        })
    }
    fn scalar(&mut self, index: usize) -> ExtensionResult<ExtensionScalar> {
        self.validate_kind(index)?;
        let scalar = self
            .inner
            .scalar_data(index, crate::host::MissingPolicy::Blank)?;
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
        let scalar = self.inner.read_cell(index, row, column)?;
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
pub(crate) fn execute(
    session: &mut crate::Session<'_>,
    sheet: usize,
    address: CellAddress,
    binding: &ExtensionBinding,
    arguments: crate::ArgumentsRef<'_>,
) -> crate::budget::EvalResult<crate::Value> {
    let Some(function) = session.extensions.and_then(|r| r.implementation(binding)) else {
        return Err(EvaluationFailure::UnsupportedCapability { capability: crate::CapabilityId::UserDefinedFunction, source: Some(EvaluationTarget { sheet, address }), detail: "bound extension implementation is absent or changed; rebind against the current registry".into() });
    };
    session
        .budget
        .allocation(session.extension_trace.invocations.len().saturating_add(1))?;
    session
        .extension_trace
        .invocations
        .push(ExtensionInvocation {
            target: EvaluationTarget { sheet, address },
            binding: binding.clone(),
        });
    session.read_lazy(|| {
        crate::DependencyRead::Extension(ExtensionInvocation {
            target: EvaluationTarget { sheet, address },
            binding: binding.clone(),
        })
    });
    let context = session.context;
    let mut host = CallbackHost {
        inner: crate::host::CallHost::new(session, sheet, address, arguments),
        capabilities: binding.registration.contract.capabilities,
        context,
    };
    let result = (|| -> ExtensionResult<crate::Value> {
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
                Ok(crate::Value::Scalar(match value {
                    CellValue::Number(n) => crate::finite(n),
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
                    "provider contract does not allow array output",
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
                Ok(crate::Value::Array(crate::ArrayView::unknown(rows)))
            }
        }
    })();
    match result {
        Ok(value) => Ok(value),
        Err(ExtensionFailure::Spreadsheet(error)) => Ok(crate::err(error)),
        Err(ExtensionFailure::Operational(mut failure)) => {
            failure.set_target(EvaluationTarget { sheet, address });
            Err(failure)
        }
    }
}
