//! Prospective actual poirot facade operations; no HSSF in-cell/backend parity claim.
use eval::{
    EvaluationOutcome, FormulaCacheMaterializationPolicy as M, MaterializedScalarSummary as S,
    RunConfiguration,
};
use poirot::{CellAddress, CellError, CellValue, Workbook, eval, model};
fn at(row: u32, col: u32) -> CellAddress {
    CellAddress { row, col }
}
fn config(m: M) -> RunConfiguration {
    RunConfiguration::default().with_materialization_policy(m)
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.model_mut().add_sheet("S").unwrap();
    b
}
fn set_formula(b: &mut Workbook, row: u32, col: u32, f: &str) {
    b.model_mut()
        .sheet_mut(0)
        .unwrap()
        .set_formula(at(row, col), f)
        .unwrap();
}
fn get(b: &Workbook, row: u32, col: u32) -> CellValue {
    b.model()
        .sheet(0)
        .unwrap()
        .get(at(row, col))
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
fn policy(materialization: M) -> poirot::eval::overlay::OverlayPolicy {
    poirot::eval::overlay::OverlayPolicy {
        materialization,
        ..Default::default()
    }
}
fn output(overlay: &mut poirot::eval::overlay::Overlay<'_>, source: &Workbook) -> Workbook {
    let mut workbook = source.model().clone();
    workbook.apply(overlay.changes()).unwrap();
    Workbook::from_model(workbook)
}
#[path = "public_api_numeric152/publication_receipts.rs"]
mod publication_receipts;
#[path = "public_api_numeric152/retained_publication.rs"]
mod retained_publication;

#[test]
fn finite_errors_and_caught_results_survive_formula_cache_save_roundtrip() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "SQRT(-1)");
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    assert!(matches!(
        eval::run_recalculation_with_configuration(
            source.model_mut(),
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
            .model()
            .sheet(0)
            .unwrap()
            .get(at(0, 0))
            .unwrap()
            .formula
            .is_some()
    );
}
