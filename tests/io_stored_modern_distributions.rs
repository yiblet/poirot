//! Probability function storage spellings survive actual XLSX serialization.
use poirot::model::{CellAddress, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
#[test]
fn stored_modern_distribution_names_roundtrip() {
    let mut b = Workbook::new();
    let sheet = b.add_sheet("S").unwrap();
    let names = [
        "BETA.DIST",
        "BINOM.DIST",
        "BINOM.DIST.RANGE",
        "CHISQ.DIST",
        "CHISQ.DIST.RT",
        "EXPON.DIST",
        "F.DIST",
        "F.DIST.RT",
        "GAMMA",
        "GAMMA.DIST",
        "GAMMALN",
        "GAMMALN.PRECISE",
        "GAUSS",
        "HYPGEOM.DIST",
        "LOGNORM.DIST",
        "NEGBINOM.DIST",
        "PHI",
        "WEIBULL.DIST",
    ];
    for (row, name) in names.iter().enumerate() {
        sheet
            .set_formula(
                CellAddress {
                    row: row as u32,
                    col: 0,
                },
                format!("{name}(1)"),
            )
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&b).unwrap();
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (row, name) in names.iter().enumerate() {
        let prefix = if *name == "GAMMALN" { "" } else { "_xlfn." };
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
            Some(format!("{prefix}{name}(1)").as_str())
        );
    }
}
