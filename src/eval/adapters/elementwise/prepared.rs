use super::broadcast::Broadcast;
use super::source::{Projection, SourceIdentity, array_projection};
use crate::eval::host::{CallHost, DynamicReferences, OperandReader};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::values::{
    Area, AreaKind, FunctionFailure, FunctionResult, ReferenceAreas, VALUE_ERROR, Value,
};
use crate::eval::vm::Shape;
use crate::model::CellError::Ref;
use crate::model::CellValue::Blank;
use crate::model::{CellAddress, CellValue, MAX_COLUMNS, MAX_ROWS};

/// Owns a value after a deferred argument has been selected and evaluated.
/// It preserves the scalar, array, or reference channel for subsequent member reads or whole-value consumption; computed arrays are still evaluated as a whole.
pub(in crate::eval::adapters) struct Prepared<R> {
    pub(super) value: Value,
    pub(super) role: std::marker::PhantomData<R>,
}

impl<R> Prepared<R> {
    /// Retain an owned selected value or a function's omitted-result default.
    pub(in crate::eval::adapters) fn new(value: Value) -> Self {
        Self {
            value,
            role: std::marker::PhantomData,
        }
    }

    /// Return the original result channel without scalarizing references or arrays.
    pub(in crate::eval::adapters) fn into_value(self) -> Value {
        self.value
    }
}

impl Prepared<Broadcast> {
    /// Inspect selected geometry without reading any reference members.
    pub(in crate::eval::adapters) fn shape(&self) -> Option<Shape> {
        Shape::inspect(&self.value)
    }

    /// Read only a selected output coordinate through the checked broadcast reader.
    pub(in crate::eval::adapters) fn read(
        &self,
        reader: &mut crate::eval::host::SelectedReader<'_, '_>,
        shape: crate::eval::vm::Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        reader.member(&self.value, shape, row, column)
    }
}

impl Prepared<SourceIdentity> {
    /// Borrow the original channel for pure policy decisions without reading members.
    pub(in crate::eval::adapters) fn value(&self) -> &Value {
        &self.value
    }

    /// Count selectable areas while retaining union order.
    pub(in crate::eval::adapters) fn area_count(&self) -> usize {
        match &self.value {
            Value::Refs(a) => a.len(),
            Value::SignedRefs(a) => a.len(),
            _ => 1,
        }
    }

    /// Count reference areas visited by implicit intersection; scalar/array values need no scan.
    pub(in crate::eval::adapters) fn reference_work(&self) -> usize {
        match &self.value {
            Value::Refs(a) => a.len(),
            Value::SignedRefs(a) => a.len(),
            Value::SheetSpan(a) => usize::from(!a.is_empty()),
            _ => 0,
        }
    }

    /// Inspect one selected area's dimensions without dereferencing its members.
    pub(in crate::eval::adapters) fn dimensions(
        &self,
        area: usize,
    ) -> FunctionResult<(usize, usize)> {
        match &self.value {
            Value::Array(a) => Ok((a.len(), a.first().map_or(0, Vec::len))),
            Value::SheetSpan(areas) if areas.len() != 1 => {
                Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "matrix materialization of a multi-sheet reference is not verified"
                        .into(),
                }
                .into())
            }
            Value::Refs(_) | Value::SignedRefs(_) | Value::SheetSpan(_) => {
                let geometry =
                    SourceIdentity::area_geometry(&self.value, area).map_err(|failure| {
                        match failure {
                            FunctionFailure::Spreadsheet(_) => FunctionFailure::Spreadsheet(Ref),
                            other => other,
                        }
                    })?;
                Ok((geometry.rows, geometry.columns))
            }
            _ => Ok((1, 1)),
        }
    }

    /// Select one unique reference point, or the top-left item of an array/scalar.
    pub(in crate::eval::adapters) fn intersection(
        &self,
        at: CellAddress,
    ) -> FunctionResult<Projection> {
        if matches!(
            self.value,
            Value::Scalar(_) | Value::SelectedScalar(_, _) | Value::Array(_) | Value::Missing
        ) {
            return Ok(Projection::Scalar);
        }
        let count = match &self.value {
            Value::SheetSpan(a) if !a.is_empty() => 1,
            _ => self.area_count(),
        };
        let mut selected = None;
        for area in 0..count {
            let g = SourceIdentity::area_geometry(&self.value, area)?;
            let last_row = g
                .row
                .checked_add(
                    i64::try_from(g.rows)
                        .map_err(|_| Ref)?
                        .checked_sub(1)
                        .ok_or(Ref)?,
                )
                .ok_or(Ref)?;
            let last_column = g
                .column
                .checked_add(
                    i64::try_from(g.columns)
                        .map_err(|_| Ref)?
                        .checked_sub(1)
                        .ok_or(Ref)?,
                )
                .ok_or(Ref)?;
            if let Some((row, column)) = poirot_functions::lookup::intersection_offsets(
                (g.row, last_row),
                (g.column, last_column),
                (i64::from(at.row), i64::from(at.col)),
            ) {
                if selected.is_some() {
                    return Err(VALUE_ERROR.into());
                }
                selected = Some(Projection::Point { area, row, column });
            }
        }
        selected.ok_or(VALUE_ERROR.into())
    }

    /// Publish selected geometry through checked reference and array services.
    pub(super) fn publish(
        self,
        host: &mut CallHost<'_, '_>,
        projection: Projection,
    ) -> FunctionResult<Value> {
        let (area, row, column) = match projection {
            Projection::Scalar => return Ok(Value::Scalar(host.select_legacy_scalar(self.value)?)),
            Projection::Rectangle { area, row, column } => (area, row, column),
            Projection::Point { area, row, column } => {
                let g = SourceIdentity::area_geometry(&self.value, area)?;
                if row == 0 || column == 0 || row > g.rows || column > g.columns {
                    return Err(VALUE_ERROR.into());
                }
                let row = g
                    .row
                    .checked_add(i64::try_from(row.checked_sub(1).ok_or(Ref)?).map_err(|_| Ref)?)
                    .ok_or(Ref)?;
                let column = g
                    .column
                    .checked_add(i64::try_from(column.checked_sub(1).ok_or(Ref)?).map_err(|_| Ref)?)
                    .ok_or(Ref)?;
                let value = if (0..i64::from(MAX_ROWS)).contains(&row)
                    && (0..i64::from(MAX_COLUMNS)).contains(&column)
                {
                    DynamicReferences::new(host).sheet_cell(
                        g.sheet,
                        CellAddress {
                            row: u32::try_from(row).map_err(|_| Ref)?,
                            col: u32::try_from(column).map_err(|_| Ref)?,
                        },
                    )?
                } else {
                    Blank
                };
                return Ok(Value::Scalar(value));
            }
            Projection::BoundedRectangle { rows, columns } => {
                let geometry = SourceIdentity::geometry(&self.value)?;
                if rows.0 > rows.1
                    || columns.0 > columns.1
                    || rows.1 >= MAX_ROWS
                    || columns.1 >= MAX_COLUMNS
                {
                    return Err(Ref.into());
                }
                host.check_values(1)?;
                return Ok(Value::Refs(ReferenceAreas::One(Area {
                    kind: AreaKind::Area,
                    sheet: geometry.sheet,
                    r0: rows.0,
                    r1: rows.1,
                    c0: columns.0,
                    c1: columns.1,
                })));
            }
            Projection::Spill => {
                let anchor = crate::eval::spill::source_anchor(&self.value)?;
                let area = OperandReader::new(host).spill_extent(anchor)?;
                host.check_values(1)?;
                return Ok(Value::Refs(ReferenceAreas::One(area)));
            }
        };
        let selected = match self.value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut a = *areas.get(area).ok_or(Ref)?;
                a.kind = AreaKind::Area;
                if row != 0 {
                    a.r0 =
                        a.r0.checked_add(
                            u32::try_from(row.checked_sub(1).ok_or(Ref)?).map_err(|_| Ref)?,
                        )
                        .ok_or(Ref)?;
                    a.r1 = a.r0;
                }
                if column != 0 {
                    a.c0 =
                        a.c0.checked_add(
                            u32::try_from(column.checked_sub(1).ok_or(Ref)?).map_err(|_| Ref)?,
                        )
                        .ok_or(Ref)?;
                    a.c1 = a.c0;
                }
                host.check_values(1)?;
                Value::Refs(ReferenceAreas::One(a))
            }
            Value::SignedRefs(areas) => {
                let a = areas.get(area).ok_or(Ref)?.select(row, column);
                host.check_values(1)?;
                let mut selected = Vec::new();
                selected
                    .try_reserve_exact(1)
                    .map_err(|_| host.allocation_failure())?;
                // SAFETY: One reference slot was reserved immediately above.
                selected.push(a);
                Value::SignedRefs(selected)
            }
            value => array_projection(host, value, row, column)?,
        };
        Ok(selected)
    }
}
