use crate::operands::{charge, checked_product};
use crate::{Error, Failure, Scalar, Value, Work, Workbook};
/// Borrowed array or reference backing for a logical lookup table.
enum Source<'a> {
    Signed(&'a crate::SignedReference),
    Reference(crate::domain::CheckedReference<'a>),
    Array(crate::domain::CheckedArray<'a>),
}
/// Validated table geometry with lazy host reads for referenced members.
pub(super) struct Table<'a> {
    source: Source<'a>,
    /// Logical number of rows.
    pub(super) rows: usize,
    /// Logical number of columns.
    pub(super) cols: usize,
}
impl<'a> Table<'a> {
    /// Validate an operand's geometry before exposing table access.
    pub(super) fn new(value: &'a Value) -> Result<Self, Failure> {
        let (source, rows, cols) = match value {
            Value::Reference(r) => {
                let checked = crate::domain::CheckedReference::new(r)
                    .ok_or(Failure::Spreadsheet(Error::Value))?;
                let (rows, cols) = (checked.height(), checked.width());
                (Source::Reference(checked), rows, cols)
            }
            Value::SignedReference(r) if r.valid() => {
                if r.source_height() <= 0 || r.source_width() <= 0 {
                    return Err(Failure::Unsupported(crate::Unsupported::SourceDimension));
                }
                (Source::Signed(r), r.height(), r.width())
            }
            Value::Array(a) => {
                let checked = crate::domain::CheckedArray::new(a)
                    .ok_or(Failure::Spreadsheet(Error::Value))?;
                let (rows, cols) = (checked.array().rows, checked.array().cols);
                (Source::Array(checked), rows, cols)
            }
            Value::Scalar(Scalar::Error(e)) | Value::SelectedScalar(Scalar::Error(e), _) => {
                return Err((*e).into())
            }
            _ => return Err(Error::Value.into()),
        };
        Ok(Self { source, rows, cols })
    }
    /// Retain source provenance when forwarding a table member as a scalar.
    pub(super) fn selected(&self, value: Scalar) -> Value {
        Value::SelectedScalar(
            value,
            match &self.source {
                Source::Array(_) => poirot_coercionv2::Origin::Array,
                _ => poirot_coercionv2::Origin::Reference,
            },
        )
    }
    /// Read one logical table member through its backing channel.
    pub(super) fn get(
        &self,
        row: usize,
        col: usize,
        w: &mut impl Workbook,
    ) -> Result<Scalar, Failure> {
        match &self.source {
            Source::Signed(r) => {
                if row >= self.rows || col >= self.cols {
                    return Err(Error::Ref.into());
                }
                r.read(
                    r.first_row.wrapping_add(row as i32),
                    r.first_col.wrapping_add(col as i32),
                    w,
                )
            }
            Source::Array(checked) => {
                let a = checked.array();
                if row >= a.rows || col >= a.cols {
                    return Err(Error::Ref.into());
                }
                if let Some(Scalar::Text(text)) = a.values.get(row * a.cols + col) {
                    charge(w, Work::VectorCopy, text.len())?;
                }
                checked.get(row, col)
            }
            Source::Reference(checked) => {
                let r = checked.reference();
                if row >= self.rows || col >= self.cols {
                    return Err(Error::Ref.into());
                }
                w.cell(
                    r,
                    r.first_sheet,
                    r.first_row + row as u32,
                    r.first_col + col as u32,
                )
            }
        }
    }
    /// Materialize an admitted row or column for lookup search.
    pub(super) fn vector(
        &self,
        column: bool,
        index: usize,
        w: &mut impl Workbook,
    ) -> Result<Vec<Scalar>, Failure> {
        let count = if column { self.rows } else { self.cols };
        charge(
            w,
            Work::VectorCopy,
            checked_product(count, std::mem::size_of::<Scalar>())?,
        )?;
        (0..count)
            .map(|i| {
                if column {
                    self.get(i, index, w)
                } else {
                    self.get(index, i, w)
                }
            })
            .collect()
    }
}
