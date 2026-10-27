//! All inverse/confidence names retain their actual future-function XLSX spelling.
use poirot::model::{CellAddress, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
#[test]
fn stored_modern_inverse_names_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let formulas = [
        "BETA.INV(0.5,1,1)",
        "BINOM.INV(4,0.5,0.5)",
        "CHISQ.INV(0.5,2)",
        "CHISQ.INV.RT(0.5,2)",
        "CONFIDENCE.NORM(0.05,1,1)",
        "CONFIDENCE.T(0.5,2,2)",
        "F.INV(0.5,2,2)",
        "F.INV.RT(0.5,2,2)",
        "GAMMA.INV(0.5,1,2)",
        "LOGNORM.INV(0.5,0,1)",
        "T.INV(0.75,1)",
        "T.INV.2T(0.5,1)",
    ];
    for (row, formula) in formulas.iter().enumerate() {
        sheet
            .set_formula(
                CellAddress {
                    row: row as u32,
                    col: 0,
                },
                *formula,
            )
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (row, formula) in formulas.iter().enumerate() {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(CellAddress {
                    row: row as u32,
                    col: 0
                })
                .unwrap()
                .unwrap()
                .formula
                .as_deref(),
            Some(format!("_xlfn.{formula}").as_str())
        );
    }
}
