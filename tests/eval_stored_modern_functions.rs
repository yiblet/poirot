//! Independently authored tests of stored function names from MS-XLSX §2.2.3.
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
//! Retrieved 2026-10-21. No upstream implementation or tests reused.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};
fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
#[test]
fn stored_worksheet_functions_dispatch_and_publish_spills() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "_xlfn._xlws.FILTER({3;7;11},{TRUE;FALSE;TRUE})"),
        ("D1", "_xlfn._xlws.sort({9;2;6})"),
        ("G1", "_xlfn.TAKE({4;8},2)"),
        ("J1", "_xlfn.TEXTBEFORE(\"east:west\",\":\")"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    // Read children before anchors: typed preparation must recognize the stored aliases.
    for (cell, expected) in [
        ("A2", 11.0.into()),
        ("A1", 3.0.into()),
        ("D3", 9.0.into()),
        ("D1", 2.0.into()),
        ("G2", 8.0.into()),
        ("J1", CellValue::Text("east".into())),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), expected, "Direct {cell}");
        assert_eq!(run.cell(at(cell)).unwrap(), expected, "Overlay {cell}");
    }
}
#[test]
fn worksheet_namespace_is_exact_and_does_not_admit_unknown_functions() {
    for name in [
        "_xlws.FILTER",
        "_XLFN._xlws.FILTER",
        "_xlfn._XLWS.FILTER",
        "_xlfn._xlws.SUM",
        "_xlfn._xlws.PY",
        "_xlfn._xlws.UNKNOWN",
        "_xlfn._xlws._xlws.SORT",
    ] {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula(at("A1").address, format!("{name}(1)"))
            .unwrap();
        let mut direct = Evaluation::with_workbook(&book)
            .build(Default::default())
            .unwrap();
        assert_eq!(
            direct.cell(at("A1")).unwrap(),
            CellValue::Error(CellError::Name),
            "{name}"
        );
    }
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "_xlfn._xlws.FILTER({3;7},{TRUE;TRUE})")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            builtin: BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct.cell(at("A1")).unwrap(),
        CellValue::Error(CellError::Name)
    );
}
#[test]
fn explicitly_bound_extensions_keep_precedence_and_source_spelling() {
    use poirot::eval::{
        compile::bound::FormulaPlan, functions::extensions::*, functions::registry::Arity,
    };
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "_xlfn._xlws.FILTER",
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
    let source = "_xlfn._xlws.FILTER(7)";
    let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
    let plan = FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert_eq!(plan.syntax(), &syntax);
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, source)
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_extensions(&registry)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_extensions(&registry)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(direct.cell(at("A1")).unwrap(), CellValue::Number(41.));
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Number(41.)
    );
}
#[test]
fn adapter_aliases_share_typed_descriptors_without_advertising_new_names() {
    use poirot::eval::adapters::{FunctionAdapter, builtins};
    let adapter = builtins();
    for name in ["FILTER", "SORT"] {
        let descriptor = adapter.resolve_profile(&name.into()).unwrap();
        let stored = format!("_xlfn._xlws.{name}");
        assert!(std::ptr::eq(
            descriptor,
            adapter.resolve_profile(&stored.as_str().into()).unwrap()
        ));
        assert_eq!(descriptor.name, name);
        assert!(
            !adapter
                .functions()
                .chain(adapter.profile_functions())
                .any(|d| d.name == stored)
        );
    }
}
