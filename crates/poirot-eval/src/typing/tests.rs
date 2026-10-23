use super::*;
use crate::{Session, Value};

/// Check the expression-channel concretization without conflating root materialization.
fn admits(ty: T, value: &Value) -> bool {
    match value {
        Value::Scalar(v) | Value::SelectedScalar(v, _) => ty.admits_scalar(v),
        Value::Missing => ty.missing,
        Value::Array(rows) => {
            !ty.array.is_empty()
                && rows
                    .iter()
                    .flatten()
                    .all(|v| K::of(v).is_subtype_of(ty.array))
        }
        Value::Refs(areas) | Value::SheetSpan(areas) => {
            let single = areas.len() == 1 && areas.first().is_some_and(|a| a.size() == 1);
            !ty.multi_reference.is_empty() || single && !ty.single_reference.is_empty()
        }
        Value::SignedRefs(_) => !ty.single_reference.is_empty() || !ty.multi_reference.is_empty(),
    }
}

#[test]
fn representative_successful_expression_results_belong_to_their_possible_types() {
    let mut workbook = poirot_model::Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 2.).unwrap();
    sheet.set_value("A2".parse().unwrap(), "text").unwrap();
    let formulas = [
        "2",
        "\"text\"",
        "TRUE",
        "#N/A",
        "A1",
        "A1:A2",
        "@A1:A2",
        "{1,2;3,4}",
        "{1,2}+1",
        "A1:A2=2",
        "SUM(TRANSPOSE({1,2}))",
        "RAND()+NOW()",
        "IF(TRUE,A1,\"x\")",
        "IF({TRUE,FALSE},1,2)",
        "IFERROR({1,#N/A},7)",
        "IFNA({1,#N/A},7)",
        "IFS({TRUE,FALSE},1,TRUE,2)",
        "SWITCH({1,2},1,3,2,4)",
        "CHOOSE({1,2},3,4)",
        "CHOOSE(1,A1,A1:A2)",
        "ISNUMBER(A1:A2)",
        "TRANSPOSE(A1:A2)",
        "INDEX(A1:A2,1)",
        "INDEX({1,2;3,4},0,1)",
        "INDIRECT(\"A1:A2\")",
        "OFFSET(A1,0,0,2,1)",
        "UNKNOWN()",
    ];
    for formula in formulas {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::bytecode::compile(&syntax).unwrap();
        assert!(compiled.typing.matches(&compiled.template));
        let ty = compiled.typing.root().unwrap().result;
        let mut session = Session::with_context(&workbook, crate::EvaluationInputs::default());
        let value = session
            .bytecode_expr(0, "D10".parse().unwrap(), &compiled)
            .unwrap();
        assert!(admits(ty, &value), "{formula}: {ty:?}");
    }
}

#[test]
fn effects_join_is_a_least_upper_bound_with_opaque_subsuming_volatile() {
    let effects = [
        FormulaEffects::default(),
        FormulaEffects {
            possible_reads: DependencyPolicy::TIME,
            external: ExternalBehavior::Tracked,
        },
        FormulaEffects {
            possible_reads: DependencyPolicy::SEED,
            external: ExternalBehavior::Volatile,
        },
        FormulaEffects {
            possible_reads: DependencyPolicy::TIME.union(DependencyPolicy::SEED),
            external: ExternalBehavior::Opaque,
        },
    ];
    for a in effects {
        for b in effects {
            assert_eq!(a.join(b), b.join(a));
            assert_eq!(a.join(a), a);
            assert!(a.is_subtype_of(a.join(b)));
            assert!(b.is_subtype_of(a.join(b)));
            for c in effects {
                assert_eq!(a.join(b).join(c), a.join(b.join(c)));
                if a.is_subtype_of(c) && b.is_subtype_of(c) {
                    assert!(a.join(b).is_subtype_of(c));
                }
            }
        }
    }
}
