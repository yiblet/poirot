//! Financial adapters over actual deferred Session operands.
use crate::adapters::failure::project_host;
use crate::adapters::financial::policy::{
    self as helper, CurrencyContext, EvaluationPort, Failure, Scalar,
};
use crate::host::{
    CallHost, MissingPolicy, NumericPolicy, OperandKind, OperandOrigin, ScalarOperand,
};
use crate::*;
use poirot_functions_core::Input;

fn failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}

fn scalar(host: &mut CallHost<'_, '_>, value: ScalarOperand) -> FunctionResult<Scalar> {
    if value.missing {
        return Ok(Scalar::Missing);
    }
    Ok(match value.value {
        Number(value) => Scalar::Number(value),
        Boolean(value) => Scalar::Boolean(value),
        Text(value) => Scalar::Text(value),
        CellValue::TextUtf16(text) => {
            Scalar::Text(host.project_text(poirot_model::TextRef::Utf16(text.units()))?)
        }
        Blank => Scalar::Blank,
        Error(value) => Scalar::Error(value),
    })
}

impl EvaluationPort for CallHost<'_, '_> {
    /// Deferred argument identity supplied to the family's host port.
    type Argument = usize;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = EvaluationFailure;

    fn resolve_scalar(&mut self, index: &usize) -> Result<Scalar, Failure<EvaluationFailure>> {
        let value = self
            .input(*index, MissingPolicy::Blank)
            .force()
            .map_err(failure)?;
        scalar(self, value).map_err(failure)
    }
    fn collection_len(&mut self, index: &usize) -> Result<usize, Failure<EvaluationFailure>> {
        // Validate rectangular array shape before the helper reserves its collection.
        if self.kind(*index).map_err(failure)? == OperandKind::Array {
            self.shape(*index).map_err(failure)?;
        }
        CallHost::collection_len(self, *index).map_err(failure)
    }
    fn collection_cell(
        &mut self,
        index: &usize,
        offset: usize,
    ) -> Result<(Scalar, helper::Origin), Failure<EvaluationFailure>> {
        let value = CallHost::collection_cell(self, *index, offset).map_err(failure)?;
        let origin = match value.origin {
            OperandOrigin::Direct => helper::Origin::Direct,
            OperandOrigin::ArrayElement => helper::Origin::Array,
            OperandOrigin::ReferencedCell => helper::Origin::Reference,
        };
        Ok((scalar(self, value).map_err(failure)?, origin))
    }
    fn charge_work(&mut self, units: u64) -> Result<(), EvaluationFailure> {
        CallHost::charge_work(self, units)
    }
    fn check_collection(&mut self, elements: usize) -> Result<(), EvaluationFailure> {
        self.check_values(elements)
    }
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        CallHost::check_text(self, bytes)
    }
    fn coercev2_text_number(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        self.coercev2_number_text(text, NumericPolicy::PinnedPoi)
            .map_err(failure)
    }
    fn currency_format(&mut self) -> Result<CurrencyContext, Failure<EvaluationFailure>> {
        self.currency_context().map_err(failure)
    }
}

/// Bridge financial helpers to lazy operands and explicit currency settings.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: helper::FinancialFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    session.read(DependencyRead::BuiltinPolicy);
    let policy = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        helper::SemanticsPolicy::ExcelDocumentation
    } else {
        helper::SemanticsPolicy::PinnedPoi
    };
    // Arity errors must precede work, allocation, and any operand evaluation.
    if !function.accepts_with_policy(arguments.len(), policy) {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new(session, sheet, address, arguments);
    host.check_values(arguments.len())?;
    let indices: poirot_functions_core::ParamVec<_> = (0..arguments.len()).collect();
    match helper::evaluate_with_policy(&mut host, function, &indices, policy) {
        Ok(helper::Output::Number(value)) => Ok(Number(value).into()),
        Ok(helper::Output::Text(value)) => Ok(Text(value).into()),
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(Failure::UnsupportedCapability(detail)) => {
            Err(host.capability_failure(CapabilityId::FunctionAdapter, detail))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undeclared_currency_locale_remains_an_operational_diagnostic() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        assert!(matches!(
            execute(
                &mut session,
                0,
                CellAddress { row: 0, col: 0 },
                helper::FinancialFunction::Dollar,
                (&[Expr::Number(1.)]).into(),
            ),
            Err(EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::Formatting,
                ..
            })
        ));
    }
}

/// Financial operation contracts, coercion, and formatting policies.
pub mod policy;

mod euro;
mod modern;
mod odd;
mod securities;
/// Dispatch documented euro conversion separately from unchanged pinned helpers.
pub(crate) fn execute_function(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: FinancialFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if function == FinancialFunction::Euroconvert {
        return euro::execute(session, sheet, address, args);
    }
    if odd::handles(function) {
        return odd::execute(session, sheet, address, function, args);
    }
    if securities::handles(function) {
        return securities::execute(session, sheet, address, function, args);
    }
    if let Some(helper) = function.helper() {
        execute(session, sheet, address, helper, args)
    } else {
        modern::execute(session, sheet, address, function, args)
    }
}
