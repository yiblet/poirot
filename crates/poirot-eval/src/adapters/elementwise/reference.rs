//! Intact source roles and checked metadata projection without reference reads.
use super::{ArgumentRole, SourceIdentity, matrices};
use crate::host::CallHost;
use crate::*;

/// The axis inspected by a metadata consumer.
#[derive(Clone, Copy)]
pub(in crate::adapters) enum Axis {
    Row,
    Column,
}
/// A checked reference rectangle retaining signed coordinates and sheet identity.
pub(in crate::adapters) struct Geometry {
    pub(in crate::adapters) sheet: usize,
    pub(in crate::adapters) row: i64,
    pub(in crate::adapters) column: i64,
    pub(in crate::adapters) rows: usize,
    pub(in crate::adapters) columns: usize,
}
impl SourceIdentity {
    /// Transfer the original source once, without scalar intersection or cell reads.
    pub(in crate::adapters) fn read(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
        Ok(host.selected_argument(self.argument())?.1)
    }
    /// Require one reference rectangle without materializing worksheet cells.
    pub(in crate::adapters) fn geometry(value: &Value) -> FunctionResult<Geometry> {
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
    pub(in crate::adapters) fn areas(
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
    pub(in crate::adapters) fn dimension(
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
    pub(in crate::adapters) fn sheet(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<CellValue> {
        let value = self.read(host)?;
        let sheet = match value {
            Value::SheetSpan(areas) => areas.first().ok_or(NA)?.sheet,
            Value::SignedRefs(areas) if areas.len() == 1 => areas.first().ok_or(NA)?.sheet,
            Value::Scalar(Text(name)) | Value::SelectedScalar(Text(name), _) => {
                host.resolve_sheet(name)?.ok_or(NA)?
            }
            Value::Scalar(value @ CellValue::TextUtf16(_))
            | Value::SelectedScalar(value @ CellValue::TextUtf16(_), _) => {
                let name = host.coercev2_scalar_text(&value)?;
                host.resolve_sheet(name)?.ok_or(NA)?
            }
            value => {
                Self::geometry(&value)
                    .map_err(|failure| match failure {
                        FunctionFailure::Spreadsheet(CellError::Value) => {
                            FunctionFailure::Spreadsheet(NA)
                        }
                        other => other,
                    })?
                    .sheet
            }
        };
        Ok(Number(sheet as f64 + 1.))
    }
    /// Publish a reference axis as a modern array with checked output admission.
    pub(in crate::adapters) fn coordinates(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signed_reference::SignedArea;

    #[test]
    fn signed_geometry_retains_logical_coordinates_without_grid_projection() {
        let value = Value::SignedRefs(vec![
            SignedArea::new(AreaKind::Area, 2, (-2, 1), (-1, 1)).unwrap(),
        ]);
        let geometry = SourceIdentity::geometry(&value).unwrap();
        assert_eq!((geometry.sheet, geometry.row, geometry.column), (2, -2, -1));
        assert_eq!((geometry.rows, geometry.columns), (4, 3));
    }
}
