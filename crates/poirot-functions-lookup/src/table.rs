use crate::operands::{charge, checked_product};
use crate::{Error, Failure, Scalar, Value, Work, Workbook};
/// Borrowed array or reference backing for a logical lookup table.
enum Source<'a> {
    Signed(&'a crate::SignedReference),
    Reference(&'a crate::Reference),
    Array(&'a crate::Array),
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
            Value::Reference(r) => (Source::Reference(r), r.height(), r.width()),
            Value::SignedReference(r) if r.valid() => {
                if r.source_height() <= 0 || r.source_width() <= 0 {
                    return Err(Failure::Unsupported(crate::Unsupported::SourceDimension));
                }
                (Source::Signed(r), r.height(), r.width())
            }
            Value::Array(a) => (Source::Array(a), a.rows(), a.cols()),
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
            Source::Array(a) => {
                let value = a.get(row, col)?;
                if let Scalar::Text(text) = value {
                    charge(w, Work::VectorCopy, text.len())?;
                }
                Ok(value.clone())
            }
            Source::Reference(r) => {
                if row >= self.rows || col >= self.cols {
                    return Err(Error::Ref.into());
                }
                w.cell(
                    r,
                    r.first_sheet(),
                    r.first_row().checked_add(row as u32).ok_or(Error::Ref)?,
                    r.first_col().checked_add(col as u32).ok_or(Error::Ref)?,
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
