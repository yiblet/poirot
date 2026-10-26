//! Intact source preparation, checked selection, and channel-preserving publication.
use super::{ArgumentRole, Prepared};
use crate::host::{CallHost, MissingPolicy, OperandKind, ScalarOperand};
use crate::*;

/// An intact scalar, array, or reference source excluded from broadcast geometry.
pub(in crate::adapters) struct SourceIdentity(usize);
impl ArgumentRole for SourceIdentity {
    const BROADCAST: bool = false;
    fn argument(&self) -> usize {
        self.0
    }
    fn bind(argument: usize, _member: usize, _present: bool) -> Self {
        Self(argument)
    }
}
impl SourceIdentity {
    /// Classify the original source channel without reading reference cells.
    pub(in crate::adapters) fn kind(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<OperandKind> {
        host.kind(self.0)
    }
    /// Describe the intact source without reading its member cells.
    pub(in crate::adapters) fn describe(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<crate::host::OperandDescription> {
        host.describe(self.0)
    }
    /// Select a scalar only when the consuming kernel requests its contents.
    pub(in crate::adapters) fn scalar(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<ScalarOperand> {
        host.scalar_data(self.0, MissingPolicy::Blank)
    }
}

/// Pure projection policy applied after typed controls have been prepared.
pub(in crate::adapters) trait Selector<C> {
    /// Bound the pure geometry work before visiting any candidate reference areas.
    fn work(&self, _: &Prepared<SourceIdentity>) -> usize {
        0
    }
    /// Choose source-relative geometry without reading cells or accessing the host.
    fn select(
        &self,
        source: &Prepared<SourceIdentity>,
        controls: C,
        at: CellAddress,
    ) -> FunctionResult<Projection>;
}
/// Typed fixed controls retaining each function's demand and coercion stages.
pub(in crate::adapters) trait Controls {
    /// Coerced controls consumed by the pure selector.
    type Values;
    /// Prepare controls against the intact source without repeating expression evaluation.
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        source: &Prepared<SourceIdentity>,
    ) -> FunctionResult<Self::Values>;
}
impl Controls for () {
    /// No fixed controls are needed by this projection.
    type Values = ();
    fn prepare(
        &self,
        _: &mut CallHost<'_, '_>,
        _: &Prepared<SourceIdentity>,
    ) -> FunctionResult<()> {
        Ok(())
    }
}
/// A source-relative selection, preserving zero axes until publication.
pub(in crate::adapters) enum Projection {
    /// Consume one scalar or the array top-left item without copying its members.
    Scalar,
    /// Retain reference identity or select an array rectangle.
    Rectangle {
        area: usize,
        row: usize,
        column: usize,
    },
    /// Read exactly one selected item and publish an explicit scalar expression.
    Point {
        area: usize,
        row: usize,
        column: usize,
    },
    /// Publish checked absolute endpoints on the intact source's sheet.
    BoundedRectangle {
        rows: (u32, u32),
        columns: (u32, u32),
    },
    /// Resolve finalized spill ownership before publishing its reference rectangle.
    Spill,
}
/// Force sources and controls, apply pure geometry, and publish selected channels.
pub(in crate::adapters) fn execute<C: Controls, S: Selector<C::Values>>(
    host: &mut CallHost<'_, '_>,
    controls: C,
    selector: S,
) -> FunctionResult<Value> {
    let source = SourceIdentity::bind(0, 0, true).prepare(host)?;
    let values = controls.prepare(host, &source)?;
    project(host, source, values, selector)
}
/// Apply the same projection/publication to an already evaluated operator source.
pub(in crate::adapters) fn project<C, S: Selector<C>>(
    host: &mut CallHost<'_, '_>,
    source: Prepared<SourceIdentity>,
    controls: C,
    selector: S,
) -> FunctionResult<Value> {
    host.charge_work(selector.work(&source) as u64)?;
    let projection = selector.select(&source, controls, host.target().address)?;
    source.publish(host, projection)
}

impl SourceIdentity {
    /// Own an intact source through control admission, projection, and publication.
    pub(in crate::adapters) fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<Prepared<Self>> {
        Ok(Prepared::new(self.read(host)?))
    }
}
impl Prepared<SourceIdentity> {
    /// Borrow the original channel for pure policy decisions without reading members.
    pub(in crate::adapters) fn value(&self) -> &Value {
        &self.value
    }
    /// Count selectable areas while retaining union order.
    pub(in crate::adapters) fn area_count(&self) -> usize {
        match &self.value {
            Value::Refs(a) => a.len(),
            Value::SignedRefs(a) => a.len(),
            _ => 1,
        }
    }
    /// Count reference areas visited by implicit intersection; scalar/array values need no scan.
    pub(in crate::adapters) fn reference_work(&self) -> usize {
        match &self.value {
            Value::Refs(a) => a.len(),
            Value::SignedRefs(a) => a.len(),
            Value::SheetSpan(a) => usize::from(!a.is_empty()),
            _ => 0,
        }
    }
    /// Inspect one selected area's dimensions without dereferencing its members.
    pub(in crate::adapters) fn dimensions(&self, area: usize) -> FunctionResult<(usize, usize)> {
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
    pub(in crate::adapters) fn intersection(&self, at: CellAddress) -> FunctionResult<Projection> {
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
            if let Some((row, column)) = poirot_functions_lookup::intersection_offsets(
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
    fn publish(self, host: &mut CallHost<'_, '_>, projection: Projection) -> FunctionResult<Value> {
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
                    host.sheet_cell(
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
                let anchor = crate::spill_publication::source_anchor(&self.value)?;
                let area = host.spill_extent(anchor)?;
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
/// Select array members while retaining their logical anchor and element origin.
fn array_projection(
    host: &mut CallHost<'_, '_>,
    value: Value,
    row: usize,
    column: usize,
) -> FunctionResult<Value> {
    if matches!(value, Value::Scalar(_) | Value::SelectedScalar(_, _)) {
        return Ok(value);
    }
    let anchor = match &value {
        Value::Array(a) => a.anchor,
        _ => None,
    };
    let values = host.matrix(value)?.map_err(FunctionFailure::Spreadsheet)?;
    let height = values.len();
    let width = values.first().map_or(0, Vec::len);
    let rows = if row == 0 { height } else { 1 };
    let columns = if column == 0 { width } else { 1 };
    let start_row = row.saturating_sub(1);
    let start_column = column.saturating_sub(1);
    if row != 0 && column != 0 {
        let value = values
            .get(start_row)
            .and_then(|r| r.get(start_column))
            .ok_or(Ref)?;
        return Ok(Value::SelectedScalar(
            host.copy_cell_value(value)?,
            crate::host::OperandOrigin::ArrayElement,
        ));
    }
    let anchor = anchor.and_then(|a| {
        Some(CellAddress {
            row: a.row.checked_add(u32::try_from(start_row).ok()?)?,
            col: a.col.checked_add(u32::try_from(start_column).ok()?)?,
        })
    });
    let source = Value::Array(ArrayView::anchored(values, anchor));
    let mut result = host.selected_array(
        crate::array_operator::Shape { rows, columns },
        |reader, r, c| {
            reader.member(
                &source,
                crate::array_operator::Shape {
                    rows: height,
                    columns: width,
                },
                r.checked_add(start_row)
                    .ok_or(EvaluationFailure::StalePlan)?,
                c.checked_add(start_column)
                    .ok_or(EvaluationFailure::StalePlan)?,
            )
        },
    )?;
    if let Value::Array(a) = &mut result {
        a.anchor = anchor;
    }
    Ok(result)
}
