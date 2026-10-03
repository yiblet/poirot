use poirot_model::{CellValue, Workbook};

#[test]
fn dynamic_ranges_survive_xls_write_import_and_recalculation() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Data").unwrap();
    for (address, formula) in [
        ("H1", "ROWS(OFFSET(B3,0,0,-2,-2):C4)"),
        ("H2", "COLUMNS(OFFSET(B3,0,0,-2,-2):C4)"),
        ("H3", "ROWS((A1:B3) (B2:C4))"),
        ("H4", "ROWS(IF(TRUE,A1,B1):C4)"),
    ] {
        sheet
            .set_formula(address.parse().unwrap(), formula, CellValue::Number(-999.))
            .unwrap();
    }
    workbook.define_name("Start", "A1", None).unwrap();
    workbook.define_name("End", "B3", None).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "H5".parse().unwrap(),
            "ROWS(Start:End)",
            CellValue::Number(-999.),
        )
        .unwrap();
    let bytes = poirot_xls::write_xls_to_buffer(&workbook).unwrap();
    let mut imported = poirot_io::read_xls_bytes(&bytes).unwrap().workbook;
    for address in ["H1", "H2", "H3", "H4", "H5"] {
        let cell = imported
            .sheet(0)
            .unwrap()
            .get(address.parse().unwrap())
            .unwrap();
        assert!(cell.formula.is_some(), "{address}");
        assert_eq!(cell.value, CellValue::Number(-999.));
    }
    match poirot_eval::run_recalculation(&mut imported, Default::default(), Default::default()) {
        poirot_eval::EvaluationOutcome::Complete { .. } => (),
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    for (address, expected) in [("H1", 5.), ("H2", 5.), ("H3", 2.), ("H4", 4.), ("H5", 3.)] {
        assert_eq!(
            imported
                .sheet(0)
                .unwrap()
                .get(address.parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(expected),
            "{address}"
        );
    }
}
