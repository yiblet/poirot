use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::host::{ArgumentCoercion, CallHost, ContextReads, PolicyReads};
use crate::eval::limits::{EvalResult, WorkKind};
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, core_projection};
use crate::model::CellValue;
use crate::model::CellValue::{Number, Text};

/// Format a selected value using the workbook's numeric and date context.
pub(super) fn text_format(
    host: &mut CallHost<'_, '_>,
    values: &[CellValue],
) -> FunctionResult<CellValue> {
    PolicyReads::new(host).text_policy();
    let value = values.first().ok_or(VALUE_ERROR)?.clone();
    let pattern =
        ArgumentCoercion::new(host).coerce_scalar_text(values.get(1).ok_or(VALUE_ERROR)?)?;
    let (value, format_value) = coerce_text_format_value(host, value)?;
    if !format_value {
        return Ok(value);
    }
    host.check_text(pattern.len())?;
    host.check_values(pattern.chars().count())?;
    let format = poirot_functions::format::compile_format_with_source(
        &pattern,
        poirot_functions::format::FormatCompilePolicy::default(),
        poirot_functions::format::FormatSourcePolicy::Poi7321209DataFormatter,
    )
    .map_err(|_| VALUE_ERROR)?;
    if format
        .sections()
        .iter()
        .any(|section| section.kind() == poirot_functions::format::SectionKind::DateTime)
    {
        ContextReads::new(host).selected_date_system();
    }
    let context = poirot_functions::format::FormatRenderContext {
        date_system: ContextReads::new(host).date_system_snapshot(),
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
    let display = poirot_functions::format::render_value(&value, &format, context)
        .map_err(core_projection::format_failure)?;
    Ok(Text(display.text))
}

// Unicode projections admit variable buffers; bounded numeric formatting
// checks its actual output through the same scalar-text path.
/// Render one operator operand under the selected text policy.
pub(super) fn coerce_text_render(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<String> {
    if matches!(value, Number(_)) {
        PolicyReads::new(host).text_policy();
    }
    ArgumentCoercion::new(host).coerce_scalar_text(value)
}

/// TEXT requests its own text admission policy; contextual parsing reuses the
/// same metered locale/calendar grammar as other numeric argument consumers.
pub(super) fn coerce_text_format_value(
    host: &mut CallHost<'_, '_>,
    value: CellValue,
) -> EvalResult<(CellValue, bool)> {
    use poirot_functions::coercion::compatibility::{NumericTextDecision, TextFormatPolicy};
    PolicyReads::new(host).builtin_policy();
    let policy = if PolicyReads::new(host).builtin_policy_snapshot()
        == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        TextFormatPolicy::Contextual
    } else {
        TextFormatPolicy::PinnedPoi
    };
    poirot_functions::coercion::compatibility::coerce_text_format_value(value, policy, |view| {
        host.charge_kind(WorkKind::TextByte, view.len_units() as u64)?;
        if view.utf8_len().is_err() {
            return Ok(None);
        }
        let projected = match ArgumentCoercion::new(host).scalar_text_view(view) {
            Ok(text) => text,
            Err(FunctionFailure::Operational(error)) => return Err(error),
            Err(FunctionFailure::Spreadsheet(_)) => return Ok(None),
        };
        if policy == TextFormatPolicy::PinnedPoi {
            return Ok(
                poirot_functions::coercion::compatibility::coerce_arithmetic_text(&projected).ok(),
            );
        }
        let number = match ArgumentCoercion::new(host).formatted_number_text(&projected) {
            Ok(NumericTextDecision::Number(result)) => result.map_err(FunctionFailure::from),
            Ok(NumericTextDecision::DateTime) => {
                ArgumentCoercion::new(host).legacy_numeric_date_time(&projected)
            }
            Err(error) => Err(error),
        };
        match number {
            Ok(number) => Ok(Some(number)),
            Err(FunctionFailure::Spreadsheet(_)) => Ok(None),
            Err(FunctionFailure::Operational(error)) => Err(error),
        }
    })
}
