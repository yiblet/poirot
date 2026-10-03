use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    RunConfiguration, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn run(f: &str, excel: bool, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (r, v) in [
        CellValue::Number(1.),
        CellValue::Number(4.),
        CellValue::Number(4.),
        CellValue::Boolean(true),
        CellValue::Text("9".into()),
        CellValue::Error(CellError::Ref),
    ]
    .into_iter()
    .enumerate()
    {
        w.sheet_mut(0)
            .unwrap()
            .set_value(
                CellAddress {
                    row: r as u32,
                    col: 0,
                },
                v,
            )
            .unwrap();
    }
    let c = RunConfiguration::default().with_builtin_policy(if excel {
        BuiltinSemanticsPolicy::ExcelDocumentation
    } else {
        BuiltinSemanticsPolicy::PinnedPoi
    });
    run_formula_with_configuration(&w, 0, CellAddress { row: 0, col: 25 }, f, &c, budgets)
}
fn value(f: &str, excel: bool) -> CellValue {
    match run(f, excel, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{f}: {other:?}"),
    }
}
#[test]
fn rank_arrays_preserve_ties_nonzero_order_and_ignore_nonnumeric_members() {
    for f in ["RANK.EQ", "RANK", "_xlfn.RANK.EQ"] {
        for (args, expected) in [
            ("4,{1,4,4,9}", 2.),
            ("4,{1;4;4;9},-0.1", 2.),
            ("9,{1,4;4,9},1", 4.),
            ("4,{1,TRUE,\"9\",4,4},1", 2.),
        ] {
            assert_eq!(
                value(&format!("{f}({args})"), true),
                CellValue::Number(expected)
            );
        }
        assert_eq!(
            value(&format!("{f}(4,{{1}},1)"), true),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value(&format!("{f}(4,A1:A6,1)"), true),
            CellValue::Number(2.)
        );
        assert_eq!(
            value(&format!("{f}(4,TRANSPOSE(A1:A6),1)"), true),
            CellValue::Number(2.)
        );
        // The existing Rank collection ignores error members as nonnumeric;
        // admission shares that source contract instead of adding coercion.
        assert_eq!(
            value(&format!("{f}(4,{{1,4,#REF!,4}},1)"), true),
            CellValue::Number(2.)
        );
        assert_eq!(
            value(&format!("{f}(4,{{TRUE,\"4\"}})"), true),
            CellValue::Error(CellError::NA)
        );
    }
}
#[test]
fn rank_array_admission_does_not_admit_scalar_or_missing_lists_or_mask_parameter_errors() {
    for f in ["RANK.EQ", "RANK"] {
        for args in ["4,4", "4,TRUE", "4,\"4\"", "4,", "4"] {
            assert_eq!(
                value(&format!("{f}({args})"), true),
                CellValue::Error(CellError::Value)
            );
        }
        assert_eq!(
            value(&format!("{f}(#DIV/0!,{{1,4}})"), true),
            CellValue::Error(CellError::Div0)
        );
        assert_eq!(
            value(&format!("{f}(4,{{1,4}},#REF!)"), true),
            CellValue::Error(CellError::Ref)
        );
    }
    assert_eq!(
        value("RANK(4,{1,4,4})", false),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(value("RANK(4,A1:A3)", false), CellValue::Number(1.));
    assert_eq!(
        value("RANK.EQ(4,{1,4,4})", false),
        CellValue::Error(CellError::Name)
    );
}
#[test]
fn rank_array_budgets_remain_operational_inside_iferror() {
    for f in ["RANK.EQ", "RANK"] {
        let formula = format!("IFERROR({f}(4,{{1,4,4,9}}),7)");
        for budgets in [
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            },
            EvaluationBudgets {
                materialized_values: 3,
                ..Default::default()
            },
        ] {
            assert!(
                matches!(
                    run(&formula, true, budgets),
                    EvaluationOutcome::Aborted {
                        failure: EvaluationFailure::Budget(_),
                        ..
                    }
                ),
                "{formula}"
            );
        }
    }
}
