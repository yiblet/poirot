//! Negative-size OFFSET expectations use checked A2:B3 normalization; native Excel
//! behavior is unverified. Direct SignedArea tests retain off-grid channel contracts.
use crate::eval::compile;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs, RunConfiguration};
use crate::eval::limits::{BudgetMeter, EvaluationBudgets, EvaluationError};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::{ArgumentReader, ItemFolder, ResultProjection, Session};
use crate::eval::values::area::Area;
use crate::eval::values::area_kind::AreaKind;
use crate::eval::values::reference_areas::ReferenceAreas;
use crate::eval::values::signed_reference;
use crate::eval::values::value::{VALUE_ERROR, Value};
use crate::eval::vm::{ExpressionRunner, ReferenceOperators};
use crate::model::CellError::{Div0, Null, Ref};
use crate::model::CellValue::{Blank, Boolean, Error, Number};
use crate::model::{CellValue, Workbook};
use poirot_parsing::formula::BinaryOp;

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
        w.sheet_mut(0).unwrap().set_value(address, value).unwrap();
    }
    w
}

fn formula(w: &Workbook, expression: &str) -> CellValue {
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(expression).unwrap(),
    )
    .unwrap();
    match crate::eval::entry::run_plan_with_configuration(
        w,
        0,
        "P1".parse().unwrap(),
        &plan,
        &RunConfiguration::default()
            .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
        Default::default(),
    ) {
        crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}

#[test]
fn normalized_negative_offset_has_checked_geometry() {
    let w = fixture();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(55.));
    for (expression, expected) in [
        ("ROWS(OFFSET(B3,0,0,-2,-2))", 2.),
        ("COLUMNS(OFFSET(B3,0,0,-2,-2))", 2.),
        ("ROW(OFFSET(B3,0,0,-2,-2))", 2.),
        ("COLUMN(OFFSET(B3,0,0,-2,-2))", 1.),
        ("COUNTBLANK(OFFSET(B3,0,0,-2,-2))", 0.),
        ("SUM(OFFSET(OFFSET(B3,0,0,-2,-2),2,2,2,2))", 0.),
        ("ROWS(OFFSET(OFFSET(B3,0,0,-2,-2),2,2))", 2.),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
}

#[test]
fn normalized_offset_index_retains_reference_identity() {
    let w = fixture();
    for (expression, expected) in [
        ("SUM(INDEX(OFFSET(B3,0,0,-2,-2),0,2))", 50.),
        ("SUM(INDEX(OFFSET(B3,0,0,-2,-2),0,1))", 5.),
        ("ROW(INDEX(OFFSET(B3,0,0,-2,-2),1,1))", 2.),
        ("COLUMN(INDEX(OFFSET(B3,0,0,-2,-2),1,1))", 1.),
        ("SHEET(OFFSET(B3,0,0,-2,-2))", 1.),
        ("SUM(OFFSET(B3,0,0,-2,-2) A1:B2)", 22.),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
    assert_eq!(formula(&w, "INDEX(OFFSET(B3,0,0,-2,-2),1)"), Error(Ref));
    assert_eq!(formula(&w, "SUM(OFFSET(A1,0,0,-2,1))"), Error(Ref));
}

#[test]
fn normalized_offset_dense_readers_use_checked_dimensions() {
    let w = fixture();
    // SUMPRODUCT and MATCH consume the normalized two-row B2:B3 reference.
    assert_eq!(
        formula(&w, "SUMPRODUCT(OFFSET(B3,0,0,-2,1),{1;1})"),
        Number(50.)
    );
    assert_eq!(formula(&w, "MATCH(20,OFFSET(B3,0,0,-2,1),0)"), Number(1.));
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,1,-2))"), Number(33.));
}

#[test]
fn normalized_offset_sparse_projection_tracks_selected_rectangle() {
    let mut w = fixture();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(55.));
    w.sheet_mut(0).unwrap().set_value("A2", 200.).unwrap();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(253.));
    w.sheet_mut(0).unwrap().set_value("C2", 9999.).unwrap();
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))"), Number(253.));
}

#[test]
fn normalized_offset_representation_stays_unsigned_and_reads_are_lazy() {
    let w = fixture();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    let at = "P1".parse().unwrap();
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula("OFFSET(G5:H6,1,-1,-2,3)").unwrap(),
    )
    .unwrap();
    assert!(
        matches!(ExpressionRunner::new(&mut session).expr(0, at, &plan.expression()).unwrap(), Value::Refs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (4,5,5,7))
    );
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula("OFFSET(B3,0,0,-2,-2)").unwrap(),
    )
    .unwrap();
    assert!(
        matches!(ExpressionRunner::new(&mut session).expr(0, at, &plan.expression()).unwrap(), Value::Refs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (1,2,0,1))
    );
}

fn formula_at(w: &Workbook, expression: &str, address: &str, sheet: usize) -> CellValue {
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(expression).unwrap(),
    )
    .unwrap();
    match crate::eval::entry::run_plan_with_configuration(
        w,
        sheet,
        address.parse().unwrap(),
        &plan,
        &RunConfiguration::default()
            .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
        Default::default(),
    ) {
        crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}

#[test]
fn normalized_offset_uses_ordinary_scalar_intersection() {
    let w = fixture();
    assert_eq!(
        formula_at(&w, "OFFSET(B3,0,0,-2,-2)", "A2", 0),
        Error(VALUE_ERROR)
    );
    assert_eq!(
        formula_at(&w, "OFFSET(B3,0,0,-2,-2)+1", "A2", 0),
        Error(VALUE_ERROR)
    );
    assert_eq!(
        formula_at(&w, "OFFSET(B3,0,0,-2,-2)", "C2", 0),
        Error(VALUE_ERROR)
    );
    // The normalized top-left is the ordinary referenced A2 value.
    assert_eq!(formula(&w, "INDEX(OFFSET(B3,0,0,-2,-2),1,1)"), Number(2.));
}

#[test]
fn indirect_transports_normalized_offset_named_references() {
    let mut w = fixture();
    w.define_name("SignedGlobal", "OFFSET(Sheet1!B3,0,0,-2,-2)", None)
        .unwrap();
    w.define_name("SignedLocal", "OFFSET(B3,0,0,-2,-2)", Some(0))
        .unwrap();
    for name in ["SignedGlobal", "Sheet1!SignedLocal"] {
        for (consumer, expected) in [("SUM", 55.), ("ROWS", 2.), ("COUNTBLANK", 0.)] {
            assert_eq!(
                formula_at(&w, &format!("{consumer}(INDIRECT(\"{name}\"))"), "P1", 1),
                Number(expected)
            );
        }
        assert_eq!(
            formula(&w, &format!("COLUMN(INDEX(INDIRECT(\"{name}\"),1,1))")),
            Number(1.)
        );
        assert_eq!(
            formula(&w, &format!("SUM(OFFSET(INDIRECT(\"{name}\"),2,2,2,2))")),
            Number(0.)
        );
    }
}

#[test]
fn normalized_offset_countblank_classification_and_union_counting() {
    let mut w = fixture();
    w.sheet_mut(0).unwrap().set_value("A1", "").unwrap();
    w.sheet_mut(0).unwrap().set_formula("B1", "\"\"").unwrap();
    w.sheet_mut(0).unwrap().set_value("A2", Blank).unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("B2", Error(Div0))
        .unwrap();
    w.sheet_mut(0).unwrap().set_value("A3", true).unwrap();
    assert_eq!(formula(&w, "COUNTBLANK(OFFSET(B3,0,0,-2,-2))"), Number(1.));
    assert_eq!(
        formula(
            &w,
            "COUNTBLANK((OFFSET(B3,0,0,-2,-2),OFFSET(B3,0,0,-2,-2)))"
        ),
        Number(2.)
    );
    assert_eq!(
        formula(&w, "COUNTBLANK(INDEX(OFFSET(B3,0,0,-2,-2),0,1))"),
        Number(1.)
    );
    assert_eq!(
        formula(&w, "COUNTBLANK((OFFSET(B3,0,0,-2,-2),A1:B1))"),
        Number(3.)
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
    inputs: Option<&crate::eval::inputs::RunInputs>,
    budgets: EvaluationBudgets,
) -> (CellValue, Vec<DependencyRead>) {
    let mut reads = SignedReads::default();
    let mut session = Session::new(
        w,
        EvaluationInputs::default(),
        BudgetMeter::new(budgets).unwrap(),
    );
    session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    session.sources.inputs = inputs;
    session.ports.dependency_sink = Some(&mut reads);
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(expression).unwrap(),
    )
    .unwrap();
    let at = "P1".parse().unwrap();
    let value = ExpressionRunner::new(&mut session)
        .expr(0, at, &plan.expression())
        .unwrap();
    let value = ResultProjection::new(&mut session)
        .select_legacy_scalar(at, value)
        .unwrap();
    drop(session);
    (value, reads.0)
}

#[test]
fn normalized_offset_sparse_dependencies_and_overlays_use_selected_rectangle() {
    let mut w = fixture();
    w.sheet_mut(0).unwrap().remove("A2").unwrap();
    let inputs = crate::eval::inputs::RunInputs::new(
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
        ("SUM(OFFSET(B3,0,0,-2,-2))", 253.),
        ("COUNTBLANK(OFFSET(B3,0,0,-2,-2))", 0.),
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
        assert_eq!(ranges, [(0, "A2".parse().unwrap(), "B3".parse().unwrap())]);
        let cells: Vec<_> = reads
            .iter()
            .filter_map(|read| match read {
                DependencyRead::Cell(target) => Some(target.address),
                _ => None,
            })
            .collect();
        assert_eq!(cells, ["A2", "B2", "A3", "B3"].map(|a| a.parse().unwrap()));
    }
    let unrelated = crate::eval::inputs::RunInputs::new(
        &w,
        [(0, "C2".parse().unwrap(), Number(9999.))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    assert_eq!(
        traced_formula(&w, "SUM(OFFSET(B3,0,0,-2,-2))", Some(&unrelated), budgets).0,
        Number(53.)
    );
    assert_eq!(
        traced_formula(
            &w,
            "COUNTBLANK(OFFSET(B3,0,0,-2,-2))",
            Some(&unrelated),
            budgets
        )
        .0,
        Number(1.)
    );
}

#[test]
fn normalized_offset_sparse_membership_insert_remove_and_unrelated_edit() {
    use crate::eval::config::EvaluationContext;
    use crate::eval::overlay::Overlay;
    let mut w = fixture();
    w.sheet_mut(0).unwrap().remove("A2").unwrap();
    for (address, formula) in [
        ("P1", "SUM(OFFSET(B3,0,0,-2,-2))"),
        ("Q1", "COUNTBLANK(OFFSET(B3,0,0,-2,-2))"),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(address, formula)
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(crate::eval::config::EvaluationPolicy {
            arrays: ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
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
    for (insert, expected) in [(true, [253., 0.]), (false, [53., 1.])] {
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
    w.sheet_mut(0).unwrap().set_value("A1", 7.).unwrap();
    w.sheet_mut(0).unwrap().set_value("B500000", 9999.).unwrap();
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
fn direct_signed_padding_sparse_dense_origin_and_relative_resize_are_preserved() {
    use signed_reference::{GridRead, SignedArea};
    let w = fixture();
    let area = SignedArea::new(AreaKind::Area, 0, (-1, 2), (-2, 1)).unwrap();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    let matrix = ArgumentReader::new(&mut session)
        .matrix(Value::SignedRefs(vec![area]))
        .unwrap()
        .unwrap();
    assert_eq!((matrix.len(), matrix[0].len()), (4, 4));
    assert_eq!(matrix.iter().flatten().filter(|v| **v == Blank).count(), 10);
    assert_eq!(matrix[1][2], Number(1.));
    assert_eq!(matrix[3][3], Number(30.));
    assert_eq!(
        area.relative(2, 4),
        Some(GridRead::Cell {
            sheet: 0,
            address: "C2".parse().unwrap()
        })
    );
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar("A2".parse().unwrap(), Value::SignedRefs(vec![area]))
            .unwrap(),
        Number(2.)
    );
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar(
                "P1".parse().unwrap(),
                Value::SignedRefs(vec![area.select(1, 1)])
            )
            .unwrap(),
        Blank
    );
    let members = ItemFolder::new(&mut session)
        .fold_items(
            crate::eval::host::HostOperand::from_value(Value::SignedRefs(vec![area])),
            Vec::new(),
            |members, count| members.try_reserve_exact(count),
            |members, scalar| {
                members.push(scalar);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(members.len(), 6);
    assert!(members.iter().all(|member| member.origin
        == crate::eval::values::operand_origin::OperandOrigin::ReferencedCell
        && !member.missing));
}

#[test]
fn normalized_offset_algebra_retains_union_and_range_geometry() {
    let w = fixture();
    for (expression, expected) in [
        ("SUM(OFFSET(B3,0,0,-2,-2),A1:B2)", 88.),
        ("AREAS((OFFSET(B3,0,0,-2,-2),A1:B2))", 2.),
        (
            "COUNTBLANK(INDEX(OFFSET(B3,0,0,-2,-2),0,1) INDEX(OFFSET(B3,0,0,-2,-2),1,0))",
            0.,
        ),
    ] {
        assert_eq!(formula(&w, expression), Number(expected), "{expression}");
    }
    assert_eq!(formula(&w, "SUM(OFFSET(B3,0,0,-2,-2) C4:D5)"), Error(Null));
    // Current parser intentionally rejects function-valued range endpoints.
    // Exercise the evaluated Range operator directly, without widening that grammar.
    let at = "P1".parse().unwrap();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    let read = |session: &mut Session<'_>, expression: &str| {
        let expression =
            compile::bound::bind(&poirot_parsing::formula::parse_formula(expression).unwrap());
        ExpressionRunner::new(session)
            .expr(0, at, &expression)
            .unwrap()
    };
    let signed = read(&mut session, "OFFSET(B3,0,0,-2,-2)");
    let ordinary = read(&mut session, "C4");
    let range = ReferenceOperators::new(&mut session)
        .references(BinaryOp::Range, signed, ordinary)
        .unwrap();
    assert!(matches!(range, Value::Refs(areas)
        if areas.len() == 1 && (areas[0].r0, areas[0].r1, areas[0].c0, areas[0].c1) == (1,3,0,2)));
    let signed = read(&mut session, "OFFSET(B3,0,0,-2,-2)");
    let other = read(&mut session, "Other!A1");
    assert!(matches!(
        ReferenceOperators::new(&mut session)
            .references(BinaryOp::Range, signed, other)
            .unwrap(),
        Value::Scalar(Error(Ref))
    ));
}

#[test]
fn normalized_offset_union_consumes_ordinary_references() {
    let w = fixture();
    for expression in [
        "SUM((OFFSET(B3,0,0,-2,-2),A1:B2))",
        "IFERROR(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
        "IFNA(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
        "IF(TRUE,SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)",
    ] {
        assert_eq!(formula(&w, expression), Number(88.), "{expression}");
    }
    assert_eq!(
        formula(&w, "ISERROR(SUM((OFFSET(B3,0,0,-2,-2),A1:B2)))"),
        Boolean(false)
    );
    assert_eq!(
        formula(&w, "IF(FALSE,SUM((OFFSET(B3,0,0,-2,-2),A1:B2)),42)"),
        Number(42.)
    );
    assert_eq!(
        formula(&w, "SUM(1/0,(OFFSET(B3,0,0,-2,-2),A1:B2))"),
        Error(Div0)
    );
}

#[test]
fn direct_signed_range_algebra_retains_padding_and_sheet_guards() {
    let w = fixture();
    let source = || {
        Value::SignedRefs(vec![
            signed_reference::SignedArea::new(AreaKind::Area, 0, (-1, 2), (-2, 1)).unwrap(),
        ])
    };
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    let endpoint = |sheet| {
        Value::Refs(ReferenceAreas::One(Area {
            kind: AreaKind::Cell,
            sheet,
            r0: 3,
            r1: 3,
            c0: 2,
            c1: 2,
        }))
    };
    assert!(
        matches!(ReferenceOperators::new(&mut session).references(BinaryOp::Range, source(), endpoint(0)).unwrap(), Value::SignedRefs(areas) if areas.len()==1 && (areas[0].r0,areas[0].r1,areas[0].c0,areas[0].c1)==(-1,3,-2,2))
    );
    assert!(matches!(
        ReferenceOperators::new(&mut session)
            .references(BinaryOp::Range, source(), endpoint(1))
            .unwrap(),
        Value::Scalar(Error(Ref))
    ));
}

#[test]
fn range_envelope_reads_only_consumed_rectangle_in_row_major_order() {
    let w = fixture();
    for source in [
        "SUM((A1,C3):B2)",
        "SUM(B2:(A1,C3))",
        "SUM((OFFSET(B3,0,0,-2,-2),A1):C3)",
    ] {
        let (value, reads) = traced_formula(&w, source, None, EvaluationBudgets::default());
        assert_eq!(value, Number(369.), "{source}");
        let ranges: Vec<_> = reads
            .iter()
            .filter_map(|read| match read {
                DependencyRead::SparseMembership { sheet, first, last } => {
                    Some((*sheet, *first, *last))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            ranges,
            [(0, "A1".parse().unwrap(), "C3".parse().unwrap())],
            "{source}"
        );
        let cells: Vec<_> = reads
            .iter()
            .filter_map(|read| match read {
                DependencyRead::Cell(target) => Some(target.address),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells,
            ["A1", "B1", "C1", "A2", "B2", "C2", "A3", "B3"].map(|a| a.parse().unwrap()),
            "{source}"
        );
    }
    for source in [
        "ROWS((A1,C3):B2)",
        "ROWS((OFFSET(B3,0,0,-2,-2),A1):C3)",
        "ROWS((IF(TRUE,1/0,A1),C3):B2)",
    ] {
        let (_, reads) = traced_formula(&w, source, None, EvaluationBudgets::default());
        assert!(
            !reads.iter().any(|read| matches!(
                read,
                DependencyRead::Cell(_) | DependencyRead::SparseMembership { .. }
            )),
            "{source}: {reads:?}"
        );
    }
}

#[test]
fn range_envelopes_reject_empty_and_mixed_sheet_operands_in_both_channels() {
    let w = fixture();
    let a = Area {
        kind: AreaKind::Cell,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    let b = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 2,
        r1: 4,
        c0: 1,
        c1: 3,
    };
    for signed in [false, true] {
        let wrap = |areas: Vec<Area>| {
            if signed {
                Value::SignedRefs(
                    areas
                        .into_iter()
                        .map(signed_reference::SignedArea::from_unsigned)
                        .collect(),
                )
            } else {
                Value::Refs(areas.into())
            }
        };
        let mut session = Session::with_context(&w, EvaluationInputs::default());
        session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
        for (left, right) in [
            (vec![], vec![a]),
            (vec![a], vec![]),
            (vec![a, Area { sheet: 1, ..b }], vec![b]),
            (vec![a], vec![b, Area { sheet: 1, ..a }]),
        ] {
            assert!(matches!(
                ReferenceOperators::new(&mut session)
                    .references(BinaryOp::Range, wrap(left), wrap(right))
                    .unwrap(),
                Value::Scalar(Error(Ref))
            ));
        }
        let result = ReferenceOperators::new(&mut session)
            .references(BinaryOp::Range, wrap(vec![a, b]), wrap(vec![a]))
            .unwrap();
        if signed {
            assert!(
                matches!(result,Value::SignedRefs(v) if v.len()==1 && (v[0].r0,v[0].r1,v[0].c0,v[0].c1)==(0,4,0,3))
            );
        } else {
            assert!(
                matches!(result,Value::Refs(v) if v.len()==1 && (v[0].r0,v[0].r1,v[0].c0,v[0].c1)==(0,4,0,3))
            );
        }
    }
}

#[test]
fn additional_envelope_members_consume_work_without_reading_cells() {
    let w = fixture();
    let a = Area {
        kind: AreaKind::Cell,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    for signed in [false, true] {
        let wrap = |areas: Vec<Area>| {
            if signed {
                Value::SignedRefs(
                    areas
                        .into_iter()
                        .map(signed_reference::SignedArea::from_unsigned)
                        .collect(),
                )
            } else {
                Value::Refs(areas.into())
            }
        };
        for extra in [false, true] {
            let mut session = Session::new(
                &w,
                EvaluationInputs::default(),
                BudgetMeter::new(EvaluationBudgets {
                    run_work: 1,
                    cell_work: 1,
                    ..Default::default()
                })
                .unwrap(),
            );
            let left = if extra { vec![a, a] } else { vec![a] };
            let result = ReferenceOperators::new(&mut session).references(
                BinaryOp::Range,
                wrap(left),
                wrap(vec![a]),
            );
            if extra {
                assert!(matches!(result, Err(EvaluationError::Budget(_))));
            } else {
                assert!(result.is_ok());
            }
        }
    }
}
