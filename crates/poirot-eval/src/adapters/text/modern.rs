//! Modern text adapters preserve deferred arguments, UTF-16 and spill geometry.
//! Extraction delimiter arrays currently broadcast. Whether native Excel treats
//! them as delimiter sets is an open contract question tracked by audit 024.
use crate::adapters::elementwise::{
    self, ArgumentRole, Broadcast, Deferred, FixedScalar, Kernel, MemberContext, Optional,
    WholeCollection, collections,
};
use crate::host::{CallHost, NumericPolicy, OperandDescription, ScalarOperand};
use crate::*;
use poirot_functions_text::modern::{self, Case, Side, Split};
use poirot_model::ExcelText;

fn failure(host: &CallHost<'_, '_>, error: modern::Error) -> FunctionFailure {
    match error {
        modern::Error::Value => CellError::Value.into(),
        modern::Error::NotFound => CellError::NA.into(),
        modern::Error::Empty => CellError::Calc.into(),
        modern::Error::SizeOverflow => host.size_overflow().into(),
        modern::Error::AllocationFailure => host.allocation_failure().into(),
        modern::Error::InvalidGeometry => EvaluationFailure::StalePlan.into(),
    }
}
fn admit_text(host: &mut CallHost<'_, '_>, units: usize) -> EvalResult<()> {
    let bytes = units.checked_mul(2).ok_or_else(|| host.size_overflow())?;
    host.check_text(bytes)?;
    host.check_values(units)
}
fn coercev2_text_units(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    errors: bool,
) -> FunctionResult<Vec<u16>> {
    if let Some(text) = value.text_ref() {
        admit_text(host, text.len_units())?;
        return Ok(text.units().collect());
    }
    let text = match value {
        Error(error) if errors => error.to_string(),
        Error(error) => return Err((*error).into()),
        Number(number) => host.coercev2_number_to_text(*number, NumericPolicy::PinnedPoi)?,
        _ => host.coercev2_scalar_text(value)?,
    };
    admit_text(host, text.encode_utf16().count())?;
    Ok(text.encode_utf16().collect())
}
fn text_value(units: Vec<u16>) -> CellValue {
    CellValue::TextUtf16(ExcelText::from_units(units))
}
pub(super) fn description(
    host: &mut CallHost<'_, '_>,
    index: usize,
) -> FunctionResult<OperandDescription> {
    let shape = host.describe(index)?;
    if shape.sheets != 1 || shape.rows == 0 || shape.columns == 0 {
        return Err(CellError::Value.into());
    }
    Ok(shape)
}
pub(super) fn number(
    host: &mut CallHost<'_, '_>,
    operand: ScalarOperand,
    default: f64,
) -> FunctionResult<f64> {
    if operand.missing {
        Ok(default)
    } else {
        host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)
    }
}
fn mode(n: f64) -> FunctionResult<Case> {
    match n {
        0. => Ok(Case::Sensitive),
        1. => Ok(Case::Insensitive),
        _ => Err(CellError::Value.into()),
    }
}
fn bool_mode(n: f64) -> FunctionResult<bool> {
    match n {
        0. => Ok(false),
        1. => Ok(true),
        _ => Err(CellError::Value.into()),
    }
}
pub(super) fn shape_admission(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    columns: usize,
) -> FunctionResult<()> {
    if rows > poirot_model::MAX_ROWS as usize || columns > poirot_model::MAX_COLUMNS as usize {
        return Err(CellError::Num.into());
    }
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    Ok(host.check_values(rows)?)
}
/// Delimiter extraction retains optional fallback demand outside broadcast admission.
struct ExtractionKernel(TextFunction);
impl elementwise::text::FallbackKernel for ExtractionKernel {
    type Signature = (
        Broadcast,
        Broadcast,
        Optional<Broadcast>,
        Optional<Broadcast>,
        Optional<Broadcast>,
        Optional<Deferred<Broadcast>>,
    );
    fn fallback(arguments: &Self::Signature) -> Option<&Deferred<Broadcast>> {
        arguments.5.0.as_ref()
    }
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<elementwise::text::Selection> {
        use elementwise::text::Selection;
        let input = arguments.0.read(context)?;
        let delimiter = arguments.1.read(context)?;
        let text = coercev2_text_units(context.host, &input.value, false)?;
        let delimiter = coercev2_text_units(context.host, &delimiter.value, false)?;
        let mut options = [1.0, 0.0, 0.0];
        for (index, role) in [&arguments.2, &arguments.3, &arguments.4]
            .into_iter()
            .enumerate()
        {
            if let Some(role) = &role.0 {
                let default = if index == 0 { 1.0 } else { 0.0 };
                *options.get_mut(index).ok_or(CellError::Value)? =
                    role.number(context, Some(default))?;
            }
        }
        let occurrence = *options.first().ok_or(CellError::Value)?;
        if !occurrence.is_finite() || occurrence.abs() >= i64::MAX as f64 {
            return Err(CellError::Value.into());
        }
        let case = mode(*options.get(1).ok_or(CellError::Value)?)?;
        let match_end = bool_mode(*options.get(2).ok_or(CellError::Value)?)?;
        let host = &mut *context.host;
        let work = text
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(delimiter.len().max(1)))
            .ok_or_else(|| host.size_overflow())?;
        host.charge_work(work as u64)?;
        match modern::extract(
            &text,
            &delimiter,
            occurrence.trunc() as i64,
            case,
            match_end,
            if self.0 == TextFunction::TextBefore {
                Side::Before
            } else {
                Side::After
            },
        ) {
            Ok(span) => {
                let selected = text.get(span).ok_or(CellError::Value)?;
                Ok(Selection::Value(elementwise::text::copied(host, selected)?))
            }
            Err(modern::Error::NotFound) => Ok(Selection::Fallback),
            Err(error) => Err(failure(host, error)),
        }
    }
}
/// Materialize a delimiter set through the shared checked collection fold.
fn delimiters(
    host: &mut CallHost<'_, '_>,
    role: &WholeCollection,
) -> FunctionResult<Vec<Vec<u16>>> {
    if description(host, role.argument())?.missing {
        return Ok(Vec::new());
    }
    elementwise::text::collection_units(host, [*role])
}
/// TEXTSPLIT combines collection delimiter sets with fixed scalar options.
struct SplitKernel;
impl elementwise::text::ExpansionKernel for SplitKernel {
    type Signature = (
        WholeCollection,
        WholeCollection,
        Optional<WholeCollection>,
        Optional<FixedScalar>,
        Optional<FixedScalar>,
        Optional<FixedScalar>,
    );
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<elementwise::text::Expansion> {
        let host = &mut *context.host;
        let source = description(host, arguments.0.argument())?;
        if source.rows != 1 || source.columns != 1 {
            return Err(CellError::Value.into());
        }
        let input = arguments.0.read(host, 0)?;
        let text = coercev2_text_units(host, &input.value, false)?;
        let columns = delimiters(host, &arguments.1)?;
        let rows = arguments
            .2
            .0
            .as_ref()
            .map(|role| delimiters(host, role))
            .transpose()?
            .unwrap_or_default();
        let ignore_empty = match &arguments.3.0 {
            Some(role) => {
                let operand = role.read(host)?;
                if operand.missing {
                    false
                } else {
                    core_projection::coercev2_operand_boolean(&operand.value)?
                }
            }
            None => false,
        };
        let case = match &arguments.4.0 {
            Some(role) => {
                let operand = role.read(host)?;
                mode(number(host, operand, 0.0)?)?
            }
            None => Case::Sensitive,
        };
        let plan = Split {
            columns: &columns,
            rows: &rows,
            ignore_empty,
            case,
        };
        let work = plan.work(text.len()).map_err(|e| failure(host, e))?;
        host.charge_work(work as u64)?;
        let shape = plan.shape(&text).map_err(|e| failure(host, e))?;
        shape_admission(host, shape.rows, shape.columns)?;
        host.check_values(shape.fields)?;
        host.charge_work(work as u64)?;
        let fields = plan.fields(&text, shape).map_err(|e| failure(host, e))?;
        let padding = match &arguments.5.0 {
            Some(role) => {
                let operand = role.read(host)?;
                if operand.missing {
                    Error(CellError::NA)
                } else {
                    poirot_coercionv2::compatibility::coercev2_array_member(operand.value)
                }
            }
            None => Error(CellError::NA),
        };
        let count = shape
            .rows
            .checked_mul(shape.columns)
            .ok_or_else(|| host.size_overflow())?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| host.allocation_failure())?;
        let mut fields = fields.into_iter().peekable();
        for row in 0..shape.rows {
            for column in 0..shape.columns {
                if fields
                    .peek()
                    .is_some_and(|f| f.row == row && f.column == column)
                {
                    let field = fields.next().ok_or(CellError::Value)?;
                    let selected = text.get(field.span).ok_or(CellError::Value)?;
                    admit_text(host, selected.len())?;
                    // SAFETY: count is the locally reserved checked row/column product.
                    values.push(text_value(selected.to_vec()));
                } else {
                    host.check_text(padding.text_ref().map_or(0, |t| t.storage_bytes()))?;
                    // SAFETY: count is the locally reserved checked row/column product.
                    values.push(padding.clone());
                }
            }
        }
        Ok(elementwise::text::Expansion {
            values,
            rows: shape.rows,
            columns: shape.columns,
        })
    }
}
fn serialize_member(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    strict: bool,
) -> FunctionResult<Vec<u16>> {
    let text = coercev2_text_units(host, value, true)?;
    let quoted = strict && value.text_ref().is_some();
    let count = modern::escaped_len(&text, quoted).map_err(|e| failure(host, e))?;
    if count > 32767 {
        return Err(CellError::Value.into());
    }
    admit_text(host, count)?;
    modern::escape(&text, quoted).map_err(|e| failure(host, e))
}
fn append(host: &mut CallHost<'_, '_>, output: &mut Vec<u16>, text: &[u16]) -> FunctionResult<()> {
    let size = output
        .len()
        .checked_add(text.len())
        .ok_or_else(|| host.size_overflow())?;
    if size > 32767 {
        return Err(CellError::Value.into());
    }
    host.check_text_capacity(size.checked_mul(2).ok_or_else(|| host.size_overflow())?)?;
    host.check_values_capacity(size)?;
    output
        .try_reserve(text.len())
        .map_err(|_| host.allocation_failure())?;
    host.charge_work(text.len() as u64)?;
    // SAFETY: Checked size is at most 32767 and try_reserve admitted all appended units locally.
    output.extend_from_slice(text);
    Ok(())
}
/// Serialize one broadcast value while retaining spreadsheet errors as text.
struct ValueKernel;
impl Kernel for ValueKernel {
    type Signature = (Broadcast, Optional<Broadcast>);
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (source, format): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let scalar = source.read(context)?;
        let strict = match &format.0 {
            Some(role) => {
                let operand = role.read(context)?;
                bool_mode(number(context.host, operand, 0.)?)?
            }
            None => false,
        };
        Ok(text_value(serialize_member(
            context.host,
            &scalar.value,
            strict,
        )?))
    }
}
/// Serialize a complete rectangle using shared ordered collection traversal.
struct ArrayKernel;
impl collections::CollectionKernel for ArrayKernel {
    type Signature = (WholeCollection, Optional<FixedScalar>);
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        (source, format): &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        let shape = description(host, source.argument())?;
        let strict = match &format.0 {
            Some(role) => {
                let operand = role.read(host)?;
                bool_mode(number(host, operand, 0.)?)?
            }
            None => false,
        };
        let mut prefix = Vec::new();
        if strict {
            append(host, &mut prefix, &[123])?;
        }
        let (mut output, _) = collections::fold(
            host,
            [*source],
            (prefix, 0usize),
            |host, (output, index), operand| {
                if *index != 0 {
                    append(
                        host,
                        output,
                        if strict {
                            if index.is_multiple_of(shape.columns) {
                                &[59]
                            } else {
                                &[44]
                            }
                        } else {
                            &[44, 32]
                        },
                    )?;
                }
                let text = serialize_member(host, &operand.value, strict)?;
                append(host, output, &text)?;
                *index = index.checked_add(1).ok_or_else(|| host.size_overflow())?;
                Ok(())
            },
        )?;
        if strict {
            append(host, &mut output, &[125])?;
        }
        Ok(text_value(output).into())
    }
}
/// Evaluate modern text functions with typed array results and operational failures.
pub(crate) fn execute(
    name: TextFunction,
    mut host: CallHost<'_, '_>,
    address: CellAddress,
    length: usize,
) -> EvalResult<Value> {
    let valid = if matches!(name, TextFunction::ValueToText | TextFunction::ArrayToText) {
        (1..=2).contains(&length)
    } else {
        (2..=6).contains(&length)
    };
    if !valid {
        return Ok(err(CellError::Value));
    }
    let result = match name {
        TextFunction::TextBefore | TextFunction::TextAfter => {
            elementwise::text::fallback(&mut host, address, ExtractionKernel(name))
        }
        TextFunction::TextSplit => elementwise::text::expand(&mut host, address, SplitKernel),
        TextFunction::ValueToText => elementwise::execute(&mut host, address, ValueKernel),
        TextFunction::ArrayToText => collections::execute(&mut host, address, ArrayKernel),
        _ => Err(CellError::Value.into()),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}

#[cfg(test)]
mod resource_mapping_tests {
    use super::*;
    #[test]
    fn text_size_allocation_and_stale_geometry_are_distinct_operational_failures() {
        let book = Workbook::new();
        let mut session = Session::with_context(&book, Default::default());
        let address = CellAddress { row: 2, col: 3 };
        let host = CallHost::new_array(&mut session, 0, address, ArgumentsRef::Empty, 0);
        for (error, expected) in [
            (
                modern::Error::SizeOverflow,
                ResourceFailureKind::SizeOverflow,
            ),
            (
                modern::Error::AllocationFailure,
                ResourceFailureKind::Allocation,
            ),
        ] {
            assert!(
                matches!(failure(&host, error), FunctionFailure::Operational(EvaluationFailure::Resource { kind, source: Some(source), .. }) if kind == expected && source == EvaluationTarget { sheet: 0, address })
            );
        }
        assert!(matches!(
            failure(&host, modern::Error::InvalidGeometry),
            FunctionFailure::Operational(EvaluationFailure::StalePlan)
        ));
    }
}
