//! Independently authored shape proofs and public SEQUENCE contracts.
//! Microsoft documents rectangular sequential values and defaults at
//! https://support.microsoft.com/en-us/excel/functions/sequence-function.
//! Error coercion details not stated there are explicit Poirot contracts here.
use poirot_eval::{
    compiler::PreparedFormula,
    overlay::{Evaluation, OverlayPolicy},
    *,
};
use poirot_model::{CellAddress, CellError, CellValue, ShapeBounds as S, Workbook};

fn properties(formula: &str) -> typing::ExpressionProperties {
    let prepared = FormulaCompiler::new().compile_formula(formula, &EvaluationPolicy::default());
    let PreparedFormula::Ready(formula) = prepared.as_ref() else {
        panic!("not prepared: {formula}")
    };
    formula.properties().unwrap()
}
fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn value(formula: &str, address: &str) -> CellValue {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("D10").address, formula)
        .unwrap();
    Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap()
        .cell(at(address))
        .unwrap()
}
#[test]
fn static_and_partial_bounds_follow_result_contracts() {
    for (source, expected) in [
        ("1", S::SCALAR),
        ("A1", S::SCALAR),
        ("A1:C4", S::new(4, 3)),
        ("C4:A1", S::new(4, 3)),
        ("A:C", S::new(poirot_model::MAX_ROWS, 3)),
        ("2:4", S::new(3, poirot_model::MAX_COLUMNS)),
        ("{1,2,3;4,5,6}", S::new(2, 3)),
        ("SEQUENCE(3)", S::new(3, 1)),
        ("_xlfn.SEQUENCE(3,2)", S::new(3, 2)),
        ("SEQUENCE(,4)", S::new(1, 4)),
        ("SEQUENCE(3,,7)", S::new(3, 1)),
        (
            "SEQUENCE(A1)",
            S {
                rows: None,
                columns: std::num::NonZeroU32::new(1),
            },
        ),
        (
            "SEQUENCE(RAND(),2)",
            S {
                rows: None,
                columns: std::num::NonZeroU32::new(2),
            },
        ),
        (
            "SEQUENCE(3,A1)",
            S {
                rows: std::num::NonZeroU32::new(3),
                columns: None,
            },
        ),
        (
            "TRANSPOSE(SEQUENCE(A1,2))",
            S {
                rows: std::num::NonZeroU32::new(2),
                columns: None,
            },
        ),
        ("IF(A1,SEQUENCE(3),{1,2})", S::new(3, 2)),
        ("IFERROR(SEQUENCE(3,2),{1;2;3;4})", S::new(4, 2)),
        ("ISNUMBER({1,2;3,4})", S::new(2, 2)),
        ("SUM(SEQUENCE(A1,A2))", S::SCALAR),
        ("@SEQUENCE(A1,A2)", S::SCALAR),
        ("SEQUENCE(3,1)+{1,2}", S::new(3, 2)),
        ("A1#", S::UNKNOWN),
        ("ANCHORARRAY(A1)", S::UNKNOWN),
        ("INDIRECT(A1)", S::UNKNOWN),
        ("INDEX(A1:C4,0,1)", S::UNKNOWN),
        ("unbound_name", S::UNKNOWN),
        ("UNKNOWN()", S::SCALAR),
    ] {
        assert_eq!(properties(source).shape, expected, "{source}");
    }
}
#[test]
fn sequence_defaults_steps_and_reference_dimensions_are_public_results() {
    for (source, address, expected) in [
        ("SEQUENCE(3)", "D12", 3.0),
        ("SEQUENCE(2,3)", "F11", 6.0),
        ("SEQUENCE(,3)", "F10", 3.0),
        ("SEQUENCE(2,,9,-2)", "D11", 7.0),
        ("SEQUENCE(2,2,8,0)", "E11", 8.0),
        ("SEQUENCE(2,3,-3,2)", "F11", 7.0),
        ("TRANSPOSE(SEQUENCE(3))", "F10", 3.0),
    ] {
        assert_eq!(
            value(source, address),
            CellValue::Number(expected),
            "{source}"
        );
    }
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 3.0).unwrap();
    sheet
        .set_formula(at("D10").address, "SEQUENCE(A1,2,5,3)")
        .unwrap();
    let mut evaluation = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(evaluation.cell(at("E12")).unwrap(), 20.0.into());
    assert_eq!(evaluation.cell(at("F12")).unwrap(), CellValue::Blank);
}
#[test]
fn generated_sequence_lifting_and_branch_shapes_bound_actual_spills() {
    // Mathematical row-major coordinates and broadcasting give these dimensions;
    // expected shapes do not come from Poirot's type inference.
    for rows in 1..=5u32 {
        for columns in 1..=4u32 {
            for source in [
                format!("SEQUENCE({rows},{columns})"),
                format!("SEQUENCE({rows},{columns})+1"),
                format!("IF(TRUE,SEQUENCE({rows},{columns}),0)"),
                format!("IFERROR(SEQUENCE({rows},{columns}),7)"),
            ] {
                assert_eq!(properties(&source).shape, S::new(rows, columns));
                let last = CellAddress {
                    row: 9 + rows - 1,
                    col: 3 + columns - 1,
                };
                let mut book = Workbook::new();
                let sheet = book.add_sheet("S").unwrap();
                sheet.set_formula(at("D10").address, &source).unwrap();
                sheet.set_formula(at("A1").address, "ROWS(D10#)").unwrap();
                sheet
                    .set_formula(at("A2").address, "COLUMNS(D10#)")
                    .unwrap();
                let mut evaluation = Evaluation::with_workbook(&book)
                    .build(Default::default())
                    .unwrap();
                assert!(
                    matches!(
                        evaluation
                            .cell(EvaluationTarget {
                                sheet: 0,
                                address: last
                            })
                            .unwrap(),
                        CellValue::Number(_)
                    ),
                    "{source}"
                );
                assert_eq!(
                    evaluation.cell(at("A1")).unwrap(),
                    f64::from(rows).into(),
                    "{source}"
                );
                assert_eq!(
                    evaluation.cell(at("A2")).unwrap(),
                    f64::from(columns).into(),
                    "{source}"
                );
            }
        }
    }
}
#[test]
fn sequence_dimension_and_error_failures_do_not_publish_children() {
    // Invalid dimension and nonfinite arithmetic behavior are Poirot contracts;
    // Microsoft's brief SEQUENCE page does not specify these coercion details.
    for source in [
        "SEQUENCE(0)",
        "SEQUENCE(-1)",
        "SEQUENCE(1,0)",
        "SEQUENCE(1048577)",
        "SEQUENCE(1,16385)",
        "SEQUENCE()",
        "SEQUENCE(,)",
        "SEQUENCE(,,,)",
        "SEQUENCE(1,1,1,1,1)",
    ] {
        assert_eq!(
            value(source, "D10"),
            CellValue::Error(CellError::Value),
            "{source}"
        );
        assert_eq!(value(source, "E10"), CellValue::Blank, "{source}");
    }
    assert_eq!(
        value("SEQUENCE(#N/A)", "D10"),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        value("SEQUENCE(2,1,1E308,1E308)", "D10"),
        CellValue::Error(CellError::Num)
    );
    assert_eq!(value("SEQUENCE(2.9,1)", "D11"), 2.0.into());
}
#[test]
fn saved_footprints_are_independent_of_computed_bounds() {
    for complete in [false, true] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_formula(at("A1").address, "SEQUENCE(1)").unwrap();
        sheet.get_mut(at("A1").address).unwrap().value = 11.0.into();
        sheet
            .set_spill_extent(at("A1").address, at("B2").address, "SEQUENCE(1)")
            .unwrap();
        if complete {
            for (address, n) in [("B1", 12.), ("A2", 13.), ("B2", 14.)] {
                sheet
                    .set_spill_cached_value(
                        at(address).address,
                        at("A1").address,
                        "SEQUENCE(1)",
                        n.into(),
                    )
                    .unwrap();
            }
        }
        let mut evaluation = Evaluation::with_workbook(&book)
            .with_policy(OverlayPolicy {
                producers: ProducerPolicy::TrustSaved,
                ..Default::default()
            })
            .build(Default::default())
            .unwrap();
        let result = evaluation.cell(at("B2"));
        if complete {
            assert_eq!(result.unwrap(), 14.0.into());
        } else {
            assert!(matches!(
                result,
                Err(EvaluationFailure::UnsupportedCapability { .. })
            ));
        }
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
}
#[test]
fn sequence_is_modern_profile_only_and_unknown_geometry_survives_profile_resolution() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "SEQUENCE(3)")
        .unwrap();
    let mut evaluation = Evaluation::with_workbook(&book)
        .with_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .build(Default::default())
        .unwrap();
    assert_eq!(
        evaluation.cell(at("A1")).unwrap(),
        CellValue::Error(CellError::Name)
    );
    assert_eq!(properties("SEQUENCE(3)").shape, S::new(3, 1));
}

#[test]
fn sequence_allocation_failure_is_operational_and_edge_spills_are_blocked() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(at("A1").address, "SEQUENCE(2,3)")
        .unwrap();
    let context = overlay::EvaluationContext {
        budgets: EvaluationBudgets {
            materialized_values: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut evaluation = Evaluation::with_workbook(&book).build(context).unwrap();
    assert!(matches!(
        evaluation.cell(at("A1")),
        Err(EvaluationFailure::Budget(_))
    ));
    assert_eq!(evaluation.cached_cells(), 0);
    let edge: CellAddress = "XFD1048576".parse().unwrap();
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(edge, "SEQUENCE(2)")
        .unwrap();
    let mut evaluation = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(
        evaluation
            .cell(EvaluationTarget {
                sheet: 0,
                address: edge
            })
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
}
#[test]
fn bounds_follow_formula_edits_style_changes_and_overlay_snapshot_retention() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "SEQUENCE(2,2)")
        .unwrap();
    let mut overlay = overlay::Overlay::with_workbook(&book).build().unwrap();
    let context = overlay::EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        4.0.into()
    );
    overlay
        .set_style(
            at("A1"),
            poirot_model::CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        4.0.into()
    );
    let snapshot = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_book, state) =
        overlay::Overlay::from_snapshot(postcard::from_bytes(&snapshot).unwrap()).unwrap();
    let mut overlay = state.attach(&restored_book).unwrap();
    overlay.set_formula(at("A1"), "SEQUENCE(3)").unwrap();
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("B2")).unwrap(), CellValue::Blank);
        assert_eq!(evaluation.cell(at("A3")).unwrap(), 3.0.into());
    }
    overlay.set_formula(at("A1"), "SUM(SEQUENCE(3))").unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("A1")).unwrap(), 6.0.into());
    assert_eq!(evaluation.cell(at("A3")).unwrap(), CellValue::Blank);
}
#[test]
fn compiler_rebinding_and_parser_policy_cannot_retain_narrow_old_bounds() {
    let mut compiler = FormulaCompiler::new();
    let policy = EvaluationPolicy::default();
    for (source, expected) in [
        ("A1:A3", S::new(3, 1)),
        ("A1:A30", S::new(30, 1)),
        ("B7:E8", S::new(2, 4)),
    ] {
        let prepared = compiler.compile_formula(source, &policy);
        let PreparedFormula::Ready(formula) = prepared.as_ref() else {
            panic!()
        };
        assert_eq!(formula.properties().unwrap().shape, expected);
    }
    let limited = EvaluationPolicy {
        parse: poirot_parsing::formula::FormulaParsePolicy {
            max_expressions: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        compiler.compile_formula("SEQUENCE(3,2)", &limited).as_ref(),
        PreparedFormula::ParseFailure { .. }
    ));
    assert!(matches!(
        compiler.compile_formula("SEQUENCE(", &policy).as_ref(),
        PreparedFormula::ParseFailure { .. }
    ));
    assert_eq!(
        compiler.compile_formula("SEQUENCE(3,2)", &policy).source(),
        "SEQUENCE(3,2)"
    );
}
#[test]
fn extension_created_arrays_have_unknown_bounds_instead_of_argument_guesses() {
    use extensions::*;
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "RECTANGLE",
            "geometry/v1",
            ExtensionContract {
                arity: registry::Arity {
                    minimum: 0,
                    maximum: 0,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    arrays: true,
                    references: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_: &mut dyn ExtensionHost| {
                Ok(ExtensionOutput::Array(vec![
                    vec![1.0.into(), 2.0.into()];
                    3
                ]))
            },
        )
        .unwrap();
    let policy = EvaluationPolicy::with_extensions(Default::default(), &registry);
    let prepared =
        FormulaCompiler::new().compile_formula_with_extensions("RECTANGLE()", &policy, &registry);
    let PreparedFormula::Ready(formula) = prepared.as_ref() else {
        panic!()
    };
    assert_eq!(formula.properties().unwrap().shape, S::UNKNOWN);
    assert!(formula.properties().unwrap().result.may_spill());
}

#[test]
fn compositional_bounds_cover_stacking_flattening_selection_products_and_filter_fallbacks() {
    // Independent rectangular arithmetic, not inferred Poirot output, supplies these bounds.
    // HSTACK's documented rule is max(rows), sum(columns):
    // https://support.microsoft.com/en-us/excel/functions/hstack-function
    for (source, bound, actual) in [
        ("HSTACK({1;2},{3,4})", S::new(2, 3), (2, 3)),
        ("VSTACK({1,2},{3;4})", S::new(3, 2), (3, 2)),
        ("TOCOL({1,2;3,4})", S::new(4, 1), (4, 1)),
        ("TOROW({1,2;3,4})", S::new(1, 4), (1, 4)),
        ("CHOOSECOLS({1,2;3,4},2,2,1)", S::new(2, 3), (2, 3)),
        ("CHOOSEROWS({1,2;3,4},1,1,2)", S::new(3, 2), (3, 2)),
        ("MMULT({1,2,3;4,5,6},{1;2;3})", S::new(2, 1), (2, 1)),
        ("FILTER({1,2;3,4},{TRUE;FALSE})", S::new(2, 2), (1, 2)),
        (
            "FILTER({1,2;3,4},{FALSE;FALSE},{5,6,7;8,9,10;11,12,13})",
            S::new(3, 3),
            (3, 3),
        ),
    ] {
        assert_eq!(properties(source).shape, bound, "{source}");
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_formula(at("D10").address, source).unwrap();
        sheet.set_formula(at("A1").address, "ROWS(D10#)").unwrap();
        sheet
            .set_formula(at("A2").address, "COLUMNS(D10#)")
            .unwrap();
        let mut evaluation = Evaluation::with_workbook(&book)
            .build(Default::default())
            .unwrap();
        assert_eq!(
            evaluation.cell(at("A1")).unwrap(),
            CellValue::Number(f64::from(actual.0)),
            "{source}"
        );
        assert_eq!(
            evaluation.cell(at("A2")).unwrap(),
            CellValue::Number(f64::from(actual.1)),
            "{source}"
        );
        assert!(bound.admits(actual.0, actual.1));
    }
    // Products/sums that exceed the finite dimension representation must widen.
    assert_eq!(properties("TOCOL(A:XFD)").shape.rows, None);
    assert_eq!(
        properties("HSTACK(SEQUENCE(A1,2),SEQUENCE(3,4))").shape,
        S {
            rows: None,
            columns: std::num::NonZeroU32::new(6)
        }
    );
    assert_eq!(
        properties("VSTACK(SEQUENCE(2,A1),SEQUENCE(4,3))").shape,
        S {
            rows: std::num::NonZeroU32::new(6),
            columns: None
        }
    );
}
