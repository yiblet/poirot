use model::{CellError, CellValue};
use poirot::eval;
use poirot::model;
fn target(address: &str) -> eval::EvaluationTarget {
    eval::EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn policy() -> poirot::eval::overlay::EvaluationPolicy {
    poirot::eval::overlay::EvaluationPolicy {
        builtin: eval::BuiltinSemanticsPolicy::ExcelDocumentation,
        arrays: eval::ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    }
}
fn model(formulas: &[(&str, &str)]) -> model::Workbook {
    let mut w = model::Workbook::new();
    w.add_sheet("S").unwrap();
    for &(at, source) in formulas {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(target(at).address, source)
            .unwrap();
    }
    w
}
fn source(model: model::Workbook) -> Result<model::Workbook, std::convert::Infallible> {
    Ok(model)
}
/// Public overlay harness that retains the latest evaluation's observable values.
struct SpillCase<'a> {
    overlay: poirot::eval::overlay::Overlay<'a>,
    context: poirot::calculation::EvaluationContext,
    results: std::collections::BTreeMap<(usize, model::CellAddress), CellValue>,
}
fn context(w: &model::Workbook) -> SpillCase<'_> {
    SpillCase {
        overlay: poirot::eval::overlay::Overlay::with_workbook(w)
            .with_policy(policy())
            .build()
            .unwrap(),
        context: Default::default(),
        results: Default::default(),
    }
}
impl SpillCase<'_> {
    fn run(
        &mut self,
        targets: &[eval::EvaluationTarget],
    ) -> Result<eval::RunReport<usize>, poirot::calculation::OverlayError> {
        self.results.clear();
        let mut evaluation = self.overlay.evaluation(&self.context)?;
        let outputs = evaluation.outputs(targets);
        self.results.extend(
            evaluation
                .values()
                .map(|(at, value)| ((at.sheet, at.address), value.clone())),
        );
        if let Ok(outputs) = &outputs {
            self.results.extend(
                outputs
                    .iter()
                    .map(|r| ((r.target.sheet, r.target.address), r.value.clone())),
            );
        }
        let report = evaluation.finish();
        Ok(eval::RunReport {
            execution: report.execution,
            outcome: match report.outcome {
                eval::EvaluationOutcome::Complete { usage, .. } => {
                    eval::EvaluationOutcome::Complete {
                        value: targets.len(),
                        usage,
                    }
                }
                eval::EvaluationOutcome::Aborted { failure, usage } => {
                    eval::EvaluationOutcome::Aborted { failure, usage }
                }
            },
        })
    }
    fn get(&self, at: eval::EvaluationTarget) -> Option<&CellValue> {
        self.results.get(&(at.sheet, at.address))
    }
    fn set_input(
        &mut self,
        at: eval::EvaluationTarget,
        value: CellValue,
    ) -> Result<(), model::ModelError> {
        self.results.clear();
        self.overlay.set_value(at, value)
    }
    fn set_policy(
        &mut self,
        policy: poirot::eval::overlay::EvaluationPolicy,
    ) -> Result<(), poirot::calculation::OverlayError> {
        self.results.clear();
        self.overlay.set_policy(policy)
    }
    fn set_budgets(&mut self, budgets: eval::EvaluationBudgets) {
        self.context.limits = budgets.into();
    }
    fn set_retention_limits(&mut self, limits: eval::run::MemoRetentionLimits) {
        self.context.retention = limits;
    }
    fn reset(&mut self) {
        self.overlay.clear_cache();
        self.results.clear();
    }
}
fn complete(ctx: &mut SpillCase<'_>, cells: &[&str]) {
    let targets = cells.iter().map(|cell| target(cell)).collect::<Vec<_>>();
    let report = ctx.run(&targets).unwrap();
    assert!(
        matches!(report.outcome, eval::EvaluationOutcome::Complete { .. }),
        "{:?}",
        report.outcome
    );
}
fn value(ctx: &SpillCase<'_>, at: &str) -> CellValue {
    ctx.get(target(at)).unwrap().clone()
}
#[test]
fn child_before_anchor_preserves_typed_outputs_and_selected_dependencies() {
    let mut m = model(&[
        ("D1", "TRANSPOSE(A1:C2)"),
        ("H1", "D3"),
        ("A3", "TRANSPOSE(RAND())"),
        ("B3", "TRANSPOSE(1/0)"),
    ]);
    let values = [
        (
            "A1",
            CellValue::TextUtf16(model::ExcelText::from_units(vec![0xd83d])),
        ),
        ("B1", CellValue::Boolean(true)),
        ("C1", CellValue::Error(CellError::NA)),
        ("A2", CellValue::Number(3.)),
        ("B2", CellValue::Text("tail".into())),
    ];
    for (at, value) in values {
        m.sheet_mut(0)
            .unwrap()
            .set_value(target(at).address, value)
            .unwrap();
    }
    let w = source(m).unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["E3", "H1"]);
    assert_eq!(
        value(&ctx, "D1"),
        CellValue::TextUtf16(model::ExcelText::from_units(vec![0xd83d]))
    );
    assert_eq!(value(&ctx, "D2"), CellValue::Boolean(true));
    assert_eq!(value(&ctx, "D3"), CellValue::Error(CellError::NA));
    assert_eq!(value(&ctx, "E3"), CellValue::Number(0.));
    assert_eq!(value(&ctx, "H1"), CellValue::Error(CellError::NA));
    assert!(ctx.get(target("A3")).is_none());
    assert!(ctx.get(target("B3")).is_none());
}
#[test]
fn blocked_spills_preserve_literals_formulas_and_overlay_inputs() {
    for literal in [CellValue::Number(9.), CellValue::Text("".into())] {
        let mut m = model(&[("D1", "{1,2;3,4}")]);
        m.sheet_mut(0)
            .unwrap()
            .set_value(target("E2").address, literal.clone())
            .unwrap();
        let w = source(m).unwrap();
        let mut ctx = context(&w);
        complete(&mut ctx, &["D1", "E2"]);
        assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::Spill));
        assert_eq!(value(&ctx, "E2"), literal);
        assert!(ctx.get(target("D2")).is_none());
    }
    let w = source(model(&[("D1", "{1,2}"), ("E1", "\"\"")])).unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::Spill));
    assert_eq!(value(&ctx, "E1"), CellValue::Text("".into()));
    let w = source(model(&[("D1", "{1,2}")])).unwrap();
    let mut ctx = context(&w);
    ctx.set_input(target("E1"), CellValue::Number(19.)).unwrap();
    complete(&mut ctx, &["D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::Spill));
    assert_eq!(value(&ctx, "E1"), CellValue::Number(19.));
    ctx.set_input(target("D1"), CellValue::Number(8.)).unwrap();
    complete(&mut ctx, &["E1", "D1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(8.));
}
#[test]
fn overlap_and_grid_edges_publish_only_valid_footprints() {
    // Crossing rectangles, neither anchor occupies the other footprint.
    let w = source(model(&[("D2", "{1,2,3}"), ("E1", "{8;9;10}")])).unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["D2", "E1", "F2"]);
    assert_eq!(value(&ctx, "D2"), CellValue::Error(CellError::Spill));
    assert_eq!(value(&ctx, "E1"), CellValue::Number(8.));
    assert_eq!(value(&ctx, "F2"), CellValue::Blank);
    for (at, formula) in [("XFD1", "{1,2}"), ("A1048576", "{1;2}")] {
        let w = source(model(&[(at, formula)])).unwrap();
        let mut ctx = context(&w);
        complete(&mut ctx, &[at]);
        assert_eq!(value(&ctx, at), CellValue::Error(CellError::Spill));
        assert_eq!(ctx.results.len(), 1);
    }
}
#[test]
fn edits_shrink_and_policy_changes_clear_old_children() {
    let w = source(model(&[("D1", "IF(A1,{1,2;3,4},{8})")])).unwrap();
    let mut ctx = context(&w);
    ctx.set_input(target("A1"), CellValue::Boolean(true))
        .unwrap();
    complete(&mut ctx, &["E2"]);
    assert_eq!(value(&ctx, "E2"), CellValue::Number(4.));
    ctx.set_input(target("A1"), CellValue::Boolean(false))
        .unwrap();
    complete(&mut ctx, &["E2", "D1"]);
    assert_eq!(value(&ctx, "E2"), CellValue::Blank);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(8.));
    ctx.set_input(target("A1"), CellValue::Boolean(true))
        .unwrap();
    ctx.set_policy(poirot::eval::overlay::EvaluationPolicy {
        arrays: eval::ArrayResultPolicy::ScalarIntersection,
        ..Default::default()
    })
    .unwrap();
    complete(&mut ctx, &["D1", "E2"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(1.));
    assert_eq!(value(&ctx, "E2"), CellValue::Blank);
    ctx.reset();
    let w = source(model(&[("D1", "{7}")])).unwrap();
    ctx = context(&w);
    complete(&mut ctx, &["D1", "E2"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(7.));
    assert_eq!(value(&ctx, "E2"), CellValue::Blank);
}
#[test]
fn self_footprint_reads_are_explicitly_unsupported() {
    for formula in [
        "TRANSPOSE(D2:E2)",
        "TRANSPOSE(D1:E1)",
        "D2:D3+1",
        "IF(COUNT(D2:E2),{1;2},{3;4})",
    ] {
        let w = source(model(&[("D1", formula)])).unwrap();
        let mut ctx = context(&w);
        let report = ctx.run(&[target("D1")]).unwrap();
        assert!(
            matches!(
                report.outcome,
                eval::EvaluationOutcome::Aborted {
                    failure: eval::EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}: {:?}",
            report.outcome
        );
        assert!(ctx.get(target("D1")).is_none());
    }
    let w = source(model(&[("D1", "@{1,2}")])).unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(1.));
    assert_eq!(value(&ctx, "E1"), CellValue::Blank);
}
#[test]
fn retention_admits_the_whole_spill_or_none() {
    let w = source(model(&[("D1", "{1,2;3,4}")])).unwrap();
    for limits in [
        eval::run::MemoRetentionLimits {
            cells: 3,
            bytes: 1_000_000,
        },
        eval::run::MemoRetentionLimits {
            cells: 100,
            bytes: 1,
        },
    ] {
        let mut ctx = context(&w);
        ctx.set_retention_limits(limits);
        let report = ctx.run(&[target("D1")]).unwrap();
        assert!(matches!(
            report.outcome,
            eval::EvaluationOutcome::Aborted {
                failure: eval::EvaluationFailure::MemoRetentionLimit { .. },
                ..
            }
        ));
        assert!(ctx.results.is_empty());
    }
    let mut ctx = context(&w);
    ctx.set_retention_limits(eval::run::MemoRetentionLimits {
        cells: 4,
        bytes: 1_000_000,
    });
    complete(&mut ctx, &["D1"]);
    assert_eq!(ctx.results.len(), 4);
}
#[test]
fn work_and_materialization_abort_before_any_spill_publication() {
    let w = source(model(&[("D1", "{1,2;3,4}")])).unwrap();
    let mut good = context(&w);
    let report = good.run(&[target("D1")]).unwrap();
    let work = match report.outcome {
        eval::EvaluationOutcome::Complete { usage, .. } => usage.work_units,
        _ => panic!(),
    };
    for budgets in [
        eval::EvaluationBudgets {
            run_work: work - 1,
            ..Default::default()
        },
        eval::EvaluationBudgets {
            materialized_values: 3,
            ..Default::default()
        },
    ] {
        let mut ctx = context(&w);
        ctx.set_budgets(budgets);
        let report = ctx.run(&[target("D1")]).unwrap();
        assert!(matches!(
            report.outcome,
            eval::EvaluationOutcome::Aborted {
                failure: eval::EvaluationFailure::Budget(_),
                ..
            }
        ));
        assert!(ctx.results.is_empty());
    }
}

#[test]
fn sparse_range_consumers_discover_and_read_full_or_partial_spills() {
    let w = source(model(&[
        ("D1", "{1,2;3,4}"),
        ("H1", "SUM(D1:E2)"),
        ("H2", "SUM(E1:E2)"),
        ("H3", "COUNT(D1:E2)"),
        ("H4", "COUNTA(D1:E2)"),
    ]))
    .unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["H2", "H1", "H3", "H4"]);
    assert_eq!(value(&ctx, "H1"), CellValue::Number(10.));
    assert_eq!(value(&ctx, "H2"), CellValue::Number(6.));
    assert_eq!(value(&ctx, "H3"), CellValue::Number(4.));
    assert_eq!(value(&ctx, "H4"), CellValue::Number(4.));
}

#[test]
fn numeric_matrix_children_are_discoverable_before_the_anchor() {
    for (formula, child, expected) in [
        ("MMULT({1,2;3,4},{1,0;0,1})", "E2", 4.),
        ("MINVERSE({1,0;0,2})", "E2", 0.5),
        ("FREQUENCY({1,2,3,6},{2,5})", "D3", 1.),
    ] {
        let w = source(model(&[("D1", formula)])).unwrap();
        let mut ctx = context(&w);
        complete(&mut ctx, &[child]);
        assert_eq!(value(&ctx, child), CellValue::Number(expected));
        assert!(ctx.get(target("D1")).is_some());
    }
}
#[test]
fn dynamically_sized_producers_publish_in_both_read_orders_and_preserve_lazy_branches() {
    for (formula, child, expected) in [
        ("TRANSPOSE(INDIRECT(\"A1:A2\"))", "E1", 0.0),
        ("INDEX(A1:B2,0,1)+1", "D2", 1.0),
        ("-INDEX(A1:B2,0,1)", "D2", 0.0),
    ] {
        let w = source(model(&[("D1", formula)])).unwrap();
        for first in ["D1", child] {
            let mut ctx = context(&w);
            complete(&mut ctx, &[first, child]);
            assert_eq!(value(&ctx, child), CellValue::Number(expected));
        }
    }
    // Dynamic dimensions still respect allocation budgets before publication.
    let w = source(model(&[("D1", "A:A+1")])).unwrap();
    let mut ctx = context(&w);
    ctx.set_budgets(eval::EvaluationBudgets {
        materialized_values: 64,
        ..Default::default()
    });
    let report = ctx.run(&[target("D2")]).unwrap();
    assert!(matches!(
        report.outcome,
        eval::EvaluationOutcome::Aborted {
            failure: eval::EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert!(ctx.get(target("D1")).is_none());
    assert!(ctx.get(target("D2")).is_none());
    let w = source(model(&[(
        "D1",
        "IF(TRUE,{1,2},TRANSPOSE(INDIRECT(\"A1:A2\")))",
    )]))
    .unwrap();
    let mut ctx = context(&w);
    complete(&mut ctx, &["E1"]);
    assert_eq!(value(&ctx, "E1"), CellValue::Number(2.));
}

#[test]
fn transitive_reads_of_the_unpublished_footprint_refuse() {
    for dependency in ["E1", "COUNT(E1:E2)"] {
        let w = source(model(&[
            ("D1", "TRANSPOSE(H1:H2)"),
            ("H1", dependency),
            ("H2", "1"),
        ]))
        .unwrap();
        let mut ctx = context(&w);
        let report = ctx.run(&[target("D1")]).unwrap();
        assert!(matches!(
            report.outcome,
            eval::EvaluationOutcome::Aborted {
                failure: eval::EvaluationFailure::UnsupportedCapability { .. },
                ..
            }
        ));
        assert!(ctx.get(target("D1")).is_none());
    }
}

#[test]
fn crossed_spills_are_identical_for_all_anchor_and_child_target_permutations() {
    fn permutations<'a>(cells: &mut [&'a str], index: usize, output: &mut Vec<Vec<&'a str>>) {
        if index == cells.len() {
            output.push(cells.to_vec());
            return;
        }
        for next in index..cells.len() {
            cells.swap(index, next);
            permutations(cells, index + 1, output);
            cells.swap(index, next);
        }
    }
    let mut orders = Vec::new();
    permutations(&mut ["D2", "E1", "E2", "E3", "F2"], 0, &mut orders);
    assert_eq!(orders.len(), 120);
    for (earlier, blocked, overlay) in [
        ("IF(TRUE,{8;9;10},1/0)", false, false),
        ("IF(FALSE,{8;9;10},{8})", false, false),
        ("{8;9;10}", true, false),
        ("{8;9;10}", false, true),
    ] {
        let mut m = model(&[
            ("D2", "IF(FALSE,1/0,{1,2,3})"),
            ("E1", earlier),
            ("A1", "TRANSPOSE(RAND())"),
            ("B1", "TRANSPOSE(1/0)"),
        ]);
        if blocked {
            m.sheet_mut(0)
                .unwrap()
                .set_value(target("E3").address, 77.)
                .unwrap();
        }
        let w = source(m).unwrap();
        for order in &orders {
            let mut ctx = context(&w);
            if overlay {
                ctx.set_input(target("E1"), CellValue::Number(88.)).unwrap();
            }
            complete(&mut ctx, order);
            let earlier_wins = !blocked && !overlay && earlier != "IF(FALSE,{8;9;10},{8})";
            assert_eq!(
                value(&ctx, "D2"),
                if earlier_wins {
                    CellValue::Error(CellError::Spill)
                } else {
                    CellValue::Number(1.)
                },
                "{order:?}"
            );
            assert_eq!(
                value(&ctx, "E1"),
                if blocked {
                    CellValue::Error(CellError::Spill)
                } else {
                    CellValue::Number(if overlay { 88. } else { 8. })
                },
                "{order:?}"
            );
            assert_eq!(
                value(&ctx, "E2"),
                CellValue::Number(if earlier_wins { 9. } else { 2. }),
                "{order:?}"
            );
            assert_eq!(
                value(&ctx, "E3"),
                if earlier_wins {
                    CellValue::Number(10.)
                } else if blocked {
                    CellValue::Number(77.)
                } else {
                    CellValue::Blank
                },
                "{order:?}"
            );
            assert_eq!(
                value(&ctx, "F2"),
                if earlier_wins {
                    CellValue::Blank
                } else {
                    CellValue::Number(3.)
                },
                "{order:?}"
            );
            assert!(ctx.get(target("A1")).is_none());
            assert!(ctx.get(target("B1")).is_none());
        }
    }
}

#[test]
fn priority_ownership_with_a_direct_self_footprint_refuses_a_supplier_first_answer() {
    // E1 has priority. Once D2 loses, E1's operand includes its own E2 child;
    // this remains an explicit circular-reference refusal, not supplier priority.
    let w = source(model(&[("E1", "TRANSPOSE(D2:F2)"), ("D2", "{1,2,3}")])).unwrap();
    for first in ["E1", "D2", "E2"] {
        let mut ctx = context(&w);
        let report = ctx.run(&[target(first)]).unwrap();
        assert!(matches!(
            report.outcome,
            eval::EvaluationOutcome::Aborted {
                failure: eval::EvaluationFailure::UnsupportedCapability { .. },
                ..
            }
        ));
        assert!(ctx.results.is_empty());
    }
}

#[test]
fn independent_overlap_preparation_does_not_consume_recursive_depth() {
    // Staircase rectangles intersect their predecessor, with no anchor inside
    // another footprint. Iterative canonical preparation does not consume a
    // recursive frame for each independent candidate.
    let mut m = model(&[]);
    for index in 0..20u32 {
        let at = model::CellAddress {
            row: index,
            col: 40 - index,
        };
        m.sheet_mut(0)
            .unwrap()
            .set_formula(at, "{1,2;3,4}")
            .unwrap();
    }
    let w = source(m).unwrap();
    let mut ctx = context(&w);
    ctx.set_budgets(eval::EvaluationBudgets {
        depth: 5,
        ..Default::default()
    });
    let report = ctx
        .run(&[eval::EvaluationTarget {
            sheet: 0,
            address: model::CellAddress { row: 19, col: 21 },
        }])
        .unwrap();
    assert!(
        matches!(report.outcome, eval::EvaluationOutcome::Complete { .. }),
        "{:?}",
        report.outcome
    );
    assert!(report.outcome.usage().maximum_depth <= 5);
}

#[test]
fn array_operators_broadcast_each_axis_and_preserve_scalar_semantics() {
    for (formula, expected) in [
        ("{1,2;3,4}+10", vec![11., 12., 13., 14.]),
        ("10-{1,2;3,4}", vec![9., 8., 7., 6.]),
        ("{1,2}+{10;20}", vec![11., 12., 21., 22.]),
        ("{1,2;3,4}*{10,20}", vec![10., 40., 30., 80.]),
        ("{1,2;3,4}/{1;2}", vec![1., 2., 1.5, 2.]),
        ("{1,2;3,4}^{1,2;2,1}", vec![1., 4., 9., 4.]),
        ("-+{1,2;3,4}", vec![-1., -2., -3., -4.]),
        ("{100,200;300,400}%", vec![1., 2., 3., 4.]),
        ("TRANSPOSE({1,2;3,4})+1", vec![2., 4., 3., 5.]),
    ] {
        let w = model(&[("D1", formula)]);
        let mut ctx = context(&w);
        complete(&mut ctx, &["E2", "D1", "E1", "D2"]);
        let actual: Vec<_> = ["D1", "E1", "D2", "E2"].map(|at| value(&ctx, at)).into();
        assert_eq!(
            actual,
            expected
                .into_iter()
                .map(CellValue::Number)
                .collect::<Vec<_>>(),
            "{formula}"
        );
    }
    for (formula, expected) in [
        (
            "{1,2}={1,3}",
            vec![CellValue::Boolean(true), CellValue::Boolean(false)],
        ),
        (
            "{1,2}<2",
            vec![CellValue::Boolean(true), CellValue::Boolean(false)],
        ),
        (
            "{1,2}<>2",
            vec![CellValue::Boolean(true), CellValue::Boolean(false)],
        ),
        (
            "{1,2}<=2",
            vec![CellValue::Boolean(true), CellValue::Boolean(true)],
        ),
        (
            "{1,2}>1",
            vec![CellValue::Boolean(false), CellValue::Boolean(true)],
        ),
        (
            "{1,2}>=1",
            vec![CellValue::Boolean(true), CellValue::Boolean(true)],
        ),
        (
            "\"x\"&{1,2}",
            vec![CellValue::Text("x1".into()), CellValue::Text("x2".into())],
        ),
        (
            "{1,2}&\"x\"",
            vec![CellValue::Text("1x".into()), CellValue::Text("2x".into())],
        ),
        (
            "{#N/A,2}+{#DIV/0!,3}",
            vec![CellValue::Error(CellError::NA), CellValue::Number(5.)],
        ),
        (
            "{1,2}/{0,2}",
            vec![CellValue::Error(CellError::Div0), CellValue::Number(1.)],
        ),
    ] {
        let w = model(&[("D1", formula)]);
        let mut ctx = context(&w);
        complete(&mut ctx, &["E1", "D1"]);
        assert_eq!(
            vec![value(&ctx, "D1"), value(&ctx, "E1")],
            expected,
            "{formula}"
        );
    }
}

#[test]
fn exhausted_non_singleton_axes_use_poi_value_errors_and_at_stays_scalar() {
    let w = model(&[("D1", "{1,2}+{10,20,30}")]);
    let mut ctx = context(&w);
    complete(&mut ctx, &["F1", "D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(11.));
    assert_eq!(value(&ctx, "E1"), CellValue::Number(22.));
    assert_eq!(value(&ctx, "F1"), CellValue::Error(CellError::Value));
    for (formula, scalar) in [("@({1,2}+1)", 2.), ("(@{1,2})+1", 2.)] {
        let w = model(&[("D1", formula)]);
        let mut ctx = context(&w);
        complete(&mut ctx, &["E1", "D1"]);
        assert_eq!(value(&ctx, "D1"), CellValue::Number(scalar));
        assert_eq!(value(&ctx, "E1"), CellValue::Blank);
    }
    let w = model(&[("D1", "{1,2}+1")]);
    let mut ctx = context(&w);
    ctx.set_policy(poirot::eval::overlay::EvaluationPolicy {
        arrays: eval::ArrayResultPolicy::ScalarIntersection,
        ..Default::default()
    })
    .unwrap();
    complete(&mut ctx, &["D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(2.));
    assert_eq!(value(&ctx, "E1"), CellValue::Blank);
}

#[test]
fn operator_inputs_blockers_and_transitive_consumers_invalidate_across_evaluations() {
    let w = model(&[("D1", "A1:B2+10"), ("H1", "SUM(E1:E2)"), ("H2", "H1*2")]);
    let mut ctx = context(&w);
    complete(&mut ctx, &["H2"]);
    assert_eq!(value(&ctx, "H2"), CellValue::Number(40.));
    ctx.set_input(target("B2"), CellValue::Number(7.)).unwrap();
    complete(&mut ctx, &["H2"]);
    assert_eq!(value(&ctx, "H2"), CellValue::Number(54.));
    ctx.set_input(target("E2"), CellValue::Number(99.)).unwrap();
    complete(&mut ctx, &["D1", "H2"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::Spill));
    assert_eq!(value(&ctx, "H2"), CellValue::Number(198.));
    ctx.overlay.remove_cell(target("E2")).unwrap();
    complete(&mut ctx, &["H2"]);
    assert_eq!(value(&ctx, "H2"), CellValue::Number(54.));
    ctx.overlay.set_formula(target("D1"), "{1}+10").unwrap();
    complete(&mut ctx, &["H2", "E2", "D1"]);
    assert_eq!(value(&ctx, "H2"), CellValue::Number(0.));
    assert_eq!(value(&ctx, "E2"), CellValue::Blank);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(11.));
}

#[test]
fn operator_snapshot_reuses_bytecode_and_retains_blank_input_dependencies() {
    let w = model(&[("D1", "A1:B2*2"), ("H1", "E2+1")]);
    let mut ctx = context(&w);
    complete(&mut ctx, &["H1"]);
    let bytes = postcard::to_allocvec(&ctx.overlay.snapshot()).unwrap();
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    let (restored_workbook, restored_state) =
        poirot::eval::overlay::Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    let config = poirot::calculation::EvaluationContext::default();
    assert_eq!(
        restored
            .evaluation(&config)
            .unwrap()
            .cell(target("H1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
    restored.set_value(target("B2"), 9.).unwrap();
    assert!(restored.cached_value(target("H1")).is_none());
    assert_eq!(
        restored
            .evaluation(&config)
            .unwrap()
            .cell(target("H1"))
            .unwrap(),
        CellValue::Number(19.)
    );
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
}

#[test]
fn operator_budget_failures_are_atomic_and_cannot_be_masked_by_iferror() {
    let w = model(&[("D1", "IFERROR({1,2}+{10;20},99)")]);
    let mut good = context(&w);
    let report = good.run(&[target("D1")]).unwrap();
    let work = match report.outcome {
        eval::EvaluationOutcome::Complete { usage, .. } => usage.work_units,
        outcome => panic!("{outcome:?}"),
    };
    for budgets in [
        eval::EvaluationBudgets {
            run_work: work - 1,
            ..Default::default()
        },
        eval::EvaluationBudgets {
            materialized_values: 3,
            ..Default::default()
        },
    ] {
        let mut ctx = context(&w);
        ctx.set_budgets(budgets);
        let report = ctx.run(&[target("D1")]).unwrap();
        assert!(
            matches!(
                report.outcome,
                eval::EvaluationOutcome::Aborted {
                    failure: eval::EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{:?}",
            report.outcome
        );
        assert!(ctx.results.is_empty());
        assert!(ctx.overlay.cached_value(target("D1")).is_none());
        assert!(ctx.overlay.cached_value(target("E2")).is_none());
    }
}

#[test]
fn bounded_conditional_operator_shapes_shrink_and_scalar_calls_do_not_claim_children() {
    let w = model(&[("D1", "IF(A3,A1:B2,8)+1")]);
    let mut ctx = context(&w);
    ctx.set_input(target("A3"), CellValue::Boolean(true))
        .unwrap();
    complete(&mut ctx, &["E2"]);
    assert_eq!(value(&ctx, "E2"), CellValue::Number(1.));
    ctx.set_input(target("A3"), CellValue::Boolean(false))
        .unwrap();
    complete(&mut ctx, &["E2", "D1"]);
    assert_eq!(value(&ctx, "E2"), CellValue::Blank);
    assert_eq!(value(&ctx, "D1"), CellValue::Number(9.));
    for formula in [
        "IF(TRUE,1,2)+1",
        "-AND(TRUE,FALSE)",
        "ROW(A1)+1",
        "SUMPRODUCT({1,2},{3,4})+1",
    ] {
        let w = model(&[("D1", formula)]);
        let mut ctx = context(&w);
        complete(&mut ctx, &["E1", "D1"]);
        assert_eq!(value(&ctx, "E1"), CellValue::Blank, "{formula}");
    }
}

#[test]
fn captured_random_operator_producers_reuse_and_refresh_when_seed_changes() {
    let w = model(&[("D1", "{1,2}+RAND()"), ("H1", "E1+1"), ("H2", "21")]);
    let mut overlay = poirot::eval::overlay::Overlay::with_workbook(&w)
        .with_policy(policy())
        .build()
        .unwrap();
    let mut config = poirot::calculation::EvaluationContext::default();
    for iteration in 0..3 {
        if iteration == 2 {
            config.randomness_seed = config.randomness_seed.wrapping_add(1);
        }
        let mut evaluation = overlay.evaluation(&config).unwrap();
        evaluation.outputs(&[target("H1"), target("H2")]).unwrap();
        assert_eq!(
            evaluation.usage().formula_executions,
            match iteration {
                0 => 3,
                1 => 0,
                _ => 2,
            }
        );
        if iteration == 1 {
            assert!(evaluation.usage().external_cache_hits > 0);
        }
    }
}

#[test]
fn error_elements_still_track_the_corresponding_right_input() {
    let w = model(&[("D1", "{#N/A,2}+A1:B1")]);
    let mut ctx = context(&w);
    complete(&mut ctx, &["E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::NA));
    assert!(ctx.overlay.cached_value(target("D1")).is_some());
    ctx.set_input(target("A1"), CellValue::Number(7.)).unwrap();
    assert!(ctx.overlay.cached_value(target("D1")).is_none());
    assert!(ctx.overlay.cached_value(target("E1")).is_none());
    complete(&mut ctx, &["D1", "E1"]);
    assert_eq!(value(&ctx, "D1"), CellValue::Error(CellError::NA));
    assert_eq!(value(&ctx, "E1"), CellValue::Number(2.));
}

#[test]
fn retained_operator_results_match_fresh_evaluation_through_interleaved_edits() {
    let w = model(&[("D1", "A1:B2+10"), ("H1", "SUM(E1:E2)"), ("H2", "H1+E2")]);
    let mut oracle = w.clone();
    let mut retained = context(&w);
    for step in 0..32 {
        match step % 8 {
            0 | 1 => {
                let at = if step % 8 == 0 { "B2" } else { "A1" };
                let value = CellValue::Number(step as f64);
                retained.set_input(target(at), value.clone()).unwrap();
                oracle
                    .sheet_mut(0)
                    .unwrap()
                    .set_value(target(at).address, value)
                    .unwrap();
            }
            2 => {
                retained
                    .set_input(target("E2"), CellValue::Number(77.))
                    .unwrap();
                oracle
                    .sheet_mut(0)
                    .unwrap()
                    .set_value(target("E2").address, 77.)
                    .unwrap();
            }
            3 | 7 => {
                let at = if step % 8 == 3 { "E2" } else { "B2" };
                retained.overlay.remove_cell(target(at)).unwrap();
                oracle.sheet_mut(0).unwrap().remove(target(at).address);
            }
            4..=6 => {
                let formula = match step % 8 {
                    4 => "{8}+1",
                    5 => "A1:B2*{1,2}",
                    _ => "IF(TRUE,A1:B2,UNKNOWN())+{10;20}",
                };
                retained.overlay.set_formula(target("D1"), formula).unwrap();
                oracle
                    .sheet_mut(0)
                    .unwrap()
                    .set_formula(target("D1").address, formula)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let mut fresh = context(&oracle);
        let outputs = ["H2", "E2", "D1", "E1", "D2"];
        complete(&mut retained, &outputs);
        complete(&mut fresh, &outputs);
        for at in outputs {
            assert_eq!(value(&retained, at), value(&fresh, at), "step {step}: {at}");
        }
    }
}

#[test]
fn spill_anchor_children_publish_zero_and_preserve_empty_text() {
    let mut workbook = model(&[
        ("D1", "TRANSPOSE(A1:A3)"),
        ("H1", "ISBLANK(D1)"),
        ("H2", "ISNUMBER(E1)"),
        ("H3", "ISTEXT(F1)"),
    ]);
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet.set_value(target("A2").address, 0.).unwrap();
    sheet.set_value(target("A3").address, "").unwrap();
    let mut ctx = context(&workbook);
    for _ in 0..2 {
        complete(&mut ctx, &["F1", "D1", "E1", "H1", "H2", "H3"]);
        assert_eq!(value(&ctx, "D1"), CellValue::Number(0.));
        assert_eq!(value(&ctx, "E1"), CellValue::Number(0.));
        assert_eq!(value(&ctx, "F1"), CellValue::Text(String::new()));
        assert_eq!(value(&ctx, "H1"), CellValue::Boolean(false));
        for at in ["H2", "H3"] {
            assert_eq!(value(&ctx, at), CellValue::Boolean(true));
        }
    }
}

#[test]
fn authored_scalar_blockers_survive_reads_and_become_spill_children_after_removal() {
    // SEQUENCE(2,2) is [1,2;3,4]. An authored value/formula at B2 blocks it.
    for formula_blocker in [false, true] {
        let mut w = model(&[("A1", "SEQUENCE(2,2)"), ("D1", "B2*2")]);
        let sheet = w.sheet_mut(0).unwrap();
        if formula_blocker {
            sheet.set_formula("B2".parse().unwrap(), "40+2").unwrap();
        } else {
            sheet.set_value("B2".parse().unwrap(), 42.).unwrap();
        }
        let mut direct = poirot::eval::overlay::Evaluation::with_workbook(&w)
            .with_policy(policy())
            .build(Default::default())
            .unwrap();
        assert_eq!(direct.cell(target("B2")).unwrap(), CellValue::Number(42.));
        assert_eq!(
            direct.cell(target("A1")).unwrap(),
            CellValue::Error(CellError::Spill)
        );
        let mut ctx = context(&w);
        complete(&mut ctx, &["B2", "A1", "D1"]);
        assert_eq!(value(&ctx, "B2"), CellValue::Number(42.));
        assert_eq!(value(&ctx, "A1"), CellValue::Error(CellError::Spill));
        assert_eq!(value(&ctx, "D1"), CellValue::Number(84.));
        ctx.overlay.remove_cell(target("B2")).unwrap();
        complete(&mut ctx, &["B2", "D1"]);
        assert_eq!(value(&ctx, "B2"), CellValue::Number(4.));
        assert_eq!(value(&ctx, "D1"), CellValue::Number(8.));
        ctx.overlay.set_value(target("B2"), 99.).unwrap();
        complete(&mut ctx, &["D1", "A1"]);
        assert_eq!(value(&ctx, "D1"), CellValue::Number(198.));
        assert_eq!(value(&ctx, "A1"), CellValue::Error(CellError::Spill));
    }
}
