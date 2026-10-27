//! Prospective actual poirot facade operations; no HSSF in-cell/backend parity claim.
use eval::{EvaluationOutcome, MaterializedScalarSummary as S, RunConfiguration};
use poirot::eval;
use poirot::model;
use poirot::{CellAddress, CellError, CellValue, Workbook};
fn at(row: u32, col: u32) -> CellAddress {
    CellAddress { row, col }
}
fn config() -> RunConfiguration {
    RunConfiguration::default()
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn set_formula(b: &mut Workbook, row: u32, col: u32, f: &str) {
    b.sheet_mut(0)
        .unwrap()
        .set_formula(at(row, col), f)
        .unwrap();
}
fn get(b: &Workbook, row: u32, col: u32) -> CellValue {
    b.sheet(0)
        .unwrap()
        .get(at(row, col))
        .unwrap()
        .unwrap()
        .value
        .clone()
}
fn bits(v: CellValue) -> u64 {
    match v {
        CellValue::Number(n) => n.to_bits(),
        other => panic!("{other:?}"),
    }
}
fn target(row: u32, col: u32) -> eval::EvaluationTarget {
    eval::EvaluationTarget {
        sheet: 0,
        address: at(row, col),
    }
}
fn policy() -> poirot::eval::EvaluationPolicy {
    poirot::eval::EvaluationPolicy {
        ..Default::default()
    }
}
fn output(overlay: &mut poirot::eval::overlay::Overlay<'_>, source: &Workbook) -> Workbook {
    let mut workbook = source.clone();
    workbook.apply(overlay.changes()).unwrap();
    workbook
}
mod publication_receipts;
mod retained_publication;

#[test]
fn finite_errors_and_caught_results_survive_formula_cache_save_roundtrip() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "SQRT(-1)");
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    assert!(matches!(
        eval::run_recalculation_with_configuration(
            &mut source,
            &RunConfiguration::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete { .. }
    ));
    let bytes = source.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    assert_eq!(get(&reopened, 0, 0), CellValue::Error(CellError::Num));
    assert_eq!(get(&reopened, 0, 1), CellValue::Number(42.));
    assert!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at(0, 0))
            .unwrap()
            .unwrap()
            .formula
            .is_some()
    );
}
