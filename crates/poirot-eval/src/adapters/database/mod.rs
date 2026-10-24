//! Lazy workbook reads for the pinned POI DStarRunner database family.
use crate::host::{CallHost, MissingPolicy};
use crate::*;
use poirot_functions_database::{
    DatabaseError, DatabaseFunction, FinalizationPolicy, aggregate_selected,
    coercev2_database_criteria_header_index, coercev2_database_field_index,
    coercev2_database_number, coercev2_database_text, condition_matches_metered, equal_ignore_case,
};

/// Execute database aggregation through bounded criteria-table and record reads.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: DatabaseFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if function == DatabaseFunction::Average {
        session.read(DependencyRead::BuiltinPolicy);
    }
    let mut host = CallHost::new(session, sheet, address, arguments);
    function_value(evaluate(&mut host, function))
}
/// Preserve host aborts separately from database spreadsheet/domain failures.
enum DatabaseMatchFailure {
    Domain(DatabaseError),
    Host(FunctionFailure),
}
impl From<DatabaseError> for DatabaseMatchFailure {
    fn from(error: DatabaseError) -> Self {
        Self::Domain(error)
    }
}
fn database_failure(host: &CallHost<'_, '_>, error: DatabaseError) -> FunctionFailure {
    match error {
        DatabaseError::Cell(error) => error.into(),
        DatabaseError::FormulaCriteria => {
            host.unsupported(CapabilityId::FunctionAdapter, "database formula criteria")
        }
        DatabaseError::UnsupportedTextProjection => host.unsupported(
            CapabilityId::FunctionAdapter,
            "database scalar projection cannot preserve isolated UTF16 surrogates",
        ),
    }
}
fn coercev2_database_header_text(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<String> {
    if let CellValue::TextUtf16(text) = value {
        return host.project_text(poirot_model::TextRef::Utf16(text.units()));
    }
    coercev2_database_text(value).map_err(|error| database_failure(host, error))
}
fn header(
    host: &mut CallHost<'_, '_>,
    columns: usize,
    name: &str,
) -> FunctionResult<Option<usize>> {
    for col in 0..columns {
        let value = host.read_cell(0, 0, col)?.value;
        scratch(host, &value)?;
        if matches!(value, Blank | Error(_)) {
            continue;
        }
        if equal_ignore_case(&coercev2_database_header_text(host, &value)?, name) {
            return Ok(Some(col));
        }
    }
    Ok(None)
}
fn scratch(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<()> {
    let bytes = value.text_storage_bytes();
    host.check_text_capacity(bytes.saturating_mul(3))?;
    // Scalar numeric normalization has statically bounded scratch; only input
    // text grows with the workbook. Charge the text actually inspected.
    host.charge_work((bytes as u64).saturating_add(1))
        .map_err(FunctionFailure::Operational)
}
fn row_matches(
    host: &mut CallHost<'_, '_>,
    row: usize,
    columns: usize,
    criteria_shape: (usize, usize),
) -> FunctionResult<bool> {
    for cr in 1..criteria_shape.0 {
        let mut matched = true;
        for cc in 0..criteria_shape.1 {
            let condition = host.read_cell(2, cr, cc)?.value;
            if condition == Blank {
                continue;
            }
            let heading = host.read_cell(2, 0, cc)?.value;
            scratch(host, &heading)?;
            let selected = match &heading {
                Text(name) => header(host, columns, name)?,
                CellValue::TextUtf16(_) => {
                    let name = coercev2_database_header_text(host, &heading)?;
                    header(host, columns, &name)?
                }
                // NumberEval and BoolEval also implement StringValueEval,
                // passing DStarRunner's header guard before positional lookup.
                heading if coercev2_database_number(heading).is_some() => {
                    coercev2_database_criteria_header_index(heading, columns)?
                }
                _ => return Err(VALUE_ERROR.into()),
            };
            scratch(host, &condition)?;
            let Some(selected) = selected else {
                if coercev2_database_header_text(host, &condition)?.is_empty() {
                    return Err(VALUE_ERROR.into());
                }
                return Err(
                    host.unsupported(CapabilityId::FunctionAdapter, "database formula criteria")
                );
            };
            let value = host.read_cell(0, row, selected)?.value;
            scratch(host, &value)?;
            if let Some(pattern) = condition.text_ref() {
                let pattern = host.project_text(pattern)?;
                let (operator, operand) = poirot_parsing::criteria::parse_comparison(&pattern);
                if operator == poirot_parsing::criteria::Comparison::None
                    && poirot_functions_criteria::has_wildcards(operand)
                {
                    let input = coercev2_database_header_text(host, &value)?;
                    host.admit_wildcard(operand, &input)?;
                }
            }
            let mut visited = 0;
            let selected =
                condition_matches_metered::<DatabaseMatchFailure>(&value, &condition, &mut || {
                    host.wildcard_step(&mut visited)
                        .map_err(DatabaseMatchFailure::Host)
                })
                .map_err(|error| match error {
                    DatabaseMatchFailure::Domain(error) => database_failure(host, error),
                    DatabaseMatchFailure::Host(error) => error,
                })?;
            if !selected {
                matched = false;
                break;
            }
        }
        if matched {
            return Ok(true);
        }
    }
    Ok(false)
}
fn evaluate(host: &mut CallHost<'_, '_>, function: DatabaseFunction) -> FunctionResult<CellValue> {
    if host.len() != 3 {
        return Err(VALUE_ERROR.into());
    }
    // Prepare all three operands in source order, then validate reference classes.
    let database = host.describe(0)?;
    host.kind(1)?;
    let criteria = host.describe(2)?;
    if !database.is_area || !criteria.is_area {
        return Err(VALUE_ERROR.into());
    }
    let scalar = host.scalar_data(1, MissingPolicy::Blank)?;
    let empty = matches!(function, DatabaseFunction::Count | DatabaseFunction::CountA);
    // DStarRunner catches unexpected field types (including MissingArgEval)
    // as VALUE; the two count algorithms instead retain the absent field.
    if scalar.missing && !empty {
        return Err(VALUE_ERROR.into());
    }
    let field = scalar.value;
    scratch(host, &field)?;
    let selected = if scalar.missing {
        None
    } else {
        match &field {
            Error(e) if !empty => return Err((*e).into()),
            Error(_) => None,
            value if coercev2_database_number(value).is_some() => {
                coercev2_database_field_index(value, database.columns)
            }
            Text(name) => header(host, database.columns, name)?,
            Blank => header(host, database.columns, "")?,
            value => {
                let name = coercev2_database_header_text(host, value)?;
                header(host, database.columns, &name)?
            }
        }
    };
    if selected.is_none() && !empty {
        return Err(VALUE_ERROR.into());
    }
    // Collect only values already selected by lazily evaluated criteria. Unvisited
    // columns and rejected rows never execute their worksheet formula subtrees.
    host.check_values(database.rows)?;
    let mut values = Vec::new();
    let mut unique = false;
    for row in 1..database.rows {
        let matched = match row_matches(
            host,
            row,
            database.columns,
            (criteria.rows, criteria.columns),
        ) {
            Ok(matched) => matched,
            Err(FunctionFailure::Spreadsheet(_)) => return Err(VALUE_ERROR.into()),
            Err(failure) => return Err(failure),
        };
        if !matched {
            continue;
        }
        let value = if let Some(col) = selected {
            host.read_cell(0, row, col)?.value
        } else {
            Number(0.)
        };
        if function == DatabaseFunction::Get && value != Blank {
            if unique {
                return Ok(Error(Num));
            }
            unique = true;
        }
        values.push(value);
    }
    // Finalization visits each selected value once, or twice for variance.
    let passes = if matches!(
        function,
        DatabaseFunction::Stdev
            | DatabaseFunction::StdevP
            | DatabaseFunction::Var
            | DatabaseFunction::VarP
    ) {
        2
    } else {
        1
    };
    host.charge_work((values.len() as u64).saturating_mul(passes))?;
    let policy = match host.builtin_semantics_policy() {
        BuiltinSemanticsPolicy::PinnedPoi => FinalizationPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation => FinalizationPolicy::ExcelDocumentation,
    };
    aggregate_selected(function, &values, policy).map_err(|error| database_failure(host, error))
}
