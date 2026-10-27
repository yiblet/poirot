use super::binding_error::BindingError;
use super::domain::Expression;
use super::formula_plan::FormulaPlan;
use crate::eval::functions::registry::*;
use crate::model::{CellAddress, CellError, CellValue, Workbook};
use poirot_parsing::formula::{CellReference, Expr as Syntax, UnaryOp};

const AT: CellAddress = CellAddress { row: 0, col: 0 };

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

fn plan(formula: &str) -> FormulaPlan {
    FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap()
}

#[test]
fn aliases_bind_to_one_typed_identity_and_keep_syntax() {
    for (alias, canonical) in [
        ("STDEV.S", "STDEV"),
        ("STDEVP", "STDEV.P"),
        ("VAR.S", "VAR"),
        ("VARP", "VAR.P"),
    ] {
        assert_eq!(
            resolve_function(&poirot_parsing::formula::FunctionName::from(alias))
                .unwrap()
                .handler,
            resolve_function(&poirot_parsing::formula::FunctionName::from(canonical))
                .unwrap()
                .handler
        );
        assert_eq!(
            resolve_function(&poirot_parsing::formula::FunctionName::from(alias))
                .unwrap()
                .arity,
            resolve_function(&poirot_parsing::formula::FunctionName::from(canonical))
                .unwrap()
                .arity
        );
    }
    let syntax = Syntax::Function {
        // Explicit raw syntax retains caller spelling; normal parsed builtins use a slot.
        name: poirot_parsing::formula::FunctionName::Unknown("vlookup".into()),
        args: vec![
            Syntax::Number(1.),
            Syntax::Array(vec![vec![Syntax::Number(1.), Syntax::Number(7.)]]),
            Syntax::Number(2.),
            Syntax::Bool(false),
        ],
    };
    let mut compiled = FormulaPlan::bind(&syntax).unwrap();
    assert_eq!(compiled.syntax(), &syntax);
    if let Expression::Search(call) = compiled.expression() {
        assert_eq!(call.function, SearchFunction::VLookup);
        assert_eq!(call.arguments.source_name, "vlookup");
        // Source spelling is diagnostic data, never an executable dispatcher.
        let call = &compiled.bytecode.calls[0];
        let offset = call.source_offset as usize + 4;
        compiled.bytecode.template.operands[offset..offset + 7].copy_from_slice(b"UNKNOWN");
    } else {
        panic!("expected typed search family")
    }
    assert_eq!(
        match crate::eval::entry::run_plan(
            &workbook(),
            0,
            AT,
            &compiled,
            Default::default(),
            Default::default()
        ) {
            crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(7.)
    );
}

#[test]
fn lazy_unknowns_and_error_timing_survive_binding() {
    let workbook = workbook();
    for (formula, expected) in [
        ("IF(TRUE,1,UNKNOWN(1/0))", CellValue::Number(1.)),
        ("IF(FALSE,UNKNOWN(),2)", CellValue::Number(2.)),
        ("IFERROR(1,UNKNOWN())", CellValue::Number(1.)),
        ("IFERROR(UNKNOWN(),2)", CellValue::Number(2.)),
        ("IFNA(UNKNOWN(),2)", CellValue::Error(CellError::Name)),
        ("CHOOSE(2,UNKNOWN(),4)", CellValue::Number(4.)),
        ("UNKNOWN(1/0)", CellValue::Error(CellError::Name)),
        // NumericFunction.oneDouble rejects arity before operand conversion.
        ("ABS(#REF!,2)", CellValue::Error(CellError::Value)),
        ("ABS(1,2)", CellValue::Error(CellError::Value)),
        ("DATE(#REF!)", CellValue::Error(CellError::Value)),
        (
            "YEARFRAC(#REF!,1,#DIV/0!)",
            CellValue::Error(CellError::Div0),
        ),
        ("IF(#REF!)", CellValue::Error(CellError::Value)),
        ("_xlfn.IFNA(#N/A,2)", CellValue::Number(2.)),
        ("_XLFN.IFNA(#N/A,2)", CellValue::Error(CellError::Name)),
    ] {
        assert_eq!(
            match crate::eval::entry::run_plan(
                &workbook,
                0,
                AT,
                &plan(formula),
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
        assert_eq!(
            match crate::eval::entry::run_formula(
                &workbook,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "wrapper {formula}"
        );
    }
    assert!(matches!(
        plan("UNKNOWN()").expression(),
        Expression::UnresolvedFunction(_)
    ));
}

#[test]
fn reusable_plan_reads_current_inputs_without_rebinding() {
    let mut workbook = workbook();
    let compiled = plan("SUM(A1:A2)+IF(TRUE,3,UNKNOWN())");
    let root = std::sync::Arc::as_ptr(&compiled.bytecode().executable);
    for (input, expected) in [(2., 5.), (9., 12.)] {
        workbook.sheet_mut(0).unwrap().set_value(AT, input).unwrap();
        assert_eq!(
            match crate::eval::entry::run_plan(
                &workbook,
                0,
                AT,
                &compiled,
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected)
        );
        assert_eq!(
            std::sync::Arc::as_ptr(&compiled.bytecode().executable),
            root
        );
    }
}

#[test]
fn public_handmade_syntax_has_bounded_fallible_binding() {
    let mut syntax = Syntax::Number(1.);
    for _ in 0..128 {
        syntax = Syntax::Unary {
            op: UnaryOp::Plus,
            operand: Box::new(syntax),
        };
    }
    assert_eq!(FormulaPlan::bind(&syntax), Err(BindingError::ResourceLimit));
    let wide = Syntax::Function {
        name: "SUM".into(),
        args: vec![Syntax::Number(1.); 100_001],
    };
    assert_eq!(FormulaPlan::bind(&wide), Err(BindingError::ResourceLimit));
}

#[test]
fn handmade_array_and_reference_shapes_fail_with_typed_diagnostics() {
    for array in [
        Syntax::Array(vec![]),
        Syntax::Array(vec![vec![]]),
        Syntax::Array(vec![
            vec![Syntax::Number(1.), Syntax::Number(2.)],
            vec![Syntax::Number(3.)],
        ]),
    ] {
        let syntax = Syntax::Function {
            name: "VLOOKUP".into(),
            args: vec![Syntax::Number(3.), array, Syntax::Number(2.)],
        };
        assert_eq!(FormulaPlan::bind(&syntax), Err(BindingError::InvalidArray));
    }
    let invalid_cell = Syntax::Cell(CellReference {
        row: u32::MAX,
        column: 0,
        row_absolute: false,
        column_absolute: false,
        sheet: None,
    });
    for reference in [
        invalid_cell,
        Syntax::Column {
            column: crate::model::MAX_COLUMNS,
            absolute: false,
            sheet: None,
        },
        Syntax::Row {
            row: crate::model::MAX_ROWS,
            absolute: false,
            sheet: None,
        },
    ] {
        let syntax = Syntax::Function {
            name: "ROW".into(),
            args: vec![reference],
        };
        assert_eq!(
            FormulaPlan::bind(&syntax),
            Err(BindingError::InvalidReference)
        );
    }
    let edge = Syntax::Cell(CellReference {
        row: crate::model::MAX_ROWS - 1,
        column: crate::model::MAX_COLUMNS - 1,
        row_absolute: true,
        column_absolute: true,
        sheet: None,
    });
    assert!(FormulaPlan::bind(&edge).is_ok());
}

#[test]
fn nonfinite_handmade_literals_are_guarded_and_parser_rejections_preserved() {
    let workbook = workbook();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let compiled = FormulaPlan::bind(&Syntax::Number(value)).unwrap();
        assert_eq!(
            match crate::eval::entry::run_plan(
                &workbook,
                0,
                AT,
                &compiled,
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Error(CellError::Num)
        );
    }
    let syntax = Syntax::Function {
        name: "IF".into(),
        args: vec![
            Syntax::Bool(true),
            Syntax::Number(3.),
            Syntax::Number(f64::INFINITY),
        ],
    };
    let compiled = FormulaPlan::bind(&syntax).unwrap();
    assert_eq!(
        match crate::eval::entry::run_plan(
            &workbook,
            0,
            AT,
            &compiled,
            Default::default(),
            Default::default()
        ) {
            crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(3.)
    );
    assert_eq!(
        match crate::eval::entry::run_formula(
            &workbook,
            0,
            AT,
            "1e999",
            Default::default(),
            Default::default()
        ) {
            crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        match crate::eval::entry::run_formula(
            &workbook,
            0,
            AT,
            "IF(TRUE,3,1e999)",
            Default::default(),
            Default::default()
        ) {
            crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn admission_keeps_lifo_diagnostic_precedence_and_exact_node_budget() {
    let invalid_reference = Syntax::Row {
        row: crate::model::MAX_ROWS,
        absolute: false,
        sheet: None,
    };
    let invalid_array = Syntax::Array(vec![]);
    for (left, right, expected) in [
        (
            invalid_reference.clone(),
            invalid_array.clone(),
            BindingError::InvalidArray,
        ),
        (
            invalid_array,
            invalid_reference,
            BindingError::InvalidReference,
        ),
    ] {
        let syntax = Syntax::Binary {
            op: poirot_parsing::formula::BinaryOp::Add,
            left: Box::new(left),
            right: Box::new(right),
        };
        assert_eq!(FormulaPlan::bind(&syntax), Err(expected));
    }
    let mut syntax = Syntax::Function {
        name: "UNKNOWN".into(),
        args: vec![Syntax::Missing; 99_999],
    };
    assert!(FormulaPlan::bind(&syntax).is_ok());
    if let Syntax::Function { args, .. } = &mut syntax {
        args.push(Syntax::Missing);
    }
    assert_eq!(FormulaPlan::bind(&syntax), Err(BindingError::ResourceLimit));
}

#[test]
fn hierarchical_sheet_spans_and_qualified_names_keep_exact_records() {
    let span = poirot_parsing::formula::SheetSpanReference {
        first_sheet: "First sheet".into(),
        last_sheet: "Last'Sheet".into(),
        start: CellReference {
            row: 0,
            column: 1,
            row_absolute: true,
            column_absolute: false,
            sheet: None,
        },
        end: Some(CellReference {
            row: crate::model::MAX_ROWS - 1,
            column: crate::model::MAX_COLUMNS - 1,
            row_absolute: false,
            column_absolute: true,
            sheet: None,
        }),
    };
    let syntax = Syntax::Array(vec![vec![
        Syntax::SheetSpan(span.clone()),
        Syntax::QualifiedName {
            sheet: "Mixed Sheet".into(),
            name: "Mixed_Name".into(),
        },
    ]]);
    let compiled = FormulaPlan::bind(&syntax).unwrap();
    assert_eq!(compiled.syntax(), &syntax);
    assert_eq!(
        compiled.expression(),
        Expression::Array(vec![vec![
            Expression::SheetSpan(span.clone()),
            Expression::QualifiedName {
                sheet: "Mixed Sheet".into(),
                name: "Mixed_Name".into()
            },
        ]])
    );
    let mut invalid = span;
    invalid.end.as_mut().unwrap().sheet = Some("nested qualifier".into());
    assert_eq!(
        FormulaPlan::bind(&Syntax::SheetSpan(invalid)),
        Err(BindingError::InvalidReference)
    );
}

#[test]
fn extension_binding_retains_identity_spelling_and_deferred_operands() {
    use crate::eval::functions::extensions::*;
    /// Panics if pure binding incorrectly executes an extension provider.
    struct NeverCalled;
    impl ExtensionFunction for NeverCalled {
        fn evaluate(&self, _: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
            panic!("binding must not execute providers")
        }
    }
    let mut registry = ExtensionRegistry::new();
    let binding = registry
        .register(
            ExtensionNamespace::UserDefined,
            "MY_EXTENSION",
            "revision-1",
            ExtensionContract {
                arity: Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Deferred,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            NeverCalled,
        )
        .unwrap();
    // Wrong arity and an unknown nested call remain executable-tree records.
    let syntax = Syntax::Function {
        name: "my_extension".into(),
        args: vec![
            Syntax::Function {
                name: "UNKNOWN".into(),
                args: vec![],
            },
            Syntax::Missing,
        ],
    };
    let compiled = FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert_eq!(compiled.syntax(), &syntax);

    let Expression::Extension {
        binding: actual,
        arguments,
    } = compiled.expression()
    else {
        panic!("expected extension binding")
    };
    assert_eq!(actual, binding);
    assert_eq!(arguments.source_name, "my_extension");
    assert_eq!(arguments.resolved_spelling(), "my_extension");
    assert!(matches!(
        &arguments.expressions[..],
        [Expression::UnresolvedFunction(_), Expression::Missing]
    ));
}

#[test]
fn binding_keeps_name_and_typed_function_identity_for_reference_consumers() {
    use poirot_parsing::formula::BinaryOp;
    let syntax = poirot_parsing::formula::parse_formula("A1:Z").unwrap();
    let plan = FormulaPlan::bind(&syntax).unwrap();
    let Expression::Binary {
        op: BinaryOp::Range,
        right,
        ..
    } = plan.expression()
    else {
        panic!()
    };
    assert_eq!(right.as_ref(), &Expression::Name("Z".into()));
    let syntax = poirot_parsing::formula::parse_formula("OFFSET(B3,0,0,-2,-2):C4").unwrap();
    let plan = FormulaPlan::bind(&syntax).unwrap();
    let Expression::Binary {
        op: BinaryOp::Range,
        left,
        ..
    } = plan.expression()
    else {
        panic!()
    };
    assert!(!matches!(
        left.as_ref(),
        Expression::Name(_) | Expression::Number(_) | Expression::Cell(_)
    ));
}
