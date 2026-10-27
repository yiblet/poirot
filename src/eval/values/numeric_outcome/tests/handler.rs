use crate::eval::compile;
use crate::eval::config::RunConfiguration;
use crate::eval::entry::{run_formula_with_configuration, run_plan_with_configuration};
use crate::eval::limits::{EvaluationBudgets, EvaluationFailure, EvaluationOutcome};
use crate::eval::values::value::VALUE_ERROR;
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::{Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellValue, KnownSourceException, Workbook};

fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w
}

fn at(col: u32) -> CellAddress {
    CellAddress { row: 0, col }
}

fn pinned() -> RunConfiguration {
    RunConfiguration::default()
}

fn formula(w: &Workbook, f: &str, c: &RunConfiguration) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(w, 0, at(25), f, c, EvaluationBudgets::default())
}

fn value(o: EvaluationOutcome<CellValue>) -> CellValue {
    match o {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}

fn bits(v: CellValue) -> u64 {
    match v {
        Number(n) => n.to_bits(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn actual_bound_nonfinite_number_ptg_is_catchable_num_and_not_a_number() {
    use poirot_parsing::formula::Expr as Syntax;
    let w = book();
    let c = pinned();
    for raw in [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0xfff8_0000_0000_0042),
    ] {
        for (name, args, expected) in [
            (
                "IFERROR",
                vec![Syntax::Number(raw), Syntax::Number(42.0)],
                Number(42.0),
            ),
            ("ISNUMBER", vec![Syntax::Number(raw)], Boolean(false)),
        ] {
            let syntax = Syntax::Function {
                name: name.into(),
                args,
            };
            let plan = compile::bound::FormulaPlan::bind(&syntax).unwrap();
            assert_eq!(
                value(run_plan_with_configuration(
                    &w,
                    0,
                    at(25),
                    &plan,
                    &c,
                    EvaluationBudgets::default()
                )),
                expected
            );
        }
    }
}

#[test]
fn add_sub_follow_source_cancellation() {
    let mut w = book();
    let c = pinned();
    assert_eq!(bits(value(formula(&w, "0.1+0.2-0.3", &c))), 0);
    let s = w.sheet_mut(0).unwrap();
    s.set_value(at(0), Number(1.0)).unwrap();
    s.set_value(at(1), Number(f64::from_bits(1.0f64.to_bits() + 1)))
        .unwrap();
    s.set_value(at(2), Number(2.0)).unwrap();
    for f in ["A1-B1", "B1-A1", "A1+-B1"] {
        assert_eq!(bits(value(formula(&w, f, &c))), 0);
    }
    assert_eq!(value(formula(&w, "C1-A1", &c)), Number(1.0));
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::INFINITY))
        .unwrap();
    for f in ["A1-A1", "A1+-A1", "A1+1"] {
        assert_eq!(value(formula(&w, f, &c)), Error(Num));
    }
}

#[test]
fn selected_sum_has_positive_zero_accumulator_and_preserves_error_order() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(-0.0))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(1), Number(0.0))
        .unwrap();
    let c = pinned();
    for f in ["SUM(A1)", "SUM()", "SUM(A1,B1)"] {
        assert_eq!(bits(value(formula(&w, f, &c))), 0);
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::NEG_INFINITY))
        .unwrap();
    assert_eq!(value(formula(&w, "SUM(A1,#DIV/0!)", &c)), Error(Div0));
}

#[test]
fn exceptional_expressions_in_unused_if_and_iferror_branches_stay_lazy() {
    let w = book();
    let c = pinned();
    for f in [
        "IFERROR(1,POISSON(1,0,FALSE))",
        "IF(FALSE,POISSON(1,0,FALSE),1)",
        "IF(TRUE,1,NORMSINV(1E-300)<0)",
    ] {
        let actual = value(formula(&w, f, &c));
        assert_eq!(actual, Number(1.0));
    }
}

#[test]
fn special_operand_handlers_follow_source_type_and_error_order() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::NEG_INFINITY))
        .unwrap();
    let c = pinned();
    for (f, expected) in [
        ("+A1", f64::NEG_INFINITY),
        ("-A1", f64::INFINITY),
        ("A1%", f64::NEG_INFINITY),
    ] {
        assert_eq!(bits(value(formula(&w, f, &c))), expected.to_bits());
    }
    for f in ["0<A1", "B1<A1"] {
        assert!(
            matches!(formula(&w,f,&c),EvaluationOutcome::Aborted {failure:EvaluationFailure::KnownSourceException {
            exception:KnownSourceException::SpecialDoubleComparison {argument:1,bits},..},..} if bits.bits()==f64::NEG_INFINITY.to_bits())
        );
    }
    assert_eq!(value(formula(&w, "A1<TRUE", &c)), Boolean(true));
    assert_eq!(value(formula(&w, "TRUE>A1", &c)), Boolean(true));
    assert_eq!(value(formula(&w, "A1<#DIV/0!", &c)), Error(Div0));
    assert_eq!(value(formula(&w, "#DIV/0!<A1", &c)), Error(Div0));
    assert_eq!(value(formula(&w, "+\"hello\"", &c)), Text("hello".into()));
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(-0.0))
        .unwrap();
    assert_eq!(bits(value(formula(&w, "A1%", &c))), 0);
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(-f64::MIN_POSITIVE))
        .unwrap();
    assert_eq!(bits(value(formula(&w, "A1%", &c))), 0);
}

#[test]
fn inverse_string_grammar_and_constructor_order_are_consumed_per_alias() {
    let w = book();
    let c = pinned();
    for f in [
        "NORMSINV(\"Infinity\")",
        "NORM.S.INV(\"NaN\")",
        "NORMINV(\"0x1p-1\",0,1)",
        "NORM.INV(\"0.5d\",0,1)",
    ] {
        assert_eq!(value(formula(&w, f, &c)), Error(VALUE_ERROR));
    }
    for f in [
        "NORMINV(0,0,1)",
        "NORM.INV(0,0,1)",
        "NORMSINV(0)",
        "NORM.S.INV(0)",
    ] {
        assert_eq!(value(formula(&w, f, &c)), Error(Num));
    }
    for f in ["NORMINV(0.5,#DIV/0!,0)", "NORM.INV(0.5,#DIV/0!,0)"] {
        assert_eq!(value(formula(&w, f, &c)), Error(Div0));
    }
    for f in ["POISSON(1,0,#DIV/0!)", "POISSON.DIST(1,0,#DIV/0!)"] {
        assert_eq!(value(formula(&w, f, &c)), Error(Div0));
    }
}

#[test]
fn bounded_numeric_comparison_does_not_materialize_spreadsheet_values() {
    let w = book();
    let c = pinned();
    for f in ["1=1", "1-1", "1.000000000000001=1", "1E100=2E100"] {
        let budgets = EvaluationBudgets {
            materialized_values: 1,
            ..EvaluationBudgets::default()
        };
        let outcome = run_formula_with_configuration(&w, 0, at(25), f, &c, budgets);
        assert!(
            matches!(outcome, EvaluationOutcome::Complete { .. }),
            "{outcome:?}"
        );
        assert_eq!(outcome.usage().comparisons, 1);
    }
}
