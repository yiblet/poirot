use super::super::expression_contract::ExpressionContract;
use super::super::external_behavior::ExternalBehavior;
use super::super::formula_effects::FormulaEffects;
use super::super::inference_machine::InferenceMachine;
use crate::eval::compile::bytecode::{CallTarget, CompileError};
use crate::eval::functions::registry::DependencyPolicy;
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::eval::vm::Vm;
use crate::model::{FormulaType, ScalarKinds, ShapeBounds};
use poirot_parsing::formula::{Expr, UnaryOp};

/// Check the expression-channel concretization without conflating root materialization.
fn admits(ty: FormulaType, value: &Value) -> bool {
    match value {
        Value::Scalar(v) | Value::SelectedScalar(v, _) => ty.admits_scalar(v),
        Value::Missing => ty.missing,
        Value::Array(rows) => {
            !ty.array.is_empty()
                && rows
                    .iter()
                    .flatten()
                    .all(|v| ScalarKinds::of(v).is_subtype_of(ty.array))
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
    let mut workbook = crate::model::Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 2.).unwrap();
    sheet.set_value("A2".parse().unwrap(), "text").unwrap();
    let formulas = [
        "MUNIT(2)",
        "TEXTBEFORE(\"a\",\",\",1,0,0,42)",
        "TEXTAFTER(\"a\",\",\",1,0,0,TRUE)",
        "TEXTBEFORE(\"a\",\",\",1,0,0,)",
        "ACOT({1;2})",
        "NETWORKDAYS.INTL(45292,45298,1,{45293;45294})",
        "XOR(TRUE,FALSE)",
        "RANDARRAY(2,3)",
        "TRIMRANGE(A1:A2)",
        "TRIMRANGE({1,2})",
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
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        assert!(compiled.typing.matches(&compiled.template));
        let ty = compiled.typing.root().unwrap().result;
        let mut session =
            Session::with_context(&workbook, crate::eval::config::EvaluationInputs::default());
        let value = Vm::new(&mut session)
            .run(0, "D10".parse().unwrap(), &compiled)
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

#[test]
fn random_array_and_trim_geometry_effects_and_channels_are_truthful() {
    for (formula, rows, columns) in [
        ("RANDARRAY()", Some(1), Some(1)),
        ("RANDARRAY(2,3)", Some(2), Some(3)),
        ("RANDARRAY(A1,3)", None, Some(3)),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(properties.shape.rows.map(std::num::NonZeroU32::get), rows);
        assert_eq!(
            properties.shape.columns.map(std::num::NonZeroU32::get),
            columns
        );
        assert_eq!(properties.result.array, ScalarKinds::NUMBER);
        assert!(
            properties
                .effects
                .possible_reads
                .contains(DependencyPolicy::RANDOM_READS)
        );
        assert!(
            properties
                .effects
                .possible_reads
                .contains(DependencyPolicy::NUMERIC_COERCION)
        );
        // Profile-only calls retain the existing unresolved-binding opacity.
        assert_eq!(properties.effects.external, ExternalBehavior::Opaque);
    }
    for (formula, reference) in [("TRIMRANGE(A1:A2)", true), ("TRIMRANGE({1,2})", false)] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(!properties.result.multi_reference.is_empty(), reference);
        // A reference expression can itself fail, and trimming a scalar error
        // has an array channel, so the joined compiled contract admits both.
        assert!(!properties.result.array.is_empty());
        assert_eq!(properties.shape, ShapeBounds::UNKNOWN);
    }
}

#[test]
fn trim_result_rule_preserves_a_proven_reference_channel() {
    let reference = crate::eval::functions::registry::ResultRule::TrimRange
        .apply(&[FormulaType::multi_reference(ScalarKinds::ANY)]);
    assert!(!reference.multi_reference.is_empty());
    assert!(reference.array.is_empty());
    let array = crate::eval::functions::registry::ResultRule::TrimRange
        .apply(&[FormulaType::array(ScalarKinds::ANY)]);
    assert!(!array.array.is_empty());
    assert!(array.multi_reference.is_empty());
}

#[test]
fn utility_numeric_geometry_excludes_reduced_holiday_sets() {
    for (formula, rows, columns) in [
        ("MUNIT(3)", Some(3), Some(3)),
        ("MUNIT(A1)", None, None),
        ("ACOT({1;2})", Some(2), Some(1)),
        ("ABS(A1:A3)", Some(3), Some(1)),
        ("ABS(@A1:A3)", Some(1), Some(1)),
        ("ISO.CEILING({1;2},{1,2,3})", Some(2), Some(3)),
        ("NETWORKDAYS.INTL(45292,45298,1,A1:A10)", Some(1), Some(1)),
        (
            "NETWORKDAYS.INTL({45292;45293},45298,1,A1:A10)",
            Some(2),
            Some(1),
        ),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(
            properties.shape.rows.map(std::num::NonZeroU32::get),
            rows,
            "{formula}"
        );
        assert_eq!(
            properties.shape.columns.map(std::num::NonZeroU32::get),
            columns,
            "{formula}"
        );
    }
    let t = crate::eval::functions::registry::ResultRule::FirstThreeNumericArguments.apply(&[
        FormulaType::scalar(ScalarKinds::NUMBER),
        FormulaType::scalar(ScalarKinds::NUMBER),
        FormulaType::scalar(ScalarKinds::NUMBER),
        FormulaType::array(ScalarKinds::NUMBER),
    ]);
    assert!(t.array.is_empty());
    assert_eq!(t.scalar, ScalarKinds::NUMBER.join(ScalarKinds::ERROR));
}

#[test]
fn complex_results_distinguish_lifting_from_text_reduction() {
    for (formula, kinds, rows, columns) in [
        ("IMABS({\"3+4i\";\"5+12i\"})", ScalarKinds::NUMBER, 2, 1),
        ("IMCONJUGATE({\"i\",\"j\"})", ScalarKinds::TEXT, 1, 2),
        ("IMPOWER({\"i\";\"2\"},{2,-1})", ScalarKinds::TEXT, 2, 2),
        ("IMSUM({\"i\";\"2i\"})", ScalarKinds::TEXT, 1, 1),
        ("IMPRODUCT({\"i\";\"-i\"})", ScalarKinds::TEXT, 1, 1),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(
            properties.result.elements(),
            kinds.join(ScalarKinds::ERROR),
            "{formula}"
        );
        assert_eq!(
            properties.shape,
            crate::model::ShapeBounds::new(rows, columns),
            "{formula}"
        );
        if rows == 1 && columns == 1 {
            assert!(properties.result.array.is_empty(), "{formula}");
        } else {
            assert!(!properties.result.array.is_empty(), "{formula}");
        }
    }
}

#[test]
fn radix_unit_and_bit_results_have_truthful_lifted_kinds() {
    for (formula, kinds, rows, columns) in [
        ("BIN2HEX({1;10})", ScalarKinds::TEXT, 2, 1),
        ("DEC2OCT({8;16},{2,4})", ScalarKinds::TEXT, 2, 2),
        ("BITOR({1;2},4)", ScalarKinds::NUMBER, 2, 1),
        (
            "CONVERT(1,{\"byte\",\"kibyte\"},\"bit\")",
            ScalarKinds::NUMBER,
            1,
            2,
        ),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(
            properties.result.elements(),
            kinds.join(ScalarKinds::ERROR),
            "{formula}"
        );
        assert_eq!(
            properties.shape,
            crate::model::ShapeBounds::new(rows, columns),
            "{formula}"
        );
    }
}

#[test]
fn documented_special_functions_have_numeric_broadcast_shape() {
    for (formula, rows, columns) in [
        ("BESSELI({0;1},{0,1})", 2, 2),
        ("ERF({0;1},2)", 2, 1),
        ("ERFC.PRECISE(1)", 1, 1),
        ("GESTEP(1,{0,1,2})", 1, 3),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(
            properties.result.elements(),
            ScalarKinds::NUMBER.join(ScalarKinds::ERROR)
        );
        assert_eq!(
            properties.shape,
            crate::model::ShapeBounds::new(rows, columns)
        );
        assert!(
            properties
                .effects
                .possible_reads
                .contains(crate::eval::functions::registry::DependencyPolicy::NUMERIC_COERCION)
        );
    }
}

#[test]
fn financial_collections_do_not_expand_numeric_parameter_results() {
    for (formula, rows, columns) in [
        ("FVSCHEDULE(100,{0.1;0.2})", 1, 1),
        ("FVSCHEDULE({100,200},{0.1;0.2})", 1, 2),
        ("XNPV({0;0.1},{-100;110},{1000;1365})", 2, 1),
        ("XIRR({-100;110},{1000;1365})", 1, 1),
        ("XIRR({-100;110},{1000;1365},{0.05,0.2})", 1, 2),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let compiled = crate::eval::compile::bytecode::compile(&syntax).unwrap();
        let properties = compiled.typing.root().unwrap();
        assert_eq!(
            properties.shape.rows.map(std::num::NonZeroU32::get),
            Some(rows),
            "{formula}"
        );
        assert_eq!(
            properties.shape.columns.map(std::num::NonZeroU32::get),
            Some(columns),
            "{formula}"
        );
        assert_eq!(
            properties.result.elements(),
            ScalarKinds::NUMBER.join(ScalarKinds::ERROR)
        );
    }
}

#[test]
fn inference_machine_rejects_incomplete_event_streams_and_unrepresentable_storage() {
    assert!(matches!(
        InferenceMachine::new(usize::MAX),
        Err(CompileError::ResourceLimit)
    ));
    let expression = Expr::Number(1.);
    let mut state = InferenceMachine::new(1).unwrap();
    assert!(matches!(
        state.leave(&[]),
        Err(CompileError::InvalidReference)
    ));
    state.enter(&expression, 0, 0).unwrap();
    assert!(matches!(
        state.bind(&CallTarget::Unresolved, 0),
        Err(CompileError::InvalidReference)
    ));
    assert!(matches!(
        state.finish(),
        Err(CompileError::InvalidReference)
    ));
}

#[test]
fn depth_truncation_has_an_error_atom_contract_without_unvisited_operands() {
    let mut expression = Expr::Number(1.);
    for _ in 0..crate::eval::compile::bytecode::DEPTH + 2 {
        expression = Expr::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(expression),
        };
    }
    let compiled =
        crate::eval::compile::bytecode::compile_with_registry(&expression, None).unwrap();
    let deepest = compiled.typing.expressions().last().unwrap();
    assert_eq!(deepest.contract, ExpressionContract::Atom);
    assert_eq!(
        deepest.properties.result,
        FormulaType::scalar(ScalarKinds::ERROR)
    );
    assert_eq!(deepest.properties.shape, ShapeBounds::SCALAR);
}

#[test]
fn failed_parent_inference_cannot_publish_its_completed_child_as_a_root() {
    let expression = poirot_parsing::formula::parse_formula("SUM(1)").unwrap();
    let Expr::Function { args, .. } = &expression else {
        panic!("function syntax");
    };
    let mut state = InferenceMachine::new(2).unwrap();
    state.enter(&expression, 0, 0).unwrap();
    state.enter(args.first().unwrap(), 1, 1).unwrap();
    state.leave(&[]).unwrap();
    // The driver omitted the parent's binding. Completed children are not a root.
    assert!(matches!(
        state.leave(&[]),
        Err(CompileError::InvalidReference)
    ));
    assert!(matches!(
        state.finish(),
        Err(CompileError::InvalidReference)
    ));
}
