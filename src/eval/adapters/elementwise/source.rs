//! Intact source preparation, checked selection, and channel-preserving publication.
use super::argument_role::ArgumentRole;
use super::matrices;
use super::prepared::Prepared;
use super::reference::{Axis, Geometry};
use crate::eval::host::{
    ArgumentCoercion, CallHost, DynamicReferences, MissingPolicy, OperandKind, OperandReader,
    ScalarOperand, SelectionAccess,
};
use crate::eval::limits::EvaluationError;
use crate::eval::values::{ArrayView, FunctionError, FunctionResult, VALUE_ERROR, Value};
use crate::model::CellError::{NA, Ref};
use crate::model::CellValue::{Blank, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue};

/// Argument role that preserves a source's scalar, array, or reference identity.
/// Metadata and reference selectors can inspect or project that source without adding its shape to broadcast output.
pub(in crate::eval::adapters) struct SourceIdentity(usize);

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
    pub(in crate::eval::adapters) fn kind(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<OperandKind> {
        host.kind(self.0)
    }

    /// Describe the intact source without reading its member cells.
    pub(in crate::eval::adapters) fn describe(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<crate::eval::host::OperandDescription> {
        OperandReader::new(host).describe(self.0)
    }

    /// Select a scalar only when the consuming kernel requests its contents.
    pub(in crate::eval::adapters) fn scalar(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<ScalarOperand> {
        host.scalar_data(self.0, MissingPolicy::Blank)
    }

    /// Transfer the original source once, without scalar intersection or cell reads.
    pub(in crate::eval::adapters) fn read(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<Value> {
        Ok(SelectionAccess::new(host).argument(self.argument())?.1)
    }

    /// Require one reference rectangle without materializing worksheet cells.
    pub(in crate::eval::adapters) fn geometry(value: &Value) -> FunctionResult<Geometry> {
        match value {
            Value::SheetSpan(_) => Err(VALUE_ERROR.into()),
            Value::Refs(areas) if areas.len() != 1 => Err(VALUE_ERROR.into()),
            Value::SignedRefs(areas) if areas.len() != 1 => Err(VALUE_ERROR.into()),
            _ => Self::area_geometry(value, 0),
        }
    }

    /// Inspect one retained union area using the same checked geometry as metadata consumers.
    pub(super) fn area_geometry(value: &Value, index: usize) -> FunctionResult<Geometry> {
        match value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let a = areas.get(index).ok_or(VALUE_ERROR)?;
                Ok(Geometry {
                    sheet: a.sheet,
                    row: i64::from(a.r0),
                    column: i64::from(a.c0),
                    rows: a
                        .r1
                        .checked_sub(a.r0)
                        .and_then(|n| n.checked_add(1))
                        .ok_or(VALUE_ERROR)? as usize,
                    columns: a
                        .c1
                        .checked_sub(a.c0)
                        .and_then(|n| n.checked_add(1))
                        .ok_or(VALUE_ERROR)? as usize,
                })
            }
            Value::SignedRefs(areas) => {
                let a = areas.get(index).ok_or(VALUE_ERROR)?;
                let (rows, columns) = a.positive_shape().ok_or(VALUE_ERROR)?;
                Ok(Geometry {
                    sheet: a.sheet,
                    row: i64::from(a.r0),
                    column: i64::from(a.c0),
                    rows,
                    columns,
                })
            }
            Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => {
                Err((*error).into())
            }
            _ => Err(VALUE_ERROR.into()),
        }
    }

    /// Count retained union areas; non-reference operands are invalid references.
    pub(in crate::eval::adapters) fn areas(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<CellValue> {
        let count = match self.read(host)? {
            Value::Refs(areas) => areas.len(),
            Value::SheetSpan(areas) if !areas.is_empty() => 1,
            Value::SignedRefs(areas) => areas.len(),
            Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => {
                return Err(error.into());
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        Ok(Number(count as f64))
    }

    /// Return array dimensions or a reference's logical extent without reading cells.
    pub(in crate::eval::adapters) fn dimension(
        &self,
        host: &mut CallHost<'_, '_>,
        axis: Axis,
    ) -> FunctionResult<CellValue> {
        let value = self.read(host)?;
        let (rows, columns) = match &value {
            Value::Array(array) => (array.len(), array.first().map_or(0, Vec::len)),
            Value::Scalar(Blank) | Value::SelectedScalar(Blank, _) | Value::Missing => {
                return Err(VALUE_ERROR.into());
            }
            Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => {
                return Err((*error).into());
            }
            Value::Scalar(_) | Value::SelectedScalar(_, _) => (1, 1),
            _ => {
                let geometry = Self::geometry(&value)?;
                (geometry.rows, geometry.columns)
            }
        };
        Ok(Number(match axis {
            Axis::Row => rows,
            Axis::Column => columns,
        } as f64))
    }

    /// Resolve a reference's sheet or a text sheet name with dependency tracking.
    pub(in crate::eval::adapters) fn sheet(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<CellValue> {
        let value = self.read(host)?;
        let sheet = match value {
            Value::SheetSpan(areas) => areas.first().ok_or(NA)?.sheet,
            Value::SignedRefs(areas) if areas.len() == 1 => areas.first().ok_or(NA)?.sheet,
            Value::Scalar(Text(name)) | Value::SelectedScalar(Text(name), _) => {
                DynamicReferences::new(host)
                    .resolve_sheet(name)?
                    .ok_or(NA)?
            }
            Value::Scalar(value @ CellValue::TextUtf16(_))
            | Value::SelectedScalar(value @ CellValue::TextUtf16(_), _) => {
                let name = ArgumentCoercion::new(host).coerce_scalar_text(&value)?;
                DynamicReferences::new(host)
                    .resolve_sheet(name)?
                    .ok_or(NA)?
            }
            value => {
                Self::geometry(&value)
                    .map_err(|failure| match failure {
                        FunctionError::Spreadsheet(CellError::Value) => {
                            FunctionError::Spreadsheet(NA)
                        }
                        other => other,
                    })?
                    .sheet
            }
        };
        Ok(Number(sheet as f64 + 1.))
    }

    /// Publish a reference axis as a modern array with checked output admission.
    pub(in crate::eval::adapters) fn coordinates(
        &self,
        host: &mut CallHost<'_, '_>,
        axis: Axis,
    ) -> FunctionResult<Value> {
        let value = self.read(host)?;
        let geometry = Self::geometry(&value)?;
        let (start, rows, columns) = match axis {
            Axis::Row => (geometry.row, geometry.rows, 1),
            Axis::Column => (geometry.column, 1, geometry.columns),
        };
        if rows == 1 && columns == 1 {
            return Ok(Number(start as f64 + 1.).into());
        }
        matrices::generate(
            host,
            rows,
            columns,
            host.target().address,
            |_, row, column| Ok(Number(start as f64 + row as f64 + column as f64 + 1.)),
        )
    }
}

/// Pure projection policy applied after typed controls have been prepared.
pub(in crate::eval::adapters) trait Selector<C> {
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
pub(in crate::eval::adapters) trait Controls {
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
pub(in crate::eval::adapters) enum Projection {
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
pub(in crate::eval::adapters) fn execute<C: Controls, S: Selector<C::Values>>(
    host: &mut CallHost<'_, '_>,
    controls: C,
    selector: S,
) -> FunctionResult<Value> {
    let source = SourceIdentity::bind(0, 0, true).prepare(host)?;
    let values = controls.prepare(host, &source)?;
    project(host, source, values, selector)
}

/// Apply the same projection/publication to an already evaluated operator source.
pub(in crate::eval::adapters) fn project<C, S: Selector<C>>(
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
    pub(in crate::eval::adapters) fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<Prepared<Self>> {
        Ok(Prepared::new(self.read(host)?))
    }
}

/// Select array members while retaining their logical anchor and element origin.
pub(super) fn array_projection(
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
    let values = OperandReader::new(host)
        .matrix(value)?
        .map_err(FunctionError::Spreadsheet)?;
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
            crate::eval::values::OperandOrigin::ArrayElement,
        ));
    }
    let anchor = anchor.and_then(|a| {
        Some(CellAddress {
            row: a.row.checked_add(u32::try_from(start_row).ok()?)?,
            col: a.col.checked_add(u32::try_from(start_column).ok()?)?,
        })
    });
    let source = Value::Array(ArrayView::anchored(values, anchor));
    let mut result = SelectionAccess::new(host).array(
        crate::eval::vm::Shape { rows, columns },
        |reader, r, c| {
            reader.member(
                &source,
                crate::eval::vm::Shape {
                    rows: height,
                    columns: width,
                },
                r.checked_add(start_row).ok_or(EvaluationError::StalePlan)?,
                c.checked_add(start_column)
                    .ok_or(EvaluationError::StalePlan)?,
            )
        },
    )?;
    if let Value::Array(a) = &mut result {
        a.anchor = anchor;
    }
    Ok(result)
}
