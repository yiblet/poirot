//! Shared boundaries behind the independently authored JSON corpus regressions.
use poirot_eval::overlay::{EvaluationContext, Overlay};
use poirot_eval::*;
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (at, value) in [("A1", -2.), ("A2", 2.), ("A3", 3.)] {
        sheet.set_value(at.parse().unwrap(), value).unwrap();
    }
    book
}

#[test]
fn shared_corpus_boundaries_agree_across_public_execution_paths() {
    // DATE/NUMBERVALUE and direct-blank SUMPRODUCT examples adapt ClosedXML tests:
    // revision 09f2f21a60e821ce7fb18ca3576b272e5dd6d921, MIT (LICENSES/ClosedXML-MIT.txt).
    // https://github.com/ClosedXML/ClosedXML/tree/09f2f21a60e821ce7fb18ca3576b272e5dd6d921/ClosedXML.Tests/Excel/CalcEngine
    // Criteria siblings derive from the independently authored saved-Excel/Calc corpus.
    // Referenced-blank SUMPRODUCT probes were independently recalculated with Calc 26.2.6.3.
    let cases = [
        ("DATE(10000,-32767,3)", CellValue::Number(1961112.)),
        ("DATE(10000,-32000,1)", CellValue::Number(1984455.)),
        ("DATE(10001,-32000,1)", CellValue::Number(1984455.)),
        ("DATE(1e100,-32000,1)", CellValue::Number(1984455.)),
        ("DATE(10000,1,1)", CellValue::Error(CellError::Num)),
        ("DATE(10000,0,1)", CellValue::Number(2958435.)),
        ("DATE(-1,32000,1)", CellValue::Number(973586.)),
        ("DATE(10000,-32767.1,3)", CellValue::Error(CellError::Num)),
        ("DATE(2000,32767,1)", CellValue::Error(CellError::Num)),
        ("DATE(2000,1,1e100)", CellValue::Number(69292.)),
        ("DATE(2000,1,-1e100)", CellValue::Number(69292.)),
        (
            "SUMPRODUCT(IF(TRUE,,),5)",
            CellValue::Error(CellError::Value),
        ),
        (
            "SUMPRODUCT(5,IF(TRUE,,))",
            CellValue::Error(CellError::Value),
        ),
        ("SUMPRODUCT(D1,5)", CellValue::Number(0.)),
        ("SUMPRODUCT(D1:D1,5)", CellValue::Number(0.)),
        ("SUMPRODUCT(5,D1)", CellValue::Number(0.)),
        ("SUMPRODUCT(\"4\",5)", CellValue::Number(0.)),
        ("SUMPRODUCT(NA(),5)", CellValue::Error(CellError::NA)),
        ("SUMPRODUCT(,5)", CellValue::Error(CellError::Value)),
        (
            "NUMBERVALUE(\"1.234567890e308\")",
            CellValue::Error(CellError::Value),
        ),
        (
            "NUMBERVALUE(\"-1.234567890e308\")",
            CellValue::Error(CellError::Value),
        ),
        (
            "NUMBERVALUE(\"1.234567890e-310\")",
            CellValue::Error(CellError::Value),
        ),
        ("NUMBERVALUE(\"1e308%\")", CellValue::Number(1e306)),
        ("NUMBERVALUE(\"1e-320%%\")", CellValue::Number(0.)),
        ("NUMBERVALUE(\"   \")", CellValue::Number(0.)),
        ("COUNTIF(A1:A3,\"<>(2)\")", CellValue::Number(2.)),
        ("COUNTIF(A1:A3,\"=(2)\")", CellValue::Number(1.)),
        ("COUNTIF(A1:A3,\"-2\")", CellValue::Number(1.)),
        ("COUNTIF(A1:A3,\">(2)\")", CellValue::Number(2.)),
        ("COUNTIF(A1:A3,\"(-2)\")", CellValue::Number(0.)),
        ("COUNTIF(A1:A3,\"(+2)\")", CellValue::Number(0.)),
        ("COUNTIF(A1:A3,\"((2))\")", CellValue::Number(0.)),
        (r#"MATCH("a*",{"ab","ab","ax"},1)"#, CellValue::Number(3.)),
        (r#"MATCH("a*",{"ab","ab","ax"},0)"#, CellValue::Number(1.)),
        (r#"MATCH("a~*",{"a*","ab","ax"},0)"#, CellValue::Number(1.)),
        (
            r#"MATCH("z*",{"ab","ab","ax"},0)"#,
            CellValue::Error(CellError::NA),
        ),
        ("MATCH(2,{1,2,3},1)", CellValue::Number(2.)),
        ("MATCH(2,{3,2,1},-1)", CellValue::Number(2.)),
    ];
    let mut book = book();
    for (row, (formula, _)) in cases.iter().enumerate() {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                CellAddress {
                    row: row as u32,
                    col: 5,
                },
                *formula,
                CellValue::Blank,
            )
            .unwrap();
    }
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (row, (formula, expected)) in cases.iter().enumerate() {
        let at = EvaluationTarget {
            sheet: 0,
            address: CellAddress {
                row: row as u32,
                col: 5,
            },
        };
        let prepared = compiler.compile_formula(formula, &policy);
        let bound =
            bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
                .unwrap();
        let direct = run_formula(
            &book,
            0,
            at.address,
            formula,
            Default::default(),
            Default::default(),
        );
        match direct {
            EvaluationOutcome::Complete { value, .. } => {
                assert_eq!(value, *expected, "direct {formula}")
            }
            other => panic!("direct {formula}: {other:?}"),
        }
        assert_eq!(
            run.evaluate_formula(at, &prepared).unwrap(),
            *expected,
            "prepared {formula}"
        );
        assert_eq!(
            run.evaluate_bound(at, &bound).unwrap(),
            *expected,
            "bound {formula}"
        );
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(at).unwrap(),
            *expected,
            "overlay {formula}"
        );
    }
    overlay
        .set_value(
            EvaluationTarget {
                sheet: 0,
                address: "A1".parse().unwrap(),
            },
            4.,
        )
        .unwrap();
    let at = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 25, col: 5 },
    };
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(at).unwrap(),
        CellValue::Number(3.)
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&book).unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(at).unwrap(),
        CellValue::Number(3.)
    );
}
