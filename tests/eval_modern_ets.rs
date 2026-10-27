//! Public ETS contracts from primary documentation and independently exact models.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn call(b: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        b,
        0,
        target("Z100").address,
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn near(value: CellValue, expected: f64) {
    let CellValue::Number(actual) = value else {
        panic!("{value:?}")
    };
    assert!(
        (actual - expected).abs() < expected.abs().max(1.0) * 1e-8,
        "{actual} != {expected}"
    );
}
#[test]
fn perfect_lines_and_seasons_are_actual_fitted_forecasts() {
    let b = book();
    for (formula, expected) in [
        ("FORECAST.ETS(7,{3;5;7;9;11;13},{1;2;3;4;5;6},0)", 15.0),
        ("FORECAST.ETS(8.5,{3;5;7;9;11;13},{1;2;3;4;5;6},0)", 18.0),
        (
            "FORECAST.ETS(9,{8;12;8;12;8;12;8;12},{1;2;3;4;5;6;7;8},2)",
            8.0,
        ),
        (
            "FORECAST.ETS(10,{8;12;8;12;8;12;8;12},{1;2;3;4;5;6;7;8})",
            12.0,
        ),
        (
            "FORECAST.ETS.SEASONALITY({8;12;8;12;8;12;8;12},{1;2;3;4;5;6;7;8})",
            2.0,
        ),
        (
            "FORECAST.ETS.SEASONALITY({3;5;7;9;11;13},{1;2;3;4;5;6})",
            0.0,
        ),
        (
            "FORECAST.ETS.CONFINT(7,{3;5;7;9;11;13},{1;2;3;4;5;6},0.95,0)",
            0.0,
        ),
        ("FORECAST.ETS.STAT({3;5;7;9;11;13},{1;2;3;4;5;6},8,0)", 1.0),
        ("FORECAST.ETS(18,{2;5;14;17;20},{11;12;15;16;17},0)", 23.0),
        (
            "FORECAST.ETS(45413,{1;2;3;4},{45292;45323;45352;45383},0)",
            5.0,
        ),
    ] {
        near(call(&b, formula), expected);
    }
}
#[test]
fn documented_enum_duplicate_defaults_and_positional_failures() {
    let b = book();
    // Duplicate reductions produce perfect constant series, independently fixing forecasts.
    for (selector, expected) in [
        (1, 3.0),
        (2, 2.0),
        (3, 2.0),
        (4, 4.0),
        (5, 3.0),
        (6, 2.0),
        (7, 6.0),
    ] {
        near(
            call(
                &b,
                &format!("FORECAST.ETS(4,{{2;4;2;4;2;4}},{{1;1;2;2;3;3}},0,1,{selector})"),
            ),
            expected,
        );
    }
    near(
        call(&b, "FORECAST.ETS(4,{2;4;2;4;2;4},{1;1;2;2;3;3},0)"),
        3.0,
    );
    for (formula, error) in [
        ("FORECAST.ETS(4,{1;2;3},{1;2},0)", CellError::NA),
        ("FORECAST.ETS(4,{1;2;3},{1,2,3},0)", CellError::NA),
        ("FORECAST.ETS(4,{1;2;3},{1;2;3},0,1,0)", CellError::Num),
        ("FORECAST.ETS(4,{1;2;3},{1;2;3},2.5)", CellError::Num),
        ("FORECAST.ETS(4,{1;2;3},{1;2;3},8761)", CellError::Num),
        ("FORECAST.ETS(1,{1;2;3},{1;2;3},0)", CellError::Num),
        ("FORECAST.ETS(4,{1;\"x\";3},{1;2;3},0)", CellError::Value),
        ("FORECAST.ETS(4,{1;2;3},{1;2;3},0,2)", CellError::Num),
        ("FORECAST.ETS.STAT({1;2;3},{1;2;3},9,0)", CellError::Num),
        (
            "FORECAST.ETS.CONFINT(4,{1;2;3},{1;2;3},1,0)",
            CellError::Num,
        ),
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(error), "{formula}");
    }
}
#[test]
fn lifted_targets_and_options_spill_and_overlay_edits_invalidate() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value("A1", 7.0).unwrap();
    s.set_formula("D1", "FORECAST.ETS(A1:A2,{3;5;7;9;11;13},{1;2;3;4;5;6},0)")
        .unwrap();
    s.set_value("A2", 8.0).unwrap();
    s.set_formula(
        "G1",
        "FORECAST.ETS.STAT({3;5;7;9;11;13},{1;2;3;4;5;6},{6,7,8},0)",
    )
    .unwrap();
    let mut direct = Evaluation::with_workbook(b.clone())
        .build(EvaluationContext::default())
        .unwrap();
    near(direct.cell(target("D1")).unwrap(), 15.0);
    near(direct.cell(target("D2")).unwrap(), 17.0);
    near(direct.cell(target("G1")).unwrap(), 0.0);
    near(direct.cell(target("H1")).unwrap(), 0.0);
    near(direct.cell(target("I1")).unwrap(), 1.0);
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    near(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(target("D2"))
            .unwrap(),
        17.0,
    );
    overlay.set_value(target("A2"), 10.0).unwrap();
    near(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(target("D2"))
            .unwrap(),
        21.0,
    );
}
#[test]
fn lazy_branches_preserve_ets_behavior() {
    let b = book();
    near(
        call(&b, "IF(FALSE,FORECAST.ETS(4,{1;2;3},{1;2;3},0,2),9)"),
        9.0,
    );
}

#[test]
fn optimizer_work_remains_an_operational_failure_outside_iferror() {
    let b = book();
    assert!(matches!(
        run_formula(
            &b,
            0,
            target("Z100").address,
            "IFERROR(FORECAST.ETS(11,{10;11;13;12;15;16;15;19;20;18},{1;2;3;4;5;6;7;8;9;10},0),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 1000,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn too_many_missing_points_are_a_catchable_data_error_before_capacity() {
    let b = book();
    assert_eq!(
        call(&b, "FORECAST.ETS(101,{1;2;3},{1;2;100},0)"),
        CellValue::Error(CellError::Num)
    );
    near(
        call(&b, "IFERROR(FORECAST.ETS(101,{1;2;3},{1;2;100},0),42)"),
        42.0,
    );
}
