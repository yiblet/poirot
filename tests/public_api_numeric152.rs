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
        .set_formula(at(row, col), f, CellValue::Blank)
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
fn policy(materialization: M) -> poirot::OverlayPolicy {
    poirot::OverlayPolicy {
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
#[path = "public_api_numeric152/save_fidelity.rs"]
mod save_fidelity;
