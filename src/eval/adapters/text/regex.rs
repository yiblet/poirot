//! Modern PCRE2 functions retain operational failures and dynamic array geometry.
use crate::eval::adapters::elementwise::{Broadcast, MemberContext, Variadic, text as shared};
use crate::eval::functions::registry::TextFunction;
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::values::{FunctionError, FunctionResult, Value, err};
use crate::model::CellValue::{Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::text::regex::{self, Case, Extraction, Program};

/// Connects the independent regex library's resource requests to evaluator budgets and execution limits.
/// Admission failures propagate as operational errors.
struct RegexHost<'a, 'b, 'c>(&'a mut CallHost<'b, 'c>, regex::ExecutionLimits);

impl<'a, 'b, 'c> RegexHost<'a, 'b, 'c> {
    /// Borrow the call host with the run's regex execution guards captured beside it.
    fn new(host: &'a mut CallHost<'b, 'c>) -> Self {
        let limits = host.regex_settings();
        Self(host, limits)
    }
}

impl regex::Host for RegexHost<'_, '_, '_> {
    /// Admission failures abort execution rather than becoming spreadsheet values.
    type Error = EvaluationError;

    fn execution_limits(&self) -> regex::ExecutionLimits {
        self.1
    }

    fn work(&mut self, units: usize) -> EvalResult<()> {
        self.0.charge_work(units as u64)
    }

    fn text(&mut self, bytes: usize) -> EvalResult<()> {
        self.0.check_text(bytes)
    }

    fn values(&mut self, count: usize) -> EvalResult<()> {
        self.0.check_values(count)
    }
}

fn failure(
    host: &CallHost<'_, '_>,
    failure: poirot_functions::text::RegexError<EvaluationError>,
) -> FunctionError {
    match failure {
        poirot_functions::text::RegexError::Pattern(_) | poirot_functions::text::RegexError::TextLimit => CellError::Value.into(),
        poirot_functions::text::RegexError::NoMatch => CellError::NA.into(),
        poirot_functions::text::RegexError::Host(failure) => failure.into(),
        error @ (poirot_functions::text::RegexError::ByteEscape | poirot_functions::text::RegexError::MatchBoundary | poirot_functions::text::RegexError::CaptureBoundary) => host.unsupported(CapabilityId::FunctionAdapter, crate::eval::limits::EvaluationReasonError::Regex(std::sync::Arc::new(error))),
        poirot_functions::text::RegexError::Resource => host.allocation_failure().into(),
        poirot_functions::text::RegexError::SizeOverflow => host.size_overflow().into(),
        poirot_functions::text::RegexError::InvalidState => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::Pcre2CaptureGeometryDidNotSatisfyTheCheckedAdapterState,
        ),
        // PCRE2 public ABI: NOMEMORY=-48, HEAP_FAILED=121. The verified
        // pcre2-sys revision/source is recorded beside compile_failure in the domain crate.
        error @ poirot_functions::text::RegexError::Native(_) => {
            let kind = match &error {
                poirot_functions::text::RegexError::Native(source) if matches!(source.code(), -48 | 121) => crate::eval::limits::ResourceFailureKind::Allocation,
                _ => crate::eval::limits::ResourceFailureKind::BackendGuard,
            };
            host.resource_failure(kind, crate::eval::limits::EvaluationReasonError::Regex(std::sync::Arc::new(error))).into()
        },
    }
}

fn text(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<String> {
    if let Some(text) = value.text_ref() {
        return ArgumentCoercion::new(host).project_text(text);
    }
    match value {
        Error(error) => Err((*error).into()),
        Number(value) => ArgumentCoercion::new(host).coerce_number_to_text(*value),
        _ => ArgumentCoercion::new(host).coerce_scalar_text(value),
    }
}

fn case(value: f64) -> FunctionResult<Case> {
    match value {
        0. => Ok(Case::Sensitive),
        1. => Ok(Case::Insensitive),
        _ => Err(CellError::Value.into()),
    }
}

fn mode(value: f64) -> FunctionResult<Extraction> {
    match value {
        0. => Ok(Extraction::First),
        1. => Ok(Extraction::All),
        2. => Ok(Extraction::Groups),
        _ => Err(CellError::Value.into()),
    }
}

/// Runs the selected regex operation for broadcast operands and returns its possibly expanded result.
/// Shared text execution assembles capture or match arrays into the worksheet output.
struct RegexKernel(TextFunction);

impl shared::ExpansionKernel for RegexKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<shared::Expansion> {
        let name = self.0;
        let source = arguments.operand(context, 0)?;
        let pattern = arguments.operand(context, 1)?;
        let source = text(context.host, &source.value)?;
        let pattern = text(context.host, &pattern.value)?;
        let (extraction, occurrence, replacement, sensitivity) = match name {
            TextFunction::RegexTest => (
                Extraction::First,
                0,
                None,
                case(arguments.number(context, 2, Some(0.0))?)?,
            ),
            TextFunction::RegexExtract => (
                mode(arguments.number(context, 2, Some(0.0))?)?,
                0,
                None,
                case(arguments.number(context, 3, Some(0.0))?)?,
            ),
            TextFunction::RegexReplace => {
                let operand = arguments.operand(context, 2)?;
                let replacement = text(context.host, &operand.value)?;
                let occurrence = arguments.number(context, 3, Some(0.0))?;
                if !occurrence.is_finite() || occurrence.abs() >= i64::MAX as f64 {
                    return Err(CellError::Value.into());
                }
                (
                    Extraction::First,
                    occurrence.trunc() as i64,
                    Some(replacement),
                    case(arguments.number(context, 4, Some(0.0))?)?,
                )
            }
            _ => return Err(CellError::Value.into()),
        };
        let host = &mut *context.host;
        let program = Program::compile(&pattern, sensitivity, &mut RegexHost::new(host))
            .map_err(|e| failure(host, e))?;
        let values: FunctionResult<Vec<CellValue>> = match name {
            TextFunction::RegexTest => {
                let value = program
                    .test(&source, &mut RegexHost::new(host))
                    .map_err(|e| failure(host, e))?;
                host.check_values(1)?;
                Ok(vec![Boolean(value)])
            }
            TextFunction::RegexReplace => {
                let value = program
                    .replace(
                        &source,
                        replacement.as_deref().ok_or(CellError::Value)?,
                        occurrence,
                        &mut RegexHost::new(host),
                    )
                    .map_err(|e| failure(host, e))?;
                host.check_values(1)?;
                Ok(vec![Text(value)])
            }
            TextFunction::RegexExtract => {
                let values = program
                    .extract(&source, extraction, &mut RegexHost::new(host))
                    .map_err(|e| failure(host, e))?;
                host.check_values(values.len())?;
                Ok(values.into_iter().map(Text).collect())
            }
            _ => Err(CellError::Value.into()),
        };
        let values = values?;
        let length = values.len();
        let groups = name == TextFunction::RegexExtract && extraction == Extraction::Groups;
        Ok(shared::Expansion {
            values,
            rows: if groups { 1 } else { length },
            columns: if groups { length } else { 1 },
        })
    }
}

/// Evaluate modern regex functions under the documentation profile.
pub(in crate::eval) fn execute(
    name: TextFunction,
    mut host: CallHost<'_, '_>,
    address: CellAddress,
    length: usize,
) -> EvalResult<Value> {
    let valid = match name {
        TextFunction::RegexTest => (2..=3).contains(&length),
        TextFunction::RegexExtract => (2..=4).contains(&length),
        TextFunction::RegexReplace => (3..=5).contains(&length),
        _ => false,
    };
    if !valid {
        return Ok(err(CellError::Value));
    }
    match shared::expand(&mut host, address, RegexKernel(name)) {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}
