use poirot_eval::{
    EvaluationOutcome, RunConfiguration, TextSemanticsPolicy, TextSourceVersion,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellValue, Workbook};

#[test]
fn both_pinned_profiles_consume_numeric_text_optional_zero_policy() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for source in [
        TextSourceVersion::Poi7321209,
        TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    ] {
        let configuration =
            RunConfiguration::default().with_text_policy(TextSemanticsPolicy { source });
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
                EvaluationOutcome::Complete { value, .. } => assert_eq!(
                    value,
                    CellValue::Text(expected.into()),
                    "{source:?} {formula}"
                ),
                other => panic!("{other:?}"),
            }
        }
    }
}
