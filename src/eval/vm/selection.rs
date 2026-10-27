//! Call-local rectangular branch selection shared by logical execution and the host.

use super::shape::Shape;
use crate::model::CellError;

/// A pending, selected, or failed coordinate in a rectangular branch result.
#[derive(Clone, Copy)]
pub(in crate::eval) enum Selection {
    Pending,
    Branch(usize),
    Error(CellError),
}

/// One broadcasting rule applies equally to needed conditions and selected results.
pub(in crate::eval) fn broadcast(left: Shape, right: Shape) -> Option<Shape> {
    fn axis(a: usize, b: usize) -> Option<usize> {
        (a == b || a == 1 || b == 1).then_some(a.max(b))
    }
    Some(Shape {
        rows: axis(left.rows, right.rows)?,
        columns: axis(left.columns, right.columns)?,
    })
}
