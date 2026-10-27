use poirot::eval;
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
use poirot_io as io;

#[test]
fn xlsx_imported_qualified_names_recalculate_selected_scope_in_caller_context() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.add_sheet("O'Brien").unwrap();
    workbook.define_name("Rate", "10", None).unwrap();
    workbook.define_name("Rate", "20", Some(0)).unwrap();
    workbook.define_name("Rate", "30", Some(1)).unwrap();
    workbook.define_name("CallerCell", "A1", Some(1)).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, 5.)
        .unwrap();
    workbook
        .sheet_mut(1)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, 9.)
        .unwrap();
    let cases = [
        ("'O''Brien'!Rate", CellValue::Number(30.)),
        ("'O''Brien'!CallerCell", CellValue::Number(5.)),
        ("'O''Brien'!MissingName", CellValue::Error(CellError::Name)),
        ("IF(FALSE,MissingSheet!Rate,7)", CellValue::Number(7.)),
    ];
    for (index, (formula, _)) in cases.iter().enumerate() {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula_with_cached_value(
                CellAddress {
                    row: index as u32,
                    col: 2,
                },
                *formula,
                CellValue::Number(-999.),
            )
            .unwrap();
    }
    let bytes = io::write_xlsx_to_buffer(&workbook).unwrap();
    let mut imported = io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(imported.named_ranges, workbook.named_ranges);
    match eval::run_recalculation(&mut imported, Default::default(), Default::default()) {
        eval::EvaluationOutcome::Complete { .. } => (),
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    for (index, (formula, expected)) in cases.iter().enumerate() {
        let cell = imported
            .sheet(0)
            .unwrap()
            .get(CellAddress {
                row: index as u32,
                col: 2,
            })
            .unwrap();
        assert_eq!(cell.formula.as_deref(), Some(*formula));
        assert_eq!(&cell.value, expected, "{formula}");
    }
}
