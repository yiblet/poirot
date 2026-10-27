use super::scalar_domain::{cell, error};
use crate::eval::adapters::elementwise::rows::{
    self, Numbers, PairLayout, PairedCollections, PredicateErrors,
};
use crate::eval::adapters::elementwise::{FixedScalar, NumericParameter, NumericSignature};
use crate::eval::functions::registry::CriteriaFunction;
use crate::eval::host::{ArgumentCoercion, CallHost, NumericPolicy};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{Calc, GettingData, Num, Spill};
use crate::model::CellValue::{Blank, Error, Number, Text};
use crate::model::{CellAddress, CellValue};
use poirot_functions::criteria::{AggregateKind, Cell, Criterion};
use poirot_parsing::criteria::Comparison;

/// Dispatch criteria aggregation with lazy, metered workbook reads.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: CriteriaFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = CallHost::new(session, sheet, address, arguments);
    match evaluate(&mut host, address, function) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(e)) => Ok(err(e)),
        Err(FunctionFailure::Operational(e)) => Err(e),
    }
}

/// Admit text payloads and reject error kinds unsupported by criteria helpers.
pub(in crate::eval::adapters) fn preflight(
    host: &mut CallHost<'_, '_>,
    values: &[&CellValue],
) -> FunctionResult<()> {
    let bytes = values
        .iter()
        .map(|v| v.text_storage_bytes())
        .fold(0usize, usize::saturating_add);
    host.check_text_capacity(bytes.saturating_mul(3))?;
    host.charge_work((bytes as u64).saturating_add(1))?;
    if values
        .iter()
        .any(|v| matches!(v, Error(GettingData | Spill | Calc)))
    {
        return Err(host.unsupported(CapabilityId::FunctionAdapter, "criteria domain error code"));
    }
    Ok(())
}

pub(super) fn evaluate(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    function: CriteriaFunction,
) -> FunctionResult<Value> {
    use CriteriaFunction::{
        AverageIf, AverageIfs, CountIf, CountIfs, MaxIfs, MinIfs, Subtotal, SumIf, SumIfs,
    };
    if function == Subtotal {
        return subtotal(host);
    }
    let layout = match function {
        CountIf => PairLayout::Single,
        SumIf | AverageIf => PairLayout::OptionalPayload,
        CountIfs => PairLayout::Repeated,
        SumIfs | AverageIfs | MaxIfs | MinIfs => PairLayout::LeadingPayload,
        Subtotal => return Err(VALUE_ERROR.into()),
    };
    let roles =
        PairedCollections::<crate::eval::adapters::elementwise::Broadcast>::admit(host, layout)?;
    roles.evaluate(host, address, compile, |host, roles, predicates| {
        if !matches!(function, CountIf | SumIf)
            && let Some(domain_error) =
                predicates
                    .iter()
                    .find_map(|predicate| match predicate.value {
                        Cell::Error(error) => Some(error),
                        _ => None,
                    })
        {
            return Ok(error(domain_error));
        }
        aggregate(host, function, roles, predicates)
    })
}

/// Compile one admitted control without binding or traversing workbook arguments.
pub(super) fn compile(host: &mut CallHost<'_, '_>, value: CellValue) -> FunctionResult<Criterion> {
    preflight(host, &[&value])?;
    let admitted =
        poirot_functions::coercion::criteria::coerce_criteria_excel_operand(cell(host, &value)?);
    let mut predicate = Criterion::parse(&admitted);
    predicate.value =
        poirot_functions::coercion::criteria::coerce_criteria_accounting_threshold(predicate.value);
    if matches!(
        predicate.comparison,
        Comparison::Lt | Comparison::Le | Comparison::Gt | Comparison::Ge
    ) {
        predicate.value = poirot_functions::coercion::criteria::coerce_criteria_ordered_threshold(
            predicate.value,
            |text| match ArgumentCoercion::new(host)
                .coerce_number_text(text, NumericPolicy::PinnedPoi)
            {
                Ok(number) => Ok(Some(number)),
                Err(FunctionFailure::Spreadsheet(_)) => Ok(None),
                Err(failure) => Err(failure),
            },
        )?;
    }
    Ok(predicate)
}

pub(super) fn aggregate(
    host: &mut CallHost<'_, '_>,
    function: CriteriaFunction,
    roles: &PairedCollections,
    criteria: &[Criterion],
) -> FunctionResult<CellValue> {
    use CriteriaFunction::{
        AverageIf, AverageIfs, CountIf, CountIfs, MaxIfs, MinIfs, Subtotal, SumIf, SumIfs,
    };
    let operation = match function {
        CountIf | CountIfs => AggregateKind::Count,
        SumIf | SumIfs => AggregateKind::Sum,
        AverageIf => AggregateKind::AverageIf,
        AverageIfs => AggregateKind::Average,
        MinIfs => AggregateKind::Min,
        MaxIfs => AggregateKind::Max,
        Subtotal => return Err(VALUE_ERROR.into()),
    };
    let source = roles.source(
        host,
        criteria
            .first()
            .is_some_and(|criterion| !criterion.matches(&Cell::Blank)),
    )?;
    rows::select(
        host,
        source,
        roles.field(),
        PredicateErrors::Preserve,
        |host, r, c| {
            rows::any_all(host, 0..1, 0..roles.pairs().len(), |host, _, term| {
                let (range, _) = roles.pairs().get(term).ok_or(VALUE_ERROR)?;
                let criterion = criteria.get(term).ok_or(VALUE_ERROR)?;
                let v = range.read_at(host, r, c)?.value;
                let cv = match &criterion.value {
                    Cell::Text(s) => Text(s.clone()),
                    _ => Blank,
                };
                preflight(host, &[&v, &cv])?;
                let wildcard =
                    if let (Cell::Text(pattern), Some(input)) = (&criterion.value, v.text_ref()) {
                        if matches!(
                            criterion.comparison,
                            Comparison::None | Comparison::Eq | Comparison::Ne
                        ) {
                            Some((
                                pattern.as_str(),
                                ArgumentCoercion::new(host).project_text(input)?,
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                let projected = cell(host, &v)?;
                rows::matching(
                    host,
                    wildcard
                        .as_ref()
                        .map(|(pattern, input)| (*pattern, input.as_str())),
                    |mut visit| criterion.matches_metered(&projected, &mut visit),
                )
            })
        },
        Numbers::new(operation),
    )
}

pub(super) fn subtotal(host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
    if host.len() < 2 {
        return Err(VALUE_ERROR.into());
    }
    // SUBTOTAL selectors have an i32 domain; omitted selectors remain unsupported.
    let scalar = FixedScalar::at(0).read(host)?;
    if scalar.missing {
        return Err(host.unsupported(CapabilityId::NumericConversion, "omitted SUBTOTAL selector"));
    }
    let code = NumericSignature::new(&[NumericParameter::required()])
        .scalars(host, 0)?
        .get(0)?
        .floor();
    if code.is_nan() || code > i32::MAX as f64 || code < i32::MIN as f64 {
        return Err(Num.into());
    }
    if !((1. ..=11.).contains(&code) || (101. ..=111.).contains(&code)) {
        return Err(VALUE_ERROR.into());
    }
    let cells = rows::visible_fold(host, 1, code > 100., Vec::new(), |host, cells, v| {
        let value = if v.missing { Number(0.) } else { v.value };
        preflight(host, &[&value])?;
        host.check_values(cells.len().saturating_add(1))?;
        cells
            .try_reserve(1)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: one additional slot was reserved immediately above.
        cells.push((
            cell(host, &value)?,
            v.origin != crate::eval::values::OperandOrigin::Direct,
        ));
        Ok(())
    })?;
    host.check_values(cells.len())?;
    let supplied: Vec<_> = cells
        .iter()
        .map(
            |(value, referenced)| poirot_functions::criteria::subtotal::SubtotalCell {
                value,
                hidden: false,
                nested_subtotal: false,
                referenced: *referenced,
            },
        )
        .collect();
    host.charge_work(cells.len() as u64)?;
    Ok(
        match poirot_functions::criteria::subtotal::subtotal(code as i32, &supplied) {
            Ok(n) => Number(n),
            Err(e) => error(e),
        }
        .into(),
    )
}
