use poirot::eval::{EvaluationOutcome, RunConfiguration, run_formula_with_configuration};
use poirot::model::{CellAddress, CellValue, Workbook};

#[test]
fn numeric_text_optional_zero_arguments_follow_documented_rules() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    {
        let configuration = RunConfiguration::default();
        for (formula, expected) in [
            (r####"TEXT(0,"$###,###")"####, "$0"),
            (r####"TEXT(0,"###")"####, "0"),
            (r####"TEXT(-0,"$###,###")"####, "$0"),
            (r####"TEXT(0.4,"$###,###")"####, "$0"),
            (r####"TEXT(-0.5,"$###,###")"####, "-$1"),
            (r####"TEXT("0","$###,###")"####, "$0"),
            (r####"TEXT(-0.4,"$###,###")"####, "-$0"),
            (r####"TEXT(0,"#.##")"####, "0"),
            (r####"TEXT(0.5,"$###,###")"####, "$1"),
            (r####"TEXT(0,"$#,##0")"####, "$0"),
            (r####"TEXT(0,".###")"####, ".0"),
            (r####"TEXT(0.4,".###")"####, ".4"),
            (r####"TEXT(0,"###.")"####, "0."),
            (r####"TEXT(0.4,"#.##")"####, "0.4"),
            (r####"TEXT(0.004,"###%")"####, "0%"),
            (r####"TEXT(400,"###,")"####, "0"),
            (r####"TEXT(0,"# ?/?")"####, "0"),
            (r####"TEXT(1,"# ?/?")"####, "1"),
            (r####"TEXT(0.5,"# ?/?")"####, "1/2"),
            (r####"TEXT(1.25,"#.0;(#.00)")"####, "1.3"),
            (r####"TEXT(-1.25,"#.0;(#.00)")"####, "(1.3)"),
            (r####"TEXT(0,"$###,###;-$###,###;""ZERO""")"####, "ZERO"),
        ] {
            match run_formula_with_configuration(
                &workbook,
                0,
                CellAddress { row: 0, col: 0 },
                formula,
                &configuration,
                Default::default(),
            ) {
                EvaluationOutcome::Complete { value, .. } => {
                    assert_eq!(value, CellValue::Text(expected.into()), "{formula}")
                }
                other => panic!("{other:?}"),
            }
        }
    }
}
