use poirot::eval::{EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

fn worksheet() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("A1", 3.).unwrap();
    sheet.set_value("A2", 7.).unwrap();
    book
}

fn value(book: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        book,
        0,
        "B1".parse().unwrap(),
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn inline_lambda_checks_arity_and_returns_calc_when_uninvoked() {
    let book = worksheet();
    // https://support.microsoft.com/en-us/excel/functions/lambda-function
    for (formula, expected) in [
        ("LAMBDA(x,x+1)(1)", CellValue::Number(2.)),
        ("LAMBDA(7)()", CellValue::Number(7.)),
        ("LAMBDA(x,x+1)(1,2)", CellValue::Error(CellError::Value)),
        ("LAMBDA(x,x+1)", CellValue::Error(CellError::Calc)),
        ("LAMBDA(A1,5)(1)", CellValue::Error(CellError::Value)),
        ("LAMBDA(R1C1,5)(1)", CellValue::Error(CellError::Value)),
        ("LAMBDA(x.y,5)(1)", CellValue::Error(CellError::Value)),
        ("_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1)(4)", CellValue::Number(5.)),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn nested_inline_lambda_uses_lexical_captures_and_caller_arguments() {
    let book = worksheet();
    for (formula, expected) in [
        ("LET(base,A1,LAMBDA(y,LAMBDA(z,base+y+z)))(4)(5)", 12.),
        ("LET(x,2,LAMBDA(x,LET(x,x+1,IF(TRUE,x,0)))(x)+x)", 5.),
        ("LET(x,4,(LET(x,10,LAMBDA(y,x+y)))(x))", 14.),
        ("SUM(LAMBDA(area,area)(A1:A2))", 10.),
        ("LET(data,SEQUENCE(2),LAMBDA(SUM(data))())", 3.),
        ("IF(TRUE,LAMBDA(x,x),LAMBDA(x,0))(6)", 6.),
    ] {
        assert_eq!(
            value(&book, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn inline_lambda_prepared_snapshot_restores_captures_and_dependency_reads() {
    use poirot::calculation::EvaluationContext;
    use poirot::snapshots::PreparedPolicy;

    let mut book = worksheet();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula("B1", "LET(base,A1,LAMBDA(y,LAMBDA(z,base+y+z+A2)))(4)(5)")
        .unwrap();
    sheet.set_formula("B2", "LAMBDA(x,x+1)").unwrap();
    let context = EvaluationContext::default();
    let mut scenario = book.overlay().build().unwrap();
    let first = scenario.target("Data!A1").unwrap();
    let second = scenario.target("Data!A2").unwrap();
    let output = scenario.target("Data!B1").unwrap();
    let uninvoked = scenario.target("Data!B2").unwrap();
    let mut evaluation = scenario.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(19.));
    assert_eq!(
        evaluation.cell(uninvoked).unwrap(),
        CellValue::Error(CellError::Calc)
    );
    drop(evaluation);
    let bytes = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(first, 5.).unwrap();
    restored.set_value(second, 8.).unwrap();
    assert_eq!(
        restored.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(22.)
    );
}

#[test]
fn named_lambda_calls_respect_scope_lexical_capture_and_direct_builtins() {
    let mut book = worksheet();
    book.add_sheet("Other").unwrap();
    book.define_name("Bump", "LAMBDA(x,x+10)", None).unwrap();
    book.define_name("Bump", "LAMBDA(x,x+20)", Some(0)).unwrap();
    book.define_name("Sum", "LAMBDA(x,100)", None).unwrap();
    book.define_name("Plain", "7", None).unwrap();
    for (formula, expected) in [
        ("Bump(2)", CellValue::Number(22.)),
        ("(Other!Bump)(2)", CellValue::Number(12.)),
        ("LET(Bump,LAMBDA(x,x+30),Bump(2))", CellValue::Number(32.)),
        (
            "LET(Bump,LAMBDA(x,x+30),LAMBDA(y,LAMBDA(z,Bump(z)+y)))(2)(3)",
            CellValue::Number(35.),
        ),
        ("LAMBDA(Bump,Bump(4))(LAMBDA(x,x*2))", CellValue::Number(8.)),
        (
            "_xlfn.LAMBDA(_xlpm.Bump,_xlpm.Bump(4))(_xlfn.LAMBDA(_xlpm.x,_xlpm.x*2))",
            CellValue::Number(8.),
        ),
        ("LET(Bump,9,Bump(2))", CellValue::Error(CellError::Value)),
        ("Plain(2)", CellValue::Error(CellError::Value)),
        ("DoesNotExist(2)", CellValue::Error(CellError::Name)),
        ("SUM(5,2)", CellValue::Number(7.)),
        ("LET(Sum,LAMBDA(x,100),Sum(5,2))", CellValue::Number(7.)),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn named_lambda_recursion_terminates_or_reaches_its_callable_limit() {
    let mut book = worksheet();
    book.define_name("Factorial", "LAMBDA(n,IF(n<=1,1,n*Factorial(n-1)))", None)
        .unwrap();
    book.define_name("Forever", "LAMBDA(n,Forever(n))", None)
        .unwrap();
    book.define_name("OrdinaryCycle", "OrdinaryCycle", None)
        .unwrap();
    // Microsoft documents #NUM! when there are too many recursive calls.
    // https://support.microsoft.com/en-us/excel/functions/lambda-function
    assert_eq!(value(&book, "Factorial(5)"), CellValue::Number(120.));
    assert_eq!(
        value(&book, "Factorial(20)"),
        CellValue::Error(CellError::Num)
    );
    assert_eq!(value(&book, "Forever(1)"), CellValue::Error(CellError::Num));
    assert_eq!(
        value(&book, "OrdinaryCycle"),
        CellValue::Error(CellError::Ref)
    );
    let constrained = poirot::eval::EvaluationBudgets {
        depth: 8,
        ..Default::default()
    };
    assert!(matches!(
        run_formula(
            &book,
            0,
            "B1".parse().unwrap(),
            "Forever(1)",
            Default::default(),
            constrained
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn deferred_named_lambda_recursion_reports_its_limit() {
    // The 2 MiB reproducer is measured on x86-64 Linux. Elsewhere, verify the result
    // with extra stack space for ABI and compiler differences.
    let stack_mib = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        2
    } else {
        4
    };
    std::thread::Builder::new()
        .stack_size(stack_mib * 1024 * 1024)
        .spawn(|| {
            let mut book = worksheet();
            book.define_name(
                "Deferred",
                "LAMBDA(n,IF(n<=1,1,SUM(SUM(LET(k,n-1,IF(k>=0,Deferred(k),0))),1)))",
                None,
            )
            .unwrap();
            assert_eq!(value(&book, "Deferred(16)"), CellValue::Number(16.));
            assert_eq!(
                value(&book, "Deferred(17)"),
                CellValue::Error(CellError::Num)
            );

            book.define_name(
                "Heavy",
                "LAMBDA(n,IF(n<=1,1,SUM(SUM(SUM(SUM(LET(k,n-1,IF(k>=0,Heavy(k),0))))),1)))",
                None,
            )
            .unwrap();
            assert!(matches!(
                run_formula(
                    &book,
                    0,
                    "B1".parse().unwrap(),
                    "Heavy(16)",
                    Default::default(),
                    Default::default(),
                ),
                EvaluationOutcome::Aborted {
                    failure: poirot::eval::EvaluationError::Resource {
                        kind: poirot::eval::ResourceFailureKind::BackendGuard,
                        reason: poirot::eval::EvaluationReasonError::LambdaDeferredBytecodeDepth,
                        ..
                    },
                    ..
                }
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn stored_named_lambda_roundtrips_xlsx_and_reacts_to_changed_inputs() {
    use poirot::calculation::EvaluationContext;
    use poirot::snapshots::PreparedPolicy;
    use std::io::{Cursor, Read};

    let mut book = worksheet();
    let source = "_xlfn.LAMBDA(_xlpm.n,_xlpm.n+Data!$A$1)";
    book.define_name("PlusInput", source, None).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "PlusInput(4)")
        .unwrap();
    let bytes = poirot_io::write_xlsx_to_buffer(&book).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut workbook_xml = String::new();
    zip.by_name("xl/workbook.xml")
        .unwrap()
        .read_to_string(&mut workbook_xml)
        .unwrap();
    assert!(workbook_xml.contains(source));
    let mut imported = poirot_io::read_workbook_bytes::<Workbook>(&bytes).unwrap();
    assert_eq!(imported.named_ranges[0].formula, source);
    let mut scenario = imported.overlay().build().unwrap();
    let input = scenario.target("Data!A1").unwrap();
    let output = scenario.target("Data!B1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        scenario.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(7.)
    );
    let snapshot = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&snapshot)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(input, 6.).unwrap();
    assert_eq!(
        restored.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(10.)
    );
    drop(scenario);
    imported.named_ranges[0].formula = "LAMBDA(n,n*Data!$A$1)".into();
    let mut rebuilt = imported.overlay().build().unwrap();
    assert_eq!(
        rebuilt.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(12.)
    );
}

#[test]
fn registered_extension_stays_authoritative_in_named_call_position() {
    use poirot::eval::functions::{extensions::*, registry::Arity};

    let mut book = worksheet();
    book.define_name("Bump", "LAMBDA(x,100)", None).unwrap();
    let mut extensions = ExtensionRegistry::new();
    extensions
        .register(
            ExtensionNamespace::UserDefined,
            "BUMP",
            "test-v1",
            ExtensionContract {
                arity: Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_: &mut dyn ExtensionHost| Ok(ExtensionOutput::Scalar(CellValue::Number(41.))),
        )
        .unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_formula("B1", "Bump(7)").unwrap();
    sheet
        .set_formula("B2", "LET(Bump,LAMBDA(x,100),LAMBDA(x,Bump(x)))(7)")
        .unwrap();
    let mut scenario = book.overlay().extensions(&extensions).build().unwrap();
    let named = scenario.target("Data!B1").unwrap();
    let lexical = scenario.target("Data!B2").unwrap();
    let context = poirot::calculation::EvaluationContext::default();
    let mut eval = scenario.evaluation(&context).unwrap();
    assert_eq!(eval.cell(named).unwrap(), CellValue::Number(41.));
    assert_eq!(eval.cell(lexical).unwrap(), CellValue::Number(41.));
}

#[test]
fn isomitted_distinguishes_a_missing_lambda_argument_from_a_passed_blank() {
    let book = worksheet();
    // Microsoft's example explicitly supplies an empty positional slot.
    // https://support.microsoft.com/en-us/excel/functions/isomitted-function
    for (formula, expected) in [
        (
            "LAMBDA(x,y,IF(ISOMITTED(y),\"Missing second argument\",x+y))(1,)",
            CellValue::Text("Missing second argument".into()),
        ),
        (
            "LAMBDA(x,y,IF(ISOMITTED(y),-1,x+y))(1,A9)",
            CellValue::Number(1.),
        ),
        ("LAMBDA(x,y,ISOMITTED(y))(1,0)", CellValue::Boolean(false)),
        (
            "LAMBDA(x,y,ISOMITTED(y))(1,\"\")",
            CellValue::Boolean(false),
        ),
        (
            "LAMBDA(x,y,_xlfn.ISOMITTED(y))(1,1/0)",
            CellValue::Boolean(false),
        ),
        (
            "LAMBDA(x,y,LAMBDA(ISOMITTED(y))())(1,)",
            CellValue::Boolean(true),
        ),
        (
            "LAMBDA(x,y,LET(local,y,ISOMITTED(local)))(1,)",
            CellValue::Boolean(true),
        ),
        ("ISOMITTED()", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn named_lambda_spill_discovery_uses_the_callers_scope() {
    use poirot::calculation::EvaluationContext;
    use poirot::eval::{ArrayResultPolicy, EvaluationPolicy};

    let mut book = Workbook::new();
    for sheet in ["Included", "Excluded"] {
        book.add_sheet(sheet)
            .unwrap()
            .set_formula("B1", "MakeValues()")
            .unwrap();
    }
    book.define_name("MakeValues", "LAMBDA({7;8})", Some(0))
        .unwrap();
    let mut scenario = book
        .overlay()
        .policy(EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            ..Default::default()
        })
        .build()
        .unwrap();
    let absent_child = scenario.target("Excluded!B2").unwrap();
    let live_child = scenario.target("Included!B2").unwrap();
    let absent_anchor = scenario.target("Excluded!B1").unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = scenario.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(absent_child).unwrap(), CellValue::Blank);
    assert_eq!(evaluation.cell(live_child).unwrap(), CellValue::Number(8.));
    assert_eq!(
        evaluation.cell(absent_anchor).unwrap(),
        CellValue::Error(CellError::Name)
    );
}

#[test]
fn optional_lambda_parameters_fill_omitted_slots() {
    let book = worksheet();
    // The Excel team documents bracketed parameter names and the ISOMITTED fallback:
    // https://techcommunity.microsoft.com/blog/microsoft365insiderblog/new-lambda-functions-available-in-excel/4213017
    for (formula, expected) in [
        (
            "LAMBDA(base,[other],IF(ISOMITTED(other),base,other))(3)",
            3.0.into(),
        ),
        (
            "LAMBDA(base,[other],IF(ISOMITTED(other),base,other))(3,)",
            3.0.into(),
        ),
        (
            "LAMBDA(base,[other],IF(ISOMITTED(other),base,other))(3,7)",
            7.0.into(),
        ),
        (
            "LAMBDA([other],ISOMITTED(other))(A3)",
            CellValue::Boolean(false),
        ),
        (
            "LAMBDA([other],ISOMITTED(other))()",
            CellValue::Boolean(true),
        ),
        (
            "LAMBDA(base,[other],base)()",
            CellValue::Error(CellError::Value),
        ),
        (
            "LAMBDA([other],other)(3,7)",
            CellValue::Error(CellError::Value),
        ),
        (
            "LAMBDA(item,[ITEM],item)(3)",
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn stored_optional_lambda_survives_xlsx_and_prepared_snapshot_roundtrips() {
    use poirot::calculation::EvaluationContext;
    use poirot::snapshots::PreparedPolicy;

    let mut book = worksheet();
    // The existing LibreOffice isomitted.fods compatibility fixture demonstrates
    // the _xlop. declaration spelling and _xlpm. references. This test uses
    // independently authored input and arithmetic.
    book.define_name(
        "OptionalFn",
        "_xlfn.LAMBDA(_xlop.extra,IF(_xlfn.ISOMITTED(_xlpm.extra),A1,_xlpm.extra))",
        None,
    )
    .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "OptionalFn()")
        .unwrap();
    let bytes = poirot_io::write_xlsx_to_buffer(&book).unwrap();
    let reopened = Workbook::load().from_bytes(&bytes).unwrap();
    assert_eq!(value(&reopened, "OptionalFn()"), 3.0.into());
    assert_eq!(value(&reopened, "OptionalFn(15)"), 15.0.into());
    let scenario = reopened.overlay().build().unwrap();
    let output = scenario.target("Data!B1").unwrap();
    let input = scenario.target("Data!A1").unwrap();
    let bytes = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(input, 9.0).unwrap();
    assert_eq!(
        restored
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        9.0.into()
    );
}
