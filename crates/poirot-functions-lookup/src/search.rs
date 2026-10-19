use crate::operands::{
    charge, checked_product, coercev2_lookup_integer, missing, num, preflight_value, scalar,
    Arguments,
};
use crate::table::Table;
use crate::{
    approximate_index, xmatch_index, Array, Context, Error, Failure, MatchMode, ReferenceKind,
    Scalar, Search, SearchMode, Unsupported, Value, Work, Workbook,
};
// XLOOKUP and XMATCH default omitted modes to exact matching and forward search.
// https://support.microsoft.com/en-us/excel/functions/xlookup-function
// https://support.microsoft.com/en-gb/excel/functions/xmatch-function?nochrome=true
fn modes(
    args: &Arguments<'_>,
    match_index: usize,
    c: Context,
    w: &mut impl Workbook,
) -> Result<(MatchMode, SearchMode), Failure> {
    let mode = match if args.len() > match_index {
        Some(args.get(match_index, c, w)?)
    } else {
        None
    } {
        None => 0,
        Some(v) if missing(&v) => 0,
        Some(v) => coercev2_lookup_integer(scalar(&v, c, w)?, true, w)?,
    };
    let mode = match mode {
        -1 => MatchMode::Smaller,
        0 => MatchMode::Exact,
        1 => MatchMode::Larger,
        2 => MatchMode::Wildcard,
        _ => return Err(Error::Value.into()),
    };
    let search = match if args.len() > match_index + 1 {
        Some(args.get(match_index + 1, c, w)?)
    } else {
        None
    } {
        None => 1,
        Some(v) if missing(&v) => 1,
        Some(v) => coercev2_lookup_integer(scalar(&v, c, w)?, true, w)?,
    };
    let search = match search {
        -2 => SearchMode::BinaryDescending,
        -1 => SearchMode::Backward,
        1 => SearchMode::Forward,
        2 => SearchMode::BinaryAscending,
        _ => return Err(Error::Value.into()),
    };
    Ok((mode, search))
}
/// Execute a prepared lookup operation with metered candidate access.
pub(super) fn search(
    f: Search,
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    match f {
        Search::Lookup => {
            if !(2..=3).contains(&args.len()) {
                return Err(Error::Value.into());
            }
            let key = scalar(&args.get(0, c, w)?, c, w)?;
            let lookup_argument = args.get(1, c, w)?;
            let table = Table::new(&lookup_argument)?;
            let result_argument = if args.len() == 3 {
                Some(args.get(2, c, w)?)
            } else {
                None
            };
            let result_table = result_argument.as_ref().map(Table::new).transpose()?;
            let column = if args.len() == 2 {
                table.cols <= table.rows
            } else {
                if table.rows != 1 && table.cols != 1 {
                    return Err(Failure::Unsupported(Unsupported::LookupNonVector));
                }
                table.cols == 1
            };
            let vector = table.vector(column, 0, w)?;
            if let Some(result) = result_table {
                if result.rows != 1 && result.cols != 1 {
                    return Err(Failure::Unsupported(Unsupported::LookupNonVector));
                }
                if vector.len() > result.rows.max(result.cols) {
                    return Err(Failure::Unsupported(Unsupported::LookupShortResult));
                }
                let index = approximate_index(&key, &vector, w)?;
                Ok(result.selected(if result.cols == 1 {
                    result.get(index, 0, w)?
                } else {
                    result.get(0, index, w)?
                }))
            } else {
                let index = approximate_index(&key, &vector, w)?;
                Ok(table.selected(if column {
                    table.get(index, table.cols - 1, w)?
                } else {
                    table.get(table.rows - 1, index, w)?
                }))
            }
        }
        Search::XMatch | Search::XLookup => {
            let minimum = if f == Search::XLookup { 3 } else { 2 };
            if args.len() < minimum {
                return Err(Error::Value.into());
            }
            // This fallback is resolved before search even when a match exists.
            // Missing optional arguments use the same defaults as absent slots:
            // https://support.microsoft.com/en-us/excel/functions/xlookup-function
            let fallback = if f == Search::XLookup && args.len() > 3 {
                let value = args.get(3, c, w)?;
                if missing(&value) {
                    None
                } else {
                    Some(scalar(&value, c, w)?)
                }
            } else {
                None
            };
            let (mode, direction) = modes(args, if f == Search::XLookup { 4 } else { 2 }, c, w)?;
            let key = scalar(&args.get(0, c, w)?, c, w)?;
            let key = match key {
                Scalar::Number(n) if mode == MatchMode::Wildcard => {
                    charge(w, Work::SearchText, 32)?;
                    Scalar::Text(w.coercev2_number_to_text(n)?)
                }
                value => value,
            };
            let lookup_argument = args.get(1, c, w)?;
            let table = Table::new(&lookup_argument)?;
            let column = table.cols == 1;
            let vector = table.vector(column, 0, w)?;
            let index = match (xmatch_index(&key, &vector, mode, direction, w), fallback) {
                (Ok(index), _) => index,
                (Err(Failure::Spreadsheet(Error::Na)), Some(fallback)) if f == Search::XLookup => {
                    let return_argument = args.get(2, c, w)?;
                    if let Value::SignedReference(r) = &return_argument {
                        if r.kind == ReferenceKind::Area && r.width() > 1 {
                            charge(
                                w,
                                Work::QueryReturn,
                                checked_product(r.width(), std::mem::size_of::<Scalar>())?,
                            )?;
                            let mut values = vec![Scalar::Text(String::new()); r.width()];
                            // SAFETY: This branch checks width > 1 and allocates exactly width elements before writing element zero.
                            values[0] = fallback;
                            return Ok(Value::Array(Array::new(1, r.width(), values)?));
                        }
                    }
                    if let Value::Reference(r) = &return_argument {
                        if r.kind == ReferenceKind::Area && r.width() > 1 {
                            charge(
                                w,
                                Work::QueryReturn,
                                checked_product(r.width(), std::mem::size_of::<Scalar>())?,
                            )?;
                            let mut values = vec![Scalar::Text(String::new()); r.width()];
                            // SAFETY: This branch checks width > 1 and allocates exactly width elements before writing element zero.
                            values[0] = fallback;
                            return Ok(Value::Array(Array::new(1, r.width(), values)?));
                        }
                    }
                    return Ok(Value::Scalar(fallback));
                }
                (Err(e), _) => return Err(e),
            };
            if f == Search::XMatch {
                return Ok(num(index as f64 + 1.0));
            }
            match &args.get(2, c, w)? {
                Value::Reference(r) if r.kind == ReferenceKind::Area => {
                    let mut result = r.clone();
                    if column {
                        result.first_row = r
                            .first_row
                            .checked_add(index as u32)
                            .ok_or(Failure::Spreadsheet(Error::Ref))?;
                        result.last_row = result.first_row;
                    } else {
                        result.first_col = r
                            .first_col
                            .checked_add(index as u32)
                            .ok_or(Failure::Spreadsheet(Error::Ref))?;
                        result.last_col = result.first_col;
                    }
                    if !result.valid() {
                        return Err(Error::Ref.into());
                    }
                    Ok(Value::Reference(result))
                }
                Value::SignedReference(r) if r.kind == ReferenceKind::Area => {
                    let mut result = r.clone();
                    if column {
                        result.first_row = r.first_row.wrapping_add(index as i32);
                        result.last_row = result.first_row;
                    } else {
                        result.first_col = r.first_col.wrapping_add(index as i32);
                        result.last_col = result.first_col;
                    }
                    if !result.valid() {
                        return Err(Error::Ref.into());
                    }
                    Ok(Value::SignedReference(result))
                }
                // Pinned POI returns non-AreaEval return values unchanged.
                v => {
                    preflight_value(v, w, Work::QueryReturn)?;
                    Ok(v.clone())
                }
            }
        }
    }
}
