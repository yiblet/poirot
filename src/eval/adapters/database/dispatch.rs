use super::database_match_error::DatabaseMatchError;
use super::selected::Selected;
use crate::eval::adapters::elementwise::rows::{self, Field, Layout, PredicateErrors, Rows};
use crate::eval::adapters::elementwise::{
    FixedScalar, PreparedSources, Signature, WholeCollection,
};
use crate::eval::host::{ArgumentCoercion, CallHost, OperandReader};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, function_value};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Blank, Error, Text};
use crate::model::{CellAddress, CellValue};
use poirot_functions::database::{
    DatabaseError, DatabaseFunction, coerce_database_criteria_header_index,
    coerce_database_field_index, coerce_database_number, coerce_database_text,
    condition_matches_metered, equal_ignore_case,
};

/// Execute database aggregation through bounded criteria-table and record reads.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: DatabaseFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = CallHost::new(session, sheet, address, arguments);
    function_value(evaluate(&mut host, function))
}

pub(super) fn database_failure(host: &CallHost<'_, '_>, error: DatabaseError) -> FunctionError {
    match error {
        DatabaseError::Cell(error) => error.into(),
        DatabaseError::FormulaCriteria => {
            host.unsupported(CapabilityId::FunctionAdapter, crate::eval::limits::EvaluationReasonError::DatabaseFormulaCriteria)
        }
        DatabaseError::UnsupportedTextProjection => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::DatabaseScalarProjectionCannotPreserveIsolatedUtf16Surrogates,
        ),
    }
}

pub(super) fn coerce_database_header_text(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<String> {
    if let CellValue::TextUtf16(text) = value {
        return ArgumentCoercion::new(host)
            .project_text(crate::model::TextRef::Utf16(text.units()));
    }
    coerce_database_text(value).map_err(|error| database_failure(host, error))
}

pub(super) fn header(
    host: &mut CallHost<'_, '_>,
    columns: usize,
    name: &str,
) -> FunctionResult<Option<usize>> {
    for col in 0..columns {
        let value = OperandReader::new(host).read_cell(0, 0, col)?.value;
        scratch(host, &value)?;
        if matches!(value, Blank | Error(_)) {
            continue;
        }
        if equal_ignore_case(&coerce_database_header_text(host, &value)?, name) {
            return Ok(Some(col));
        }
    }
    Ok(None)
}

pub(super) fn scratch(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<()> {
    let bytes = value.text_storage_bytes();
    host.check_text_capacity(bytes.saturating_mul(3))?;
    // Scalar numeric normalization has statically bounded scratch; only input
    // text grows with the workbook. Charge the text actually inspected.
    host.charge_work((bytes as u64).saturating_add(1))
        .map_err(FunctionError::Operational)
}

pub(super) fn row_matches(
    host: &mut CallHost<'_, '_>,
    row: usize,
    columns: usize,
    criteria_shape: (usize, usize),
) -> FunctionResult<bool> {
    rows::any_all(
        host,
        1..criteria_shape.0,
        0..criteria_shape.1,
        |host, cr, cc| {
            let condition = OperandReader::new(host).read_cell(2, cr, cc)?.value;
            if condition == Blank {
                return Ok(true);
            }
            let heading = OperandReader::new(host).read_cell(2, 0, cc)?.value;
            scratch(host, &heading)?;
            let selected = match &heading {
                Text(name) => header(host, columns, name)?,
                CellValue::TextUtf16(_) => {
                    let name = coerce_database_header_text(host, &heading)?;
                    header(host, columns, &name)?
                }
                // NumberEval and BoolEval also implement StringValueEval,
                // passing DStarRunner's header guard before positional lookup.
                heading if coerce_database_number(heading).is_some() => {
                    coerce_database_criteria_header_index(heading, columns)?
                }
                _ => return Err(VALUE_ERROR.into()),
            };
            scratch(host, &condition)?;
            let Some(selected) = selected else {
                if coerce_database_header_text(host, &condition)?.is_empty() {
                    return Err(VALUE_ERROR.into());
                }
                return Err(host.unsupported(
                    CapabilityId::FunctionAdapter,
                    crate::eval::limits::EvaluationReasonError::DatabaseFormulaCriteria,
                ));
            };
            let value = OperandReader::new(host).read_cell(0, row, selected)?.value;
            scratch(host, &value)?;
            let wildcard = if let Some(pattern) = condition.text_ref() {
                let pattern = ArgumentCoercion::new(host).project_text(pattern)?;
                let (operator, operand) = poirot_parsing::criteria::parse_comparison(&pattern);
                if operator == poirot_parsing::criteria::Comparison::None
                    && poirot_functions::criteria::has_wildcards(operand)
                {
                    Some((
                        operand.to_owned(),
                        coerce_database_header_text(host, &value)?,
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            let selected = rows::matching(
                host,
                wildcard
                    .as_ref()
                    .map(|(pattern, input)| (pattern.as_str(), input.as_str())),
                |mut visit| {
                    condition_matches_metered::<DatabaseMatchError>(&value, &condition, &mut visit)
                },
            )
            .map_err(|error| match error {
                DatabaseMatchError::Domain(error) => database_failure(host, error),
                DatabaseMatchError::Host(error) => error,
            })?;
            Ok(selected)
        },
    )
}

pub(super) fn evaluate(
    host: &mut CallHost<'_, '_>,
    function: DatabaseFunction,
) -> FunctionResult<CellValue> {
    if host.len() != 3 {
        return Err(VALUE_ERROR.into());
    }
    /// Database and criteria collections retain their original field control between them.
    type Roles = PreparedSources<(WholeCollection, FixedScalar, WholeCollection)>;
    let (_, PreparedSources((database_role, field_role, criteria_role))) = Roles::admit(host)?;
    let database = database_role.describe(host)?;
    let criteria = criteria_role.describe(host)?;
    if !database.is_area || !criteria.is_area {
        return Err(VALUE_ERROR.into());
    }
    let scalar = field_role.read(host)?;
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
            value if coerce_database_number(value).is_some() => {
                coerce_database_field_index(value, database.columns)
            }
            Text(name) => header(host, database.columns, name)?,
            Blank => header(host, database.columns, "")?,
            value => {
                let name = coerce_database_header_text(host, value)?;
                header(host, database.columns, &name)?
            }
        }
    };
    if selected.is_none() && !empty {
        return Err(VALUE_ERROR.into());
    }
    let source = Rows::bind(host, 0, Layout::Records)?;
    rows::select(
        host,
        source,
        Field::Column(selected),
        PredicateErrors::Value,
        |host, row, _| {
            row_matches(
                host,
                row,
                database.columns,
                (criteria.rows, criteria.columns),
            )
        },
        Selected {
            function,
            values: Vec::new(),
            unique: false,
        },
    )
}
