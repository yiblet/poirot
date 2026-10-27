use super::super::incremental::Overlay;
use super::super::snapshot_state::SnapshotState;
use crate::eval::coordinates::EvaluationTarget;
use crate::model::{Cell, CellAddress, Workbook};
use std::borrow::Cow;

#[test]
fn malformed_style_only_markers_cannot_authorize_content_or_missing_writes() {
    for malformed in 0..3 {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_value(CellAddress { row: 0, col: 0 }, 1.0)
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let key = (0, CellAddress { row: 0, col: 0 });
        overlay
            .set_value(
                EvaluationTarget {
                    sheet: key.0,
                    address: key.1,
                },
                2.0,
            )
            .unwrap();
        let mut snapshot = overlay.snapshot().into_owned().unwrap();
        let SnapshotState::Owned(payload) = &mut snapshot.state else {
            panic!("owned payload")
        };
        match malformed {
            0 => {
                payload.style_only.to_mut().insert(key);
            }
            1 => {
                payload
                    .style_only
                    .to_mut()
                    .insert((0, CellAddress { row: 1, col: 0 }));
            }
            _ => {
                payload.writes.clear();
                payload.writes.push((0, key.1, Cow::Owned(Cell::new(1.0))));
                payload.style_only.to_mut().insert(key);
                payload.deleted.to_mut().insert(key);
            }
        }
        assert!(snapshot.restore().is_err());
    }
}
