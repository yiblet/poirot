//! Column traversal preserves SUM reference behavior across edits and dynamic spills.
//! Expected sums follow https://support.microsoft.com/en-us/excel/functions/sum-function
//! and arrays fill adjacent cells as documented at:
//! https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::{Evaluation, Overlay};
use crate::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn column_ranges_preserve_order_inputs_tombstones_and_generated_cells() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [
        ("A1", 1e16),
        ("A17", 1.),
        ("A33", -1e16),
        ("A49", 2.),
        ("J25", 99.),
    ] {
        sheet.set_value(at(address).address, value).unwrap();
    }
    sheet.set_formula(at("Z1").address, "SUM(A1:A64)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for (step, expected) in [(0, 2.), (1, 7.), (2, 6.), (3, 13.), (4, 6.)] {
        match step {
            1 => {
                direct.set_value(at("A49"), 7.).unwrap();
                overlay.set_value(at("A49"), 7.).unwrap();
            }
            2 => {
                direct.remove_cell(at("A17")).unwrap();
                overlay.remove_cell(at("A17")).unwrap();
                direct.set_value(at("A49"), 6.).unwrap();
                overlay.set_value(at("A49"), 6.).unwrap();
            }
            3 => {
                // These coordinates exist only in inputs/generated cells.
                direct.set_formula(at("A60"), "{3;4}").unwrap();
                overlay.set_formula(at("A60"), "{3;4}").unwrap();
            }
            4 => {
                direct.remove_cell(at("A60")).unwrap();
                overlay.remove_cell(at("A60")).unwrap();
            }
            _ => {}
        }
        assert_eq!(direct.cell(at("Z1")).unwrap(), CellValue::Number(expected));
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("Z1")).unwrap(), CellValue::Number(expected));
    }
}

#[test]
fn authored_column_view_filters_generated_cache_without_erasing_its_visit_cost() {
    use crate::eval::inputs::effective_workbook::EffectiveWorkbook;
    use crate::model::query::{QueryCounters, RangeMembership};
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "{1;2;3}").unwrap();
    sheet.set_value(at("A2").address, CellValue::Blank).unwrap();
    for (address, value) in [("A2", 2.), ("A3", 3.)] {
        sheet
            .set_spill_cached_value(
                at(address).address,
                at("A1").address,
                "{1;2;3}",
                CellValue::Number(value),
            )
            .unwrap();
    }
    let area = RangeMembership::new(0, at("A2").address, at("A3").address).unwrap();
    let mut cells = EffectiveWorkbook::new(&book, None).stored(area).unwrap();
    let (address, source) = cells.next().unwrap();
    assert_eq!(*address, at("A2").address);
    assert_eq!(source.value, CellValue::Blank);
    assert!(cells.next().is_none());
    // One column bound seek and one loaded page visit both physical cached
    // payloads, although only A2 has an authored source to return.
    assert_eq!(
        cells.counters(),
        QueryCounters {
            row_seeks: 1,
            column_seeks: 1,
            cells_visited: 2,
            key_lookups: 0,
        }
    );
}
