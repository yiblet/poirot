use super::*;
use crate::*;
const AT: CellAddress = CellAddress { row: 0, col: 0 };
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn plan(formula: &str) -> bound::FormulaPlan {
    bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap()
}
fn value(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("expected semantic completion: {other:?}"),
    }
}
fn failure<T: std::fmt::Debug>(outcome: EvaluationOutcome<T>) -> (BudgetFailure, EvaluationUsage) {
    match outcome {
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(failure),
            usage,
        } => (failure, usage),
        other => panic!("expected operational failure: {other:?}"),
    }
}
#[test]
fn operational_failures_are_distinct_from_real_spreadsheet_errors() {
    let workbook = workbook();
    let budgets = EvaluationBudgets {
        run_work: 0,
        ..EvaluationBudgets::default()
    };
    let (error, usage) = failure(run_plan(
        &workbook,
        0,
        AT,
        &plan("1"),
        EvaluationInputs::default(),
        budgets,
    ));
    assert_eq!(error.kind, BudgetKind::RunWork);
    assert_eq!(usage.work_units, 0);
    for (formula, expected) in [
        ("#NUM!", CellValue::Error(CellError::Num)),
        ("#REF!", CellValue::Error(CellError::Ref)),
        ("SQRT(-1)", CellValue::Error(CellError::Num)),
    ] {
        assert_eq!(
            value(run_plan(
                &workbook,
                0,
                AT,
                &plan(formula),
                EvaluationInputs::default(),
                EvaluationBudgets::default()
            )),
            expected
        );
    }
    let invalid = EvaluationBudgets {
        depth: 129,
        ..EvaluationBudgets::default()
    };
    let (error, usage) = failure(run_cell(
        &workbook,
        0,
        AT,
        EvaluationInputs::default(),
        invalid,
    ));
    assert_eq!(error.kind, BudgetKind::Configuration);
    assert_eq!(usage, EvaluationUsage::default());
}
#[test]
fn lazy_branches_do_not_spend_work_and_error_handlers_cannot_catch_abort() {
    let workbook = workbook();
    let tiny = EvaluationBudgets {
        run_work: 3,
        cell_work: 3,
        depth: 2,
        materialized_values: 1,
        ..EvaluationBudgets::default()
    };
    let outcome = run_plan(
        &workbook,
        0,
        AT,
        &plan("IF(TRUE,4,SUM({1,2,3}))"),
        EvaluationInputs::default(),
        tiny,
    );
    assert_eq!(outcome.usage().expressions, 3);
    assert_eq!(value(outcome), CellValue::Number(4.));
    for (formula, budgets, kind) in [
        (
            "IFERROR(SUM({1,2}),9)",
            EvaluationBudgets {
                materialized_values: 1,
                ..EvaluationBudgets::default()
            },
            BudgetKind::Materialization,
        ),
        (
            "IFERROR(1+2,9)",
            EvaluationBudgets {
                depth: 1,
                ..EvaluationBudgets::default()
            },
            BudgetKind::Depth,
        ),
        (
            "IFERROR(1+2,9)",
            EvaluationBudgets {
                run_work: 1,
                ..EvaluationBudgets::default()
            },
            BudgetKind::RunWork,
        ),
        (
            "IFERROR(MATCH(\"a*\",{\"abc\"},0),9)",
            EvaluationBudgets {
                wildcard_states: 1,
                ..EvaluationBudgets::default()
            },
            BudgetKind::WildcardStates,
        ),
    ] {
        assert_eq!(
            failure(run_plan(
                &workbook,
                0,
                AT,
                &plan(formula),
                EvaluationInputs::default(),
                budgets
            ))
            .0
            .kind,
            kind,
            "{formula}"
        );
    }
    assert_eq!(
        value(run_plan(
            &workbook,
            0,
            AT,
            &plan("IFERROR(1/0,9)"),
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        )),
        CellValue::Number(9.)
    );
}
#[test]
fn matrix_and_text_limits_abort_before_growth_and_dense_reads() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(AT, 1.).unwrap();
    let limited = EvaluationBudgets {
        materialized_values: 2,
        ..EvaluationBudgets::default()
    };
    let (error, usage) = failure(run_plan(
        &workbook,
        0,
        AT,
        &plan("IFERROR(VLOOKUP(1,{1,0;2,0;3,0},2,FALSE),9)"),
        EvaluationInputs::default(),
        limited,
    ));
    assert_eq!(error.kind, BudgetKind::Materialization);
    assert_eq!(error.limit, 2);
    assert_eq!(error.consumed, 0);
    assert_eq!(error.requested, 6);
    assert_eq!(
        error.target,
        Some(EvaluationTarget {
            sheet: 0,
            address: AT
        })
    );
    assert_eq!(usage.cell_reads, 0);
    let outcome = run_plan(
        &workbook,
        0,
        AT,
        &plan("IFERROR(VLOOKUP(1,A1:B3,2,FALSE),9)"),
        EvaluationInputs::default(),
        limited,
    );
    let EvaluationOutcome::Complete { value, usage } = outcome else {
        panic!("indexed reference must complete: {outcome:?}")
    };
    assert_eq!(value, CellValue::Blank);
    assert_eq!(usage.cell_reads, 2);
    // Two endpoint descriptors, one combined range, two memo entries, and dynamic blank-read tracking.
    // No six-cell dense table reservation is admitted; each reservation fits cap 2.
    assert_eq!(usage.materialized_values, 6);
    let limited = EvaluationBudgets {
        text_bytes: 5,
        ..EvaluationBudgets::default()
    };
    let (error, _) = failure(run_plan(
        &workbook,
        0,
        AT,
        &plan("IFERROR(REPT(\"ab\",3),9)"),
        EvaluationInputs::default(),
        limited,
    ));
    assert_eq!(error.kind, BudgetKind::TextBytes);
    assert_eq!(error.requested, 12);
    let limited = EvaluationBudgets {
        materialized_values: 2,
        ..EvaluationBudgets::default()
    };
    assert_eq!(
        failure(run_plan(
            &workbook,
            0,
            AT,
            &plan("WORKDAY(1,1,A1:A3)"),
            EvaluationInputs::default(),
            limited
        ))
        .0
        .kind,
        BudgetKind::Materialization
    );
}
#[test]
fn literal_and_absent_cells_keep_depth_failures_and_unwind_frames() {
    for stored in [false, true] {
        let mut workbook = workbook();
        let leaf: CellAddress = "B1".parse().unwrap();
        if stored {
            workbook.sheet_mut(0).unwrap().set_value(leaf, 7.).unwrap();
        }
        let limited = EvaluationBudgets {
            depth: 0,
            ..EvaluationBudgets::default()
        };
        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::new(limited).unwrap(),
        );
        let Err(EvaluationFailure::Budget(failure)) = session.cell(0, leaf) else {
            panic!("leaf read must retain its depth charge");
        };
        assert_eq!(failure.kind, BudgetKind::Depth);
        assert_eq!(
            failure.target,
            Some(EvaluationTarget {
                sheet: 0,
                address: leaf
            })
        );
        assert_eq!(session.budget.usage.cell_reads, 1);
        assert_eq!(session.budget.usage.maximum_depth, 0);
        assert_eq!(session.budget.depth, 0);
        assert!(session.budget.frames.is_empty());
        assert!(session.visiting.is_empty());
        assert!(session.cache.is_empty());
        assert!(session.consumer.is_none());

        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::new(EvaluationBudgets {
                depth: 1,
                ..limited
            })
            .unwrap(),
        );
        assert_eq!(
            session.cell(0, leaf).unwrap(),
            if stored {
                CellValue::Number(7.)
            } else {
                CellValue::Blank
            }
        );
        assert_eq!(session.budget.usage.maximum_depth, 1);
        assert_eq!(session.budget.usage.cell_reads, 1);
        assert_eq!(session.budget.usage.completed_cells, 1);
        assert_eq!(
            session.cache.get(&(0, leaf)),
            Some(&if stored {
                CellValue::Number(7.)
            } else {
                CellValue::Blank
            })
        );
        assert_eq!(session.budget.depth, 0);
        assert!(session.budget.frames.is_empty());
        assert!(session.visiting.is_empty());
        assert!(session.consumer.is_none());
    }
}
#[test]
fn depth_abort_is_not_a_cycle_and_frames_unwind_after_failure() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(AT, "A1", CellValue::Number(99.))
        .unwrap();
    assert_eq!(
        value(run_cell(
            &workbook,
            0,
            AT,
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        )),
        CellValue::Error(CellError::Ref)
    );
    let limited = EvaluationBudgets {
        depth: 1,
        ..EvaluationBudgets::default()
    };
    assert_eq!(
        failure(run_cell(
            &workbook,
            0,
            AT,
            EvaluationInputs::default(),
            limited
        ))
        .0
        .kind,
        BudgetKind::Depth
    );
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(limited).unwrap(),
    );
    assert!(session.cell(0, AT).is_err());
    assert_eq!(session.budget.depth, 0);
    assert!(session.budget.frames.is_empty());
    assert!(session.visiting.is_empty());
    assert!(session.cache.is_empty());
}
#[test]
fn aborted_bulk_pass_never_publishes_partial_caches_or_metadata() {
    let mut workbook = workbook();
    for (address, formula, cache) in [("A1", "1", 101.), ("A2", "2", 102.), ("A3", "3", 103.)] {
        let address = address.parse().unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula_with_cached_value(address, formula, CellValue::Number(cache))
            .unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .get_mut(address)
            .unwrap()
            .style
            .number_format = Some("0.00".into());
    }
    let before = workbook.clone();
    let full = run_workbook(
        &workbook,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    );
    let limit = full.usage().work_units - 1;
    let (error, usage) = failure(run_recalculation(
        &mut workbook,
        EvaluationInputs::default(),
        EvaluationBudgets {
            run_work: limit,
            ..EvaluationBudgets::default()
        },
    ));
    assert_eq!(error.kind, BudgetKind::RunWork);
    assert!(usage.completed_cells >= 1);
    assert_eq!(workbook, before);
    assert!(matches!(
        run_recalculation(
            &mut workbook,
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete { value: 3, .. }
    ));
    for (address, expected) in [("A1", 1.), ("A2", 2.), ("A3", 3.)] {
        assert_eq!(
            workbook
                .sheet(0)
                .unwrap()
                .get(address.parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(expected)
        );
    }
}
#[test]
fn completed_bulk_values_and_direct_cell_allowances_ignore_prior_workload() {
    let mut workbook = workbook();
    for (address, formula) in [("A1", "REPT(\"a\",100)"), ("A2", "LEN(A1)")] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    let context = EvaluationInputs::default();
    // Derive the tight direct-cell allowance rather than hard-code an old
    // text implementation's work cost. LEN now reserves traversal of all
    // 100 input bytes, so the former 250-unit allowance is insufficient.
    let mut cell_work = 0;
    for address in ["A1", "A2"] {
        let address = address.parse().unwrap();
        let unrestricted = run_cell(&workbook, 0, address, context, EvaluationBudgets::default());
        assert!(matches!(unrestricted, EvaluationOutcome::Complete { .. }));
        let mut low = 0;
        let mut high = unrestricted.usage().work_units;
        while low < high {
            let allowance = low + (high - low) / 2;
            match run_cell(
                &workbook,
                0,
                address,
                context,
                EvaluationBudgets {
                    cell_work: allowance,
                    ..EvaluationBudgets::default()
                },
            ) {
                EvaluationOutcome::Complete { .. } => high = allowance,
                EvaluationOutcome::Aborted {
                    failure:
                        EvaluationFailure::Budget(BudgetFailure {
                            kind: BudgetKind::CellWork,
                            ..
                        }),
                    ..
                } => low = allowance + 1,
                other => panic!("{other:?}"),
            }
        }
        let (too_small, _) = failure(run_cell(
            &workbook,
            0,
            address,
            context,
            EvaluationBudgets {
                cell_work: low - 1,
                ..EvaluationBudgets::default()
            },
        ));
        assert_eq!(too_small.kind, BudgetKind::CellWork);
        cell_work = cell_work.max(low);
    }
    let budgets = EvaluationBudgets {
        cell_work,
        ..EvaluationBudgets::default()
    };
    let bulk = run_workbook(&workbook, context, budgets);
    assert!(
        bulk.usage().work_units > cell_work,
        "the batch must exceed one cell's allowance to exercise workload isolation"
    );
    let bulk = match bulk {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    };
    for cell in bulk {
        assert_eq!(
            value(run_cell(
                &workbook,
                0,
                cell.target.address,
                context,
                budgets
            )),
            cell.value
        );
    }
}
#[test]
fn intersection_checks_actual_output_cardinality_before_each_push() {
    let workbook = workbook();
    let limited = EvaluationBudgets {
        materialized_values: 5,
        ..EvaluationBudgets::default()
    };
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(limited).unwrap(),
    );
    let area = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    let result = session.references(
        poirot_parsing::formula::BinaryOp::Intersection,
        Value::Refs(vec![area; 3]),
        Value::Refs(vec![area; 3]),
    );
    assert_eq!(
        result
            .err()
            .expect("expected materialization failure")
            .budget()
            .unwrap()
            .kind,
        BudgetKind::Materialization
    );
    assert_eq!(session.budget.usage.materialized_values, 5);
    let left = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    let right = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 1,
        r1: 1,
        c0: 1,
        c1: 1,
    };
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(limited).unwrap(),
    );
    assert!(matches!(
        session
            .references(
                poirot_parsing::formula::BinaryOp::Intersection,
                Value::Refs(vec![left; 3]),
                Value::Refs(vec![right; 3])
            )
            .unwrap(),
        Value::Scalar(CellValue::Error(CellError::Null))
    ));
}
#[test]
fn expanding_text_checks_output_bytes_before_growth() {
    let workbook = workbook();
    let context = EvaluationInputs::default();
    for (formula, limit, requested) in [
        ("SUBSTITUTE(\"aaaa\",\"a\",\"bbbb\")", 8, 16),
        ("REPLACE(\"abc\",2,1,\"123456\")", 7, 12),
        ("UPPER(\"ΐ\")", 4, 6),
    ] {
        let (error, _) = failure(run_plan_with_configuration(
            &workbook,
            0,
            AT,
            &plan(formula),
            &RunConfiguration::from(context).with_text_policy(TextSemanticsPolicy {
                source: TextSourceVersion::Poi7321209OpenJdk21Unicode15,
            }),
            EvaluationBudgets {
                text_bytes: limit,
                ..EvaluationBudgets::default()
            },
        ));
        assert_eq!(error.kind, BudgetKind::TextBytes, "{formula}");
        assert_eq!(error.requested, requested, "{formula}");
    }
    assert_eq!(
        value(run_plan(
            &workbook,
            0,
            AT,
            &plan("SUBSTITUTE(\"aaaa\",\"a\",\"x\",2)"),
            context,
            EvaluationBudgets::default()
        )),
        CellValue::Text("axaa".into())
    );
}
#[test]
fn formatter_reservations_and_unsupported_capabilities_are_operational() {
    let workbook = workbook();
    let context = EvaluationInputs::default();
    let (error, _) = failure(run_plan(
        &workbook,
        0,
        AT,
        &plan("IFERROR(TEXT(1,\"0\"),9)"),
        context,
        EvaluationBudgets {
            text_bytes: 1024,
            ..EvaluationBudgets::default()
        },
    ));
    assert_eq!(error.kind, BudgetKind::TextBytes);
    assert!(error.requested > 1024);
    assert_eq!(
        value(run_plan(
            &workbook,
            0,
            AT,
            &plan("TEXT(1,\"0\")"),
            context,
            EvaluationBudgets::default()
        )),
        CellValue::Text("1".into())
    );
    let unsupported = EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::NumericConversion,
        source: Some(EvaluationTarget {
            sheet: 0,
            address: AT,
        }),
        detail: "unsupported conversion mode".into(),
    };
    assert!(unsupported.budget().is_none());
    let result = function_value(Err(FunctionFailure::Operational(unsupported.clone())));
    assert!(matches!(result, Err(failure) if failure == unsupported));
    assert!(matches!(
        function_value(Err(FunctionFailure::Spreadsheet(CellError::Num))),
        Ok(Value::Scalar(CellValue::Error(CellError::Num)))
    ));
}

#[test]
fn unlimited_work_saturates_accounting_without_exhausting_the_sentinel() {
    let mut meter = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
    meter.push_cell(EvaluationTarget {
        sheet: 0,
        address: AT,
    });
    meter.charge(WorkKind::Comparison, 10_000_001).unwrap();
    meter.charge(WorkKind::Comparison, u64::MAX).unwrap();
    meter.charge(WorkKind::Comparison, 1).unwrap();
    assert_eq!(meter.usage.work_units, u64::MAX);
    assert_eq!(meter.usage.comparisons, u64::MAX);
    assert_eq!(meter.remaining_work(), u64::MAX);
    meter.check_allocation(100_001).unwrap();
    meter.check_text(1_048_577).unwrap();
    let mut states = u64::MAX;
    meter.wildcard_step(&mut states).unwrap();
    assert_eq!(states, u64::MAX);
    assert_eq!(meter.usage.work_units, u64::MAX);
}

#[test]
fn finite_work_admission_stays_transactional_beside_unlimited_counters() {
    let mut meter = BudgetMeter::new(EvaluationBudgets {
        cell_work: 1,
        ..Default::default()
    })
    .unwrap();
    meter.charge(WorkKind::Comparison, u64::MAX).unwrap();
    meter.push_cell(EvaluationTarget {
        sheet: 0,
        address: AT,
    });
    meter.charge(WorkKind::Comparison, 1).unwrap();
    let before = meter.usage.clone();
    assert!(matches!(
        meter.charge(WorkKind::Comparison, 1),
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::CellWork,
            consumed: 1,
            requested: 1,
            ..
        }))
    ));
    assert_eq!(meter.usage, before);
    let mut finite = BudgetMeter::new(EvaluationBudgets {
        run_work: u64::MAX - 1,
        ..Default::default()
    })
    .unwrap();
    finite.charge(WorkKind::Comparison, u64::MAX - 1).unwrap();
    assert!(
        matches!(finite.charge(WorkKind::Comparison, 1), Err(EvaluationFailure::Budget(BudgetFailure { kind: BudgetKind::RunWork, consumed, requested: 1, .. })) if consumed == u64::MAX - 1)
    );
}
