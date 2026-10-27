//! Reference geometry and metadata axes read from intact sources without reference reads.

/// The axis inspected by a metadata consumer.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum Axis {
    Row,
    Column,
}

/// Reference geometry used for metadata queries and reference transformations without cell reads.
/// Signed coordinates preserve logical off-grid sources until the consuming operation applies its bounds rules.
pub(in crate::eval::adapters) struct Geometry {
    pub(in crate::eval::adapters) sheet: usize,
    pub(in crate::eval::adapters) row: i64,
    pub(in crate::eval::adapters) column: i64,
    pub(in crate::eval::adapters) rows: usize,
    pub(in crate::eval::adapters) columns: usize,
}

#[cfg(test)]
mod tests {
    use crate::eval::adapters::elementwise::source::SourceIdentity;
    use crate::eval::values::signed_reference::SignedArea;
    use crate::eval::values::{AreaKind, Value};

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
