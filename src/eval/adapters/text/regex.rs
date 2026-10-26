//! Modern PCRE2 functions retain operational failures and dynamic array geometry.
use crate::eval::adapters::elementwise::{Broadcast, MemberContext, Variadic, text as shared};
use crate::eval::host::{CallHost, NumericPolicy};
use crate::eval::*;
use poirot_functions::text::regex::{self, Case, Extraction, Program};

/// Connects the independent regex library's resource requests to evaluator budgets and execution limits.
/// Admission failures propagate as operational errors.
struct RegexHost<'a, 'b, 'c>(&'a mut CallHost<'b, 'c>);
impl regex::Host for RegexHost<'_, '_, '_> {
    /// Admission failures abort execution rather than becoming spreadsheet values.
    type Failure = EvaluationFailure;
    fn execution_limits(&self) -> regex::ExecutionLimits {
        self.0.regex_settings()
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
fn failure(host: &CallHost<'_, '_>, failure: regex::Failure<EvaluationFailure>) -> FunctionFailure {
    match failure {
        regex::Failure::Pattern | regex::Failure::TextLimit => CellError::Value.into(),
        regex::Failure::NoMatch => CellError::NA.into(),
        regex::Failure::Host(failure) => failure.into(),
        regex::Failure::Unsupported(detail) => {
            host.unsupported(CapabilityId::FunctionAdapter, detail)
        }
        regex::Failure::Resource => host.allocation_failure().into(),
        regex::Failure::SizeOverflow => host.size_overflow().into(),
        regex::Failure::InvalidState => host.unsupported(
            CapabilityId::FunctionAdapter,
            "PCRE2 capture geometry did not satisfy the checked adapter state",
        ),
        // PCRE2 public ABI: NOMEMORY=-48, HEAP_FAILED=121. The verified
        // pcre2-sys revision/source is recorded beside compile_failure in the domain crate.
        regex::Failure::Native(code) => host.resource_failure(
            if code == -48 || code == 121 { crate::eval::ResourceFailureKind::Allocation } else { crate::eval::ResourceFailureKind::BackendGuard },
            format!("PCRE2 native execution guard or representation failure ({code}); native consumption unavailable"),
        ).into(),
    }
}
fn text(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<String> {
    if let Some(text) = value.text_ref() {
        return host.project_text(text);
    }
    match value {
        Error(error) => Err((*error).into()),
        Number(value) => host.coercev2_number_to_text(*value, NumericPolicy::PinnedPoi),
        _ => host.coercev2_scalar_text(value),
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
        let program = Program::compile(&pattern, sensitivity, &mut RegexHost(host))
            .map_err(|e| failure(host, e))?;
        let values: FunctionResult<Vec<CellValue>> = match name {
            TextFunction::RegexTest => {
                let value = program
                    .test(&source, &mut RegexHost(host))
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
                        &mut RegexHost(host),
                    )
                    .map_err(|e| failure(host, e))?;
                host.check_values(1)?;
                Ok(vec![Text(value)])
            }
            TextFunction::RegexExtract => {
                let values = program
                    .extract(&source, extraction, &mut RegexHost(host))
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
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
