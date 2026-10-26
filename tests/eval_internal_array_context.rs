//! Collection demand retains internal shape independently of worksheet publication.
use poirot::eval::overlay::{EvaluationContext, EvaluationPolicy, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};

fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (cell, value) in [("A1", 2.), ("A2", 3.), ("B1", 5.), ("B2", 7.)] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    workbook
}

#[test]
fn internal_array_shape_survives_scalar_result_publication() {
    // Expected values are ordinary arithmetic over the stated members; the matching
    // JSON suite is independently recalculated by Calc, including scalar siblings.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        {
            let workbook = workbook();
            let policy = PreparationPolicy::default();
            let mut compiler = FormulaCompiler::new();
            let plan = compiler.compile_workbook(&workbook, &policy);
            let configuration = RunConfiguration::default().with_array_result_policy(arrays);
            for (formula, expected) in [
                ("SUMPRODUCT(A1:A2*B1:B2)", 31.),
                ("SUM({1,2;3,4}*{2})", 20.),
                ("SUMPRODUCT(A1:A2+{10;20})", 35.),
                ("SUMPRODUCT({10;20}+A1:A2)", 35.),
                ("SUM({1,2}+{10;20})", 66.),
                ("SUM({10;20}+{1,2})", 66.),
                ("SUMPRODUCT({2}*A1:B2)", 34.),
                ("SUMPRODUCT(A1:B2*{2})", 34.),
                ("SUMPRODUCT(--(A1:A2>2))", 1.),
                ("SUMPRODUCT(--((A1:A2&\"x\")=\"3x\"))", 1.),
                ("SUMPRODUCT(-A1:A2)", -5.),
                ("COLUMNS({1,2;3,4}/0)", 2.),
                ("ROWS({1,2;3,4}/0)", 2.),
                ("SUMPRODUCT(IF(TRUE,A1:A2*B1:B2,1/0))", 31.),
            ] {
                let prepared = compiler.compile_formula(formula, &policy);
                let bound = bound::FormulaPlan::bind(
                    &poirot_parsing::formula::parse_formula(formula).unwrap(),
                )
                .unwrap();
                let mut run = EvaluationRun::new_with_configuration(
                    WorkbookView::new(&workbook, &plan, &policy).unwrap(),
                    &configuration,
                    Default::default(),
                )
                .unwrap();
                assert_eq!(
                    run.evaluate_formula(target("F2"), &prepared).unwrap(),
                    CellValue::Number(expected),
                    "{arrays:?} {formula}"
                );
                assert_eq!(
                    run.evaluate_bound(target("F2"), &bound).unwrap(),
                    CellValue::Number(expected),
                    "bound {arrays:?} {formula}"
                );
            }
        }
    }
}

#[test]
fn scalar_arguments_and_dependency_cells_keep_their_own_demand() {
    let mut workbook = workbook();
    // The dependency is a worksheet formula, even when read by a collection caller.
    // In scalar mode C2 intersects A1:A2 at row two; the collection must not turn
    // that cached scalar into the first member of an internal array.
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet.set_formula(target("C2").address, "A1:A2*2").unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap();
    for (formula, expected) in [
        ("SUM(C2:C2*{1})", 6.),
        // Documented ABS broadcasts: 2*5 + 3*7 = 31, ABS({4;6}) = {4;6},
        // so addition gives {35;37}; scalar result publication takes its first member.
        // https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
        // Explicit @ selects row two independently of array-result publication.
        ("SUMPRODUCT(A1:A2*B1:B2)+ABS(A1:A2*2)", 35.),
        ("ABS(A1:A2*2)+SUMPRODUCT(A1:A2*B1:B2)", 35.),
        ("SUMPRODUCT(A1:A2*B1:B2)+ABS(@A1:A2*2)", 37.),
        ("ABS(@A1:A2*2)+SUMPRODUCT(A1:A2*B1:B2)", 37.),
        ("SUM(@A1:A2)", 3.),
    ] {
        let prepared = compiler.compile_formula(formula, &policy);
        assert_eq!(
            run.evaluate_formula(target("F2"), &prepared).unwrap(),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    assert_eq!(
        run.evaluate_cell(target("C2")).unwrap(),
        CellValue::Number(6.)
    );
}

#[test]
fn collection_reads_invalidate_interior_and_absent_members_in_both_publication_modes() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let mut workbook = workbook();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(target("F2").address, "SUMPRODUCT(A1:A3*B1:B3)")
            .unwrap();
        let mut overlay = Overlay::with_workbook(&workbook)
            .with_policy(EvaluationPolicy {
                arrays,
                ..Default::default()
            })
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("F2"))
                .unwrap(),
            CellValue::Number(31.)
        );
        for (cell, value, expected) in [("A2", 4., 38.), ("B3", 10., 38.), ("A3", 2., 58.)] {
            overlay.set_value(target(cell), value).unwrap();
            assert!(overlay.cached_value(target("F2")).is_none());
            assert_eq!(
                overlay
                    .evaluation(&context)
                    .unwrap()
                    .cell(target("F2"))
                    .unwrap(),
                CellValue::Number(expected)
            );
        }
        overlay.remove_cell(target("A2")).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("F2"))
                .unwrap(),
            CellValue::Number(30.)
        );
    }
}

/// Collects cell reads to verify rectangular output admission precedes reference traversal.
#[derive(Default)]
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

#[test]
fn array_output_is_admitted_before_reference_member_reads_and_limits_are_operational() {
    let workbook = workbook();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let formula = compiler.compile_formula("SUMPRODUCT(A1:B2*2)", &policy);
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        Default::default(),
        EvaluationBudgets {
            materialized_values: 3,
            ..Default::default()
        },
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert!(matches!(
        run.evaluate_formula(target("F2"), &formula),
        Err(EvaluationFailure::Budget(_))
    ));
    run.finish(Ok(()));
    assert!(
        !reads
            .0
            .iter()
            .any(|event| matches!(event.read, DependencyRead::Cell(_)))
    );
}

#[test]
fn per_member_errors_and_incompatible_shape_keep_the_selected_source_contract() {
    let workbook = workbook();
    for (formula, expected) in [
        ("SUM({1,2}/{1,0})", CellValue::Error(CellError::Div0)),
        // Pinned POI's exhausted non-singleton axis uses #VALUE!, as documented
        // by the existing array kernel; this does not claim Calc's error spelling.
        ("SUM({1,2}+{1,2,3})", CellValue::Error(CellError::Value)),
        ("COLUMNS({1,2}+{1,2,3})", CellValue::Number(3.)),
        ("COLUMNS({#N/A,#DIV/0!}*2)", CellValue::Number(2.)),
    ] {
        assert!(
            matches!(run_formula(&workbook, 0, target("F2").address, formula, Default::default(), Default::default()), EvaluationOutcome::Complete { value, .. } if value == expected),
            "{formula}"
        );
    }
}

#[test]
fn worksheet_publication_remains_independent_of_internal_literal_array_execution() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let mut workbook = workbook();
        let sheet = workbook.sheet_mut(0).unwrap();
        sheet
            .set_formula(target("F2").address, "SUMPRODUCT(A1:A2*B1:B2)")
            .unwrap();
        sheet
            .set_formula(target("H2").address, "{1,2}+{10;20}")
            .unwrap();
        let policy = PreparationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
        let configuration = RunConfiguration::default().with_array_result_policy(arrays);
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            &configuration,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            run.evaluate_cell(target("F2")).unwrap(),
            CellValue::Number(31.)
        );
        assert_eq!(
            run.evaluate_cell(target("H2")).unwrap(),
            CellValue::Number(11.)
        );
        assert_eq!(run.evaluate_cell(target("F3")).unwrap(), CellValue::Blank);
        assert_eq!(
            run.evaluate_cell(target("I2")).unwrap(),
            if arrays == ArrayResultPolicy::DynamicSpill {
                CellValue::Number(12.)
            } else {
                CellValue::Blank
            }
        );
        assert_eq!(
            run.evaluate_cell(target("H3")).unwrap(),
            if arrays == ArrayResultPolicy::DynamicSpill {
                CellValue::Number(21.)
            } else {
                CellValue::Blank
            }
        );
        assert_eq!(
            run.evaluate_cell(target("I3")).unwrap(),
            if arrays == ArrayResultPolicy::DynamicSpill {
                CellValue::Number(22.)
            } else {
                CellValue::Blank
            }
        );
    }
}

#[test]
fn explicit_intersection_of_nested_control_skips_other_rows() {
    // Explicit intersection selects B2 before arithmetic and ROUND, preserving
    // the reference's row without evaluating the unrelated B1 formula.
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(target("B1").address, "UNKNOWN_FUNCTION()")
        .unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let formula = compiler.compile_formula("SUMPRODUCT(A1:A2*ROUND(1.234,@B1:B2-5))", &policy);
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    // Row two selects B2=7, so ROUND(1.234, 2)=1.23 and (2+3)*1.23=6.15.
    assert_eq!(
        run.evaluate_formula(target("F2"), &formula).unwrap(),
        CellValue::Number(6.15)
    );
    run.finish(Ok(()));
    assert!(
        reads
            .0
            .iter()
            .any(|event| event.read == DependencyRead::Cell(target("B2")))
    );
    assert!(
        !reads
            .0
            .iter()
            .any(|event| event.read == DependencyRead::Cell(target("B1")))
    );
}
