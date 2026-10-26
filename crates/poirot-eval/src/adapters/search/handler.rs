//! Deferred classic lookup admission and result projection over checked host services.
use super::execution::LookupTable;
use crate::*;

/// Execute classic search calls in their source operand forcing order.
pub(crate) fn execute(
    host: &mut host::CallHost<'_, '_>,
    name: SearchFunction,
) -> EvalResult<Value> {
    let is_match = name == SearchFunction::Match;
    let kind = if is_match {
        host::ClassicLookupKind::Match
    } else {
        host::ClassicLookupKind::Table
    };
    if !FunctionId::Search(name)
        .descriptor()
        .arity
        .accepts(host.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    // Match.java resolves match_type before resolving the key/range.
    let match_mode = if is_match && host.len() == 3 {
        let value = host.scalar_argument(2)?;
        if value == Blank {
            SearchMode::Ascending
        } else {
            match semantic(host.selected_number(&value))? {
                Ok(n) => SearchMode::from_number(n),
                Err(_) => return Ok(err(Ref)),
            }
        }
    } else {
        SearchMode::Ascending
    };
    let key = host.scalar_argument(0)?;
    if let Error(e) = key {
        return Ok(err(e));
    }
    let table_value = host.raw_argument(1)?;
    if is_match
        && let Value::Scalar(value) | Value::SelectedScalar(value, _) = &table_value
        && value.text_ref().is_some()
    {
        let parsed = semantic(host.selected_number(value))?;
        return Ok(err(if parsed.is_ok() { NA } else { VALUE_ERROR }));
    }
    let table = match host::LookupTable::admit(table_value, kind) {
        Ok(t) => t,
        Err(e) => return Ok(err(e)),
    };
    let (height, width) = table.shape();
    if height == 0 || width == 0 {
        return Ok(err(NA));
    }
    let axis = if is_match {
        if height != 1 && width != 1 {
            return Ok(err(NA));
        }
        if width == 1 {
            SearchAxis::Vertical
        } else {
            SearchAxis::Horizontal
        }
    } else if name == SearchFunction::VLookup {
        SearchAxis::Vertical
    } else {
        SearchAxis::Horizontal
    };
    let mode = if is_match {
        match_mode
    } else if host.len() == 4 {
        match coercev2_logical_boolean(&host.scalar_argument(3)?) {
            Ok(true) => SearchMode::Ascending,
            Ok(false) => SearchMode::Exact,
            Err(e) => return Ok(err(e)),
        }
    } else {
        SearchMode::Ascending
    };
    let found = host.lookup_index(&table, axis, &key, mode, kind);
    let index = match found {
        Ok(Some(i)) => i,
        Ok(None) => return Ok(err(NA)),
        Err(FunctionFailure::Spreadsheet(e)) => return Ok(err(e)),
        Err(FunctionFailure::Operational(e)) => return Err(e),
    };
    if is_match {
        return Ok(Number((index + 1) as f64).into());
    }
    // Vlookup.java/Hlookup.java defer the result index until selection succeeds.
    let value = host.scalar_argument(2)?;
    let result_index = match semantic(host.selected_number(&value))? {
        Ok(n) if n.floor() >= 1. => n.floor() as usize - 1,
        Ok(_) => return Ok(err(VALUE_ERROR)),
        Err(_) => return Ok(err(Ref)),
    };
    let value = match host.lookup_result(&table, axis, result_index, index) {
        Ok(value) => value,
        Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
        Err(FunctionFailure::Operational(failure)) => return Err(failure),
    };
    let origin = match &table {
        LookupTable::Array(_) => host::OperandOrigin::ArrayElement,
        _ => host::OperandOrigin::ReferencedCell,
    };
    Ok(Value::SelectedScalar(value, origin))
}
