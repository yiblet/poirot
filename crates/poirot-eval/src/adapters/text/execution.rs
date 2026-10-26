//! Family-owned evaluation; generic argument coercion remains in the execution core.
use crate::adapters::elementwise::{
    self, Broadcast, FixedScalar, Kernel, MemberContext, Optional, Variadic, WholeCollection,
    collections,
};
use crate::host::CallHost;
use crate::*;
#[path = "execution/operator_units.rs"]
mod operator_units;
/// Dispatch text builtins under the selected text and compatibility policies.
pub(crate) fn execute(
    mut host: CallHost<'_, '_>,
    at: CellAddress,
    name: TextFunction,
    length: usize,
) -> EvalResult<Value> {
    if matches!(
        name,
        TextFunction::RegexTest | TextFunction::RegexExtract | TextFunction::RegexReplace
    ) {
        return adapters::text::execute_regex(name, host, at, length);
    }
    if matches!(
        name,
        TextFunction::TextBefore
            | TextFunction::TextAfter
            | TextFunction::TextSplit
            | TextFunction::ArrayToText
            | TextFunction::ValueToText
    ) {
        return adapters::text::execute_modern(name, host, at, length);
    }
    // CODE's documented numeric contract is independent of text indexing policy.
    if name == TextFunction::Code
        && host.builtin_semantics_policy() == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        return adapters::text::execute_units(host, name, length);
    }
    if matches!(
        name,
        TextFunction::Len
            | TextFunction::Left
            | TextFunction::Right
            | TextFunction::Mid
            | TextFunction::Replace
            | TextFunction::Concat
            | TextFunction::Find
            | TextFunction::Search
            | TextFunction::Lower
            | TextFunction::Upper
            | TextFunction::Rept
            | TextFunction::Code
            | TextFunction::Proper
    ) {
        host.selected_text_policy();
        {
            if host.text_policy_snapshot().source
                == TextSourceVersion::ExcelCompatibilityV2Unverified
            {
                return Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "unverified Excel text policy".into(),
                });
            }
            return adapters::text::execute_units(host, name, length);
        }
    }

    if name == TextFunction::TextJoin {
        return super::finish(collections::execute(&mut host, at, JoinKernel));
    }
    if !FunctionId::Text(name).descriptor().arity.accepts(length) {
        return Ok(err(VALUE_ERROR));
    }
    if name == TextFunction::NumberValue {
        return super::finish(elementwise::execute(&mut host, at, NumberKernel));
    }
    super::finish(elementwise::execute(&mut host, at, ScalarKernel(name)))
}

/// Locale-aware NUMBERVALUE retains omitted separator defaults until coercion.
struct NumberKernel;
impl Kernel for NumberKernel {
    type Signature = (Broadcast, Optional<Broadcast>, Optional<Broadcast>);
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (source, decimal, grouping): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let source = source.read(context)?;
        let decimal = decimal
            .0
            .as_ref()
            .map(|role| role.read(context))
            .transpose()?;
        let grouping = grouping
            .0
            .as_ref()
            .map(|role| role.read(context))
            .transpose()?;
        let host = &mut *context.host;
        let text = host.coercev2_scalar_text(&source.value)?;
        let decimal = decimal
            .as_ref()
            .filter(|operand| !operand.missing)
            .map(|operand| host.coercev2_scalar_text(&operand.value))
            .transpose()?;
        let grouping = grouping
            .as_ref()
            .filter(|operand| !operand.missing)
            .map(|operand| host.coercev2_scalar_text(&operand.value))
            .transpose()?;
        if decimal.is_none() || grouping.is_none() {
            host.read_number_locale();
        }
        let bound = text
            .len()
            .checked_add(1)
            .ok_or_else(|| host.size_overflow())?;
        host.check_text(bound)?;
        host.check_values(bound.saturating_mul(3))?;
        host.charge_work(bound.saturating_mul(8) as u64)?;
        let number = poirot_functions_text::numeric::coercev2_number_value_excel(
            &text,
            host.number_locale_snapshot(),
            decimal.as_deref(),
            grouping.as_deref(),
        )
        .map_err(|error| match error {
            poirot_functions_text::numeric::ParseFailure::Spreadsheet(
                poirot_functions_text::Error::Value,
            ) => FunctionFailure::Spreadsheet(VALUE_ERROR),
            poirot_functions_text::numeric::ParseFailure::Spreadsheet(
                poirot_functions_text::Error::Num,
            ) => FunctionFailure::Spreadsheet(Num),
            poirot_functions_text::numeric::ParseFailure::Source(reason) => host.unsupported(
                CapabilityId::FunctionAdapter,
                format!("NUMBERVALUE separator representation: {reason:?}"),
            ),
        })?;
        Ok(Number(number))
    }
}

/// Scalar text calculation after shared source binding and rectangular traversal.
struct ScalarKernel(TextFunction);
impl Kernel for ScalarKernel {
    type Signature = Variadic<Broadcast>;
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = arguments.scalar_values(context)?;
        text_scalar(context.host, self.0, &values)
    }
}

/// TEXTJOIN consumes complete collections while scalar options retain their own roles.
struct JoinKernel;
impl collections::CollectionKernel for JoinKernel {
    type Signature = Variadic<WholeCollection>;
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        if !(3..=254).contains(&arguments.len()) {
            return Err(VALUE_ERROR.into());
        }
        // Microsoft TEXTJOIN example 3 uses a repeating delimiter range.
        // https://support.microsoft.com/en-gb/excel/functions/textjoin-function
        let delimiters = elementwise::text::collection_units(host, arguments.iter().take(1))?;
        let ignore = FixedScalar::at(1).read(host)?;
        let ignore = crate::core_projection::coercev2_operand_boolean(&ignore.value)?;
        let (output, _) = collections::fold(
            host,
            arguments.iter().skip(2),
            (Vec::new(), 0usize),
            |host, (output, count), operand| {
                let text = elementwise::text::units(host, &operand)?;
                if ignore && text.is_empty() {
                    return Ok(());
                }
                let separator = if *count > 0 && !delimiters.is_empty() {
                    // SAFETY: count is positive and the delimiter sequence is nonempty in this branch.
                    delimiters
                        .get((*count - 1) % delimiters.len())
                        .map_or(&[][..], Vec::as_slice)
                } else {
                    &[]
                };
                let additional = separator
                    .len()
                    .checked_add(text.len())
                    .ok_or_else(|| host.size_overflow())?;
                let size = output
                    .len()
                    .checked_add(additional)
                    .ok_or_else(|| host.size_overflow())?;
                if size > 32767 {
                    return Err(VALUE_ERROR.into());
                }
                host.check_text_capacity(size.saturating_mul(2))?;
                host.charge_work(additional as u64)?;
                output
                    .try_reserve(additional)
                    .map_err(|_| host.allocation_failure())?;
                // SAFETY: all separator and text units were reserved immediately above.
                output.extend_from_slice(separator);
                // SAFETY: the local additional reservation includes every text unit.
                output.extend_from_slice(&text);
                *count = count.checked_add(1).ok_or_else(|| host.size_overflow())?;
                Ok(())
            },
        )?;
        Ok(elementwise::text::value(output).into())
    }
}

/// Format a selected value using the workbook's numeric and date context.
fn text_format(host: &mut CallHost<'_, '_>, values: &[CellValue]) -> FunctionResult<CellValue> {
    host.selected_text_policy();
    let value = values.first().ok_or(VALUE_ERROR)?.clone();
    let pattern = host.coercev2_scalar_text(values.get(1).ok_or(VALUE_ERROR)?)?;
    let (value, format_value) = coercev2_text_format_value(host, value)?;
    if !format_value {
        return Ok(value);
    }
    host.check_text(pattern.len())?;
    host.check_values(pattern.chars().count())?;
    let format = poirot_format::compile_format_with_source(
        &pattern,
        poirot_format::FormatCompilePolicy::default(),
        poirot_format::FormatSourcePolicy::Poi7321209DataFormatter,
    )
    .map_err(|_| VALUE_ERROR)?;
    if format
        .sections()
        .iter()
        .any(|section| section.kind() == poirot_format::SectionKind::DateTime)
    {
        host.selected_date_system();
    }
    let context = poirot_format::FormatRenderContext {
        date_system: host.date_system_snapshot(),
        width: None,
        ..Default::default()
    };
    if let Some(text) = value.text_ref() {
        host.charge_kind(WorkKind::TextByte, text.storage_bytes() as u64)?;
    }
    let reservation = format
        .render_byte_bound(&value, context)
        .map_err(core_projection::format_failure)?;
    host.check_text(reservation)?;
    let display = poirot_format::render_value(&value, &format, context)
        .map_err(core_projection::format_failure)?;
    Ok(Text(display.text))
}

/// Evaluate eager scalar text operations in source order.
fn text_scalar(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    a: &[CellValue],
) -> FunctionResult<CellValue> {
    let n = a.len();
    if !FunctionId::Text(name).descriptor().arity.accepts(n) {
        return Err(VALUE_ERROR.into());
    }

    if name == TextFunction::Concatenate {
        let mut result = Vec::new();
        for value in a {
            let text = host.coercev2_text_units(value)?;
            let size = result
                .len()
                .checked_add(text.len())
                .ok_or_else(|| host.size_overflow())?;
            if size > 32767 {
                return Err(VALUE_ERROR.into());
            }
            host.check_text_capacity(size.saturating_mul(2))?;
            host.charge_work(text.len() as u64)?;
            result
                .try_reserve(text.len())
                .map_err(|_| host.allocation_failure())?;
            // SAFETY: text.len() additional slots were reserved immediately above.
            result.extend_from_slice(&text);
        }
        return Ok(elementwise::text::value(result));
    }
    if name == TextFunction::Text {
        return text_format(host, a);
    }
    if name == TextFunction::Arabic {
        let value = a.first().ok_or(VALUE_ERROR)?;
        let view = poirot_coercionv2::roman::coercev2_arabic_scalar(value.into())?;
        let text = host.scalar_text_view(view)?;
        return Ok(Number(f64::from(
            poirot_coercionv2::roman::coercev2_arabic_text(&text)?,
        )));
    }
    if name == TextFunction::Roman {
        let number = host
            .coercev2_scalar_number(
                a.first().ok_or(VALUE_ERROR)?,
                crate::host::NumericPolicy::PinnedPoi,
            )?
            .floor();
        if !(0.0..=3999.0).contains(&number) {
            return Err(VALUE_ERROR.into());
        }
        let form = match a.get(1) {
            None | Some(Blank) => 0,
            Some(Boolean(value)) => {
                if *value {
                    0
                } else {
                    4
                }
            }
            Some(value) => host
                .coercev2_scalar_number(value, crate::host::NumericPolicy::PinnedPoi)?
                .floor() as i32,
        };
        if !(0..=4).contains(&form) {
            return Err(VALUE_ERROR.into());
        }
        host.check_text(16)?;
        return Ok(Text(poirot_functions_text::roman(
            number as u16,
            form as u8,
        )));
    }
    if matches!(name, TextFunction::Char | TextFunction::UniChar) {
        let code = elementwise::text::integer_argument(host, a, 0)?;
        if code < 1.
            || code
                > if name == TextFunction::Char {
                    255.
                } else {
                    1114111.
                }
        {
            return Err(VALUE_ERROR.into());
        }
        let character = char::from_u32(code as u32).ok_or(VALUE_ERROR)?;
        host.check_text(character.len_utf8())?;
        return Ok(Text(character.to_string()));
    }
    if name == TextFunction::Value {
        let value = a.first().ok_or(VALUE_ERROR)?;
        let number =
            poirot_coercionv2::compatibility::coercev2_value_number(value.into(), |text| {
                let text = host.scalar_text_view(text)?;
                match host.formatted_number_text(&text)? {
                    poirot_coercionv2::compatibility::NumericTextDecision::Number(result) => {
                        result.map_err(FunctionFailure::from)
                    }
                    poirot_coercionv2::compatibility::NumericTextDecision::DateTime => {
                        host.legacy_numeric_date_time(&text)
                    }
                }
            })?;
        return Ok(Number(number));
    }

    let t = elementwise::text::argument(host, a, 0)?;
    host.check_text(t.len())?;
    host.check_values(t.chars().count())?;

    Ok(match name {
        TextFunction::NumberValue
        | TextFunction::Proper
        | TextFunction::Arabic
        | TextFunction::Roman
        | TextFunction::TextJoin => return Err(VALUE_ERROR.into()),
        TextFunction::Trim => {
            let mut result = String::new();
            result
                .try_reserve_exact(t.len())
                .map_err(|_| host.allocation_failure())?;
            for part in t.split(' ').filter(|s| !s.is_empty()) {
                if !result.is_empty() {
                    // SAFETY: Collapsing spaces and omitting edge spaces cannot exceed t.len().
                    result.push(' ');
                }
                // SAFETY: Selected parts and collapsed spaces cannot exceed t.len(), reserved locally.
                result.push_str(part);
            }
            Text(result)
        }
        TextFunction::Clean => {
            let mut result = String::new();
            result
                .try_reserve_exact(t.len())
                .map_err(|_| host.allocation_failure())?;
            for c in t.chars().filter(|c| (*c as u32) >= 32) {
                // SAFETY: Removing characters cannot exceed the locally reserved input byte length.
                result.push(c);
            }
            Text(result)
        }
        TextFunction::Unicode => Number(t.chars().next().ok_or(VALUE_ERROR)? as u32 as f64),
        TextFunction::Exact => Boolean(t == elementwise::text::argument(host, a, 1)?),
        TextFunction::Substitute => {
            let old = elementwise::text::argument(host, a, 1)?;
            let new = elementwise::text::argument(host, a, 2)?;
            if old.is_empty() {
                Text(t)
            } else {
                let nth = if n == 3 {
                    None
                } else {
                    let nth = elementwise::text::integer_argument(host, a, 3)?;
                    if nth < 1. {
                        return Err(VALUE_ERROR.into());
                    }
                    Some(nth as usize)
                };
                host.check_text(t.len())?;
                let matches = t.match_indices(&old).count();
                let replaced = nth.map_or(matches, |nth| usize::from(nth <= matches));
                let bytes = replaced
                    .checked_mul(old.len())
                    .and_then(|removed| t.len().checked_sub(removed))
                    .and_then(|remaining| {
                        replaced
                            .checked_mul(new.len())
                            .and_then(|added| remaining.checked_add(added))
                    })
                    .ok_or_else(|| host.size_overflow())?;
                host.check_text(bytes)?;
                {
                    let mut result = String::new();
                    result
                        .try_reserve_exact(bytes)
                        .map_err(|_| host.allocation_failure())?;
                    let mut occurrence = 0;
                    for part in t.split_inclusive(&old) {
                        if let Some(prefix) = part.strip_suffix(&old) {
                            occurrence += 1;
                            // SAFETY: bytes is the checked final replacement length, reserved locally.
                            result.push_str(prefix);
                            // SAFETY: bytes accounts for all replacements selected by nth.
                            result.push_str(if nth.is_none() || Some(occurrence) == nth {
                                &new
                            } else {
                                &old
                            });
                        } else {
                            // SAFETY: bytes includes the unchanged final suffix.
                            result.push_str(part);
                        }
                    }
                    Text(result)
                }
            }
        }
        TextFunction::Text
        | TextFunction::Char
        | TextFunction::UniChar
        | TextFunction::Value
        | TextFunction::Concat
        | TextFunction::Concatenate
        | TextFunction::Len
        | TextFunction::Left
        | TextFunction::Right
        | TextFunction::Mid
        | TextFunction::Replace
        | TextFunction::Find
        | TextFunction::Search
        | TextFunction::Lower
        | TextFunction::Upper
        | TextFunction::Rept
        | TextFunction::Code
        | TextFunction::TextBefore
        | TextFunction::TextAfter
        | TextFunction::TextSplit
        | TextFunction::ArrayToText
        | TextFunction::ValueToText
        | TextFunction::RegexTest
        | TextFunction::RegexExtract
        | TextFunction::RegexReplace => return Err(VALUE_ERROR.into()),
    })
}

fn text_unsupported(reason: poirot_text::Unsupported) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: None,
        detail: format!("text semantics: {reason:?}"),
    }
    .into()
}

// Unicode projections admit variable buffers; bounded numeric formatting
// checks its actual output through the same scalar-text path.
/// Render one operator operand under the selected text policy.
fn coercev2_text_render(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<String> {
    if matches!(value, Number(_)) {
        host.selected_text_policy();
    }
    host.coercev2_scalar_text(value)
}
/// Execute concatenation with selected text-unit semantics and metered storage.
pub(crate) fn concat_operator(
    host: &mut CallHost<'_, '_>,
    left: CellValue,
    right: CellValue,
) -> FunctionResult<CellValue> {
    if let Error(error) = left {
        return Ok(Error(error));
    }
    if let Error(error) = right {
        return Ok(Error(error));
    }
    host.selected_text_policy();
    let policy = host
        .text_policy_snapshot()
        .operation(poirot_text::Operation::ConcatOperator)
        .map_err(text_unsupported)?;
    if policy.units == poirot_text::Units::Utf16CodeUnits {
        return operator_units::text_unit_operator(host, &left, &right);
    }
    let left = coercev2_text_render(host, &left)?;
    let right = coercev2_text_render(host, &right)?;
    let bytes = poirot_text::checked_total(left.len(), right.len()).map_err(text_unsupported)?;
    host.charge_kind(WorkKind::TextByte, bytes as u64)?;
    let measured = poirot_text::measure_concat([left.as_str(), right.as_str()], policy)
        .map_err(text_unsupported)?;
    if !measured.within_limit {
        return Err(VALUE_ERROR.into());
    }
    host.check_text(measured.bytes)?;
    let mut output = String::new();
    output
        .try_reserve_exact(bytes)
        .map_err(|_| host.allocation_failure())?;
    // SAFETY: bytes is the checked sum of the two appended strings.
    output.push_str(&left);
    // SAFETY: bytes reserves the checked sum of the two appended strings.
    output.push_str(&right);
    Ok(Text(output))
}

/// TEXT requests its own text admission policy; contextual parsing reuses the
/// same metered locale/calendar grammar as other numeric argument consumers.
fn coercev2_text_format_value(
    host: &mut CallHost<'_, '_>,
    value: CellValue,
) -> EvalResult<(CellValue, bool)> {
    use poirot_coercionv2::compatibility::{NumericTextDecision, TextFormatPolicy};
    host.selection_builtin_policy();
    let policy = if host.builtin_semantics_policy() == BuiltinSemanticsPolicy::ExcelDocumentation {
        TextFormatPolicy::Contextual
    } else {
        TextFormatPolicy::PinnedPoi
    };
    poirot_coercionv2::compatibility::coercev2_text_format_value(value, policy, |view| {
        host.charge_kind(WorkKind::TextByte, view.len_units() as u64)?;
        if view.utf8_len().is_err() {
            return Ok(None);
        }
        let projected = match host.scalar_text_view(view) {
            Ok(text) => text,
            Err(FunctionFailure::Operational(error)) => return Err(error),
            Err(FunctionFailure::Spreadsheet(_)) => return Ok(None),
        };
        if policy == TextFormatPolicy::PinnedPoi {
            return Ok(poirot_coercionv2::compatibility::coercev2_arithmetic_text(&projected).ok());
        }
        let number = match host.formatted_number_text(&projected) {
            Ok(NumericTextDecision::Number(result)) => result.map_err(FunctionFailure::from),
            Ok(NumericTextDecision::DateTime) => host.legacy_numeric_date_time(&projected),
            Err(error) => Err(error),
        };
        match number {
            Ok(number) => Ok(Some(number)),
            Err(FunctionFailure::Spreadsheet(_)) => Ok(None),
            Err(FunctionFailure::Operational(error)) => Err(error),
        }
    })
}

#[cfg(test)]
#[path = "execution/semantics_tests.rs"]
mod semantics_tests;

/// Prepare the text family's argument demand before executing its handler.
pub(crate) fn execute_function(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    name: TextFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let host = if matches!(name, TextFunction::Concat | TextFunction::TextJoin) {
        CallHost::new(session, s, at, args)
    } else {
        CallHost::new_array(session, s, at, args, args.len())
    };
    execute(host, at, name, args.len())
}
