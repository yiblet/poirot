use poirot::{CellAddress, CellError, CellValue, Workbook};

#[test]
fn explicit_typing_resolves_lookup_fallback_branch_sum_and_max_without_global_coercion() {
    let mut source = Workbook::new();
    let sheet = source.add_sheet("Input").unwrap();
    sheet.set_value("A1".parse().unwrap(), 93514.0).unwrap();
    sheet.set_value("B1".parse().unwrap(), 2.0).unwrap();
    for (address, value) in [
        ("F1", 0.0),
        ("G1", 1.0),
        ("F2", 700000.0),
        ("G2", 2.0),
        ("J1", 2.0),
        ("K1", 3.0),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    let inputs = [
        ("D28", "93514"),
        ("D43", "0"),
        ("D73", "2015"),
        ("D6", "719800"),
    ];
    let mut legacy = source.clone();
    let mut numeric = source;
    for (address, text) in inputs {
        legacy
            .sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), text)
            .unwrap();
        numeric
            .sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), text.parse::<f64>().unwrap())
            .unwrap();
    }
    let at: CellAddress = "Z1".parse().unwrap();
    for (formula, before, after) in [
        (
            "VLOOKUP(D28,A1:B1,2,FALSE)",
            CellValue::Error(CellError::NA),
            CellValue::Number(2.0),
        ),
        (
            "IFERROR(VLOOKUP(D28,A1:B1,2,FALSE),-999999)",
            CellValue::Number(-999999.0),
            CellValue::Number(2.0),
        ),
        (
            "IF(D43=0,\"Included\",\"Other\")",
            CellValue::Text("Other".into()),
            CellValue::Text("Included".into()),
        ),
        (
            "SUM(D6,17500)",
            CellValue::Number(17500.0),
            CellValue::Number(737300.0),
        ),
        (
            "VLOOKUP(D6,F1:G2,2,TRUE)",
            CellValue::Error(CellError::NA),
            CellValue::Number(2.0),
        ),
        (
            "VLOOKUP(IFERROR(VLOOKUP(D28,A1:B1,2,FALSE),\"\"),J1:K1,2,FALSE)",
            CellValue::Error(CellError::NA),
            CellValue::Number(3.0),
        ),
        (
            "MAX(D73)",
            CellValue::Number(0.0),
            CellValue::Number(2015.0),
        ),
    ] {
        assert_eq!(
            match poirot::eval::run_formula(
                legacy.model(),
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            before,
            "legacy {formula}"
        );
        assert_eq!(
            match poirot::eval::run_formula(
                numeric.model(),
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            after,
            "declared {formula}"
        );
    }
}
