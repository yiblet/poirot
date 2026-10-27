use super::port::{Port, cell_error};
use crate::eval::host::CallHost;
use crate::eval::limits::EvalResult;
use crate::eval::values::{Value, err};
use crate::model::{MAX_COLUMNS, MAX_ROWS};
use poirot_functions::lookup as helper;

pub(super) fn r1c1(text: &str, context: helper::Context) -> Result<String, helper::Failure> {
    fn coordinate(text: &str, current: u32) -> Option<u32> {
        if text.is_empty() {
            Some(current)
        } else if let Some(relative) = text.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let n = relative.parse::<i64>().ok()?;
            u32::try_from(i64::from(current) + n).ok()
        } else {
            text.parse::<u32>().ok()?.checked_sub(1)
        }
    }
    let upper = text.to_ascii_uppercase();
    if !upper.contains('C')
        && let Some(body) = upper.strip_prefix('R')
    {
        if body.is_empty() {
            return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
        }
        let row = coordinate(body, context.row)
            .filter(|n| *n < MAX_ROWS)
            .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?
            + 1;
        return Ok(format!("{row}:{row}"));
    }
    if let Some(body) = upper.strip_prefix('C') {
        if body.is_empty() {
            return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
        }
        let col = coordinate(body, context.col)
            .filter(|n| *n < MAX_COLUMNS)
            .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
        let mut letters = String::new();
        let mut n = col + 1;
        while n > 0 {
            n -= 1;
            // SAFETY: Zero is always an in-bounds UTF-8 boundary, including for an empty string.
            letters.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        return Ok(format!("{letters}:{letters}"));
    }
    let body = upper
        .strip_prefix('R')
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let (row, col) = body
        .split_once('C')
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let row = coordinate(row, context.row)
        .filter(|n| *n < MAX_ROWS)
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let col = coordinate(col, context.col)
        .filter(|n| *n < MAX_COLUMNS)
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let mut letters = String::new();
    let mut n = col + 1;
    while n > 0 {
        n -= 1;
        // SAFETY: Zero is always an in-bounds UTF-8 boundary, including for an empty string.
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Ok(format!("{letters}{}", row + 1))
}

/// Execute the helper plan through checked services, retaining lazy and reference channels.
pub(in crate::eval) fn execute_host(
    host: CallHost<'_, '_>,
    function: helper::Function,
) -> EvalResult<Value> {
    let target = host.target();
    let count = host.len();
    let mut port = Port {
        host,
        failure: None,
    };
    let context = helper::Context {
        sheet: target.sheet as u32,
        row: target.address.row,
        col: target.address.col,
    };
    match helper::CallPlan::prepare(function, count)
        .and_then(|plan| plan.execute(context, &mut port))
    {
        Ok(value) => port.output(value),
        Err(helper::Failure::Spreadsheet(e)) => Ok(err(cell_error(e))),
        Err(failure) => Err(port
            .failure
            .take()
            .unwrap_or_else(|| port.unsupported(format!("pinned lookup boundary: {failure:?}")))),
    }
}
