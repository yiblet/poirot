use super::*;
fn fixture() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w.add_sheet("Other").unwrap();
    for (address, value) in [
        ("A1", 1.),
        ("B1", 10.),
        ("C1", 3.),
        ("A2", 2.),
        ("B2", 20.),
        ("C2", 300.),
        ("A3", 3.),
        ("B3", 30.),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    w
}
fn formula(w: &Workbook, expression: &str) -> CellValue {
    let plan =
        bound::FormulaPlan::bind(&poirot_formula::parse_formula(expression).unwrap()).unwrap();
    match crate::run_plan(
        w,
        0,
        "P1".parse().unwrap(),
        &plan,
        Default::default(),
        Default::default(),
    ) {
        crate::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn retained_native66_and_logical_signed_shape_are_distinct_from_clipping() {
    let w = fixture();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(66.));
    for (expression, expected) in [
        ("ROWS(OFFSET(B3,0,0,-2,-2))", 4.),
        ("COLUMNS(OFFSET(B3,0,0,-2,-2))", 4.),
        ("ROW(OFFSET(B3,0,0,-2,-2))", 0.),
        ("COLUMN(OFFSET(B3,0,0,-2,-2))", -1.),
        ("COUNTBLANK(OFFSET(B3,0,0,-2,-2))", 10.),
        ("SUM(OFFSET(OFFSET(B3,0,0,-2,-2),2,2,2,2))", 55.),
        ("ROWS(OFFSET(OFFSET(B3,0,0,-2,-2),2,2))", 4.),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
}
#[test]
fn signed_index_selection_retains_blank_origin_and_reference_identity() {
    let w = fixture();
    for (expression, expected) in [
        ("SUM(INDEX(OFFSET(B3,0,0,-2,-2),0,3))", 6.),
        ("SUM(INDEX(OFFSET(B3,0,0,-2,-2),0,1))", 0.),
        ("ROW(INDEX(OFFSET(B3,0,0,-2,-2),1,1))", 0.),
        ("COLUMN(INDEX(OFFSET(B3,0,0,-2,-2),1,1))", -1.),
        ("SHEET(OFFSET(B3,0,0,-2,-2))", 1.),
        ("SUM(OFFSET(B3,0,0,-2,-2) A1:B2)", 33.),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
    assert_eq!(formula(&w, "INDEX(OFFSET(B3,0,0,-2,-2),1)"), Error(Ref));
    assert_eq!(formula(&w, "SUM(OFFSET(A1,0,0,-2,1))"), Error(Ref));
}
#[test]
fn dense_readers_include_out_of_grid_blanks_without_clipped_dimensions() {
    let w = fixture();
    // SUMPRODUCT requests a dense shape while MATCH reads a logical signed vector.
    assert_eq!(
        formula(&w, "SUMPRODUCT(OFFSET(B3,0,0,-2,1),{1;1;1;1})"),
        Number(60.)
    );
    assert_eq!(formula(&w, "MATCH(20,OFFSET(B3,0,0,-2,1),0)"), Number(3.));
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,1,-2))"), Number(33.));
}
#[test]
fn signed_sparse_projection_reads_current_cells_and_excludes_unrelated_regions() {
    let mut w = fixture();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(66.));
    w.sheet_mut(0)
        .unwrap()
        .set_value("A2".parse().unwrap(), 200.)
        .unwrap();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(264.));
    w.sheet_mut(0)
        .unwrap()
        .set_value("C2".parse().unwrap(), 9999.)
        .unwrap();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(264.));
}
#[test]
fn ordinary_reference_representation_stays_unsigned_and_signed_reads_are_lazy() {
    let w = fixture();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    let at = "P1".parse().unwrap();
    let plan = bound::FormulaPlan::bind(
        &poirot_formula::parse_formula("OFFSET(G5:H6,1,-1,-2,3)").unwrap(),
    )
    .unwrap();
    assert!(
        matches!(session.expr(0, at, &plan.expression()).unwrap(), Value::Refs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (2,5,5,7))
    );
    let plan =
        bound::FormulaPlan::bind(&poirot_formula::parse_formula("OFFSET(B3,0,0,-2,-2)").unwrap())
            .unwrap();
    assert!(
        matches!(session.expr(0, at, &plan.expression()).unwrap(), Value::SignedRefs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (-1,2,-2,1))
    );
}

fn formula_at(w: &Workbook, expression: &str, address: &str, sheet: usize) -> CellValue {
    let plan =
        bound::FormulaPlan::bind(&poirot_formula::parse_formula(expression).unwrap()).unwrap();
    match crate::run_plan(
        w,
        sheet,
        address.parse().unwrap(),
        &plan,
        Default::default(),
        Default::default(),
    ) {
        crate::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn signed_contained_rectangle_final_and_operator_scalars_follow_poi() {
    let w = fixture();
    assert_eq!(formula_at(&w, "OFFSET(B3,0,0,-2,-2)", "A2", 0), Number(2.));
    assert_eq!(
        formula_at(&w, "OFFSET(B3,0,0,-2,-2)+1", "A2", 0),
        Number(3.)
    );
    assert_eq!(
        formula_at(&w, "OFFSET(B3,0,0,-2,-2)", "C2", 0),
        Error(VALUE_ERROR)
    );
    // A logical selected reference wholly outside the grid remains a referenced blank.
    assert_eq!(formula(&w, "INDEX(OFFSET(B3,0,0,-2,-2),1,1)"), Number(0.));
}
#[test]
fn indirect_transports_signed_workbook_and_sheet_scoped_named_references() {
    let mut w = fixture();
    w.define_name("SignedGlobal", "OFFSET(Sheet1!B3,0,0,-2,-2)", None)
        .unwrap();
    w.define_name("SignedLocal", "OFFSET(B3,0,0,-2,-2)", Some(0))
        .unwrap();
    for name in ["SignedGlobal", "Sheet1!SignedLocal"] {
        for (consumer, expected) in [("SUM", 66.), ("ROWS", 4.), ("COUNTBLANK", 10.)] {
            assert_eq!(
                formula_at(&w, &format!("{consumer}(INDIRECT(\"{name}\"))"), "P1", 1),
                Number(expected)
            );
        }
        assert_eq!(
            formula(&w, &format!("COLUMN(INDEX(INDIRECT(\"{name}\"),1,1))")),
            Number(-1.)
        );
        assert_eq!(
            formula(&w, &format!("SUM(OFFSET(INDIRECT(\"{name}\"),2,2,2,2))")),
            Number(55.)
        );
    }
}
#[test]
fn signed_countblank_padding_classification_and_union_refusal() {
    let mut w = fixture();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), "")
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("B1".parse().unwrap(), "\"\"", Blank)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A2".parse().unwrap(), Blank)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("B2".parse().unwrap(), Error(Div0))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A3".parse().unwrap(), true)
        .unwrap();
    assert_eq!(formula(&w, "COUNTBLANK(OFFSET(B3,0,0,-2,-2))"), Number(13.));
    assert_eq!(
        formula(
            &w,
            "COUNTBLANK((OFFSET(B3,0,0,-2,-2),OFFSET(B3,0,0,-2,-2)))"
        ),
        Error(VALUE_ERROR)
    );
    assert_eq!(
        formula(&w, "COUNTBLANK(INDEX(OFFSET(B3,0,0,-2,-2),0,1))"),
        Number(4.)
    );
    assert_eq!(
        formula(&w, "COUNTBLANK((OFFSET(B3,0,0,-2,-2),A1:B1))"),
        Error(VALUE_ERROR)
    );
}
#[derive(Default)]
/// Collects reads to distinguish logical off-grid blanks from physical cell dependencies.
struct SignedReads(Vec<DependencyRead>);
impl DependencyReadSink for SignedReads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event.read);
    }
}
fn traced_formula(
    w: &Workbook,
    expression: &str,
    inputs: Option<&run_overlay::RunInputs>,
    budgets: EvaluationBudgets,
) -> (CellValue, Vec<DependencyRead>) {
    let mut reads = SignedReads::default();
    let mut session = Session::new(
        w,
        EvaluationInputs::default(),
        BudgetMeter::new(budgets).unwrap(),
    );
    session.inputs = inputs;
    session.dependency_sink = Some(&mut reads);
    let plan =
        bound::FormulaPlan::bind(&poirot_formula::parse_formula(expression).unwrap()).unwrap();
    let at = "P1".parse().unwrap();
    let value = session.expr(0, at, &plan.expression()).unwrap();
    let value = session.scalar(at, value).unwrap();
    drop(session);
    (value, reads.0)
}
#[test]
fn signed_sparse_dependencies_and_overlays_use_only_physical_intersection() {
    let mut w = fixture();
    w.sheet_mut(0).unwrap().remove("A2".parse().unwrap());
    let inputs = run_overlay::RunInputs::new(
        &w,
        [
            (0, "A2".parse().unwrap(), Number(200.)),
            (0, "C2".parse().unwrap(), Number(9999.)),
        ],
        2,
        std::sync::Arc::new(()),
    )
    .unwrap();
    let budgets = EvaluationBudgets {
        materialized_values: 8,
        ..EvaluationBudgets::default()
    };
    for (expression, expected) in [
        ("SUM(OFFSET(B3,0,0,-2,-2))", 264.),
        ("COUNTBLANK(OFFSET(B3,0,0,-2,-2))", 10.),
    ] {
        let (value, reads) = traced_formula(&w, expression, Some(&inputs), budgets);
        assert_eq!(value, Number(expected));
        let ranges: Vec<_> = reads
            .iter()
            .filter_map(|read| match read {
                DependencyRead::SparseMembership { sheet, first, last } => {
                    Some((*sheet, *first, *last))
                }
                _ => None,
            })
            .collect();
        assert_eq!(ranges, [(0, "A1".parse().unwrap(), "B3".parse().unwrap())]);
        let cells: Vec<_> = reads
            .iter()
            .filter_map(|read| match read {
                DependencyRead::Cell(target) => Some(target.address),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells,
            ["A1", "B1", "A2", "B2", "A3", "B3"].map(|a| a.parse().unwrap())
        );
    }
    let unrelated = run_overlay::RunInputs::new(
        &w,
        [(0, "C2".parse().unwrap(), Number(9999.))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    assert_eq!(
        traced_formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))", Some(&unrelated), budgets).0,
        Number(64.)
    );
    assert_eq!(
        traced_formula(
            &w,
            "COUNTBLANK(OFFSET(B3,0,0,-2,-2))",
            Some(&unrelated),
            budgets
        )
        .0,
        Number(11.)
    );
}
#[test]
fn signed_sparse_cached_membership_insert_remove_and_unrelated_edit() {
    use crate::overlay::{EvaluationContext, Overlay};
    let mut w = fixture();
    w.sheet_mut(0).unwrap().remove("A2".parse().unwrap());
    for (address, formula) in [
        ("P1", "SUM(OFFSET(B3,0,0,-2,-2))"),
        ("Q1", "COUNTBLANK(OFFSET(B3,0,0,-2,-2))"),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(address.parse().unwrap(), formula, Blank)
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let targets = ["Sheet1!P1", "Sheet1!Q1"].map(|name| overlay.target(name).unwrap());
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&targets).unwrap();
    // OFFSET is volatile: the canonical overlay refreshes it each evaluation.
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    let input = overlay.target("Sheet1!A2").unwrap();
    for (insert, expected) in [(true, [264., 10.]), (false, [64., 11.])] {
        if insert {
            overlay.set_value(input, 200.).unwrap();
        } else {
            overlay.remove_cell(input).unwrap();
        }
        let mut evaluation = overlay.evaluation(&context).unwrap();
        for (target, value) in targets.iter().zip(expected) {
            assert_eq!(evaluation.cell(*target).unwrap(), Number(value));
        }
        assert_eq!(evaluation.usage().formula_executions, 2);
    }
    let unrelated = overlay.target("Sheet1!C2").unwrap();
    overlay.set_value(unrelated, 9999.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&targets).unwrap();
    // OFFSET is volatile: the canonical overlay refreshes it each evaluation.
    assert_eq!(evaluation.usage().formula_executions, 2);
}
#[test]
fn unsigned_sparse_fastpath_preserves_layout_and_small_materialization_budget() {
    assert_eq!(
        std::mem::size_of::<Area>(),
        std::mem::size_of::<signed_reference::SignedArea>()
    );
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), 7.)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("B500000".parse().unwrap(), 9999.)
        .unwrap();
    let budgets = EvaluationBudgets {
        materialized_values: 2,
        ..EvaluationBudgets::default()
    };
    let (value, reads) = traced_formula(&w, "SUM(A1:A1048576)", None, budgets);
    assert_eq!(value, Number(7.));
    assert!(reads.contains(&DependencyRead::SparseMembership {
        sheet: 0,
        first: "A1".parse().unwrap(),
        last: "A1048576".parse().unwrap()
    }));
    let cells: Vec<_> = reads
        .into_iter()
        .filter_map(|read| match read {
            DependencyRead::Cell(target) => Some(target.address),
            _ => None,
        })
        .collect();
    assert_eq!(cells, ["A1".parse().unwrap()]);
}
#[test]
fn signed_host_padding_origin_shape_and_relative_resize_are_preserved() {
    use crate::host::{CallHost, OperandKind, OperandOrigin};
    let w = fixture();
    let arguments = [bound::bind(
        &poirot_formula::parse_formula("OFFSET(B3,0,0,-2,-2)").unwrap(),
    )];
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, "P1".parse().unwrap(), &arguments);
    let description = host.describe(0).unwrap();
    assert_eq!(
        (description.rows, description.columns, description.sheets),
        (4, 4, 1)
    );
    assert_eq!(description.kind, OperandKind::Reference);
    assert!(description.is_area);
    assert_eq!(host.operand_anchor(0).unwrap(), None);
    for (row, col, expected) in [
        (0, 3, Blank),
        (1, 0, Blank),
        (1, 2, Number(1.)),
        (3, 3, Number(30.)),
    ] {
        let scalar = host.read_cell(0, row, col).unwrap();
        assert_eq!(scalar.value, expected);
        assert_eq!(scalar.origin, OperandOrigin::ReferencedCell);
        assert!(!scalar.missing);
    }
    let scalar = host.read_relative_cell(0, 2, 4).unwrap();
    assert_eq!(scalar.value, Number(300.));
    assert_eq!(scalar.origin, OperandOrigin::ReferencedCell);
    assert_eq!(host.collection_len(0).unwrap(), 16);
    assert_eq!(host.collection_cell(0, 6).unwrap().value, Number(1.));
    assert_eq!(host.collection_cell(0, 15).unwrap().value, Number(30.));
}
#[test]
fn signed_logical_algebra_retains_padding_union_and_range_geometry() {
    let w = fixture();
    for (expression, expected) in [
        ("SUM(OFFSET(B3,0,0,-2,-2),A1:B2)", 99.),
        ("AREAS((OFFSET(B3,0,0,-2,-2),A1:B2))", 2.),
        (
            "COUNTBLANK(INDEX(OFFSET(B3,0,0,-2,-2),0,1) INDEX(OFFSET(B3,0,0,-2,-2),1,0))",
            1.,
        ),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2) C4:D5)"), Error(Null));
    // Current parser intentionally rejects function-valued range endpoints.
    // Exercise the evaluated Range operator directly, without widening that grammar.
    let at = "P1".parse().unwrap();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    let read = |session: &mut Session<'_>, expression: &str| {
        let expression = bound::bind(&poirot_formula::parse_formula(expression).unwrap());
        session.expr(0, at, &expression).unwrap()
    };
    let signed = read(&mut session, "OFFSET(B3,0,0,-2,-2)");
    let ordinary = read(&mut session, "C4");
    let range = session
        .references(BinaryOp::Range, signed, ordinary)
        .unwrap();
    assert!(matches!(range, Value::SignedRefs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (-1,3,-2,2)
        && (areas[0].height(), areas[0].width()) == (5,5)));
    let signed = read(&mut session, "OFFSET(B3,0,0,-2,-2)");
    let other = read(&mut session, "Other!A1");
    assert!(matches!(
        session.references(BinaryOp::Range, signed, other).unwrap(),
        Value::Scalar(Error(Ref))
    ));
}
#[test]
fn signed_sum_union_source_throw_is_operational_and_bypasses_masks() {
    let w = fixture();
    let at = "P1".parse().unwrap();
    for expression in [
        "SUM((OFFSET(B3,0,0,-2,-2),A1:B2))",
        "IFERROR(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
        "IFNA(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
        "ISERROR(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)))",
        "IF(TRUE,SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
    ] {
        let plan =
            bound::FormulaPlan::bind(&poirot_formula::parse_formula(expression).unwrap()).unwrap();
        assert!(
            matches!(run_plan(&w, 0, at, &plan, EvaluationInputs::default(), EvaluationBudgets::default()),
            EvaluationOutcome::Aborted { failure: EvaluationFailure::KnownSourceException {
                exception: KnownSourceException::AggregateReferenceListConversion,
                source: Some(EvaluationTarget { sheet: 0, address }),
            }, .. } if address == at),
            "{expression}"
        );
    }
    assert_eq!(
        formula(&w, "IF(FALSE,SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)"),
        Number(42.)
    );
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(66.));
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2),A1:B2)"), Number(99.));
    assert_eq!(
        formula(&w, "AREAS((OFFSET(B3,0,0,-2,-2),A1:B2))"),
        Number(2.)
    );
    assert_eq!(
        formula(&w, "SUM(1/0,(OFFSET(B3,0,0,-2,-2),A1:B2))"),
        Error(Div0)
    );
}
#[test]
fn signed_sum_union_throw_precedes_physical_area_reads() {
    let mut w = fixture();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A1".parse().unwrap(), "UNKNOWN_FUNCTION()", Blank)
        .unwrap();
    let mut reads = SignedReads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.dependency_sink = Some(&mut reads);
    let expression =
        bound::bind(&poirot_formula::parse_formula("SUM((OFFSET(B3,0,0,-2,-2),A1:B2))").unwrap());
    assert!(matches!(
        session.expr(0, "P1".parse().unwrap(), &expression),
        Err(EvaluationFailure::KnownSourceException {
            exception: KnownSourceException::AggregateReferenceListConversion,
            ..
        })
    ));
    drop(session);
    assert!(!reads.0.iter().any(|read| matches!(
        read,
        DependencyRead::Cell(_) | DependencyRead::SparseMembership { .. }
    )));
}
