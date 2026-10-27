//! Rectangular operand dimensions.

use crate::eval::values::Value;

/// Logical operand dimensions, inspected without reading referenced cells.
#[derive(Clone, Copy)]
pub(in crate::eval) struct Shape {
    pub(in crate::eval) rows: usize,
    pub(in crate::eval) columns: usize,
}

impl Shape {
    /// Inspect an operand's rectangular dimensions, if it has one bounded shape.
    pub(in crate::eval) fn inspect(value: &Value) -> Option<Self> {
        let (rows, columns) = match value {
            Value::Scalar(_) | Value::SelectedScalar(_, _) | Value::Missing => (1, 1),
            Value::Array(array) => {
                let columns = array.first()?.len();
                if columns == 0 || array.iter().any(|row| row.len() != columns) {
                    return None;
                }
                (array.len(), columns)
            }
            Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
                let area = *areas.first()?;
                (
                    usize::try_from(area.r1.checked_sub(area.r0)?.checked_add(1)?).ok()?,
                    usize::try_from(area.c1.checked_sub(area.c0)?.checked_add(1)?).ok()?,
                )
            }
            // SAFETY: The match guard checks areas.len() == 1 before reading its first element.
            Value::SignedRefs(areas) if areas.len() == 1 => areas[0].positive_shape()?,
            _ => return None,
        };
        Some(Self { rows, columns })
    }
}
