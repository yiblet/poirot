//! Seeded random arrays and blank-border selection over existing host services.
use crate::eval::adapters::elementwise::matrices;
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::{CallHost, OperandKind, OperandReader, RandomDraws};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{
    Area, AreaKind, FunctionFailure, FunctionResult, ReferenceAreas, VALUE_ERROR, Value, err,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Ref;
use crate::model::CellValue::{Blank, Number};
use crate::model::{CellAddress, CellError, MAX_COLUMNS, MAX_ROWS};
use poirot_functions::matrix::{TrimBounds, TrimMode, TrimRectangle};

fn number(host: &mut CallHost<'_, '_>, index: usize, default: f64) -> FunctionResult<f64> {
    crate::eval::adapters::elementwise::NumericSignature::new(&[
        crate::eval::adapters::elementwise::NumericParameter::constant(default),
    ])
    .scalars(host, index)?
    .get(0)
}

fn random(host: &mut CallHost<'_, '_>, address: CellAddress) -> FunctionResult<Value> {
    // Options are forced once, left to right, before output member draws.
    let rows = number(host, 0, 1.0)?;
    let columns = number(host, 1, 1.0)?;
    let minimum = number(host, 2, 0.0)?;
    let maximum = number(host, 3, 1.0)?;
    let whole_number = number(host, 4, 0.0)?;
    let integer = whole_number != 0.0;
    if !rows.is_finite()
        || !columns.is_finite()
        || rows < 1.0
        || columns < 1.0
        || rows > f64::from(MAX_ROWS)
        || columns > f64::from(MAX_COLUMNS)
        || !minimum.is_finite()
        || !maximum.is_finite()
        || !whole_number.is_finite()
        || minimum >= maximum
    {
        return Err(VALUE_ERROR.into());
    }
    let interval = if integer {
        let bottom = minimum.ceil();
        let top = maximum.floor();
        if bottom > top {
            return Err(VALUE_ERROR.into());
        }
        Some(
            crate::eval::config::random::Interval::new(bottom, top).ok_or_else(|| {
                host.unsupported(
                    CapabilityId::FunctionAdapter,
                    "RANDARRAY integer bounds exceed the exactly representable integer envelope",
                )
            })?,
        )
    } else {
        None
    };
    let rows = rows.trunc() as usize;
    let columns = columns.trunc() as usize;
    matrices::generate(host, rows, columns, address, |host, _, _| {
        let draw = RandomDraws::new(host).draw(interval)?;
        let value = if integer {
            draw
        } else {
            // Convex interpolation avoids overflow for opposite-sign extreme bounds.
            let mapped = if minimum.signum() != maximum.signum() {
                minimum * (1.0 - draw) + maximum * draw
            } else {
                minimum + (maximum - minimum) * draw
            };
            mapped.max(minimum).min(maximum)
        };
        Ok(Number(value))
    })
}

fn mode(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<TrimMode> {
    let value = number(host, index, 3.0)?;
    if !value.is_finite() || !(0.0..4.0).contains(&value) {
        return Err(VALUE_ERROR.into());
    }
    TrimMode::new(value.trunc() as u8).ok_or(VALUE_ERROR.into())
}

fn trim(host: &mut CallHost<'_, '_>, address: CellAddress) -> FunctionResult<Value> {
    let shape = OperandReader::new(host).describe(0)?;
    if shape.missing || shape.sheets != 1 {
        return Err(VALUE_ERROR.into());
    }
    let rows = mode(host, 1)?;
    let columns = mode(host, 2)?;
    let mut bounds = TrimBounds::new(shape.rows, shape.columns)
        .map_err(|error| matrices::failure(host, error))?;
    if rows != TrimMode::None || columns != TrimMode::None {
        matrices::visit(host, 0, |host, row, column, operand| {
            if operand.value != Blank {
                bounds
                    .include(row, column)
                    .map_err(|error| matrices::failure(host, error))?;
            }
            Ok(())
        })?;
    }

    let rectangle = bounds.finish(rows, columns).ok_or(CellError::Calc)?;
    if shape.kind == OperandKind::Reference {
        let value = host.argument(0)?.into_value();
        return crop_reference(host, value, rectangle);
    }
    let row_count = rectangle
        .last_row
        .checked_sub(rectangle.first_row)
        .and_then(|n| n.checked_add(1))
        .ok_or(VALUE_ERROR)?;
    let columns = rectangle
        .last_column
        .checked_sub(rectangle.first_column)
        .and_then(|n| n.checked_add(1))
        .ok_or(VALUE_ERROR)?;
    matrices::generate(host, row_count, columns, address, |host, row, column| {
        let row = rectangle.first_row.checked_add(row).ok_or(VALUE_ERROR)?;
        let column = rectangle
            .first_column
            .checked_add(column)
            .ok_or(VALUE_ERROR)?;
        Ok(
            poirot_functions::coercion::compatibility::coerce_array_member(
                OperandReader::new(host).read_cell(0, row, column)?.value,
            ),
        )
    })
}

fn crop_reference(
    host: &mut CallHost<'_, '_>,
    value: Value,
    rectangle: TrimRectangle,
) -> FunctionResult<Value> {
    let r0 = i64::try_from(rectangle.first_row).map_err(|_| Ref)?;
    let r1 = i64::try_from(rectangle.last_row).map_err(|_| Ref)?;
    let c0 = i64::try_from(rectangle.first_column).map_err(|_| Ref)?;
    let c1 = i64::try_from(rectangle.last_column).map_err(|_| Ref)?;
    match value {
        Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
            let area = *areas.first().ok_or(VALUE_ERROR)?;
            let crop = Area {
                kind: AreaKind::Area,
                sheet: area.sheet,
                r0: u32::try_from(i64::from(area.r0).checked_add(r0).ok_or(Ref)?)
                    .map_err(|_| Ref)?,
                r1: u32::try_from(i64::from(area.r0).checked_add(r1).ok_or(Ref)?)
                    .map_err(|_| Ref)?,
                c0: u32::try_from(i64::from(area.c0).checked_add(c0).ok_or(Ref)?)
                    .map_err(|_| Ref)?,
                c1: u32::try_from(i64::from(area.c0).checked_add(c1).ok_or(Ref)?)
                    .map_err(|_| Ref)?,
            };
            if crop.r1 > area.r1 || crop.c1 > area.c1 {
                return Err(Ref.into());
            }
            Ok(Value::Refs(ReferenceAreas::One(crop)))
        }
        Value::SignedRefs(areas) if areas.len() == 1 => {
            let area = *areas.first().ok_or(VALUE_ERROR)?;
            let crop = crate::eval::values::signed_reference::SignedArea::new(
                AreaKind::Area,
                area.sheet,
                (
                    i32::try_from(i64::from(area.r0).checked_add(r0).ok_or(Ref)?)
                        .map_err(|_| Ref)?,
                    i32::try_from(i64::from(area.r0).checked_add(r1).ok_or(Ref)?)
                        .map_err(|_| Ref)?,
                ),
                (
                    i32::try_from(i64::from(area.c0).checked_add(c0).ok_or(Ref)?)
                        .map_err(|_| Ref)?,
                    i32::try_from(i64::from(area.c0).checked_add(c1).ok_or(Ref)?)
                        .map_err(|_| Ref)?,
                ),
            )
            .ok_or(Ref)?;
            if crop.r1 > area.r1 || crop.c1 > area.c1 {
                return Err(Ref.into());
            }
            let mut areas = Vec::new();
            host.check_values(1)?;
            areas
                .try_reserve_exact(1)
                .map_err(|_| host.allocation_failure())?;
            // SAFETY: One element was reserved locally before this single push.
            areas.push(crop);
            Ok(Value::SignedRefs(areas))
        }
        _ => Err(VALUE_ERROR.into()),
    }
}

/// Dispatch the two array producers without changing pinned-profile membership.
pub(in crate::eval) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let valid = match function {
        MatrixFunction::Randarray => args.len() <= 5,
        MatrixFunction::Trimrange => (1..=3).contains(&args.len()),
        _ => false,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(
        session,
        sheet,
        address,
        args,
        usize::from(function == MatrixFunction::Trimrange),
    );
    let result = match function {
        MatrixFunction::Randarray => random(&mut host, address),
        MatrixFunction::Trimrange => trim(&mut host, address),
        _ => Err(VALUE_ERROR.into()),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
