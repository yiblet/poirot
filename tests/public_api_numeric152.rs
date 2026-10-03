//! Prospective actual poirot facade operations; no HSSF in-cell/backend parity claim.
use eval::{
    EvaluationOutcome, FormulaCacheMaterializationPolicy as M, MaterializedScalarSummary as S,
    NumericOutcomePolicy as N, RunConfiguration,
};
use poirot::{CellAddress, CellError, CellValue, Spreadsheet, eval, model};
fn at(row: u32, col: u32) -> CellAddress {
    CellAddress { row, col }
}
fn config(m: M) -> RunConfiguration {
    RunConfiguration::default()
        .with_numeric_policy(N::PinnedPoi7321209)
        .with_materialization_policy(m)
}
fn book() -> Spreadsheet {
    let mut b = Spreadsheet::new();
    b.workbook_mut().add_sheet("S").unwrap();
    b
}
fn set_formula(b: &mut Spreadsheet, row: u32, col: u32, f: &str) {
    b.workbook_mut()
        .sheet_mut(0)
        .unwrap()
        .set_formula(at(row, col), f, CellValue::Blank)
        .unwrap();
}
fn get(b: &Spreadsheet, row: u32, col: u32) -> CellValue {
    b.workbook()
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
        numeric: N::PinnedPoi7321209,
        materialization,
        ..Default::default()
    }
}
fn output(overlay: &mut poirot::Overlay<'_>, source: &Spreadsheet) -> Spreadsheet {
    let mut workbook = source.workbook().clone();
    workbook.apply(overlay.changes()).unwrap();
    Spreadsheet::from_workbook(workbook)
}
#[path = "public_api_numeric152/publication_receipts.rs"]
mod publication_receipts;
#[path = "public_api_numeric152/retained_publication.rs"]
mod retained_publication;
#[path = "public_api_numeric152/save_fidelity.rs"]
mod save_fidelity;
