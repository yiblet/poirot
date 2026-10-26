use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    RunConfiguration, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn run(formula: &str, excel: bool, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, v) in [
        ("A1", CellValue::Number(1.)),
        ("A2", CellValue::Number(4.)),
        ("A3", CellValue::Number(4.)),
        ("A4", CellValue::Number(9.)),
        ("B1", CellValue::Boolean(false)),
        ("B2", CellValue::Boolean(true)),
        ("B3", CellValue::Text("4".into())),
        ("B4", CellValue::Error(CellError::NA)),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), v)
            .unwrap();
    }
    let c = RunConfiguration::default().with_builtin_policy(if excel {
        BuiltinSemanticsPolicy::ExcelDocumentation
    } else {
        BuiltinSemanticsPolicy::PinnedPoi
    });
    run_formula_with_configuration(&w, 0, CellAddress { row: 0, col: 25 }, formula, &c, budgets)
}
fn value(f: &str, excel: bool) -> CellValue {
    match run(f, excel, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{f}: {other:?}"),
    }
}
#[test]
fn address_relative_absolute_styles_sheet_escaping_and_errors() {
    for (args, expected) in [
        ("2,3,1,FALSE", "R2C3"),
        ("2,3,2,FALSE", "R2C[3]"),
        ("2,3,3,FALSE", "R[2]C3"),
        ("2,3,4,FALSE", "R[2]C[3]"),
        ("2,3,2,B1", "R2C[3]"),
        ("2,3,2,B2", "C$2"),
        ("2,3,2,", "C$2"),
        ("2,3,2", "C$2"),
        ("2,27,1,FALSE,\"O'Brien data\"", "'O''Brien data'!R2C27"),
        ("1048576,16384,1,TRUE", "$XFD$1048576"),
    ] {
        assert_eq!(
            value(&format!("ADDRESS({args})"), true),
            CellValue::Text(expected.into())
        );
    }
    for (args, e) in [
        ("2,3,1,B4", CellError::NA),
        ("2,3,1,\"bad\"", CellError::Value),
        ("0,3,1,FALSE", CellError::Value),
        ("2,3,5,FALSE", CellError::Value),
    ] {
        assert_eq!(
            value(&format!("ADDRESS({args})"), true),
            CellValue::Error(e)
        );
    }
    // Both policy labels honor a1 errors and FALSE's R1C1 style with abs_num=2.
    // https://support.microsoft.com/en-us/excel/functions/address-function
    assert_eq!(
        value("ADDRESS(2,3,2,B4)", false),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        value("ADDRESS(2,3,2,FALSE)", false),
        CellValue::Text("R2C[3]".into())
    );
}
#[test]
fn ceiling_nonzero_modes_significance_signs_and_fractional_modes() {
    for mode in ["1", "-1", "0.1", "-0.1", "2", "B2"] {
        for significance in ["2", "-2"] {
            let f = format!("CEILING.MATH(-4.3,{significance},{mode})");
            assert_eq!(value(&f, true), CellValue::Number(-6.));
        }
        assert_eq!(
            value(&format!("CEILING.MATH(4.3,2,{mode})"), true),
            CellValue::Number(6.)
        );
        assert_eq!(
            value(&format!("CEILING.MATH(-4,2,{mode})"), true),
            CellValue::Number(-4.)
        );
    }
    for excel in [true, false] {
        assert_eq!(
            value("CEILING.MATH(-4.3,2,0)", excel),
            CellValue::Number(-4.)
        );
        assert_eq!(
            value("CEILING.MATH(-4.3,0,B4)", excel),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value("CEILING.MATH(-4.3,2,B4)", excel),
            CellValue::Error(CellError::NA)
        );
    }
    assert_eq!(
        value("CEILING.MATH(-4.3,2,1)", false),
        CellValue::Number(-6.)
    );
    assert_eq!(
        value("CEILING.MATH(-4.3,2,-1)", false),
        CellValue::Number(-6.)
    );
}
#[test]
fn sample_covariance_pair_counts_filtering_and_errors() {
    // Sample covariance divides by n-1 and requires at least two paired observations.
    // https://support.microsoft.com/en-us/excel/functions/covariance-s-function
    for f in [
        "COVARIANCE.S(A1,A2)",
        "COVARIANCE.S(B1:B3,B1:B3)",
        "COVARIANCE.S(A1:B1,A2:B2)",
        "COVARIANCE.S(C1:C2,C1:C2)",
    ] {
        assert_eq!(value(f, true), CellValue::Error(CellError::Div0), "{f}");
    }
    assert_eq!(
        value("COVARIANCE.S(A1:A2,A1:A2)", true),
        CellValue::Number(4.5)
    );
    assert_eq!(
        value("COVARIANCE.S(A1:A2,A1:A3)", true),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        value("COVARIANCE.S(A1:B4,A1:B4)", true),
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn rank_reuses_equal_rank_nonzero_order_ties_absence_and_errors() {
    // Any nonzero order ranks ascending; 4 is tied at position two in {1,4,4,9}.
    // https://support.microsoft.com/en-us/excel/functions/rank-eq-function
    for mode in ["-1", "0.1", "-0.1", "2", "B2"] {
        assert_eq!(
            value(&format!("RANK(4,A1:A4,{mode})"), true),
            CellValue::Number(2.)
        );
        assert_eq!(
            value(&format!("RANK(4,A1:A4,{mode})"), true),
            value(&format!("RANK.EQ(4,A1:A4,{mode})"), true)
        );
    }
    assert_eq!(value("RANK(1,A1:A4)", true), CellValue::Number(4.));
    assert_eq!(
        value("RANK(7,A1:A4,1)", true),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        value("RANK(4,A1:A4,B4)", true),
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn roman_boolean_sources_and_numeric_styles_follow_documented_forms() {
    for f in ["ROMAN(499,FALSE)", "ROMAN(499,B1)", "ROMAN(499,4)"] {
        assert_eq!(value(f, true), CellValue::Text("ID".into()));
    }
    for f in ["ROMAN(499,TRUE)", "ROMAN(499,B2)", "ROMAN(499,0)"] {
        assert_eq!(value(f, true), CellValue::Text("CDXCIX".into()));
    }
    for (style, text) in [(1, "LDVLIV"), (2, "XDIX"), (3, "VDIV")] {
        assert_eq!(
            value(&format!("ROMAN(499,{style})"), true),
            CellValue::Text(text.into())
        );
    }
    assert_eq!(value("ROMAN(0,0)", true), CellValue::Text("".into()));
    assert_eq!(
        value("ROMAN(4000,0)", true),
        CellValue::Error(CellError::Value)
    );
    // Independently authored probes corroborated in LibreOffice 26.2.6.3;
    // native Excel error-priority behavior has not been independently verified.
    for formula in ["ROMAN(0,B4)", "ROMAN(499,B4)"] {
        assert_eq!(value(formula, true), CellValue::Error(CellError::NA));
    }
}
#[test]
fn operational_faults_survive_excel_profile_adapters() {
    for f in [
        "ADDRESS(2,3,2,FALSE)",
        "CEILING.MATH(-4.3,2,1)",
        "COVARIANCE.S(A1:A2,A1:A2)",
        "RANK(4,A1:A4,-1)",
        "ROMAN(499,FALSE)",
    ] {
        assert!(
            matches!(
                run(
                    &format!("IFERROR({f},7)"),
                    true,
                    EvaluationBudgets {
                        run_work: 1,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{f}"
        );
    }
}
