use crate::operands::{
    charge, checked_product, coercev2_lookup_boolean, coercev2_lookup_integer,
    coercev2_lookup_text, missing, num, scalar, Arguments,
};
use crate::{
    parse_dynamic_reference, Context, Error, Failure, Reference, ReferenceKind, Scalar,
    Unsupported, Value, Work, Workbook,
};
fn indirect_failure(failure: Failure) -> Failure {
    match failure {
        Failure::Spreadsheet(_) => Error::Ref.into(),
        other => other,
    }
}
/// Construct a checked address spelling from admitted arguments.
pub(super) fn address(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
    policy: crate::SemanticsPolicy,
) -> Result<Value, Failure> {
    if !(2..=5).contains(&args.len()) {
        return Err(Error::Value.into());
    }
    let row = coercev2_lookup_integer(scalar(&args.get(0, c, w)?, c, w)?, false, w)?;
    let col = coercev2_lookup_integer(scalar(&args.get(1, c, w)?, c, w)?, false, w)?;
    if row < 1 || col < 1 {
        return Err(Error::Value.into());
    }
    let kind = if args.len() > 2 {
        let value = args.get(2, c, w)?;
        if missing(&value) {
            1
        } else {
            coercev2_lookup_integer(scalar(&value, c, w)?, false, w)?
        }
    } else {
        1
    };
    if !(1..=4).contains(&kind) {
        return Err(Error::Value.into());
    }
    // The pinned source ignores the fourth argument, including its errors.
    let a1 = if policy == crate::SemanticsPolicy::ExcelDocumentation && args.len() > 3 {
        let value = args.get(3, c, w)?;
        if missing(&value) {
            true
        } else {
            coercev2_lookup_boolean(scalar(&value, c, w)?)?
        }
    } else {
        true
    };
    charge(w, Work::QueryReturn, 32)?;
    let mut result = String::new();
    if args.len() == 5 {
        let value = args.get(4, c, w)?;
        if !missing(&value) {
            let name = coercev2_lookup_text(&scalar(&value, c, w)?, w)?;
            if name.contains(['\n', '\r', '\t']) {
                return Err(Failure::Unsupported(
                    Unsupported::AddressSheetNameControlCharacter,
                ));
            }
            charge(w, Work::QueryReturn, checked_product(name.len(), 4)?)?;
            result.push_str(&crate::syntax::format_sheet_name(&name));
            result.push('!');
        }
    }
    if !a1 {
        result.push('R');
        if kind == 3 || kind == 4 {
            result.push('[');
        }
        result.push_str(&row.to_string());
        if kind == 3 || kind == 4 {
            result.push(']');
        }
        result.push('C');
        if kind == 2 || kind == 4 {
            result.push('[');
        }
        result.push_str(&col.to_string());
        if kind == 2 || kind == 4 {
            result.push(']');
        }
        return Ok(Value::Scalar(Scalar::Text(result)));
    }
    if kind == 1 || kind == 3 {
        result.push('$');
    }
    let mut letters = Vec::new();
    let mut column = col as u32;
    while column > 0 {
        column -= 1;
        letters.push((b'A' + (column % 26) as u8) as char);
        column /= 26;
    }
    result.extend(letters.iter().rev());
    if kind == 1 || kind == 2 {
        result.push('$');
    }
    result.push_str(&row.to_string());
    Ok(Value::Scalar(Scalar::Text(result)))
}
/// Resolve a worksheet identifier through the host metadata port.
pub(super) fn sheet(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    let index = match if args.is_empty() {
        None
    } else {
        Some(args.get(0, c, w)?)
    } {
        None => c.sheet,
        Some(Value::Reference(r)) => r.first_sheet,
        Some(Value::SignedReference(r)) => r.first_sheet,
        Some(Value::Scalar(Scalar::Text(name)) | Value::SelectedScalar(Scalar::Text(name), _)) => w
            .sheet_index(&name)?
            .ok_or(Failure::Spreadsheet(Error::Na))?,
        _ => return Err(Error::Na.into()),
    };
    Ok(num(index as f64 + 1.0))
}
/// Select the caller-intersected scalar from a reference or array.
pub(super) fn single(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    let mut found = None;
    for index in 0..args.len() {
        let value = args.get(index, c, w)?;
        if let Value::SignedReference(r) = &value {
            if !r.valid() || r.kind != ReferenceKind::Area {
                continue;
            }
            let (row, col) = (c.row as i32, c.col as i32);
            let contains_row = (r.first_row..=r.last_row).contains(&row);
            let contains_col = (r.first_col..=r.last_col).contains(&col);
            let coordinates = if contains_row && contains_col {
                Some((row, col))
            } else if contains_row {
                Some((row, r.first_col))
            } else if contains_col {
                Some((r.first_row, col))
            } else {
                None
            };
            if let Some((row, col)) = coordinates {
                if found.is_some() {
                    return Err(Error::Value.into());
                }
                found = Some(Value::SelectedScalar(
                    r.read(row, col, w)?,
                    poirot_coercionv2::Origin::Reference,
                ));
            }
            continue;
        }
        if let Value::Reference(r) = value {
            if r.kind != ReferenceKind::Area {
                continue;
            }
            let checked =
                crate::domain::CheckedReference::new(&r).ok_or(Failure::Spreadsheet(Error::Ref))?;
            let coordinates = crate::geometry::single_selection(&checked, c);
            if let Some((row, col)) = coordinates {
                if found.is_some() {
                    return Err(Error::Value.into());
                }
                found = Some(Value::SelectedScalar(
                    w.cell(&r, r.first_sheet, row, col)?,
                    poirot_coercionv2::Origin::Reference,
                ));
            }
        }
    }
    found.ok_or(Failure::Spreadsheet(Error::Value))
}
/// Construct displaced reference geometry without eagerly reading its cells.
pub(super) fn offset(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    // POI admits 1/2 arguments then indexes absent operands: explicitly unsupported host exception.
    if (1..=2).contains(&args.len()) {
        return Err(Failure::Unsupported(Unsupported::OffsetMissingOperands));
    }
    if !(3..=5).contains(&args.len()) {
        return Err(Error::Value.into());
    }
    let base_argument = args.get(0, c, w)?;
    let (workbook, first_sheet, last_sheet, start_row, start_col, base_height, base_width) =
        match &base_argument {
            Value::Reference(r) => {
                let checked = crate::domain::CheckedReference::new(r)
                    .ok_or(Failure::Spreadsheet(Error::Value))?;
                (
                    &r.workbook,
                    r.first_sheet,
                    r.last_sheet,
                    r.first_row as i32,
                    r.first_col as i32,
                    checked.height() as i32,
                    checked.width() as i32,
                )
            }
            Value::SignedReference(r) if r.valid() => (
                &r.workbook,
                r.first_sheet,
                r.last_sheet,
                r.first_row,
                r.first_col,
                r.source_height(),
                r.source_width(),
            ),
            Value::Scalar(Scalar::Error(e)) | Value::SelectedScalar(Scalar::Error(e), _) => {
                return Err((*e).into())
            }
            _ => return Err(Error::Value.into()),
        };
    let row_value = args.get(1, c, w)?;
    let row = if missing(&row_value) {
        0
    } else {
        coercev2_lookup_integer(scalar(&row_value, c, w)?, true, w)?
    };
    let col_value = args.get(2, c, w)?;
    let col = if missing(&col_value) {
        0
    } else {
        coercev2_lookup_integer(scalar(&col_value, c, w)?, true, w)?
    };
    // POI evaluates width before height.
    let width = if args.len() > 4 {
        let value = args.get(4, c, w)?;
        if missing(&value) {
            base_width
        } else {
            coercev2_lookup_integer(scalar(&value, c, w)?, true, w)?
        }
    } else {
        base_width
    };
    let height = if args.len() > 3 {
        let value = args.get(3, c, w)?;
        if missing(&value) {
            base_height
        } else {
            coercev2_lookup_integer(scalar(&value, c, w)?, true, w)?
        }
    } else {
        base_height
    };
    if width == 0 || height == 0 {
        return Err(Error::Ref.into());
    }
    let rows = crate::geometry::OffsetAxis {
        start: start_row,
        delta: row,
        length: height,
        limit: 1_048_576,
    }
    .plan()?;
    let cols = crate::geometry::OffsetAxis {
        start: start_col,
        delta: col,
        length: width,
        limit: 16_384,
    }
    .plan()?;
    if rows.first < 0
        || cols.first < 0
        || rows.last >= 1_048_576
        || cols.last >= 16_384
        || rows.first > rows.last
        || cols.first > cols.last
    {
        return Ok(Value::SignedReference(crate::SignedReference {
            kind: ReferenceKind::Area,
            workbook: workbook.clone(),
            first_sheet,
            last_sheet,
            first_row: rows.first,
            last_row: rows.last,
            first_col: cols.first,
            last_col: cols.last,
        }));
    }
    Ok(Value::Reference(Reference {
        kind: ReferenceKind::Area,
        first_row: rows.first as u32,
        last_row: rows.last as u32,
        first_col: cols.first as u32,
        last_col: cols.last as u32,
        workbook: workbook.clone(),
        first_sheet,
        last_sheet,
    }))
}
/// Parse and resolve a computed reference through the host.
pub(super) fn indirect(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    if args.is_empty() {
        return Err(Error::Value.into());
    }
    let text = coercev2_lookup_text(&scalar(&args.get(0, c, w)?, c, w)?, w)?;
    let a1 = match args.len() {
        1 => true,
        2 => coercev2_lookup_boolean(scalar(&args.get(1, c, w)?, c, w)?)?,
        _ => return Err(Error::Value.into()),
    };
    let query = parse_dynamic_reference(&text, a1, w)?;
    charge(w, Work::QueryReturn, 1)?;
    // Structured reference syntax is resolved by the workbook parser, with its table metadata.
    if a1 && query.first.contains('[') && query.first.ends_with(']') {
        w.structured_reference(text.rsplit_once('!').map_or(text.as_str(), |(_, r)| r), c)
            .map_err(indirect_failure)
    } else {
        w.dynamic_reference(&query, c).map_err(indirect_failure)
    }
}
